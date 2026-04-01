# Data Model: Slot-to-POSIX Time Conversion

**Feature**: `001-slot-time-conversion`
**Date**: 2026-03-18

---

## Overview

All types are pure value types (no heap allocation beyond what Rust automatically manages for `Vec`). All types implement `Copy` where possible, or `Clone` where `Vec` is involved.

---

## Core Input Types

### `SystemStart`

Represents the absolute POSIX time (in milliseconds) at which the blockchain began — i.e., the real-world time corresponding to slot 0.

```
SystemStart {
    posix_ms: i64          // POSIX milliseconds at genesis (e.g., 1_506_203_091_000 for mainnet)
}
```

**Haskell reference**: `Cardano.Slotting.Time.SystemStart` (external `cardano-slotting` library)
**Notes**: Fixed per network. Mainnet value: 2017-09-23 21:44:51 UTC → 1,506,203,091,000 ms.

---

### `SlotLength`

The duration of a single slot, in milliseconds.

```
SlotLength {
    ms: u64                // milliseconds per slot (20_000 for Byron; 1_000 for Shelley+)
}
```

**Haskell reference**: `Cardano.Slotting.Time.SlotLength` (external `cardano-slotting` library)

---

### `EpochSize`

The number of slots in one epoch.

```
EpochSize {
    slots: u64             // slots per epoch (21_600 for Byron; 432_000 for Shelley+)
}
```

**Haskell reference**: `Cardano.Slotting.Slot.EpochSize` (external `cardano-slotting` library)

---

### `SlotNo`

A non-negative slot number on the chain.

```
SlotNo(u64)                // newtype wrapping u64
```

**Haskell reference**: `Cardano.Slotting.Slot.SlotNo` (external `cardano-slotting` library)

---

### `EraSummary`

Describes all time-related parameters for a single era. An era is a contiguous range of slots with uniform slot-length and epoch-length.

```
EraSummary {
    start_slot:   SlotNo,      // first slot of this era (inclusive)
    start_time:   i64,         // POSIX ms at the start of this era
    slot_length:  SlotLength,  // duration of each slot in ms
    epoch_size:   EpochSize,   // number of slots per epoch in this era
}
```

**Haskell reference**: `Ouroboros.Consensus.HardFork.History.Summary` entries in `cardano-slotting`. Each `EraSummary` corresponds to one `EraParams + Bound` pair in the Haskell `Summary`.

**Validation rules**:
- `slot_length.ms > 0` (epoch_size of 0 is also invalid)
- `start_slot` must be non-decreasing across the ordered list in `EraHistory`
- `start_time` must be non-decreasing and consistent with the cumulative elapsed time of preceding eras

---

### `EraHistory`

An ordered, non-overlapping list of `EraSummary` entries covering the chain from genesis to the current known **horizon**.

```
EraHistory {
    eras: Vec<EraSummary>      // ordered by start_slot ascending; non-overlapping
}
```

**Haskell reference**: `Ouroboros.Consensus.HardFork.History.Summary` in `cardano-slotting`.
**Horizon**: The end of the last entry is the horizon. Any slot at or beyond the horizon triggers `TimeTranslationError::SlotPastHorizon`.

**Notes**:
- The first era always has `start_slot = 0`.
- For a slot `s`, the containing era is the last era whose `start_slot <= s`.
- The horizon is the last era's end slot. If the last era is "open" (no explicit end), only the current epoch's last slot is considered the horizon (mirrors `validateOutsideForecast` semantics).

---

### `ValidityInterval`

The transaction's validity range, expressed in slots. Half-open: `[invalid_before, invalid_hereafter)`.

```
ValidityInterval {
    invalid_before:    Option<SlotNo>,   // None = −∞ (no lower bound)
    invalid_hereafter: Option<SlotNo>,   // None = +∞ (no upper bound)
}
```

**Haskell reference**:
- Type: `eras/allegra/impl/src/Cardano/Ledger/Allegra/Scripts.hs:115–121`
- `StrictMaybe SlotNo` → `Option<SlotNo>` in Rust
- `SNothing` → `None`; `SJust n` → `Some(SlotNo(n))`

**Semantics**: A transaction is valid only in slots `s` where `invalid_before <= s < invalid_hereafter` (with missing bounds treated as infinity).

---

## Core Output Types

### `POSIXTime`

A POSIX timestamp in milliseconds, as expected by the Plutus evaluator.

```
POSIXTime(i64)             // milliseconds since Unix epoch (1970-01-01 00:00:00 UTC)
```

**Haskell reference**: `PlutusLedgerApi.V1.POSIXTime` — newtype over `Integer`, but serialized as `Int64` in practice. The Haskell reference truncates (not rounds) when converting from fractional seconds.

---

### `Extended<T>`

Extends a type with positive and negative infinity, matching the Plutus `Extended` type exactly.

```
enum Extended<T> {
    NegInf,                // negative infinity
    Finite(T),             // a concrete value
    PosInf,                // positive infinity
}
```

**Haskell reference**: `PlutusLedgerApi.V1.Interval.Extended`

---

### `LowerBound<T>`

A lower bound of an interval, with an inclusive/exclusive flag.

```
struct LowerBound<T> {
    bound:     Extended<T>,
    inclusive: bool,        // true = closed (≥), false = open (>)
}
```

**Haskell reference**: `PlutusLedgerApi.V1.Interval.LowerBound`
**Typical values**: `LowerBound { bound: Finite(t), inclusive: true }` = `[t, …)`

---

### `UpperBound<T>`

An upper bound of an interval, with an inclusive/exclusive flag.

```
struct UpperBound<T> {
    bound:     Extended<T>,
    inclusive: bool,        // true = closed (≤), false = open (<)
}
```

**Haskell reference**: `PlutusLedgerApi.V1.Interval.UpperBound`
**Typical values for validity interval**: `UpperBound { bound: Finite(t), inclusive: false }` = `…, t)` (strict upper bound)

---

### `Interval<T>`

A generic interval parameterized over a value type.

```
struct Interval<T> {
    from: LowerBound<T>,
    to:   UpperBound<T>,
}
```

**Haskell reference**: `PlutusLedgerApi.V1.Interval.Interval`
**Special instances**:
- `always`: `Interval { from: LowerBound(NegInf, true), to: UpperBound(PosInf, true) }`
- `from(t)`: `Interval { from: LowerBound(Finite(t), true), to: UpperBound(PosInf, true) }`
- `to(t)`: `Interval { from: LowerBound(NegInf, true), to: UpperBound(Finite(t), false) }`

---

### `POSIXTimeRange`

Type alias for `Interval<POSIXTime>`.

```
type POSIXTimeRange = Interval<POSIXTime>
```

**Haskell reference**: `PlutusLedgerApi.V1.POSIXTimeRange`

---

## Error Type

### `TimeTranslationError`

```
enum TimeTranslationError {
    SlotPastHorizon(SlotNo),   // slot exceeds the known era horizon
    EmptyEraHistory,           // EraHistory contains no eras
    InvalidEraParams(String),  // epoch_size or slot_length is zero
    ArithmeticOverflow,        // checked arithmetic detected overflow
}
```

**Haskell reference**: `AlonzoContextError::TimeTranslationPastHorizon(Text)` in `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Plutus/TxInfo.hs`

---

## State Transitions

The conversion is stateless — it is a pure function. No state transitions apply. Input: `(EraHistory, SystemStart, ValidityInterval)`. Output: `Result<POSIXTimeRange, TimeTranslationError>`.

---

## Type Relationship Diagram

```
ValidityInterval
  ├── invalid_before: Option<SlotNo>    ─── slot_to_posix_time() ──► POSIXTime
  └── invalid_hereafter: Option<SlotNo> ─── slot_to_posix_time() ──► POSIXTime
                                                      │
                                            uses EraHistory + SystemStart
                                                      │
                                         EraHistory.find_era(SlotNo)
                                                      │
                                              EraSummary
                                          ├── start_slot
                                          ├── start_time
                                          ├── slot_length.ms
                                          └── epoch_size.slots
                                                      │
                                        posix_ms = start_time
                                           + (slot - start_slot) * slot_length.ms
                                                      │
                                              POSIXTime(i64)
                                                      │
                                        trans_validity_interval()
                                                      │
                                           POSIXTimeRange
                                        = Interval<POSIXTime>
```
