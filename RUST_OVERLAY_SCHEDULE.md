# Rust Implementation of Cardano Overlay Schedule Lookup

This is a Rust translation of the `lookupInOverlaySchedule` function from the Cardano ledger's TPraos overlay schedule implementation.

## Overview

The overlay schedule is used during Cardano's transition from a centralized to a fully decentralized blockchain. It determines which slots are reserved for genesis nodes (BFT mode) versus which slots are available for stake pool competition (Praos mode).

## Files

- **`overlay_schedule.rs`** - Main library with types and functions
- **`overlay_example_standalone.rs`** - Standalone example (all code in one file)

## Core Function

```rust
pub fn lookup_in_overlay_schedule(
    first_slot_no: SlotNo,
    genesis_keys: &BTreeSet<GenesisKeyHash>,
    d_val: UnitInterval,
    asc_value: ActiveSlotCoeff,
    slot: SlotNo,
) -> Option<OBftSlot>
```

### Parameters

- **`first_slot_no`** - The first slot number of the current epoch
- **`genesis_keys`** - Set of genesis node key hashes (28 bytes each)
- **`d_val`** - Decentralization parameter (0.0 to 1.0)
  - `d = 1.0`: Fully centralized (all blocks by genesis nodes)
  - `d = 0.0`: Fully decentralized (all blocks by stake pools)
  - `0 < d < 1`: Mixed (some slots for genesis, some for pools)
- **`asc_value`** - Active slot coefficient (typically 0.05 or 5%)
- **`slot`** - The slot number to check

### Returns

- **`Some(OBftSlot::ActiveSlot(key_hash))`** - Slot is reserved for a specific genesis node
- **`Some(OBftSlot::NonActiveSlot)`** - Slot is in overlay schedule but not active
- **`None`** - Slot is not in overlay schedule (available for stake pools)

## Key Types

### `SlotNo`
A slot number on the blockchain (u64 wrapper).

### `GenesisKeyHash`
28-byte hash of a genesis verification key (Blake2b_224).

```rust
let key = GenesisKeyHash::from_hex(
    "ad5463153dc3d24b9ff133e46136028bdc1edbb897f5a7cf1b37950c"
)?;
```

### `UnitInterval`
Represents a value in the range [0, 1] as a rational number.

```rust
let d = UnitInterval::new(1, 2);          // d = 0.5
let d = UnitInterval::from_f64(0.75);     // d = 0.75
```

### `ActiveSlotCoeff`
The active slot coefficient from the protocol parameters.

```rust
let asc = ActiveSlotCoeff::new(0.05);  // 5% of slots can have blocks
```

### `OBftSlot`
Classification result for a slot:

```rust
pub enum OBftSlot {
    NonActiveSlot,                    // Not active for block production
    ActiveSlot(GenesisKeyHash),       // Assigned to specific genesis node
}
```

## Algorithm

The overlay schedule lookup uses the following algorithm:

1. **Check if slot is in overlay schedule** (`is_overlay_slot`):
   ```
   step(s) < step(s + 1)
   where:
     s = slot - first_slot_of_epoch
     step(x) = ceil(x * d)
   ```

2. **If in overlay schedule, classify it** (`classify_overlay_slot`):
   ```
   position = ceil(slot_offset * d)
   asc_inv = floor(1 / f)
   is_active = position % asc_inv == 0
   
   if is_active:
     genesis_idx = (position / asc_inv) % num_genesis_nodes
     return ActiveSlot(genesis_keys[genesis_idx])
   else:
     return NonActiveSlot
   ```

This implements a **round-robin schedule** where active slots are distributed evenly among genesis nodes.

## Usage Example

```rust
use std::collections::BTreeSet;

// Setup
let first_slot = SlotNo(432000);  // First slot of epoch
let d_val = UnitInterval::new(1, 1);  // d = 1.0 (fully centralized)
let asc = ActiveSlotCoeff::new(0.05);

// Genesis keys from genesis JSON
let mut genesis_keys = BTreeSet::new();
genesis_keys.insert(GenesisKeyHash::from_hex("ad54...")?);
genesis_keys.insert(GenesisKeyHash::from_hex("b954...")?);
// ... add all 7 genesis keys

// Lookup a slot
let slot = SlotNo(432000);
match lookup_in_overlay_schedule(first_slot, &genesis_keys, d_val, asc, slot) {
    Some(OBftSlot::ActiveSlot(key_hash)) => {
        println!("Slot assigned to genesis node: {:?}", key_hash);
    }
    Some(OBftSlot::NonActiveSlot) => {
        println!("Non-active overlay slot");
    }
    None => {
        println!("Slot available for stake pools");
    }
}
```

## Compilation and Testing

```bash
# Compile library with tests
rustc --crate-type lib overlay_schedule.rs --test -o overlay_schedule_test

# Run tests
./overlay_schedule_test

# Compile and run standalone example
rustc overlay_example_standalone.rs -o overlay_example
./overlay_example
```

## Differences from Haskell Implementation

1. **Type Safety**: Rust uses `BTreeSet` instead of Haskell's `Set` for ordered collections
2. **Error Handling**: Uses `Result<T, String>` instead of Haskell's Maybe/Either
3. **Numeric Types**: Uses `f64` for rational calculations (Haskell uses arbitrary precision `Rational`)
4. **Memory**: Rust requires explicit ownership and borrowing (`&BTreeSet` parameter)

## Notes

- The decentralization parameter `d` controls the percentage of slots reserved for genesis nodes
- With `d = 1.0`, all slots are in the overlay schedule, but only active slots (1/20 with f=0.05) actually produce blocks
- Genesis nodes are selected in round-robin fashion for active slots
- This mechanism was used during Cardano's Shelley era transition period

## References

- Original Haskell: `Cardano.Protocol.TPraos.Rules.Overlay`
- Formal spec: See `eras/shelley/formal-spec/chain.tex` in cardano-ledger repo
- Ouroboros Praos paper: [IOHK Research](https://iohk.io/en/research/library/)



