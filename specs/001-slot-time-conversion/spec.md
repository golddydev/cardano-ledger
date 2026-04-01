# Feature Specification: Slot-to-POSIX Time Conversion for Phase 2 Validation

**Feature Branch**: `001-slot-time-conversion`
**Created**: 2026-03-18
**Status**: Draft

## Overview

When evaluating Plutus smart contracts (phase 2 validation), a transaction's validity range is expressed in blockchain slots. Smart contracts operate on POSIX time (milliseconds since Unix epoch). The ledger must therefore convert the slot-based validity range into a POSIX time range before constructing the script evaluation context. This conversion requires knowing the full era history of the chain, because slot lengths and epoch boundaries differ across eras.

This feature captures how that conversion works in the reference Haskell implementation and specifies a portable, era-aware Rust implementation (`slot-time.rs`) of the same logic, complete with annotated Haskell cross-references.

---

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Convert Slot Range to POSIX Time Range (Priority: P1)

A developer implementing phase 2 transaction validation needs to convert a transaction's validity range — expressed as a pair of optional slot numbers (`invalidBefore`, `invalidHereafter`) — into an equivalent POSIX time range that can be passed to the Plutus evaluator.

**Why this priority**: Without this conversion, smart contracts cannot inspect the transaction time window; all time-sensitive scripts (vesting, options, deadlines) would fail or produce wrong results. It is a hard blocker for any phase 2 validation.

**Independent Test**: Given known era parameters (slot length, epoch length, system start), provide a slot number and verify the returned POSIX millisecond value matches the expected UTC timestamp.

**Acceptance Scenarios**:

1. **Given** a fully specified validity range `[slotA, slotB)` and complete era parameters, **When** the conversion runs, **Then** both slot bounds are translated to correct POSIX millisecond timestamps and returned as a closed-open interval.
2. **Given** a validity range with no lower bound (`−∞`) and a finite upper slot, **When** the conversion runs, **Then** the result is an interval from negative infinity to the converted upper POSIX time.
3. **Given** a validity range with a finite lower slot and no upper bound (`+∞`), **When** the conversion runs, **Then** the result is an interval from the converted lower POSIX time to positive infinity.
4. **Given** a validity range with neither bound set, **When** the conversion runs, **Then** the result is the "always" interval covering all time.

---

### User Story 2 - Handle Multi-Era Chain Histories (Priority: P2)

A developer building a Cardano ledger tool needs the slot-to-time conversion to work correctly for slots that span multiple eras (Byron, Shelley, Allegra, Mary, Alonzo, Babbage, Conway), each of which may have different slot lengths and epoch boundaries.

**Why this priority**: A single slot-length assumption (e.g., 1 second per slot) is incorrect for all eras. Failing to account for the Byron era's 20-second slots or era transition boundaries produces wrong POSIX timestamps, silently breaking smart contract time checks.

**Independent Test**: Given a hand-crafted multi-era summary with known parameters, convert a slot that falls in the second or third era and verify the returned POSIX time accounts for all preceding era durations.

**Acceptance Scenarios**:

1. **Given** a chain with two eras (different slot lengths), **When** a slot in the second era is converted, **Then** the result includes the total time of the first era plus the partial time through the second era.
2. **Given** a slot number that exactly equals an era boundary slot, **When** the conversion runs, **Then** the result equals the start of the new era in POSIX time.
3. **Given** a slot number beyond the known era horizon, **When** the conversion runs, **Then** the conversion returns a clear error indicating the horizon has been exceeded.

---

### User Story 3 - Annotated Reference for Implementors (Priority: P3)

A developer new to the Cardano ledger codebase needs to understand what each Haskell function does in plain English, and how the Rust implementation maps to the reference code. They need both implementations side-by-side to build confidence and correctly adapt the logic.

**Why this priority**: The correctness of consensus-critical time conversion is hard to verify in isolation. Annotated cross-references between Haskell and Rust allow independent reviewers to confirm the Rust logic is faithful to the specification.

**Independent Test**: Reading the Rust file alone, a developer can identify the corresponding Haskell function for each major step and understand why each step is performed.

**Acceptance Scenarios**:

1. **Given** the generated `slot-time.rs` file, **When** a reviewer reads each function, **Then** they find a comment citing the exact Haskell source file and line range.
2. **Given** any `[NEEDS CLARIFICATION]`-free spec, **When** a developer follows the Rust implementation, **Then** all intermediate values (epoch number, slot within epoch, offset seconds, POSIX milliseconds) match what the Haskell reference produces for the same inputs.

---

### Edge Cases

- What happens when `invalidBefore` equals `invalidHereafter`? (Empty interval — no slot satisfies the range.)
- What happens when a slot falls exactly at an era boundary? (The new era's parameters apply from that slot onward.)
- What happens when the `EpochInfo` horizon does not cover the requested slot? (Must return a descriptive error: "slot past known horizon".)
- What happens when the system start is set to the Unix epoch (zero)? (Should produce zero-based POSIX times without underflow.)
- What happens when slot numbers approach `u64::MAX`? (Overflow must be detected and reported as an error.)
- How does the conversion behave when epoch parameters are inconsistent (e.g., epoch length is zero)? (Should fail fast with a clear configuration error.)

---

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The conversion MUST accept a chain's full era summary (ordered list of era parameters: start slot, start time, slot length, epoch length) as input.
- **FR-002**: The conversion MUST accept a `SystemStart` value (the absolute POSIX time at slot 0 of the chain) as input.
- **FR-003**: Given a `SlotNo`, the conversion MUST compute the correct POSIX time in **milliseconds** (truncated, not rounded), matching the formula: locate the containing era, compute elapsed time within that era, add the era's start time offset.
- **FR-004**: The `ValidityInterval` conversion MUST handle all four bound combinations: `(None, None)`, `(Some, None)`, `(None, Some)`, `(Some, Some)`, producing the appropriate Plutus-style open/closed interval.
- **FR-005**: The upper bound of a converted validity interval MUST be a **strict upper bound** (open on top), preserving the half-open `[lower, upper)` semantics of `ValidityInterval`.
- **FR-006**: The conversion MUST return a typed error (not panic) when a slot exceeds the known era horizon.
- **FR-007**: The Rust source file MUST include, for every significant function, a comment with the corresponding Haskell source file path and line numbers, and a plain-English description of what the function does.
- **FR-008**: The Rust implementation MUST be self-contained in a single file `slot-time.rs` with no external crate dependencies beyond the Rust standard library.
- **FR-009**: The implementation MUST support the complete set of eras that have appeared on Cardano mainnet: Byron, Shelley, Allegra, Mary, Alonzo, Babbage, Conway, and be extensible to future eras.
- **FR-010**: All arithmetic involving time MUST be checked for overflow; any overflow MUST produce an error rather than silent truncation.

### Key Entities

- **SystemStart**: The absolute real-world time (POSIX milliseconds) at which the blockchain began (slot 0). Fixed per network.
- **EraSummary**: Parameters describing a single era — the slot at which it began, the POSIX time at which it began, the length of each slot (in milliseconds), and the number of slots per epoch.
- **EraHistory**: An ordered, non-overlapping list of `EraSummary` entries covering the chain from genesis to the current known horizon.
- **SlotNo**: A non-negative integer identifying a single slot on the chain (u64).
- **ValidityInterval**: A half-open interval `[invalidBefore, invalidHereafter)` where either bound may be absent (representing ±∞). Bounds are expressed in slots.
- **POSIXTimeRange**: The Plutus representation of the validity interval after conversion — a closed-open interval expressed in POSIX milliseconds, with optional infinite bounds.
- **TimeTranslationError**: A typed error returned when slot-to-time conversion cannot be completed (e.g., slot past horizon, overflow).

---

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Given any slot number within the known era horizon, the conversion produces a POSIX millisecond value that is byte-for-byte identical to what the Haskell reference implementation produces for the same inputs.
- **SC-002**: The `slot-time.rs` file compiles without warnings and all included unit tests pass against both hand-crafted and mainnet-sampled era parameters.
- **SC-003**: Every function in `slot-time.rs` has an associated Haskell cross-reference comment; a reviewer can verify this by reading the file without accessing any external documentation.
- **SC-004**: The conversion correctly handles all four `ValidityInterval` bound combinations, verified by dedicated test cases for each.
- **SC-005**: Slots beyond the known era horizon return a descriptive error in 100% of cases; no panic or silent wrong result occurs.
- **SC-006**: A developer unfamiliar with Cardano slot-time semantics can read the plain-English function descriptions in `slot-time.rs` and correctly explain the conversion algorithm to a peer within one reading.

---

## Assumptions

- **Byron era slot length**: 20 seconds per slot (as deployed on Cardano mainnet).
- **Shelley and later eras slot length**: 1 second per slot (as deployed on Cardano mainnet).
- **Epoch lengths**: Vary by era; Byron uses 21,600 slots per epoch; Shelley and later use 432,000 slots per epoch (mainnet values).
- **POSIX time units**: The Plutus evaluator expects milliseconds (Int64). The Haskell reference multiplies POSIX seconds by 1,000 and truncates.
- **Era horizon**: The conversion only guarantees correctness up to the last fully confirmed era boundary. Slots within the current epoch but beyond the last confirmed boundary may fail with a horizon error — this is intentional and mirrors the Haskell behavior of `validateOutsideForecast`.
- **No ledger state required**: The conversion is a pure mathematical operation over the era history and system start; it does not require access to UTxO or other ledger state.
- **Single chain**: This spec targets Cardano mainnet; testnet parameters differ but the algorithm is identical.

---

## Haskell Reference Summary

The following Haskell source locations are the normative reference for this specification:

| Component | Source File | Lines | Plain-English Role |
|---|---|---|---|
| `slotToPOSIXTime` | `libs/cardano-ledger-core/src/Cardano/Ledger/Plutus/TxInfo.hs` | 164–171 | Core function: converts a single slot number to a POSIX millisecond timestamp |
| `transValidityInterval` | `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Plutus/TxInfo.hs` | 220–243 | Converts a slot-based validity interval to a Plutus time range, handling all four bound cases |
| `transValidityInterval` (Conway) | `eras/conway/impl/src/Cardano/Ledger/Conway/TxInfo.hs` | 785–809 | Conway-era variant with explicit negative-infinity lower bound construction |
| `ValidityInterval` | `eras/allegra/impl/src/Cardano/Ledger/Allegra/Scripts.hs` | 115–129 | Data type representing `[invalidBefore, invalidHereafter)` in slots |
| `validateOutsideForecast` | `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Rules/Utxo.hs` | 378–405 | Ledger rule: checks that the upper validity bound is within the forecastable horizon when redeemers are present |
| `LedgerTxInfo` | `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Plutus/Context.hs` | 83–90 | Bundles `EpochInfo` and `SystemStart` required for time conversion alongside the transaction |
| `epochInfoSlotToUTCTime` | `cardano-slotting` library | — | External library function: converts a slot to a UTC timestamp using epoch structure |

---

## Out of Scope

- Converting POSIX time back to slot number (reverse direction).
- Changes to the Plutus evaluator itself.
- Support for Cardano testnets with custom genesis parameters (the algorithm is the same; only the input parameters differ).
- Any modifications to the existing Haskell ledger code.
