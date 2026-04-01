# Public API Contract: `slot-time.rs`

**Feature**: `001-slot-time-conversion`
**Date**: 2026-03-18
**Contract type**: Rust module public interface

---

## Overview

`slot-time.rs` exposes two public functions and a set of public types. The module has no external dependencies (only `std`). All functions are pure (no side effects, no global state).

---

## Public Types

### Input Types

```rust
/// The absolute POSIX time (milliseconds) at which the blockchain began.
/// Haskell ref: Cardano.Slotting.Time.SystemStart
pub struct SystemStart {
    pub posix_ms: i64,
}

/// The length of a single slot, in milliseconds.
/// Haskell ref: Cardano.Slotting.Time.SlotLength
pub struct SlotLength {
    pub ms: u64,
}

/// The number of slots in one epoch.
/// Haskell ref: Cardano.Slotting.Slot.EpochSize
pub struct EpochSize {
    pub slots: u64,
}

/// A non-negative slot number on the chain.
/// Haskell ref: Cardano.Slotting.Slot.SlotNo
pub struct SlotNo(pub u64);

/// Time parameters for a single era.
/// Haskell ref: one entry in Ouroboros.Consensus.HardFork.History.Summary
pub struct EraSummary {
    pub start_slot:  SlotNo,
    pub start_time:  i64,       // POSIX ms at start of era
    pub slot_length: SlotLength,
    pub epoch_size:  EpochSize,
}

/// Ordered list of era summaries from genesis to the known horizon.
pub struct EraHistory {
    pub eras: Vec<EraSummary>,
}

/// Transaction validity range in slots (half-open: [invalid_before, invalid_hereafter)).
/// Haskell ref: eras/allegra/impl/src/Cardano/Ledger/Allegra/Scripts.hs:115-121
pub struct ValidityInterval {
    pub invalid_before:    Option<SlotNo>,  // None = −∞
    pub invalid_hereafter: Option<SlotNo>,  // None = +∞
}
```

### Output Types

```rust
/// POSIX time in milliseconds (Int64), as expected by the Plutus evaluator.
/// Haskell ref: PlutusLedgerApi.V1.POSIXTime
pub struct POSIXTime(pub i64);

/// A value that may be ±∞ or a concrete value (mirrors Plutus Extended type).
/// Haskell ref: PlutusLedgerApi.V1.Interval.Extended
pub enum Extended<T> {
    NegInf,
    Finite(T),
    PosInf,
}

/// Lower bound of an interval.
/// Haskell ref: PlutusLedgerApi.V1.Interval.LowerBound
pub struct LowerBound<T> {
    pub bound:     Extended<T>,
    pub inclusive: bool,
}

/// Upper bound of an interval.
/// Haskell ref: PlutusLedgerApi.V1.Interval.UpperBound
pub struct UpperBound<T> {
    pub bound:     Extended<T>,
    pub inclusive: bool,
}

/// Generic interval over a type T.
/// Haskell ref: PlutusLedgerApi.V1.Interval.Interval
pub struct Interval<T> {
    pub from: LowerBound<T>,
    pub to:   UpperBound<T>,
}

/// Plutus time range: Interval<POSIXTime>.
/// Haskell ref: PlutusLedgerApi.V1.POSIXTimeRange
pub type POSIXTimeRange = Interval<POSIXTime>;
```

### Error Type

```rust
/// Errors returned by time conversion functions.
/// Haskell ref: AlonzoContextError::TimeTranslationPastHorizon in
///   eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Plutus/TxInfo.hs
#[derive(Debug, PartialEq)]
pub enum TimeTranslationError {
    SlotPastHorizon(u64),    // slot number that exceeded the known horizon
    EmptyEraHistory,         // EraHistory has no entries
    InvalidEraParams(String),// slot_length or epoch_size is zero
    ArithmeticOverflow,      // checked arithmetic detected overflow
}
```

---

## Public Functions

### `slot_to_posix_time`

Converts a single slot number to a POSIX millisecond timestamp.

```rust
/// Convert a slot number to a POSIX millisecond timestamp.
///
/// Haskell ref:
///   slotToPOSIXTime
///   libs/cardano-ledger-core/src/Cardano/Ledger/Plutus/TxInfo.hs:164-171
///
/// What it does (plain English):
///   1. Find the era that contains this slot by scanning EraHistory.
///   2. Compute how many slots have elapsed since that era began.
///   3. Multiply by the era's slot length (in ms) to get elapsed ms.
///   4. Add the era's start time (POSIX ms) to get the absolute POSIX time.
///   5. Return as i64 (truncated, not rounded — matching Haskell truncate).
///
/// Errors:
///   - SlotPastHorizon: if the slot is beyond the last known era's horizon.
///   - ArithmeticOverflow: if intermediate multiplication overflows i128.
pub fn slot_to_posix_time(
    era_history:  &EraHistory,
    slot:         SlotNo,
) -> Result<POSIXTime, TimeTranslationError>;
```

**Contract invariants**:
- For any `slot` within the known horizon, the result is deterministic and identical to the Haskell reference.
- Truncation (not rounding) is applied at the final step, matching `truncate` in Haskell.
- The `SystemStart` is embedded in the first `EraSummary`'s `start_time` field.

---

### `trans_validity_interval`

Converts a slot-based `ValidityInterval` to a Plutus `POSIXTimeRange`.

```rust
/// Convert a slot-based ValidityInterval to a Plutus POSIXTimeRange.
///
/// Haskell ref:
///   transValidityInterval (Conway variant)
///   eras/conway/impl/src/Cardano/Ledger/Conway/TxInfo.hs:785-809
///
/// What it does (plain English):
///   Handles four cases depending on which bounds are present:
///   - Neither bound: returns the "always" interval (−∞ to +∞).
///   - Lower bound only: returns [t, +∞) where t is the converted lower slot.
///   - Upper bound only: returns (−∞, t) where t is the converted upper slot
///     as a strict (exclusive) upper bound.
///   - Both bounds: returns [t1, t2) — closed lower, strict upper — where t1
///     and t2 are the converted lower and upper slots respectively.
///
/// The upper bound is ALWAYS strict (exclusive), preserving the half-open
/// [invalid_before, invalid_hereafter) semantics of ValidityInterval.
///
/// Haskell ref for strict upper bound:
///   PV1.strictUpperBound
///   eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Plutus/TxInfo.hs:238
///
/// Errors:
///   - Any error from slot_to_posix_time (propagated for each bound).
pub fn trans_validity_interval(
    era_history: &EraHistory,
    interval:    &ValidityInterval,
) -> Result<POSIXTimeRange, TimeTranslationError>;
```

**Contract invariants**:
- `Option::None` bounds are mapped to `Extended::NegInf` / `Extended::PosInf`.
- The upper bound always has `inclusive: false` (strict / open).
- The lower bound always has `inclusive: true` (closed).
- For `always` (both None): lower `inclusive: true`, upper `inclusive: true` (matching `PV1.always`).

---

## Helper Constructors (internal, exposed for testing)

```rust
/// Construct the "always" interval: (−∞, +∞).
/// Haskell ref: PV1.always
pub fn always() -> POSIXTimeRange;

/// Construct [t, +∞).
/// Haskell ref: PV1.from
pub fn from_posix(t: POSIXTime) -> POSIXTimeRange;

/// Construct (−∞, t) — strict upper bound.
/// Haskell ref: PV1.to  (note: PV1.to produces a strict upper bound)
pub fn to_posix(t: POSIXTime) -> POSIXTimeRange;

/// Construct an inclusive lower bound: LowerBound(Finite(t), true).
/// Haskell ref: PV1.lowerBound
pub fn lower_bound(t: POSIXTime) -> LowerBound<POSIXTime>;

/// Construct a strict (exclusive) upper bound: UpperBound(Finite(t), false).
/// Haskell ref: PV1.strictUpperBound
pub fn strict_upper_bound(t: POSIXTime) -> UpperBound<POSIXTime>;
```

---

## Mainnet Constants (provided for reference and testing)

```rust
/// Mainnet system start: 2017-09-23 21:44:51 UTC = 1_506_203_091_000 ms
pub const MAINNET_SYSTEM_START_MS: i64 = 1_506_203_091_000;

/// Construct the mainnet EraHistory with known era parameters.
/// Covers Byron through Conway.
pub fn mainnet_era_history() -> EraHistory;
```

---

## Behavioral Guarantees

| Input | Expected output |
|---|---|
| Slot 0, mainnet | `POSIXTime(1_506_203_091_000)` |
| Slot 4_492_800 (Shelley start, mainnet) | `POSIXTime(1_596_059_091_000)` ± rounding |
| `ValidityInterval { None, None }` | `always()` interval |
| `ValidityInterval { Some(s), None }` | `[posix(s), +∞)` |
| `ValidityInterval { None, Some(s) }` | `(−∞, posix(s))` strict |
| `ValidityInterval { Some(a), Some(b) }` | `[posix(a), posix(b))` |
| Slot beyond last era end | `Err(SlotPastHorizon(slot))` |
| Empty EraHistory | `Err(EmptyEraHistory)` |
