# Research: Slot-to-POSIX Time Conversion for Phase 2 Validation

**Feature**: `001-slot-time-conversion`
**Date**: 2026-03-18
**Status**: Complete — all NEEDS CLARIFICATION resolved

---

## Decision Log

### D-001: Algorithm approach — direct era-table lookup

**Decision**: Implement the conversion as a direct linear scan over an ordered list of era summaries (EraHistory), finding the containing era by comparing the target slot against each era's start slot.

**Rationale**: The Haskell reference (`epochInfoSlotToUTCTime` in `cardano-slotting`) works the same way internally — it holds a `Summary` (ordered list of era entries) and finds the right era. This approach is O(n) in number of eras (always ≤8 for mainnet), trivially correct, and needs no external dependencies.

**Alternatives considered**:
- Porting `cardano-slotting` fully: too large, external dependency, violates FR-008.
- Binary search over era boundaries: over-engineered for ≤8 eras.

---

### D-002: Rust representation of Plutus `Extended` / `Interval` types

**Decision**: Define local Rust enums mirroring the Plutus Haskell types (`Extended<T>`, `LowerBound<T>`, `UpperBound<T>`, `Interval<T>`), matching their structure exactly to ensure the Rust output is semantically identical to what the Plutus evaluator receives.

**Rationale**: The Plutus evaluator expects a specific interval structure. By mirroring the types structurally, the Rust output can be serialized to the same CBOR/JSON representation without a re-mapping layer.

**Relevant Haskell types** (from `plutus-ledger-api`):
```haskell
-- Extended: a value that may be ±∞
data Extended a = NegInf | Finite a | PosInf

-- LowerBound: a bound with an inclusive/exclusive flag
newtype LowerBound a = LowerBound (Extended a, Bool)  -- True = inclusive

-- UpperBound: same structure
newtype UpperBound a = UpperBound (Extended a, Bool)  -- True = inclusive (False = strict)

-- Interval: a pair of bounds
data Interval a = Interval { ivFrom :: LowerBound a, ivTo :: UpperBound a }

-- POSIXTime: Int64 milliseconds
newtype POSIXTime = POSIXTime { getPOSIXTime :: Integer }  -- represented as Int64 in practice

-- POSIXTimeRange: Interval POSIXTime
type POSIXTimeRange = Interval POSIXTime
```

**Source**: `eras/alonzo/impl/testlib/Test/Cardano/Ledger/Alonzo/Imp/UtxosSpec.hs:90-97` shows construction pattern.

**Alternatives considered**:
- Using `std::ops::RangeBounds` from Rust std: does not represent ±∞ cleanly, mismatches Plutus semantics.
- Using `Option<i64>` tuples: loses the inclusive/exclusive distinction required for the strict upper bound.

---

### D-003: Arithmetic overflow strategy — checked arithmetic returning `Result`

**Decision**: Use Rust checked arithmetic (`checked_mul`, `checked_add`, etc.) throughout. Any overflow returns `Err(TimeTranslationError::Overflow)` — no panics, no silent wrapping (FR-006, FR-010).

**Rationale**: Slot numbers are `u64`; multiplying by slot length (up to 20,000 ms) can overflow `i64` for astronomically large slot numbers. The Haskell reference uses arbitrary-precision integers which cannot overflow, so we must be explicit about this constraint in Rust.

**Alternatives considered**:
- `u128` intermediate: avoids overflow for any realistic slot number but adds casting complexity.
- Saturating arithmetic: silently produces wrong POSIX times, violates FR-010.
- Panicking assertions: violates FR-006.

**Resolution**: Use `i128` as intermediate accumulator for the multiplication step (slot_offset × slot_length_ms), then range-check before casting to `i64` for the final POSIX value.

---

### D-004: POSIX time unit — milliseconds (Int64)

**Decision**: The output of `slot_to_posix_time` is `i64` milliseconds, matching `PV1.POSIXTime`'s underlying representation.

**Rationale**: The Haskell reference explicitly multiplies by 1,000 and truncates: `PV1.POSIXTime . (truncate . (* 1000)) . nominalDiffTimeToSeconds . utcTimeToPOSIXSeconds`.

**Source**: `libs/cardano-ledger-core/src/Cardano/Ledger/Plutus/TxInfo.hs:169-171`

**Cardano mainnet genesis** (`SystemStart`): 2017-09-23 21:44:51 UTC = POSIX 1506203091 seconds = 1,506,203,091,000 ms.

---

### D-005: `ValidityInterval` upper bound semantics — strict upper bound

**Decision**: The `invalidHereafter` field, when present, must be converted to a **strict** (exclusive) upper bound in the output interval. This matches `PV1.strictUpperBound` in the Haskell reference.

**Rationale**: `ValidityInterval` is defined as a half-open interval `[invalidBefore, invalidHereafter)` — the upper slot is not included. When converted to a POSIX range, the upper bound is passed through `PV1.strictUpperBound`, which sets `inclusive = false`.

**Source**: `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Plutus/TxInfo.hs:238`

---

### D-006: Era parameters for Cardano mainnet

**Decision**: Include mainnet era parameters as hardcoded constants in `slot-time.rs` for documentation/testing purposes. A caller can pass their own `EraHistory` for alternative networks.

**Mainnet era start slots and approximate POSIX times** (from chain data, not hardcoded in the ledger source):

| Era | Start Slot | Slot Length (ms) | Epoch Length (slots) | Notes |
|---|---|---|---|---|
| Byron | 0 | 20,000 | 21,600 | Genesis: 2017-09-23 21:44:51 UTC |
| Shelley | 4,492,800 | 1,000 | 432,000 | July 29, 2020 |
| Allegra | 5,068,800 | 1,000 | 432,000 | Dec 16, 2020 |
| Mary | 5,788,800 | 1,000 | 432,000 | Mar 1, 2021 |
| Alonzo | 6,220,800 | 1,000 | 432,000 | Sept 12, 2021 |
| Babbage | 7,791,600 | 1,000 | 432,000 | June 22, 2022 |
| Conway | 10,577,800 | 1,000 | 432,000 | Sept 1, 2024 (approx) |

**Sources**:
- `eras/byron/ledger/impl/src/Cardano/Chain/Epoch/File.hs:69`: Byron epoch size = 21,600
- Spec assumptions: Byron slot = 20s, Shelley+ slot = 1s, Shelley+ epoch = 432,000

---

### D-007: File location

**Decision**: Place `slot-time.rs` as a standalone file in `specs/001-slot-time-conversion/slot-time.rs`. It is a reference implementation and documentation artifact, not a production crate integrated into the Haskell build system.

**Rationale**: The cardano-ledger repo is a Haskell project. The Rust file is a companion reference document demonstrating the conversion algorithm for developers building Rust Cardano tools (e.g., Pallas, Dolos). It lives alongside the spec rather than in a `src/` directory.

---

### D-008: Conway vs Alonzo `transValidityInterval` — use Conway semantics

**Decision**: The Rust implementation follows the Conway variant of `transValidityInterval`, which constructs the `NegInf` lower bound explicitly.

**Rationale**: Conway is the current era. The Conway variant is slightly more explicit in how it constructs the upper-only-bound case, using `PV1.LowerBound PV1.NegInf True` directly rather than relying on `PV1.to`. This makes the intent clearer in code.

**Source**: `eras/conway/impl/src/Cardano/Ledger/Conway/TxInfo.hs:785-809`

---

## Summary of Resolved Questions

| Question | Resolution |
|---|---|
| How to perform multi-era conversion in Rust? | Linear scan over EraHistory, find containing era |
| How to represent Plutus interval types? | Local mirror enums matching Haskell structure |
| How to handle arithmetic overflow? | `i128` intermediates + checked casts to `i64` |
| What time unit does Plutus expect? | `i64` milliseconds (POSIX × 1000, truncated) |
| Is the upper bound inclusive or exclusive? | Exclusive (strict upper bound) |
| What are the mainnet era parameters? | Documented in D-006 |
| Where does the file live? | `specs/001-slot-time-conversion/slot-time.rs` |
| Which era variant of transValidityInterval? | Conway variant (explicit NegInf) |
