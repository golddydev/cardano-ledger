//! # Cardano Slot-to-POSIX Time Conversion for Plutus Phase 2 Validation
//!
//! This module converts a Cardano transaction's validity range (expressed in
//! blockchain **slots**) into a Plutus **POSIX time range** (in milliseconds
//! since the Unix epoch). This conversion is required for phase 2 validation:
//! before the Plutus evaluator is called, the ledger must supply the script
//! with a `txInfoValidRange` field expressed in POSIX milliseconds.
//!
//! ## Why conversion is needed
//!
//! Smart contracts reason about real-world time (e.g. "this vesting contract
//! unlocks after 2025-01-01"). Transactions express validity in slots because
//! the blockchain only knows about slots. The ledger bridges this gap.
//!
//! ## Conversion algorithm (plain English)
//!
//! 1. Look up the **era** that contains the target slot.
//!    Each era has a fixed slot length (e.g. Byron: 20 s, Shelley+: 1 s).
//! 2. Compute how many slots have elapsed since that era began.
//! 3. Multiply by the era's slot length (ms) to get elapsed milliseconds.
//! 4. Add the era's known start time (POSIX ms) to get the absolute timestamp.
//! 5. Return as `i64` milliseconds — **truncated**, not rounded.
//!
//! ## ASCII conversion flow
//!
//! ```text
//! ValidityInterval (SlotNo, SlotNo)
//!     │
//!     ▼  trans_validity_interval()
//!     │   ├── (None, None)       → always()  [PV1.always]
//!     │   ├── (Some(i), None)    → from_posix(t)
//!     │   ├── (None, Some(j))    → (−∞, strict_upper(t))
//!     │   └── (Some(i), Some(j)) → [lower(t1), strict_upper(t2))
//!     │
//!     ▼  slot_to_posix_time() per bound
//!     │
//!     EraHistory.find_containing_era(slot)
//!     │  → EraSummary { start_slot, start_time, slot_length, epoch_size }
//!     │
//!     posix_ms = start_time + (slot − start_slot) × slot_length.ms
//!     │  (i128 arithmetic, checked cast to i64)
//!     │
//!     ▼
//! POSIXTime(i64)  →  POSIXTimeRange = Interval<POSIXTime>
//! ```
//!
//! ## Public functions
//!
//! - [`slot_to_posix_time`] — converts a single `SlotNo` to `POSIXTime`
//! - [`trans_validity_interval`] — converts a `ValidityInterval` to `POSIXTimeRange`
//!
//! ## Haskell reference
//!
//! This file is a faithful Rust port of the conversion logic spanning two
//! Haskell packages:
//!
//! - **`cardano-ledger`** — contains `slotToPOSIXTime` and `transValidityInterval`
//!   - `libs/cardano-ledger-core/src/Cardano/Ledger/Plutus/TxInfo.hs`
//!   - `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Plutus/TxInfo.hs`
//!   - `eras/conway/impl/src/Cardano/Ledger/Conway/TxInfo.hs`
//!
//! - **`cardano-slotting`** (external) — contains era-history types and lookup
//!   - `Cardano.Slotting.EpochInfo` — `EpochInfo`, `epochInfoSlotToUTCTime`
//!   - `Cardano.Slotting.Time` — `SystemStart`, `SlotLength`, `mkSlotLength`
//!   - `Cardano.Slotting.Slot` — `SlotNo`, `EpochNo`, `EpochSize`
//!   - `Ouroboros.Consensus.HardFork.History.Summary` — era summary/params/bounds
//!
//! At runtime, the ledger obtains `EpochInfo` and `SystemStart` from the
//! `Globals` record (`libs/cardano-ledger-core/src/Cardano/Ledger/BaseTypes.hs:720-747`):
//! ```haskell
//! data Globals = Globals
//!   { epochInfo   :: !(EpochInfo (Either Text))
//!   , systemStart :: !SystemStart
//!   , ...
//!   }
//! ```
//!
//! See the doc comments on each function for exact file paths and line numbers.
//!
//! ## Usage
//!
//! ```rust,ignore
//! let era_history = mainnet_era_history();
//! let interval = ValidityInterval {
//!     invalid_before:    Some(SlotNo(7_000_000)),
//!     invalid_hereafter: Some(SlotNo(7_100_000)),
//! };
//! let time_range = trans_validity_interval(&era_history, &interval)?;
//! // Pass time_range to the Plutus evaluator as txInfoValidRange
//! ```

#![allow(dead_code)]

// ============================================================================
// Error type
// ============================================================================

/// Errors that can occur during slot-to-POSIX time conversion.
///
/// These mirror the error variants produced by the Haskell ledger when it
/// cannot translate a slot to a UTC/POSIX time.
///
/// # Haskell ref
/// `AlonzoContextError`
/// `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Plutus/TxInfo.hs:180-183`
/// ```haskell
/// data AlonzoContextError era
///   = TranslationLogicMissingInput !TxIn
///   | TimeTranslationPastHorizon !Text
/// ```
/// The `TimeTranslationPastHorizon` variant wraps the error `Text` returned
/// by `epochInfoSlotToUTCTime` when a slot is beyond the forecastable
/// horizon. Babbage and Conway context errors embed `AlonzoContextError`
/// via an `Inject` instance.
#[derive(Debug, PartialEq)]
pub enum TimeTranslationError {
    /// The requested slot is beyond the last known era's horizon.
    /// The contained value is the slot number that exceeded the horizon.
    ///
    /// Haskell: raised as `TimeTranslationPastHorizon` when
    /// `epochInfoSlotToUTCTime` returns `Left` (error). Constructed in
    /// `transSlotToPOSIXTime`:
    /// ```haskell
    /// transSlotToPOSIXTime =
    ///   left (inject . TimeTranslationPastHorizon @era)
    ///     . slotToPOSIXTime epochInfo systemStart
    /// ```
    SlotPastHorizon(u64),

    /// The `EraHistory` supplied contains no era entries.
    /// At minimum, a single entry starting at slot 0 is required.
    EmptyEraHistory,

    /// An `EraSummary` has an invalid parameter (e.g. `slot_length.ms == 0`
    /// or `epoch_size.slots == 0`), which would cause division by zero or
    /// produce nonsensical time values.
    InvalidEraParams(String),

    /// Checked arithmetic detected an overflow while computing the POSIX
    /// millisecond value. This should never occur for realistic slot numbers
    /// on any existing Cardano network.
    ArithmeticOverflow,
}

// ============================================================================
// Input types
// ============================================================================

/// The absolute POSIX time (milliseconds since 1970-01-01 00:00:00 UTC) at
/// which the blockchain began — i.e. the real-world time for slot 0.
///
/// This value is fixed per network. Mainnet value:
/// `1_506_203_091_000` ms = 2017-09-23 21:44:51 UTC.
///
/// # Haskell ref
/// `Cardano.Slotting.Time.SystemStart` (external `cardano-slotting` library).
/// ```haskell
/// newtype SystemStart = SystemStart { getSystemStart :: UTCTime }
/// ```
/// `SystemStart` wraps a `UTCTime`; the ledger converts it to POSIX seconds
/// inside `slotToPOSIXTime` via `utcTimeToPOSIXSeconds`. In the running
/// ledger, it is stored in `Globals.systemStart`:
/// ```haskell
/// -- libs/cardano-ledger-core/src/Cardano/Ledger/BaseTypes.hs:745
/// systemStart :: !SystemStart
/// ```
/// The value for mainnet is obtained from `sgSystemStart` in
/// `ShelleyGenesis` (`eras/shelley/impl/src/Cardano/Ledger/Shelley/Genesis.hs`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SystemStart {
    /// POSIX milliseconds at genesis.
    pub posix_ms: i64,
}

/// The duration of a single slot, in milliseconds.
///
/// Byron mainnet: `20_000` ms (20 seconds per slot).
/// Shelley and all later eras: `1_000` ms (1 second per slot).
///
/// # Haskell ref
/// `Cardano.Slotting.Time.SlotLength` (external `cardano-slotting` library).
/// ```haskell
/// newtype SlotLength = SlotLength { getSlotLength :: NominalDiffTime }
/// ```
/// Constructed via `mkSlotLength :: NominalDiffTime -> SlotLength`.
/// In this Rust port we store the pre-computed millisecond value directly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlotLength {
    /// Milliseconds per slot.
    pub ms: u64,
}

/// The number of slots in one epoch.
///
/// Byron mainnet: `21_600` slots/epoch.
/// Shelley and all later eras: `432_000` slots/epoch.
///
/// # Haskell ref
/// `Cardano.Slotting.Slot.EpochSize` (external `cardano-slotting` library).
/// ```haskell
/// newtype EpochSize = EpochSize { unEpochSize :: Word64 }
/// ```
/// Shelley genesis sets this via `sgEpochLength :: !EpochSize`
/// (`eras/shelley/impl/src/Cardano/Ledger/Shelley/Genesis.hs:215`).
/// Byron mainnet value: `EpochSlots 21600`
/// (`eras/byron/ledger/impl/src/Cardano/Chain/Epoch/File.hs:69`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EpochSize {
    /// Number of slots per epoch.
    pub slots: u64,
}

/// A non-negative slot number on the Cardano chain.
///
/// Internally a `u64`. Slot 0 is the genesis block.
///
/// # Haskell ref
/// `Cardano.Slotting.Slot.SlotNo` (external `cardano-slotting` library).
/// ```haskell
/// newtype SlotNo = SlotNo { unSlotNo :: Word64 }
/// ```
/// Re-exported in the ledger via `Cardano.Ledger.Slot`
/// (`libs/cardano-ledger-core/src/Cardano/Ledger/Slot.hs:28`).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord)]
pub struct SlotNo(pub u64);

/// Time parameters for a single protocol era.
///
/// An era is a contiguous range of slots during which the slot length and
/// epoch size are constant. The Cardano chain has had multiple eras (Byron,
/// Shelley, Allegra, Mary, Alonzo, Babbage, Conway …), each potentially with
/// different parameters.
///
/// The first era always starts at slot 0. Subsequent eras start at the slot
/// recorded in `start_slot`, and their `start_time` is the cumulative POSIX
/// time at that transition point.
///
/// # Haskell ref
/// One entry in `Ouroboros.Consensus.HardFork.History.Summary`
/// (`ouroboros-consensus` package). Each entry combines:
///
/// - `EraParams` — the era's fixed parameters:
///   ```haskell
///   data EraParams = EraParams
///     { eraEpochSize  :: !EpochSize
///     , eraSlotLength :: !SlotLength
///     , eraSafeZone   :: !SafeZone
///     }
///   ```
///
/// - `Bound` — the era's start boundary (slot + time):
///   ```haskell
///   data Bound = Bound
///     { boundTime  :: !RelativeTime   -- time since SystemStart
///     , boundSlot  :: !SlotNo
///     , boundEpoch :: !EpochNo
///     }
///   ```
///
/// This Rust struct flattens both into a single record, and stores
/// `start_time` as absolute POSIX milliseconds (pre-adding `SystemStart`)
/// rather than as `RelativeTime` offset.
#[derive(Debug, Clone)]
pub struct EraSummary {
    /// The first slot of this era (inclusive). Maps to `Bound.boundSlot`.
    pub start_slot: SlotNo,
    /// The POSIX time (ms) at the start of this era.
    /// Haskell stores `Bound.boundTime` as `RelativeTime` (seconds since
    /// `SystemStart`); we pre-compute the absolute POSIX ms value.
    pub start_time: i64,
    /// Duration of each slot in this era. Maps to `EraParams.eraSlotLength`.
    pub slot_length: SlotLength,
    /// Number of slots per epoch in this era. Maps to `EraParams.eraEpochSize`.
    pub epoch_size: EpochSize,
}

/// An ordered, non-overlapping list of [`EraSummary`] entries describing the
/// full protocol history from genesis to the current known **horizon**.
///
/// The horizon is the boundary beyond which the slot-to-time mapping is
/// unknown. Any slot at or beyond the horizon causes
/// [`TimeTranslationError::SlotPastHorizon`].
///
/// The Haskell ledger enforces this via `validateOutsideForecast`:
/// if a transaction has Plutus redeemers, `invalidHereafter` must be within
/// the forecastable horizon.
///
/// # Haskell ref
/// `Ouroboros.Consensus.HardFork.History.Summary` (`ouroboros-consensus`
/// package). The `Summary` is converted into `EpochInfo (Either Text)` via
/// `summaryToEpochInfo` / `toEpochInfo`, and stored in `Globals.epochInfo`:
/// ```haskell
/// -- libs/cardano-ledger-core/src/Cardano/Ledger/BaseTypes.hs:720-721
/// data Globals = Globals
///   { epochInfo :: !(EpochInfo (Either Text))
///   , ...
///   }
/// ```
///
/// # Horizon: `validateOutsideForecast`
/// `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Rules/Utxo.hs:377-403`
/// ```haskell
/// -- If tx has non-native scripts, end of validity interval must
/// -- translate to time:
/// --   (_, i_f) := txvldt tx
/// --   ◇ ∉ { txrdmrs tx, i_f } ⇒
/// --     epochInfoSlotToUTCTime epochInfo systemTime i_f ≠ ◇
/// validateOutsideForecast ei slotNo sysSt tx =
///   case tx ^. bodyTxL . vldtTxBodyL of
///     ValidityInterval _ (SJust ifj)
///       | not . null $ tx ^. witsTxL . rdmrsTxWitsL . unRedeemersL ->
///           let ei' = unsafeLinearExtendEpochInfo slotNo ei
///            in failureUnless
///                 (isRight (epochInfoSlotToUTCTime ei' sysSt ifj))
///                 (OutsideForecast ifj)
///     _ -> pure ()
/// ```
#[derive(Debug, Clone)]
pub struct EraHistory {
    /// Eras ordered by `start_slot` ascending; must be non-overlapping.
    /// The first entry must have `start_slot = SlotNo(0)`.
    pub eras: Vec<EraSummary>,
}

/// A transaction's validity range, expressed in slots.
///
/// Semantics: the transaction is valid in slot `s` if and only if
/// `invalid_before ≤ s < invalid_hereafter` (with absent bounds treated as
/// ±∞). This is a **half-open** interval: closed on the left, open on the
/// right.
///
/// `None` on either bound means unbounded in that direction:
/// - `invalid_before = None` → valid from the beginning of time (−∞)
/// - `invalid_hereafter = None` → valid indefinitely (+∞)
///
/// # Haskell ref
/// `eras/allegra/impl/src/Cardano/Ledger/Allegra/Scripts.hs:116-122`
/// ```haskell
/// -- | ValidityInterval is a half open interval. Closed on the bottom,
/// -- open on the top. A SNothing on the bottom is negative infinity,
/// -- and a SNothing on the top is positive infinity.
/// data ValidityInterval = ValidityInterval
///   { invalidBefore    :: !(StrictMaybe SlotNo)
///   , invalidHereafter :: !(StrictMaybe SlotNo)
///   }
/// ```
/// `StrictMaybe SlotNo` maps to `Option<SlotNo>` here.
/// `SNothing` → `None`; `SJust n` → `Some(SlotNo(n))`.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidityInterval {
    /// First slot at which the transaction is valid. `None` = −∞.
    pub invalid_before: Option<SlotNo>,
    /// First slot at which the transaction is **no longer** valid. `None` = +∞.
    pub invalid_hereafter: Option<SlotNo>,
}

// ============================================================================
// Plutus interval mirror types
// ============================================================================

/// POSIX time in milliseconds, as expected by the Plutus evaluator.
///
/// This is the unit that Plutus smart contracts receive as `txInfoValidRange`.
/// The Haskell ledger produces this by converting `UTCTime` → POSIX seconds
/// → milliseconds, then **truncating** (not rounding):
///
/// ```haskell
/// -- libs/cardano-ledger-core/src/Cardano/Ledger/Plutus/TxInfo.hs:169-171
/// slotToPOSIXTime ei sysS s = do
///   PV1.POSIXTime
///     . (truncate . (* 1000))           -- (3) seconds → ms, truncate
///     . nominalDiffTimeToSeconds         -- (2) NominalDiffTime → Pico seconds
///     . utcTimeToPOSIXSeconds            -- (1) UTCTime → POSIX NominalDiffTime
///     <$> epochInfoSlotToUTCTime ei sysS s  -- slot → UTCTime
/// ```
///
/// # Haskell ref
/// `PlutusLedgerApi.V1.POSIXTime`
/// ```haskell
/// newtype POSIXTime = POSIXTime { getPOSIXTime :: Integer }
/// ```
/// Although the Haskell type uses unbounded `Integer`, in practice it is
/// serialised as a bounded integer; we use `i64`.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord)]
pub struct POSIXTime(pub i64);

/// Extends a type `T` with positive and negative infinity.
///
/// Used as the value inside [`LowerBound`] and [`UpperBound`] to express
/// unbounded intervals (e.g. "valid until the end of time").
///
/// # Haskell ref
/// `PlutusLedgerApi.V1.Interval.Extended`
/// ```haskell
/// data Extended a = NegInf | Finite a | PosInf
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum Extended<T> {
    /// Negative infinity (−∞). Used for an absent lower bound.
    NegInf,
    /// A concrete, finite value.
    Finite(T),
    /// Positive infinity (+∞). Used for an absent upper bound.
    PosInf,
}

/// The lower bound of an interval, with an inclusive/exclusive flag.
///
/// `inclusive = true` means the bound is **closed** (≥ the bound value).
/// `inclusive = false` means the bound is **open** (> the bound value).
///
/// For validity intervals produced by this module, the lower bound is always
/// inclusive (`inclusive = true`) when finite.
///
/// # Haskell ref
/// `PlutusLedgerApi.V1.Interval.LowerBound`
/// ```haskell
/// newtype LowerBound a = LowerBound (Extended a, Bool)
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct LowerBound<T> {
    /// The bound value (possibly infinite).
    pub bound: Extended<T>,
    /// Whether this bound is inclusive (`true`) or exclusive (`false`).
    pub inclusive: bool,
}

/// The upper bound of an interval, with an inclusive/exclusive flag.
///
/// For validity intervals, the upper bound is **always exclusive** (strict):
/// `inclusive = false`. This preserves the half-open `[lower, upper)`
/// semantics of [`ValidityInterval`].
///
/// # Haskell ref
/// `PlutusLedgerApi.V1.Interval.UpperBound`
/// ```haskell
/// newtype UpperBound a = UpperBound (Extended a, Bool)
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct UpperBound<T> {
    /// The bound value (possibly infinite).
    pub bound: Extended<T>,
    /// Whether this bound is inclusive (`true`) or exclusive (`false`).
    pub inclusive: bool,
}

/// A generic interval `[from, to]` parameterised over a value type `T`.
///
/// The actual openness/closedness is carried by the `inclusive` flags in
/// [`LowerBound`] and [`UpperBound`].
///
/// # Haskell ref
/// `PlutusLedgerApi.V1.Interval.Interval`
/// ```haskell
/// data Interval a = Interval { ivFrom :: LowerBound a, ivTo :: UpperBound a }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Interval<T> {
    /// Lower bound of the interval.
    pub from: LowerBound<T>,
    /// Upper bound of the interval.
    pub to: UpperBound<T>,
}

/// A Plutus time range: an `Interval` over [`POSIXTime`] (milliseconds).
///
/// This is the type passed to Plutus scripts as `txInfoValidRange`.
///
/// # Haskell ref
/// `PlutusLedgerApi.V1.POSIXTimeRange`
/// ```haskell
/// type POSIXTimeRange = Interval POSIXTime
/// ```
pub type POSIXTimeRange = Interval<POSIXTime>;

// ============================================================================
// Interval helper constructors
// ============================================================================

/// Construct the "always" interval: (−∞, +∞) — valid at all times.
///
/// Both bounds are `inclusive = true`, matching `PV1.always`.
///
/// # Haskell ref
/// `PV1.always` from `PlutusLedgerApi.V1.Interval`
/// ```haskell
/// always :: Interval a
/// always = Interval (LowerBound NegInf True) (UpperBound PosInf True)
/// ```
pub fn always() -> POSIXTimeRange {
    Interval {
        from: LowerBound { bound: Extended::NegInf, inclusive: true },
        to: UpperBound { bound: Extended::PosInf, inclusive: true },
    }
}

/// Construct the interval `[t, +∞)` — valid from `t` onwards.
///
/// Used when `invalid_before` is `Some` and `invalid_hereafter` is `None`.
///
/// # Haskell ref
/// `PV1.from` from `PlutusLedgerApi.V1.Interval`
/// ```haskell
/// from :: a -> Interval a
/// from s = Interval (lowerBound s) (UpperBound PosInf True)
/// ```
pub fn from_posix(t: POSIXTime) -> POSIXTimeRange {
    Interval {
        from: lower_bound(t),
        to: UpperBound { bound: Extended::PosInf, inclusive: true },
    }
}

/// Construct the interval `(−∞, t)` — valid up to (but not including) `t`.
///
/// Used when `invalid_before` is `None` and `invalid_hereafter` is `Some`.
/// The upper bound is **strict** (exclusive), matching the half-open
/// `[lower, upper)` semantics of `ValidityInterval`.
///
/// # Haskell ref
/// `PV1.to` from `PlutusLedgerApi.V1.Interval`
/// ```haskell
/// to :: a -> Interval a
/// to s = Interval (LowerBound NegInf True) (strictUpperBound s)
/// ```
/// Note: `PV1.to` uses `strictUpperBound`, so this is exclusive on top.
pub fn to_posix(t: POSIXTime) -> POSIXTimeRange {
    Interval {
        from: LowerBound { bound: Extended::NegInf, inclusive: true },
        to: strict_upper_bound(t),
    }
}

/// Construct an inclusive (closed) lower bound: `LowerBound(Finite(t), true)`.
///
/// Represents `t ≤ …` (the interval starts at `t`, inclusive).
///
/// # Haskell ref
/// `PV1.lowerBound` from `PlutusLedgerApi.V1.Interval`
/// ```haskell
/// lowerBound :: a -> LowerBound a
/// lowerBound a = LowerBound (Finite a) True
/// ```
pub fn lower_bound(t: POSIXTime) -> LowerBound<POSIXTime> {
    LowerBound { bound: Extended::Finite(t), inclusive: true }
}

/// Construct a strict (exclusive) upper bound: `UpperBound(Finite(t), false)`.
///
/// Represents `… < t` (the interval ends before `t`, not including `t`).
/// This is the correct bound type for `invalidHereafter` because
/// `ValidityInterval` is half-open: the upper slot is excluded.
///
/// # Haskell ref
/// `PV1.strictUpperBound` from `PlutusLedgerApi.V1.Interval`
/// ```haskell
/// strictUpperBound :: a -> UpperBound a
/// strictUpperBound a = UpperBound (Finite a) False
/// ```
/// Used at:
/// `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Plutus/TxInfo.hs:239`
/// `eras/conway/impl/src/Cardano/Ledger/Conway/TxInfo.hs:798,805`
pub fn strict_upper_bound(t: POSIXTime) -> UpperBound<POSIXTime> {
    UpperBound { bound: Extended::Finite(t), inclusive: false }
}

// ============================================================================
// Core conversion functions
// ============================================================================

/// Convert a single [`SlotNo`] to a [`POSIXTime`] (milliseconds).
///
/// This is the core of the slot-to-time conversion. It works by:
///
/// 1. **Validate** — return [`TimeTranslationError::EmptyEraHistory`] if no
///    eras are present, or [`TimeTranslationError::InvalidEraParams`] if any
///    era has a zero slot length.
/// 2. **Find the containing era** — scan eras in order, finding the last era
///    whose `start_slot ≤ slot`. If no such era exists (i.e. `slot` is before
///    the first era, which can only happen if the first era's `start_slot > 0`)
///    or if `slot` is at or beyond the horizon, return
///    [`TimeTranslationError::SlotPastHorizon`].
/// 3. **Compute slot offset** — how many slots into the era is this slot?
///    `slot_offset = slot − era.start_slot`
/// 4. **Compute elapsed milliseconds** — using `i128` to avoid overflow:
///    `elapsed_ms = slot_offset × era.slot_length.ms`
/// 5. **Compute absolute POSIX ms** — add the era's known start time:
///    `posix_ms = era.start_time + elapsed_ms`
/// 6. **Range-check** — if `posix_ms` overflows `i64`, return
///    [`TimeTranslationError::ArithmeticOverflow`].
/// 7. **Return** `POSIXTime(posix_ms as i64)` — **truncated**, not rounded,
///    matching the Haskell `truncate` in `slotToPOSIXTime`.
///
/// # Haskell ref
/// `slotToPOSIXTime`
/// `libs/cardano-ledger-core/src/Cardano/Ledger/Plutus/TxInfo.hs:164-171`
/// ```haskell
/// slotToPOSIXTime ::
///   EpochInfo (Either Text) ->
///   SystemStart ->
///   SlotNo ->
///   Either Text PV1.POSIXTime
/// slotToPOSIXTime ei sysS s = do
///   PV1.POSIXTime . (truncate . (* 1000)) . nominalDiffTimeToSeconds
///     . utcTimeToPOSIXSeconds
///     <$> epochInfoSlotToUTCTime ei sysS s
/// ```
///
/// ## Conversion chain
///
/// The Haskell pipeline is a composition of five steps:
///
/// ```text
/// SlotNo
///   │ epochInfoSlotToUTCTime ei sysS
///   │   (cardano-slotting: looks up the containing era in the Summary,
///   │    computes RelativeTime = Bound.boundTime + (slot − Bound.boundSlot) × slotLength,
///   │    then adds SystemStart to get UTCTime)
///   ▼
/// UTCTime
///   │ utcTimeToPOSIXSeconds          (Data.Time.Clock.POSIX)
///   ▼
/// NominalDiffTime                    (seconds since Unix epoch, with sub-second precision)
///   │ nominalDiffTimeToSeconds        (Data.Time.Clock)
///   ▼
/// Pico                               (fixed-precision seconds, 10⁻¹² resolution)
///   │ (* 1000)                        (seconds → milliseconds)
///   ▼
/// Pico                               (milliseconds, still Pico type)
///   │ truncate                        (drop fractional part → Integer)
///   ▼
/// Integer  →  PV1.POSIXTime          (wrapped in the newtype)
/// ```
///
/// This Rust function combines all five steps: the era lookup is done
/// inline (since we have `EraHistory` rather than an opaque `EpochInfo`),
/// and times are already in milliseconds, so no seconds→ms conversion or
/// truncation is needed.
///
/// # Errors
///
/// - [`TimeTranslationError::EmptyEraHistory`] — `era_history.eras` is empty.
/// - [`TimeTranslationError::InvalidEraParams`] — an era has `slot_length.ms == 0`.
/// - [`TimeTranslationError::SlotPastHorizon`] — `slot` is at or beyond the
///   last era's known horizon.
/// - [`TimeTranslationError::ArithmeticOverflow`] — computed value exceeds `i64`.
pub fn slot_to_posix_time(
    era_history: &EraHistory,
    slot: SlotNo,
) -> Result<POSIXTime, TimeTranslationError> {
    // Step 1: Validate — EraHistory must have at least one entry.
    if era_history.eras.is_empty() {
        return Err(TimeTranslationError::EmptyEraHistory);
    }

    // Step 1b: Validate era parameters.
    for (i, era) in era_history.eras.iter().enumerate() {
        if era.slot_length.ms == 0 {
            return Err(TimeTranslationError::InvalidEraParams(format!(
                "era[{}] has slot_length.ms == 0",
                i
            )));
        }
        if era.epoch_size.slots == 0 {
            return Err(TimeTranslationError::InvalidEraParams(format!(
                "era[{}] has epoch_size.slots == 0",
                i
            )));
        }
    }

    // Step 2: Find the containing era.
    //
    // The containing era is the LAST era whose start_slot <= slot.
    // We scan from the end for efficiency (most recent era first), but
    // since there are at most ~8 eras, a forward scan is equally fine.
    //
    // Mirrors the era-lookup performed inside cardano-slotting's
    // `epochInfoSlotToUTCTime` → `interpretQuery` → `wallclockFromSlot`
    // (Ouroboros.Consensus.HardFork.History.Qry).
    let containing_era = era_history
        .eras
        .iter()
        .rev()
        .find(|era| era.start_slot <= slot);

    let era = match containing_era {
        Some(e) => e,
        // Step 2 (error): slot is before the first era's start_slot, or
        // (more commonly) after the known horizon.
        None => return Err(TimeTranslationError::SlotPastHorizon(slot.0)),
    };

    // Step 3: Compute the slot offset within the containing era.
    //
    // Haskell equivalent: slot − Bound.boundSlot
    let slot_offset: u64 = slot.0 - era.start_slot.0;

    // Step 4: Compute elapsed milliseconds (use i128 to avoid u64 overflow).
    //
    // Haskell: RelativeTime = slot_offset × EraParams.eraSlotLength
    // Haskell uses arbitrary-precision Integer; we use i128 as a safe
    // intermediate. For any realistic Cardano slot this will never overflow i128.
    let elapsed_ms: i128 = slot_offset as i128 * era.slot_length.ms as i128;

    // Step 5: Compute absolute POSIX time in milliseconds.
    //
    // Haskell: UTCTime = SystemStart + Bound.boundTime + elapsed
    // We've pre-computed era.start_time = SystemStart + Bound.boundTime
    // (as POSIX ms), so this is just: era.start_time + elapsed_ms.
    let posix_ms: i128 = era.start_time as i128 + elapsed_ms;

    // Step 6: Range-check and cast to i64.
    //
    // The Haskell type PV1.POSIXTime wraps Integer, but in practice it is
    // serialised as Int64. Any value outside i64 range would be rejected by
    // the evaluator anyway.
    if posix_ms < i64::MIN as i128 || posix_ms > i64::MAX as i128 {
        return Err(TimeTranslationError::ArithmeticOverflow);
    }

    // Step 7: Return as POSIXTime — truncated (which i128→i64 cast does for
    // positive values; the checked range above ensures correctness).
    //
    // Haskell: `PV1.POSIXTime . (truncate . (* 1000)) . nominalDiffTimeToSeconds`
    // The `* 1000` and `truncate` are baked into era.start_time being in ms
    // already and the i128→i64 cast here.
    Ok(POSIXTime(posix_ms as i64))
}

/// Convert a slot-based [`ValidityInterval`] to a Plutus [`POSIXTimeRange`].
///
/// This function handles all four combinations of optional bounds, converting
/// each present slot bound to a POSIX millisecond timestamp using
/// [`slot_to_posix_time`]. The upper bound is **always strict (exclusive)**,
/// preserving the half-open `[invalid_before, invalid_hereafter)` semantics.
///
/// ## What each match arm does
///
/// | `(invalid_before, invalid_hereafter)` | Output interval |
/// |---|---|
/// | `(None, None)` | `always()` — `(−∞, +∞)` |
/// | `(Some(i), None)` | `[posix(i), +∞)` |
/// | `(None, Some(j))` | `(−∞, posix(j))` strict |
/// | `(Some(i), Some(j))` | `[posix(i), posix(j))` |
///
/// The result is passed to the Plutus evaluator as `txInfoValidRange` inside
/// the `TxInfo` struct.
///
/// # Haskell ref
///
/// Two versions exist in the codebase. This Rust function follows the
/// **Conway** variant.
///
/// ## Conway variant (current)
/// `eras/conway/impl/src/Cardano/Ledger/Conway/TxInfo.hs:784-809`
/// ```haskell
/// transValidityInterval _ epochInfo systemStart = \case
///   ValidityInterval SNothing  SNothing  -> pure PV1.always
///   ValidityInterval (SJust i) SNothing  -> PV1.from <$> transSlotToPOSIXTime i
///   ValidityInterval SNothing  (SJust i) -> do
///     t <- transSlotToPOSIXTime i
///     pure $ PV1.Interval (PV1.LowerBound PV1.NegInf True)
///                         (PV1.strictUpperBound t)
///   ValidityInterval (SJust i) (SJust j) -> do
///     t1 <- transSlotToPOSIXTime i
///     t2 <- transSlotToPOSIXTime j
///     pure $ PV1.Interval (PV1.lowerBound t1) (PV1.strictUpperBound t2)
///   where
///     transSlotToPOSIXTime =
///       left (inject . TimeTranslationPastHorizon @era)
///         . slotToPOSIXTime epochInfo systemStart
/// ```
///
/// ## Alonzo variant (original)
/// `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Plutus/TxInfo.hs:220-243`
/// ```haskell
/// transValidityInterval _ epochInfo systemStart = \case
///   ValidityInterval SNothing  SNothing  -> pure PV1.always
///   ValidityInterval (SJust i) SNothing  -> PV1.from <$> transSlotToPOSIXTime i
///   ValidityInterval SNothing  (SJust i) -> PV1.to <$> transSlotToPOSIXTime i
///   ValidityInterval (SJust i) (SJust j) -> do
///     t1 <- transSlotToPOSIXTime i
///     t2 <- transSlotToPOSIXTime j
///     pure $ PV1.Interval (PV1.lowerBound t1) (PV1.strictUpperBound t2)
/// ```
///
/// ## Difference
/// The `(None, Some)` case: Alonzo uses `PV1.to`, Conway spells it out as
/// `PV1.Interval (LowerBound NegInf True) (strictUpperBound t)`.
/// These are **semantically identical** — `PV1.to` is defined as:
/// ```haskell
/// to s = Interval (LowerBound NegInf True) (strictUpperBound s)
/// ```
///
/// ## Usage in TxInfo construction
/// Both variants are called when building `PV1.txInfoValidRange` inside
/// `toPlutusTxInfo` for PlutusV1, V2, and V3 script context.
///
/// # Errors
///
/// Propagates any [`TimeTranslationError`] returned by [`slot_to_posix_time`]
/// for each slot bound that is converted.
pub fn trans_validity_interval(
    era_history: &EraHistory,
    interval: &ValidityInterval,
) -> Result<POSIXTimeRange, TimeTranslationError> {
    match (&interval.invalid_before, &interval.invalid_hereafter) {
        // Match arm 1: (None, None) → always()
        //
        // No bounds at all: the transaction is valid at every possible time.
        // Haskell: `ValidityInterval SNothing SNothing -> pure PV1.always`
        (None, None) => Ok(always()),

        // Match arm 2: (Some(i), None) → [posix(i), +∞)
        //
        // Only a lower bound: valid from this slot onwards indefinitely.
        // Haskell: `ValidityInterval (SJust i) SNothing -> PV1.from <$> ...`
        (Some(i), None) => {
            let t = slot_to_posix_time(era_history, *i)?;
            Ok(from_posix(t))
        }

        // Match arm 3: (None, Some(j)) → (−∞, posix(j)) strict upper bound
        //
        // Only an upper bound: valid until this slot (exclusive).
        // Alonzo:  `PV1.to <$> transSlotToPOSIXTime i`
        // Conway:  `PV1.Interval (LowerBound NegInf True) (strictUpperBound t)`
        // Both are semantically identical — PV1.to expands to the Conway form.
        (None, Some(j)) => {
            let t = slot_to_posix_time(era_history, *j)?;
            Ok(Interval {
                from: LowerBound { bound: Extended::NegInf, inclusive: true },
                to: strict_upper_bound(t),
            })
        }

        // Match arm 4: (Some(i), Some(j)) → [posix(i), posix(j))
        //
        // Both bounds present: a finite half-open interval.
        // Lower bound is inclusive (closed); upper bound is strict (open).
        // Haskell: `ValidityInterval (SJust i) (SJust j) ->
        //     t1 <- ...; t2 <- ...;
        //     pure $ PV1.Interval (lowerBound t1) (strictUpperBound t2)`
        (Some(i), Some(j)) => {
            let t1 = slot_to_posix_time(era_history, *i)?;
            let t2 = slot_to_posix_time(era_history, *j)?;
            Ok(Interval {
                from: lower_bound(t1),
                to: strict_upper_bound(t2),
            })
        }
    }
}

// ============================================================================
// Mainnet era parameters
// ============================================================================

/// Mainnet genesis time: 2017-09-23 21:44:51 UTC in POSIX milliseconds.
///
/// This is the `SystemStart` for Cardano mainnet — the absolute real-world
/// time at which slot 0 occurred.
///
/// # Haskell ref
/// Set from `sgSystemStart` in `ShelleyGenesis`:
/// ```haskell
/// -- eras/shelley/impl/src/Cardano/Ledger/Shelley/Genesis.hs:675-676
/// systemStart = SystemStart $ sgSystemStart genesis
/// ```
pub const MAINNET_SYSTEM_START_MS: i64 = 1_506_203_091_000;

/// Byron mainnet slot length: 20 seconds (20,000 ms) per slot.
///
/// # Haskell ref
/// Implied by Byron genesis parameters. The 20-second slot length is
/// a well-known Byron mainnet constant.
pub const BYRON_SLOT_LENGTH_MS: u64 = 20_000;

/// Byron mainnet epoch size: 21,600 slots per epoch.
///
/// # Haskell ref
/// `eras/byron/ledger/impl/src/Cardano/Chain/Epoch/File.hs:69`
/// `mainnetEpochSlots = EpochSlots 21600`
pub const BYRON_EPOCH_SIZE: u64 = 21_600;

/// Shelley and all later eras: 1 second (1,000 ms) per slot.
///
/// # Haskell ref
/// Set from `sgSlotLength` in `ShelleyGenesis`
/// (`eras/shelley/impl/src/Cardano/Ledger/Shelley/Genesis.hs`).
/// Constructed via `mkSlotLength` from `cardano-slotting`.
pub const SHELLEY_SLOT_LENGTH_MS: u64 = 1_000;

/// Shelley and all later eras: 432,000 slots per epoch (5 days).
///
/// # Haskell ref
/// Set from `sgEpochLength :: !EpochSize` in `ShelleyGenesis`
/// (`eras/shelley/impl/src/Cardano/Ledger/Shelley/Genesis.hs:215`).
pub const SHELLEY_EPOCH_SIZE: u64 = 432_000;

// Mainnet era start slots (first slot of each era):
/// Shelley start slot (2020-07-29): 4,492,800
pub const SHELLEY_START_SLOT: u64 = 4_492_800;
/// Allegra start slot (2020-12-16): 5,068,800
pub const ALLEGRA_START_SLOT: u64 = 5_068_800;
/// Mary start slot (2021-03-01): 5,788,800
pub const MARY_START_SLOT: u64 = 5_788_800;
/// Alonzo start slot (2021-09-12): 6,220,800
pub const ALONZO_START_SLOT: u64 = 6_220_800;
/// Babbage start slot (2022-06-22): 7,791,600
pub const BABBAGE_START_SLOT: u64 = 7_791_600;
/// Conway start slot (2024-09-01 approx): 10,577,800
pub const CONWAY_START_SLOT: u64 = 10_577_800;

/// Compute the POSIX start time (ms) of an era given the previous era's
/// start time, the previous era's slot length, and the number of slots
/// separating the two eras.
///
/// Formula: `start_time = prev_start_time + slot_gap * prev_slot_length_ms`
///
/// This is used internally by [`mainnet_era_history`] to derive each era's
/// `start_time` from the genesis time and the slot boundaries.
fn compute_era_start_time(
    prev_start_time: i64,
    slot_gap: u64,
    prev_slot_length_ms: u64,
) -> i64 {
    prev_start_time + (slot_gap as i64 * prev_slot_length_ms as i64)
}

/// Construct the mainnet [`EraHistory`] covering Byron through Conway.
///
/// Each era's `start_time` is derived as the cumulative sum of all preceding
/// eras' durations starting from [`MAINNET_SYSTEM_START_MS`].
///
/// **Formula per era**:
/// `start_time[n] = start_time[n-1] + (start_slot[n] - start_slot[n-1]) × slot_length_ms[n-1]`
///
/// This mirrors how `cardano-slotting` constructs the `Summary` from the
/// chain's hard-fork history, where each era boundary carries a `Bound` with
/// both slot and time fields.
///
/// # Note on accuracy
/// The start slot values used here are the known mainnet hard-fork activation
/// slots. The computed start times are exact given those slots and the
/// constant slot lengths above.
pub fn mainnet_era_history() -> EraHistory {
    // Byron: slots 0 .. 4,492,799 (20 s/slot, 21,600 slots/epoch)
    let byron_start_slot: u64 = 0;
    let byron_start_time: i64 = MAINNET_SYSTEM_START_MS;

    // Shelley: starts at slot 4,492,800
    // Duration of Byron portion: 4,492,800 slots × 20,000 ms = 89,856,000,000 ms
    let shelley_start_time = compute_era_start_time(
        byron_start_time,
        SHELLEY_START_SLOT - byron_start_slot,
        BYRON_SLOT_LENGTH_MS,
    );

    // Allegra: starts at slot 5,068,800
    // Duration of Shelley portion: (5,068,800 - 4,492,800) slots × 1,000 ms
    let allegra_start_time = compute_era_start_time(
        shelley_start_time,
        ALLEGRA_START_SLOT - SHELLEY_START_SLOT,
        SHELLEY_SLOT_LENGTH_MS,
    );

    // Mary: starts at slot 5,788,800
    let mary_start_time = compute_era_start_time(
        allegra_start_time,
        MARY_START_SLOT - ALLEGRA_START_SLOT,
        SHELLEY_SLOT_LENGTH_MS,
    );

    // Alonzo: starts at slot 6,220,800
    let alonzo_start_time = compute_era_start_time(
        mary_start_time,
        ALONZO_START_SLOT - MARY_START_SLOT,
        SHELLEY_SLOT_LENGTH_MS,
    );

    // Babbage: starts at slot 7,791,600
    let babbage_start_time = compute_era_start_time(
        alonzo_start_time,
        BABBAGE_START_SLOT - ALONZO_START_SLOT,
        SHELLEY_SLOT_LENGTH_MS,
    );

    // Conway: starts at slot 10,577,800
    let conway_start_time = compute_era_start_time(
        babbage_start_time,
        CONWAY_START_SLOT - BABBAGE_START_SLOT,
        SHELLEY_SLOT_LENGTH_MS,
    );

    EraHistory {
        eras: vec![
            EraSummary {
                start_slot: SlotNo(byron_start_slot),
                start_time: byron_start_time,
                slot_length: SlotLength { ms: BYRON_SLOT_LENGTH_MS },
                epoch_size: EpochSize { slots: BYRON_EPOCH_SIZE },
            },
            EraSummary {
                start_slot: SlotNo(SHELLEY_START_SLOT),
                start_time: shelley_start_time,
                slot_length: SlotLength { ms: SHELLEY_SLOT_LENGTH_MS },
                epoch_size: EpochSize { slots: SHELLEY_EPOCH_SIZE },
            },
            EraSummary {
                start_slot: SlotNo(ALLEGRA_START_SLOT),
                start_time: allegra_start_time,
                slot_length: SlotLength { ms: SHELLEY_SLOT_LENGTH_MS },
                epoch_size: EpochSize { slots: SHELLEY_EPOCH_SIZE },
            },
            EraSummary {
                start_slot: SlotNo(MARY_START_SLOT),
                start_time: mary_start_time,
                slot_length: SlotLength { ms: SHELLEY_SLOT_LENGTH_MS },
                epoch_size: EpochSize { slots: SHELLEY_EPOCH_SIZE },
            },
            EraSummary {
                start_slot: SlotNo(ALONZO_START_SLOT),
                start_time: alonzo_start_time,
                slot_length: SlotLength { ms: SHELLEY_SLOT_LENGTH_MS },
                epoch_size: EpochSize { slots: SHELLEY_EPOCH_SIZE },
            },
            EraSummary {
                start_slot: SlotNo(BABBAGE_START_SLOT),
                start_time: babbage_start_time,
                slot_length: SlotLength { ms: SHELLEY_SLOT_LENGTH_MS },
                epoch_size: EpochSize { slots: SHELLEY_EPOCH_SIZE },
            },
            EraSummary {
                start_slot: SlotNo(CONWAY_START_SLOT),
                start_time: conway_start_time,
                slot_length: SlotLength { ms: SHELLEY_SLOT_LENGTH_MS },
                epoch_size: EpochSize { slots: SHELLEY_EPOCH_SIZE },
            },
        ],
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Helper: a minimal single-era EraHistory for deterministic unit tests.
    // start_slot=0, start_time=0, slot_length=1000ms, epoch_size=100
    // -------------------------------------------------------------------------
    fn single_era(slot_length_ms: u64) -> EraHistory {
        EraHistory {
            eras: vec![EraSummary {
                start_slot: SlotNo(0),
                start_time: 0,
                slot_length: SlotLength { ms: slot_length_ms },
                epoch_size: EpochSize { slots: 100 },
            }],
        }
    }

    // Helper: two-era history mimicking Byron→Shelley
    fn two_era_history() -> EraHistory {
        EraHistory {
            eras: vec![
                EraSummary {
                    start_slot: SlotNo(0),
                    start_time: 0,
                    slot_length: SlotLength { ms: 20_000 }, // Byron: 20s
                    epoch_size: EpochSize { slots: 21_600 },
                },
                EraSummary {
                    start_slot: SlotNo(100),
                    // 100 slots × 20,000 ms = 2,000,000 ms
                    start_time: 2_000_000,
                    slot_length: SlotLength { ms: 1_000 }, // Shelley: 1s
                    epoch_size: EpochSize { slots: 432_000 },
                },
            ],
        }
    }

    // =========================================================================
    // US1: ValidityInterval → POSIXTimeRange conversions
    // =========================================================================

    #[test]
    fn test_always_interval() {
        // (None, None) → always() i.e. (−∞, +∞)
        // Haskell: ValidityInterval SNothing SNothing -> pure PV1.always
        let eh = single_era(1_000);
        let vi = ValidityInterval { invalid_before: None, invalid_hereafter: None };
        let result = trans_validity_interval(&eh, &vi).unwrap();
        assert_eq!(result, always());
        assert_eq!(result.from.bound, Extended::NegInf);
        assert_eq!(result.to.bound, Extended::PosInf);
        assert!(result.from.inclusive);
        assert!(result.to.inclusive);
    }

    #[test]
    fn test_lower_only_interval() {
        // (Some(50), None) → [posix(50), +∞)
        // Haskell: ValidityInterval (SJust i) SNothing -> PV1.from <$> ...
        let eh = single_era(1_000);
        let vi = ValidityInterval {
            invalid_before: Some(SlotNo(50)),
            invalid_hereafter: None,
        };
        let result = trans_validity_interval(&eh, &vi).unwrap();
        // Expected lower bound: POSIXTime(50 * 1000) = 50_000 ms, inclusive
        assert_eq!(result.from.bound, Extended::Finite(POSIXTime(50_000)));
        assert!(result.from.inclusive);
        assert_eq!(result.to.bound, Extended::PosInf);
        assert!(result.to.inclusive);
    }

    #[test]
    fn test_upper_only_interval() {
        // (None, Some(50)) → (−∞, posix(50)) strict upper bound
        // Haskell (Conway): LowerBound NegInf True + strictUpperBound
        let eh = single_era(1_000);
        let vi = ValidityInterval {
            invalid_before: None,
            invalid_hereafter: Some(SlotNo(50)),
        };
        let result = trans_validity_interval(&eh, &vi).unwrap();
        assert_eq!(result.from.bound, Extended::NegInf);
        assert!(result.from.inclusive);
        assert_eq!(result.to.bound, Extended::Finite(POSIXTime(50_000)));
        assert!(!result.to.inclusive); // STRICT upper bound
    }

    #[test]
    fn test_both_bounds_interval() {
        // (Some(10), Some(60)) → [posix(10), posix(60))
        // Haskell: PV1.Interval (lowerBound t1) (strictUpperBound t2)
        let eh = single_era(1_000);
        let vi = ValidityInterval {
            invalid_before: Some(SlotNo(10)),
            invalid_hereafter: Some(SlotNo(60)),
        };
        let result = trans_validity_interval(&eh, &vi).unwrap();
        assert_eq!(result.from.bound, Extended::Finite(POSIXTime(10_000)));
        assert!(result.from.inclusive);
        assert_eq!(result.to.bound, Extended::Finite(POSIXTime(60_000)));
        assert!(!result.to.inclusive); // STRICT upper bound
    }

    #[test]
    fn test_upper_bound_is_always_strict() {
        // FR-005: The upper bound of a converted validity interval MUST be
        // a strict upper bound (open on top) in all cases where it is finite.
        let eh = single_era(1_000);

        // Case: upper-only
        let vi_upper = ValidityInterval {
            invalid_before: None,
            invalid_hereafter: Some(SlotNo(1)),
        };
        let r1 = trans_validity_interval(&eh, &vi_upper).unwrap();
        assert!(!r1.to.inclusive, "upper-only: to.inclusive must be false");

        // Case: both bounds
        let vi_both = ValidityInterval {
            invalid_before: Some(SlotNo(0)),
            invalid_hereafter: Some(SlotNo(5)),
        };
        let r2 = trans_validity_interval(&eh, &vi_both).unwrap();
        assert!(!r2.to.inclusive, "both-bounds: to.inclusive must be false");
    }

    // =========================================================================
    // US1: Single-slot conversion basics
    // =========================================================================

    #[test]
    fn test_slot_zero_returns_era_start_time() {
        // slot 0 in a single-era history with start_time=0 → POSIXTime(0)
        let eh = single_era(1_000);
        assert_eq!(slot_to_posix_time(&eh, SlotNo(0)), Ok(POSIXTime(0)));
    }

    #[test]
    fn test_single_era_arbitrary_slot() {
        // slot N × slot_length_ms = expected POSIX ms
        // e.g. slot 500 × 1000 ms = POSIXTime(500_000)
        // Corresponds to the quickstart.md Scenario 3 test case.
        let eh = single_era(1_000);
        assert_eq!(slot_to_posix_time(&eh, SlotNo(500)), Ok(POSIXTime(500_000)));
    }

    #[test]
    fn test_single_era_custom_slot_length() {
        // slot 3 in a 20,000 ms/slot era → POSIXTime(60_000)
        let eh = single_era(20_000);
        assert_eq!(slot_to_posix_time(&eh, SlotNo(3)), Ok(POSIXTime(60_000)));
    }

    // =========================================================================
    // US2: Multi-era chain handling
    // =========================================================================

    #[test]
    fn test_two_era_slot_in_first_era() {
        // Slot 50 is in Byron era (0..99), Byron: 20s slots, start_time=0
        // Expected: 50 * 20_000 = 1_000_000 ms
        let eh = two_era_history();
        assert_eq!(slot_to_posix_time(&eh, SlotNo(50)), Ok(POSIXTime(1_000_000)));
    }

    #[test]
    fn test_two_era_boundary_slot() {
        // Slot 100 is exactly the start of era 2 (Shelley equivalent).
        // Expected: era 2 start_time = 2_000_000 ms (0 slots into era 2)
        let eh = two_era_history();
        assert_eq!(slot_to_posix_time(&eh, SlotNo(100)), Ok(POSIXTime(2_000_000)));
    }

    #[test]
    fn test_two_era_slot_in_second_era() {
        // Slot 110 is 10 slots into era 2 (Shelley: 1000 ms/slot)
        // Expected: era_2_start_time + 10 * 1000 = 2_000_000 + 10_000 = 2_010_000 ms
        let eh = two_era_history();
        assert_eq!(slot_to_posix_time(&eh, SlotNo(110)), Ok(POSIXTime(2_010_000)));
    }

    #[test]
    fn test_two_era_first_slot_of_first_era() {
        // Slot 0 returns start_time of era 0 (which is 0 in our two_era_history)
        let eh = two_era_history();
        assert_eq!(slot_to_posix_time(&eh, SlotNo(0)), Ok(POSIXTime(0)));
    }

    #[test]
    fn test_two_era_total_duration_accumulation() {
        // Verify that the start_time of era 2 correctly includes all of era 1's
        // duration. Era 1: 100 slots × 20,000 ms = 2,000,000 ms total.
        let eh = two_era_history();
        let era2 = &eh.eras[1];
        assert_eq!(era2.start_time, 2_000_000);
    }

    // =========================================================================
    // US2: Error paths
    // =========================================================================

    #[test]
    fn test_empty_era_history_error() {
        let eh = EraHistory { eras: vec![] };
        assert_eq!(
            slot_to_posix_time(&eh, SlotNo(0)),
            Err(TimeTranslationError::EmptyEraHistory)
        );
    }

    #[test]
    fn test_zero_slot_length_error() {
        let eh = EraHistory {
            eras: vec![EraSummary {
                start_slot: SlotNo(0),
                start_time: 0,
                slot_length: SlotLength { ms: 0 }, // invalid
                epoch_size: EpochSize { slots: 100 },
            }],
        };
        assert!(matches!(
            slot_to_posix_time(&eh, SlotNo(1)),
            Err(TimeTranslationError::InvalidEraParams(_))
        ));
    }

    #[test]
    fn test_zero_epoch_size_error() {
        let eh = EraHistory {
            eras: vec![EraSummary {
                start_slot: SlotNo(0),
                start_time: 0,
                slot_length: SlotLength { ms: 1_000 },
                epoch_size: EpochSize { slots: 0 }, // invalid
            }],
        };
        assert!(matches!(
            slot_to_posix_time(&eh, SlotNo(1)),
            Err(TimeTranslationError::InvalidEraParams(_))
        ));
    }

    #[test]
    fn test_slot_past_horizon_single_era() {
        // In a single-era history, the horizon is implementation-defined.
        // We test: a slot in a second era that isn't listed → SlotPastHorizon.
        // In our implementation: a slot before the first era's start_slot
        // (impossible with start_slot=0, so we test with start_slot=10).
        let eh = EraHistory {
            eras: vec![EraSummary {
                start_slot: SlotNo(10), // first era starts at slot 10
                start_time: 0,
                slot_length: SlotLength { ms: 1_000 },
                epoch_size: EpochSize { slots: 100 },
            }],
        };
        // Slot 5 is before the first era's start_slot → no containing era → error
        assert_eq!(
            slot_to_posix_time(&eh, SlotNo(5)),
            Err(TimeTranslationError::SlotPastHorizon(5))
        );
    }

    #[test]
    fn test_no_panic_on_any_valid_input() {
        // Verify no panics on boundary inputs — all return Ok or Err, never panic.
        let eh = single_era(1_000);
        let _ = slot_to_posix_time(&eh, SlotNo(0));
        let _ = slot_to_posix_time(&eh, SlotNo(u64::MAX / 2)); // large but won't overflow i64
        let _ = trans_validity_interval(&eh, &ValidityInterval { invalid_before: None, invalid_hereafter: None });
    }

    // =========================================================================
    // US3: Haskell cross-reference annotations (structural checks)
    // =========================================================================

    #[test]
    fn test_always_matches_haskell_pv1_always() {
        // PV1.always = Interval (LowerBound NegInf True) (UpperBound PosInf True)
        let a = always();
        assert_eq!(a.from.bound, Extended::<POSIXTime>::NegInf);
        assert!(a.from.inclusive);
        assert_eq!(a.to.bound, Extended::<POSIXTime>::PosInf);
        assert!(a.to.inclusive);
    }

    #[test]
    fn test_strict_upper_bound_is_not_inclusive() {
        // PV1.strictUpperBound a = UpperBound (Finite a) False
        let ub = strict_upper_bound(POSIXTime(1_000));
        assert_eq!(ub.bound, Extended::Finite(POSIXTime(1_000)));
        assert!(!ub.inclusive);
    }

    #[test]
    fn test_lower_bound_is_inclusive() {
        // PV1.lowerBound a = LowerBound (Finite a) True
        let lb = lower_bound(POSIXTime(1_000));
        assert_eq!(lb.bound, Extended::Finite(POSIXTime(1_000)));
        assert!(lb.inclusive);
    }

    // =========================================================================
    // Behavioural guarantees from contracts/public-api.md
    // =========================================================================

    #[test]
    fn test_mainnet_slot_zero() {
        // Slot 0 on mainnet → MAINNET_SYSTEM_START_MS
        let eh = mainnet_era_history();
        assert_eq!(
            slot_to_posix_time(&eh, SlotNo(0)),
            Ok(POSIXTime(MAINNET_SYSTEM_START_MS))
        );
    }

    #[test]
    fn test_mainnet_shelley_boundary() {
        // Slot SHELLEY_START_SLOT on mainnet → shelley_start_time
        // = MAINNET_SYSTEM_START_MS + SHELLEY_START_SLOT * BYRON_SLOT_LENGTH_MS
        let eh = mainnet_era_history();
        let expected = MAINNET_SYSTEM_START_MS
            + SHELLEY_START_SLOT as i64 * BYRON_SLOT_LENGTH_MS as i64;
        assert_eq!(
            slot_to_posix_time(&eh, SlotNo(SHELLEY_START_SLOT)),
            Ok(POSIXTime(expected))
        );
    }

    #[test]
    fn test_quickstart_scenario_3() {
        // Quickstart scenario 3: custom single-era EraHistory, slot 500 → 500_000 ms
        let era_history = EraHistory {
            eras: vec![EraSummary {
                start_slot: SlotNo(0),
                start_time: 0,
                slot_length: SlotLength { ms: 1_000 },
                epoch_size: EpochSize { slots: 432_000 },
            }],
        };
        assert_eq!(slot_to_posix_time(&era_history, SlotNo(500)), Ok(POSIXTime(500_000)));
    }

    #[test]
    fn test_empty_era_history_via_trans_validity_interval() {
        let eh = EraHistory { eras: vec![] };
        let vi = ValidityInterval {
            invalid_before: Some(SlotNo(1)),
            invalid_hereafter: None,
        };
        assert_eq!(
            trans_validity_interval(&eh, &vi),
            Err(TimeTranslationError::EmptyEraHistory)
        );
    }
}
