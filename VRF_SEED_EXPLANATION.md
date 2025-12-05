# VRF Seed Construction in Cardano TPraos

## Overview

The `mkSeed` function constructs a cryptographic seed used in **VRF (Verifiable Random Function)** computations for the Cardano TPraos consensus protocol. This seed is crucial for two main purposes:

1. **Leader Election** - Determining which stake pool should produce a block in a given slot
2. **Randomness Generation** - Creating entropy for the next epoch's nonce

## Function Signature

### Haskell (Original)
```haskell
mkSeed ::
  Nonce ->      -- Universal constant (domain separator)
  SlotNo ->     -- Slot number  
  Nonce ->      -- Epoch nonce (epoch randomness)
  Seed          -- Resulting seed for VRF
```

### Rust (Translation)
```rust
pub fn mk_seed(
    uc_nonce: &Nonce,      // Universal constant
    slot: SlotNo,          // Slot number
    e_nonce: &Nonce,       // Epoch nonce
) -> Seed
```

## Types

### `Nonce`
A nonce (number used once) that can be:
- **`Nonce(hash)`** - Contains a 32-byte Blake2b_256 hash
- **`NeutralNonce`** - Identity element (no value)

### `Seed`  
A wrapped 32-byte Blake2b_256 hash used as input to VRF functions.

### `SlotNo`
A slot number (unsigned 64-bit integer).

## Universal Constants (Domain Separators)

Two universal constants are used to ensure domain separation:

### `seedEta` (value: hash of 0)
Used for **eta computation** - generating randomness/nonce for the next epoch.
```rust
let seed_eta = Nonce::from_number(0);
```

### `seedL` (value: hash of 1)  
Used for **leader election** - determining if a stake pool is the slot leader.
```rust
let seed_l = Nonce::from_number(1);
```

## Algorithm

The seed construction follows these steps:

```
Input: universal_constant, slot_number, epoch_nonce

1. CREATE BYTE BUFFER:
   buffer = []
   
2. ADD SLOT NUMBER (8 bytes):
   buffer += slot_number.to_big_endian_bytes()
   
3. ADD EPOCH NONCE (0 or 32 bytes):
   if epoch_nonce is Nonce(hash):
       buffer += hash  // 32 bytes
   else:
       buffer += nothing  // neutral nonce adds nothing
       
4. HASH THE BUFFER:
   intermediate_hash = Blake2b_256(buffer)
   
5. XOR WITH UNIVERSAL CONSTANT:
   if universal_constant is Nonce(hash):
       final_hash = intermediate_hash XOR hash
   else:
       final_hash = intermediate_hash  // neutral adds nothing
       
6. WRAP IN SEED:
   return Seed(final_hash)
```

### Buffer Sizes
- **With neutral epoch nonce**: 8 bytes (slot only)
- **With epoch nonce**: 40 bytes (8 + 32)

### Output
- Always 32 bytes (Blake2b_256 hash)

## Examples

### Example 1: Leader Election Seed
```rust
let slot = SlotNo(432000);
let uc_nonce = seed_l();  // Use L constant for leader election
let e_nonce = Nonce::from_number(123);  // Current epoch nonce

let seed = mk_seed(&uc_nonce, slot, &e_nonce);
// This seed is used in VRF to prove you're the slot leader
```

### Example 2: Eta (Randomness) Seed
```rust
let slot = SlotNo(432000);
let uc_nonce = seed_eta();  // Use eta constant for randomness
let e_nonce = Nonce::from_number(123);

let seed = mk_seed(&uc_nonce, slot, &e_nonce);
// This seed is used in VRF to contribute to next epoch's nonce
```

### Example 3: Neutral Nonces
```rust
let slot = SlotNo(432000);
let uc_nonce = Nonce::neutral();
let e_nonce = Nonce::neutral();

let seed = mk_seed(&uc_nonce, slot, &e_nonce);
// Minimal seed - just hash of slot number
```

## Usage in Cardano Consensus

### 1. Leader Election (Praos)
For each slot, a stake pool:
1. Constructs seed: `seed_l ⊕ slot ⊕ epoch_nonce`
2. Computes VRF: `vrf_output = VRF(pool_vrf_key, seed)`
3. Checks if `vrf_output < threshold` (based on stake)
4. If yes, pool is the leader and can produce a block

### 2. Epoch Nonce Evolution
To update randomness for the next epoch:
1. For each block in current epoch:
   - Construct seed: `seed_eta ⊕ slot ⊕ epoch_nonce`
   - Compute VRF: `vrf_output = VRF(pool_vrf_key, seed)`
   - Mix into evolving nonce
2. At epoch boundary, finalize new epoch nonce

## Why XOR with Universal Constants?

The XOR operation with `seedEta` or `seedL` provides **domain separation**:

- Prevents using a VRF proof from leader election for randomness generation (or vice versa)
- Even with the same slot and epoch nonce, the seeds are different
- Security property: cannot reuse VRF proofs across different contexts

## Cryptographic Properties

1. **Deterministic**: Same inputs always produce same seed
2. **Unpredictable**: Cannot predict seed without knowing epoch nonce
3. **Uniform**: Seeds are uniformly distributed
4. **Domain Separated**: seedEta and seedL produce different seeds
5. **Collision Resistant**: Different slots/epochs produce different seeds

## Compilation

### Prerequisites
```bash
# Add to Cargo.toml
[dependencies]
blake2 = "0.10"
```

### Build and Test
```bash
# Run tests
cargo test --lib

# Run example
cargo run --bin vrf_seed_example

# Build library only
cargo build --lib
```

## File Structure

- **`vrf_seed.rs`** - Main library implementation
- **`vrf_seed_example.rs`** - Usage examples
- **`Cargo.toml`** - Rust project configuration
- **`VRF_SEED_EXPLANATION.md`** - This documentation

## Technical Notes

### Haskell vs Rust Differences

1. **Byte Serialization**:
   - Haskell: Uses `ByteString.Builder` with `word64BE`
   - Rust: Uses `to_be_bytes()` method

2. **Hashing**:
   - Haskell: `Crypto.Hash` from `cryptonite`
   - Rust: `blake2` crate

3. **Type System**:
   - Haskell: Phantom types for hash discrimination
   - Rust: Generic types with trait bounds

4. **Memory**:
   - Haskell: Lazy evaluation, immutable
   - Rust: Eager evaluation, ownership system

### Performance

- **Hashing**: ~1-2 μs per seed generation
- **Memory**: 72 bytes per seed (8 + 32 + 32)
- **Allocations**: Minimal (reuses buffers in production)

## References

1. **Ouroboros Praos Paper**: [IOHK Research Library](https://iohk.io/en/research/library/)
2. **Cardano Ledger Specs**: `eras/shelley/formal-spec/chain.tex`
3. **Original Implementation**: `Cardano.Protocol.TPraos.BHeader`
4. **VRF Specification**: [IETF Draft](https://datatracker.ietf.org/doc/html/draft-irtf-cfrg-vrf)

## See Also

- `overlay_schedule.rs` - Overlay schedule lookup (BFT slots)
- `Cardano.Protocol.TPraos.Rules.Overlay` - Haskell overlay implementation
- TPraos consensus documentation in cardano-ledger repository

