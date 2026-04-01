# Quickstart: Using `slot-time.rs`

**Feature**: `001-slot-time-conversion`
**Date**: 2026-03-18

---

## What `slot-time.rs` does

`slot-time.rs` is a single self-contained Rust file that converts Cardano transaction validity intervals (expressed in slots) to POSIX time ranges (in milliseconds), exactly as the Cardano ledger does before passing context to Plutus smart contracts.

This is required for **phase 2 validation**: any implementation that evaluates Plutus scripts must perform this conversion before calling the evaluator.

---

## Drop-in usage

Copy `slot-time.rs` into your project. It has no external crate dependencies.

```rust
// In your project: include the module
mod slot_time;
use slot_time::*;
```

---

## Scenario 1: Convert a single slot to POSIX time

```rust
// Build an EraHistory for mainnet (or use mainnet_era_history())
let era_history = mainnet_era_history();

// Convert slot 7_000_000 (somewhere in Alonzo era) to POSIX milliseconds
let slot = SlotNo(7_000_000);
match slot_to_posix_time(&era_history, slot) {
    Ok(POSIXTime(ms)) => println!("POSIX time: {} ms", ms),
    Err(e) => eprintln!("Error: {:?}", e),
}
```

---

## Scenario 2: Convert a transaction's validity interval

```rust
// Transaction says: valid from slot 7_000_000 to slot 7_100_000
let interval = ValidityInterval {
    invalid_before:    Some(SlotNo(7_000_000)),
    invalid_hereafter: Some(SlotNo(7_100_000)),
};

let era_history = mainnet_era_history();
match trans_validity_interval(&era_history, &interval) {
    Ok(range) => {
        // range is a POSIXTimeRange (Interval<POSIXTime>)
        // Pass it to your Plutus evaluator as txInfoValidRange
        println!("Time range: {:?}", range);
    }
    Err(TimeTranslationError::SlotPastHorizon(slot)) => {
        eprintln!("Slot {} is past the known era horizon", slot);
    }
    Err(e) => eprintln!("Conversion error: {:?}", e),
}
```

---

## Scenario 3: Use a custom era history (testnet / custom chain)

```rust
let era_history = EraHistory {
    eras: vec![
        EraSummary {
            start_slot:  SlotNo(0),
            start_time:  0,                   // custom genesis
            slot_length: SlotLength { ms: 1_000 },
            epoch_size:  EpochSize { slots: 432_000 },
        },
        // Add more eras as needed...
    ],
};

let result = slot_to_posix_time(&era_history, SlotNo(500));
assert_eq!(result, Ok(POSIXTime(500_000)));  // 500 slots × 1000 ms
```

---

## Understanding the output format

The `POSIXTimeRange` output mirrors the Plutus `Interval<POSIXTime>` type exactly:

```
POSIXTimeRange {
    from: LowerBound { bound: Finite(POSIXTime(1_600_000_000_000)), inclusive: true  },
    to:   UpperBound { bound: Finite(POSIXTime(1_600_100_000_000)), inclusive: false },
}
```

- `from.inclusive = true` → lower bound is **closed** (≥)
- `to.inclusive = false` → upper bound is **strict/open** (<) — always

This is exactly what Plutus smart contracts receive as `txInfoValidRange`.

---

## Haskell cross-reference at a glance

| Rust function | Haskell equivalent | Source |
|---|---|---|
| `slot_to_posix_time` | `slotToPOSIXTime` | `libs/cardano-ledger-core/src/Cardano/Ledger/Plutus/TxInfo.hs:164` |
| `trans_validity_interval` | `transValidityInterval` | `eras/conway/impl/src/Cardano/Ledger/Conway/TxInfo.hs:785` |
| `always()` | `PV1.always` | `plutus-ledger-api` |
| `from_posix(t)` | `PV1.from t` | `plutus-ledger-api` |
| `to_posix(t)` | `PV1.to t` | `plutus-ledger-api` |
| `strict_upper_bound(t)` | `PV1.strictUpperBound t` | `plutus-ledger-api` |
| `lower_bound(t)` | `PV1.lowerBound t` | `plutus-ledger-api` |

---

## Common errors and fixes

| Error | Cause | Fix |
|---|---|---|
| `SlotPastHorizon(n)` | Slot `n` is beyond the last known era end | Update `EraHistory` with newer era data from the node |
| `EmptyEraHistory` | `EraHistory.eras` is empty | Always include at least one `EraSummary` starting at slot 0 |
| `InvalidEraParams(msg)` | `slot_length.ms == 0` or `epoch_size.slots == 0` | Fix the era parameters |
| `ArithmeticOverflow` | Astronomically large slot number | Slot values should always be realistic chain values |

---

## Running the built-in tests

```bash
# Compile and run tests
rustc --test slot-time.rs -o slot-time-test && ./slot-time-test
```

The file includes `#[test]` functions covering:
- Single-slot conversion for each mainnet era
- All four `ValidityInterval` bound combinations
- Era boundary edge cases
- Error paths (horizon exceeded, overflow, empty history)
