# Implementation Plan: Slot-to-POSIX Time Conversion for Phase 2 Validation

**Branch**: `001-slot-time-conversion` | **Date**: 2026-03-18 | **Spec**: [spec.md](spec.md)

## Summary

Implement a self-contained Rust reference file (`slot-time.rs`) that converts Cardano transaction validity intervals (expressed in slots) to Plutus POSIX time ranges (in milliseconds), faithfully reproducing the reference Haskell logic used in `cardano-ledger` for phase 2 script evaluation. The file is a pure computation module with no external dependencies, fully annotated with Haskell cross-references and plain-English descriptions.

---

## Technical Context

**Language/Version**: Rust stable (no specific version constraint — uses only `std`)
**Primary Dependencies**: None (std library only — per FR-008)
**Storage**: N/A (pure computation, no persistence)
**Testing**: Rust built-in test framework (`#[cfg(test)]` / `#[test]`)
**Target Platform**: Any platform supporting Rust std (Linux, macOS, Windows)
**Project Type**: Reference implementation / documentation artifact (single file module)
**Performance Goals**: Sub-millisecond per conversion (pure integer arithmetic over ≤8 era entries)
**Constraints**: Single file, no `panic!` on any input, all errors returned as `Result`
**Scale/Scope**: ~400–600 lines including types, functions, inline docs, and tests

---

## Constitution Check

*The project constitution (`constitution.md`) contains only template placeholders and has not been ratified with project-specific principles. No constitution gates apply.*

**Assessment**: No violations. Proceed to Phase 0.

---

## Project Structure

### Documentation (this feature)

```text
specs/001-slot-time-conversion/
├── spec.md              # Feature specification
├── plan.md              # This file
├── research.md          # Phase 0 — decisions and resolved unknowns
├── data-model.md        # Phase 1 — type definitions and relationships
├── quickstart.md        # Phase 1 — usage guide
├── contracts/
│   └── public-api.md    # Phase 1 — public function and type contracts
└── tasks.md             # Phase 2 output (/speckit.tasks command)
```

### Source Code (deliverable)

```text
specs/001-slot-time-conversion/
└── slot-time.rs         # The standalone Rust reference implementation
```

**Structure Decision**: The Rust file lives alongside the spec rather than in a `src/` directory because the cardano-ledger repo is a Haskell project and this is a documentation/reference artifact. Developers building Rust Cardano tools (Pallas, Dolos, etc.) can copy this file directly.

---

## Complexity Tracking

No constitution violations requiring justification.

---

## Phase 0: Research — Complete

All unknowns resolved. See [research.md](research.md) for full decision log.

**Key decisions**:

| Decision | Choice |
|---|---|
| Algorithm | Linear scan over EraHistory (≤8 eras — O(n) is fine) |
| Plutus interval types | Local mirror enums matching Haskell structure exactly |
| Overflow handling | `i128` intermediate + checked cast to `i64` |
| POSIX unit | `i64` milliseconds (multiply by 1000, truncate — not round) |
| Upper bound | Always strict/exclusive (FR-005, matches `PV1.strictUpperBound`) |
| File location | `specs/001-slot-time-conversion/slot-time.rs` |
| Era variant | Conway `transValidityInterval` (explicit NegInf construction) |
| Mainnet parameters | Documented as constants; caller supplies their own `EraHistory` |

---

## Phase 1: Design — Complete

### Data Model

See [data-model.md](data-model.md) for full type definitions and relationships.

**Core types**:
- `SlotNo(u64)` — slot number
- `EraSummary { start_slot, start_time, slot_length, epoch_size }` — single era
- `EraHistory { eras: Vec<EraSummary> }` — ordered era list
- `ValidityInterval { invalid_before, invalid_hereafter }` — input from transaction
- `POSIXTime(i64)` — output in milliseconds
- `Interval<T> { from: LowerBound<T>, to: UpperBound<T> }` — Plutus interval
- `Extended<T> { NegInf | Finite(T) | PosInf }` — possibly-infinite value
- `TimeTranslationError` — typed error enum

### Contracts

See [contracts/public-api.md](contracts/public-api.md) for full API contract.

**Two public functions**:

```rust
// Haskell: slotToPOSIXTime (TxInfo.hs:164)
pub fn slot_to_posix_time(era_history: &EraHistory, slot: SlotNo)
    -> Result<POSIXTime, TimeTranslationError>

// Haskell: transValidityInterval (Conway/TxInfo.hs:785)
pub fn trans_validity_interval(era_history: &EraHistory, interval: &ValidityInterval)
    -> Result<POSIXTimeRange, TimeTranslationError>
```

### Quickstart

See [quickstart.md](quickstart.md) for usage examples and Haskell cross-reference table.

---

## Implementation Algorithm

### `slot_to_posix_time` step-by-step

```
1. Validate EraHistory is non-empty → EmptyEraHistory
2. Find the containing era:
   - Scan eras in reverse order
   - First era whose start_slot <= slot is the containing era
   - If slot < eras[0].start_slot → impossible (start_slot[0] = 0)
   - If slot >= horizon of last era → SlotPastHorizon
3. Compute slot offset within era:
   slot_offset = slot.0 - era.start_slot.0
4. Compute elapsed milliseconds (i128 to avoid overflow):
   elapsed_ms: i128 = slot_offset as i128 * era.slot_length.ms as i128
5. Compute absolute POSIX ms:
   posix_ms: i128 = era.start_time as i128 + elapsed_ms
6. Range-check and cast to i64:
   if posix_ms < i64::MIN || posix_ms > i64::MAX → ArithmeticOverflow
7. Return POSIXTime(posix_ms as i64)
```

**Haskell mapping**:
- Steps 2–7 implement what `epochInfoSlotToUTCTime` does internally in `cardano-slotting`
- The final truncation and ×1000 correspond to `slotToPOSIXTime` lines 169–171

### `trans_validity_interval` step-by-step

```
Match on (invalid_before, invalid_hereafter):

(None, None):
    → always()
    → Interval { from: LowerBound(NegInf, true), to: UpperBound(PosInf, true) }

(Some(i), None):
    t = slot_to_posix_time(i)?
    → from_posix(t)
    → Interval { from: LowerBound(Finite(t), true), to: UpperBound(PosInf, true) }

(None, Some(j)):
    t = slot_to_posix_time(j)?
    → Interval { from: LowerBound(NegInf, true), to: strict_upper_bound(t) }
    → Interval { from: LowerBound(NegInf, true), to: UpperBound(Finite(t), false) }

(Some(i), Some(j)):
    t1 = slot_to_posix_time(i)?
    t2 = slot_to_posix_time(j)?
    → Interval { from: lower_bound(t1), to: strict_upper_bound(t2) }
    → Interval { from: LowerBound(Finite(t1), true), to: UpperBound(Finite(t2), false) }
```

**Haskell mapping**: Directly mirrors `transValidityInterval` at `Conway/TxInfo.hs:785–809`.

---

## Horizon Definition

The horizon is defined as: the slot one epoch after the start of the last era's most recently confirmed epoch. Since this boundary is not stored in `EraHistory` directly, the implementation uses a conservative definition:

- The last `EraSummary` covers slots from `start_slot` to an **open end** (no explicit end stored).
- For conversion purposes, a slot is considered "within horizon" if it falls within the last era (i.e., `slot >= last_era.start_slot`).
- An implementation may optionally compute a tighter horizon as `last_era.start_slot + known_epochs * last_era.epoch_size.slots`.

**Haskell reference**: `validateOutsideForecast` at `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Rules/Utxo.hs:378–405` checks that `invalidHereafter` is within the forecastable horizon.

---

## Test Plan

| Test | Input | Expected |
|---|---|---|
| Byron slot 0 | slot 0, mainnet EraHistory | `POSIXTime(1_506_203_091_000)` |
| Byron mid-epoch | slot 10_000 | `POSIXTime(1_506_203_091_000 + 10_000 * 20_000)` |
| Shelley boundary | slot 4_492_800 | Era start time for Shelley |
| Allegra slot | slot 5_100_000 | Correct era lookup |
| Always interval | `(None, None)` | `always()` |
| Lower-only interval | `(Some(7_000_000), None)` | `[posix(7_000_000), +∞)` |
| Upper-only interval | `(None, Some(7_000_000))` | `(−∞, posix(7_000_000))` strict |
| Both-bound interval | `(Some(7_000_000), Some(7_100_000))` | `[posix(7_000_000), posix(7_100_000))` |
| Empty interval | `(Some(s), Some(s))` | Valid — both bounds convert (Plutus handles empty range) |
| Past horizon | slot beyond horizon | `Err(SlotPastHorizon(slot))` |
| Empty history | any slot | `Err(EmptyEraHistory)` |
| Zero slot_length | bad EraSummary | `Err(InvalidEraParams(...))` |
