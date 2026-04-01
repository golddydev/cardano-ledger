# Tasks: Slot-to-POSIX Time Conversion for Phase 2 Validation

**Input**: Design documents from `/specs/001-slot-time-conversion/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/ ✓, quickstart.md ✓

**Deliverable**: A single self-contained Rust file at `specs/001-slot-time-conversion/slot-time.rs`

**Organization**: Tasks are grouped by user story. Since all tasks write to a single file, they are executed sequentially. The file is built up section by section.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1, US2, US3)
- No [P] markers for tasks targeting the same file

---

## Phase 1: Setup

**Purpose**: Create the skeleton file and configure the project structure.

- [x] T001 Create `specs/001-slot-time-conversion/slot-time.rs` with module-level documentation header: crate attributes (`#![allow(dead_code)]`), copyright comment, and a top-level doc comment explaining the file's purpose (Cardano slot-to-POSIX time conversion for Plutus phase 2 validation, per spec)
- [x] T002 Verify `.gitignore` at repo root includes `target/` and `*.rlib` patterns for Rust build artifacts

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: All core types that every function depends on. Must be complete before any user story implementation.

**⚠️ CRITICAL**: No user story work can begin until all types are defined.

- [x] T003 Add `TimeTranslationError` enum to `specs/001-slot-time-conversion/slot-time.rs` with variants: `SlotPastHorizon(u64)`, `EmptyEraHistory`, `InvalidEraParams(String)`, `ArithmeticOverflow`; derive `Debug, PartialEq`; add Haskell cross-reference comment citing `AlonzoContextError::TimeTranslationPastHorizon` in `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Plutus/TxInfo.hs`
- [x] T004 Add input type definitions to `specs/001-slot-time-conversion/slot-time.rs`: `SystemStart { posix_ms: i64 }`, `SlotLength { ms: u64 }`, `EpochSize { slots: u64 }`, `SlotNo(pub u64)`; each with `#[derive(Debug, Clone, Copy, PartialEq)]` and Haskell cross-reference doc comments citing the corresponding `cardano-slotting` types
- [x] T005 Add `EraSummary` struct to `specs/001-slot-time-conversion/slot-time.rs` with fields `start_slot: SlotNo`, `start_time: i64`, `slot_length: SlotLength`, `epoch_size: EpochSize`; derive `Debug, Clone`; add plain-English doc comment and Haskell reference citing `Ouroboros.Consensus.HardFork.History.Summary`
- [x] T006 Add `EraHistory` struct to `specs/001-slot-time-conversion/slot-time.rs` with field `eras: Vec<EraSummary>`; derive `Debug, Clone`; add doc comment explaining ordered, non-overlapping era list and the horizon concept
- [x] T007 Add `ValidityInterval` struct to `specs/001-slot-time-conversion/slot-time.rs` with fields `invalid_before: Option<SlotNo>`, `invalid_hereafter: Option<SlotNo>`; derive `Debug, Clone, PartialEq`; add plain-English doc comment and Haskell reference citing `eras/allegra/impl/src/Cardano/Ledger/Allegra/Scripts.hs:115-121`
- [x] T008 Add Plutus mirror types to `specs/001-slot-time-conversion/slot-time.rs`: `POSIXTime(pub i64)`, `Extended<T>` enum (`NegInf`, `Finite(T)`, `PosInf`), `LowerBound<T>` struct (`bound: Extended<T>`, `inclusive: bool`), `UpperBound<T>` struct (`bound: Extended<T>`, `inclusive: bool`), `Interval<T>` struct (`from: LowerBound<T>`, `to: UpperBound<T>`), and `type POSIXTimeRange = Interval<POSIXTime>`; add Haskell references to `PlutusLedgerApi.V1` for each type

**Checkpoint**: All types defined — `rustc --edition 2021 --crate-type lib specs/001-slot-time-conversion/slot-time.rs` should compile with no errors.

---

## Phase 3: User Story 1 — Convert Slot Range to POSIX Time Range (Priority: P1) 🎯 MVP

**Goal**: Implement the two core public functions that convert a transaction's validity interval to a Plutus time range.

**Independent Test**: Given a simple single-era EraHistory and a ValidityInterval with both bounds set, `trans_validity_interval` returns a closed-open `Interval<POSIXTime>` with correct millisecond values.

### Implementation for User Story 1

- [x] T009 [US1] Add interval helper constructors to `specs/001-slot-time-conversion/slot-time.rs`: `pub fn always() -> POSIXTimeRange`, `pub fn from_posix(t: POSIXTime) -> POSIXTimeRange`, `pub fn to_posix(t: POSIXTime) -> POSIXTimeRange`, `pub fn lower_bound(t: POSIXTime) -> LowerBound<POSIXTime>`, `pub fn strict_upper_bound(t: POSIXTime) -> UpperBound<POSIXTime>`; each with a Haskell cross-reference doc comment to the corresponding `PV1.*` function in `plutus-ledger-api`
- [x] T010 [US1] Implement `pub fn slot_to_posix_time(era_history: &EraHistory, slot: SlotNo) -> Result<POSIXTime, TimeTranslationError>` in `specs/001-slot-time-conversion/slot-time.rs`; algorithm: (1) return `EmptyEraHistory` if eras is empty, (2) validate each era's slot_length.ms > 0, (3) find the last era whose start_slot <= slot via linear scan, (4) return `SlotPastHorizon` if no era contains the slot, (5) compute `slot_offset = slot.0 - era.start_slot.0`, (6) compute `elapsed_ms: i128 = slot_offset as i128 * era.slot_length.ms as i128`, (7) compute `posix_ms: i128 = era.start_time as i128 + elapsed_ms`, (8) range-check and cast to i64 or return `ArithmeticOverflow`; add Haskell cross-reference to `slotToPOSIXTime` at `libs/cardano-ledger-core/src/Cardano/Ledger/Plutus/TxInfo.hs:164-171` and plain-English description of each step
- [x] T011 [US1] Implement `pub fn trans_validity_interval(era_history: &EraHistory, interval: &ValidityInterval) -> Result<POSIXTimeRange, TimeTranslationError>` in `specs/001-slot-time-conversion/slot-time.rs`; match on `(invalid_before, invalid_hereafter)`: `(None, None)` → `always()`, `(Some(i), None)` → `from_posix(slot_to_posix_time(i)?)`, `(None, Some(j))` → `Interval { from: LowerBound { bound: NegInf, inclusive: true }, to: strict_upper_bound(slot_to_posix_time(j)?) }`, `(Some(i), Some(j))` → `Interval { from: lower_bound(t1), to: strict_upper_bound(t2) }`; add Haskell cross-reference to `transValidityInterval` (Conway variant) at `eras/conway/impl/src/Cardano/Ledger/Conway/TxInfo.hs:785-809` and plain-English description of each match arm
- [x] T012 [US1] Add `#[cfg(test)]` module to `specs/001-slot-time-conversion/slot-time.rs` with four tests for `trans_validity_interval` bound combinations: `test_always_interval` (both None → NegInf/PosInf), `test_lower_only_interval` (Some lower, None upper → closed lower / PosInf), `test_upper_only_interval` (None lower, Some upper → NegInf / strict upper), `test_both_bounds_interval` (both Some → closed lower / strict upper); use a minimal single-era EraHistory with start_slot=0, start_time=0, slot_length=1000ms, epoch_size=100 for deterministic values
- [x] T013 [US1] Add `test_upper_bound_is_strict` test in `specs/001-slot-time-conversion/slot-time.rs` asserting that in the output of `trans_validity_interval` the `to.inclusive` field is always `false` when a finite upper bound is present; verifies FR-005 (strict upper bound preservation)

**Checkpoint**: `rustc --test specs/001-slot-time-conversion/slot-time.rs -o /tmp/slot-time-test && /tmp/slot-time-test` — all US1 tests pass.

---

## Phase 4: User Story 2 — Handle Multi-Era Chain Histories (Priority: P2)

**Goal**: Ensure `slot_to_posix_time` handles slots across multiple eras with different slot lengths, and expose mainnet era parameters as testable constants.

**Independent Test**: Using a two-era EraHistory (era 0: 20s slots, era 1: 1s slots), converting a slot in era 1 returns a POSIX time that accounts for the total duration of era 0 plus the partial time through era 1.

### Implementation for User Story 2

- [x] T014 [US2] Add mainnet era parameter constants to `specs/001-slot-time-conversion/slot-time.rs`: `MAINNET_SYSTEM_START_MS: i64 = 1_506_203_091_000`, `BYRON_SLOT_LENGTH_MS: u64 = 20_000`, `BYRON_EPOCH_SIZE: u64 = 21_600`, `SHELLEY_SLOT_LENGTH_MS: u64 = 1_000`, `SHELLEY_EPOCH_SIZE: u64 = 432_000`; add comments with human-readable UTC times and source references (`eras/byron/ledger/impl/src/Cardano/Chain/Epoch/File.hs:69`)
- [x] T015 [US2] Add mainnet era start slots as constants to `specs/001-slot-time-conversion/slot-time.rs`: `SHELLEY_START_SLOT: u64 = 4_492_800`, `ALLEGRA_START_SLOT: u64 = 5_068_800`, `MARY_START_SLOT: u64 = 5_788_800`, `ALONZO_START_SLOT: u64 = 6_220_800`, `BABBAGE_START_SLOT: u64 = 7_791_600`, `CONWAY_START_SLOT: u64 = 10_577_800`; with era name comments and approximate UTC dates
- [x] T016 [US2] Implement `pub fn mainnet_era_history() -> EraHistory` in `specs/001-slot-time-conversion/slot-time.rs`; construct an `EraHistory` with one `EraSummary` per mainnet era (Byron through Conway), computing each era's `start_time` as the cumulative sum of all preceding eras' durations plus `MAINNET_SYSTEM_START_MS`; add a doc comment explaining that `start_time` for each era is derived as `previous_era.start_time + (next_start_slot - previous_start_slot) * previous_slot_length_ms`
- [x] T017 [US2] Add multi-era tests to the `#[cfg(test)]` module in `specs/001-slot-time-conversion/slot-time.rs`: `test_single_era_slot_zero` (slot 0 with custom EraHistory returns start_time), `test_single_era_arbitrary_slot` (slot N in single era returns start_time + N * slot_length_ms), `test_two_era_boundary_slot` (slot exactly at era 1 start returns era 1's start_time), `test_two_era_slot_in_second_era` (slot 10 into era 2 with 1000ms slots returns era_2_start_time + 10_000), `test_byron_to_shelley` (use two-era EraHistory mirroring Byron→Shelley transition, verify Shelley boundary slot)
- [x] T018 [US2] Add error path tests to `specs/001-slot-time-conversion/slot-time.rs`: `test_empty_era_history` (returns `Err(EmptyEraHistory)`), `test_slot_past_horizon` (slot beyond last era with known end returns `Err(SlotPastHorizon(slot))`), `test_zero_slot_length` (EraSummary with slot_length.ms=0 returns `Err(InvalidEraParams(...))`); verify no panics occur on any of these inputs

**Checkpoint**: All tests pass including multi-era and error path tests.

---

## Phase 5: User Story 3 — Annotated Reference for Implementors (Priority: P3)

**Goal**: Ensure every function and type in `slot-time.rs` has a complete Haskell cross-reference comment and plain-English description.

**Independent Test**: Read `slot-time.rs` top to bottom — every `pub fn` and `pub struct`/`pub enum` has a doc comment with (1) a Haskell source file path and line numbers, and (2) a plain-English description of what the function/type does.

### Implementation for User Story 3

- [x] T019 [US3] Audit all `pub` types in `specs/001-slot-time-conversion/slot-time.rs` and ensure each has a `///` doc comment containing: (a) a one-sentence plain-English description of what the type represents, (b) a `/// Haskell ref:` line with the exact file path and line numbers; update any type added in T003–T008 that is missing either element
- [x] T020 [US3] Audit all `pub fn` functions in `specs/001-slot-time-conversion/slot-time.rs` and ensure each has a multi-line `///` doc comment containing: (a) a plain-English description of what the function does step by step, (b) a `/// Haskell ref:` section with file path and line range, (c) `/// # Errors` section listing each error variant and when it occurs; update T009–T011 functions if any element is missing
- [x] T021 [US3] Add a top-of-file module doc comment (`//!`) to `specs/001-slot-time-conversion/slot-time.rs` that: (1) states the file's purpose (slot-to-POSIX time conversion for Plutus phase 2 validation), (2) explains the conversion algorithm in plain English (find containing era, compute elapsed ms, add era start time), (3) includes a conversion flow diagram as ASCII art matching the one in `data-model.md`, (4) lists the two public functions with one-line descriptions
- [x] T022 [US3] Add inline comments inside `slot_to_posix_time` in `specs/001-slot-time-conversion/slot-time.rs` labeling each algorithmic step with its step number and corresponding Haskell operation (e.g., `// Step 3: Find containing era — mirrors cardano-slotting epochInfoSlotToUTCTime internal era lookup`); ensure all 7 steps from plan.md are labeled
- [x] T023 [US3] Add inline comments inside `trans_validity_interval` in `specs/001-slot-time-conversion/slot-time.rs` labeling each match arm with: which `ValidityInterval` case it handles, the corresponding Haskell match arm from `Conway/TxInfo.hs:785-809`, and what Plutus interval constructor is being reproduced

**Checkpoint**: A developer reading `slot-time.rs` can identify the Haskell source for every significant operation without consulting any external documentation.

---

## Phase 6: Polish & Validation

**Purpose**: Final compilation check, test run, and quickstart scenario validation.

- [x] T024 [P] Compile `specs/001-slot-time-conversion/slot-time.rs` as a library with `rustc --edition 2021 --crate-type lib specs/001-slot-time-conversion/slot-time.rs -o /tmp/libslot_time.rlib` and confirm zero warnings (fix any that appear)
- [x] T025 [P] Compile and run all tests with `rustc --edition 2021 --test specs/001-slot-time-conversion/slot-time.rs -o /tmp/slot-time-test && /tmp/slot-time-test` and confirm all tests pass; report test count
- [x] T026 Verify the three quickstart scenarios from `quickstart.md` work correctly by tracing through them manually against the implementation: (1) single slot conversion with `mainnet_era_history()`, (2) both-bound `ValidityInterval` conversion, (3) custom single-era EraHistory with slot 500 → `POSIXTime(500_000)`
- [x] T027 Verify behavioral guarantees table from `contracts/public-api.md` by adding dedicated assertion tests for each row: slot 0 on mainnet, all four ValidityInterval combinations, `SlotPastHorizon` error, `EmptyEraHistory` error

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately
- **Foundational (Phase 2)**: Depends on Phase 1; BLOCKS all user stories
- **User Story 1 (Phase 3)**: Depends on Phase 2 completion
- **User Story 2 (Phase 4)**: Depends on Phase 3 (uses `slot_to_posix_time` already implemented)
- **User Story 3 (Phase 5)**: Depends on Phase 4 (annotates completed implementation)
- **Polish (Phase 6)**: Depends on all story phases complete

### Within Each Phase

All tasks in this project are sequential (single file). No intra-phase parallelism.

### Parallel Opportunities

- T024 (compile check) and T025 (test run) are marked [P] and can run together in Phase 6 since they are independent commands.

---

## Parallel Example: Phase 6

```bash
# These two can run concurrently:
rustc --edition 2021 --crate-type lib slot-time.rs -o /tmp/libslot_time.rlib
rustc --edition 2021 --test slot-time.rs -o /tmp/slot-time-test && /tmp/slot-time-test
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001–T002)
2. Complete Phase 2: Foundational types (T003–T008)
3. Complete Phase 3: Core conversion functions (T009–T013)
4. **STOP and VALIDATE**: `rustc --test slot-time.rs` — all US1 tests pass
5. US1 deliverable: correct `trans_validity_interval` for any single-era chain

### Incremental Delivery

1. Phase 1 + 2 → All types defined, file compiles
2. Phase 3 (US1) → Core conversion works for single era
3. Phase 4 (US2) → Multi-era handling and mainnet constants
4. Phase 5 (US3) → Full cross-reference annotations
5. Phase 6 → Final validation

---

## Notes

- All tasks write to the single file `specs/001-slot-time-conversion/slot-time.rs`
- No external crate dependencies — only Rust std
- Compile check at every phase checkpoint
- Each test added in Phase 3–4 must pass before moving on
- The `mainnet_era_history()` function is documentation-quality code: correctness of specific UTC timestamps matters
