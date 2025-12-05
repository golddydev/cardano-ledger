# VRF Leader Election Validation

## Overview

This module implements the **Praos leader election check**, which determines whether a stake pool is eligible to produce a block in a given slot. This is a core component of the Cardano consensus protocol.

## Purpose

In the Ouroboros Praos protocol, slot leadership is determined probabilistically based on:
1. A verifiable random function (VRF) output - provides randomness
2. The stake pool's relative stake (σ) - higher stake = higher probability
3. The active slot coefficient (f) - controls block density

## The Leader Election Formula

A stake pool is eligible to produce a block if:

```
p < 1 - (1 - f)^σ
```

Where:
- **p** = VRF output interpreted as a probability (cert_nat / cert_nat_max)
- **f** = Active slot coefficient (probability that any slot has a leader)
- **σ** (sigma) = Stake proportion of the pool (0 ≤ σ ≤ 1)

### Intuition

- The right side `1 - (1 - f)^σ` represents the threshold probability
- Pools with higher stake (σ) have a higher threshold, making them more likely to be leaders
- The VRF output (p) acts as a random "dice roll"
- If the dice roll is below the threshold, the pool wins leadership

## Mathematical Optimization

Computing `(1 - f)^σ` directly is problematic for:
- Very small values of (1 - f)
- Non-integer exponents σ
- Numerical precision issues

### The Transformation

We use an algebraic transformation to avoid these issues:

```
p < 1 - (1 - f)^σ
```

Let q = 1 - p and c = ln(1 - f):

```
⟺  1 - p < 1 - (1 - f)^σ
⟺  q < (1 - f)^σ
⟺  ln(q) < σ · ln(1 - f)
⟺  ln(q) < σ · c
⟺  ln(1/q) > -σ · c
⟺  1/q > exp(-σ · c)
```

Where `1/q = 1/(1-p) = cert_nat_max / (cert_nat_max - cert_nat)`

## Implementation Details

### Key Components

1. **`check_leader_value`**: Main entry point
   - Takes VRF output as bytes
   - Converts to bounded natural number
   - Delegates to check_leader_nat_value

2. **`check_leader_nat_value`**: Core logic
   - Computes `recip_q = cert_nat_max / (cert_nat_max - cert_nat)`
   - Computes `x = -σ · ln(1 - f)`
   - Checks if `recip_q < exp(x)` using Taylor expansion

3. **`taylor_exp_cmp`**: Efficient comparison
   - Computes exp(x) using Taylor series: `exp(x) = 1 + x + x²/2! + x³/3! + ...`
   - Uses error bounds to stop early
   - Returns BELOW if `recip_q < exp(x)` (IS a leader)
   - Returns ABOVE if `recip_q ≥ exp(x)` (NOT a leader)

### Special Cases

1. **f = 1.0 (Active slot coefficient at maximum)**
   - In this degenerate case, ln(1 - f) is undefined
   - Every pool is always eligible (returns true)
   - Used primarily for testing

2. **cert_nat = cert_nat_max (VRF output at maximum)**
   - Would result in division by zero
   - Treated as recip_q = infinity
   - Pool is never eligible (returns false)

## Algorithm Steps

### Step 1: Convert VRF Output
```rust
// VRF output bytes → BigUint (big-endian)
let cert_nat = BigUint::from_bytes_be(vrf_output);

// Maximum value based on output size
let cert_nat_max = 2^(8 * vrf_output.len());
```

### Step 2: Check for Special Cases
```rust
if f == 1.0 {
    return true;  // Always a leader
}

if cert_nat == cert_nat_max {
    return false;  // Never a leader
}
```

### Step 3: Compute Comparison Values
```rust
// c = ln(1 - f), precomputed in ActiveSlotCoeff
let c = active_slot_coeff.log_value;

// recip_q = cert_nat_max / (cert_nat_max - cert_nat)
let recip_q = cert_nat_max / (cert_nat_max - cert_nat);

// x = -σ · c
let x = -sigma * c;
```

### Step 4: Taylor Expansion Comparison
```rust
match taylor_exp_cmp(3, recip_q, x) {
    CompareResult::Below => true,   // recip_q < exp(x) → IS leader
    CompareResult::Above => false,  // recip_q ≥ exp(x) → NOT leader
    CompareResult::MaxReached => false,  // Inconclusive → NOT leader
}
```

## Taylor Expansion Details

The `taylor_exp_cmp` function uses the Taylor series:

```
exp(x) = Σ(n=0 to ∞) x^n / n!
       = 1 + x + x²/2! + x³/3! + x⁴/4! + ...
```

### Error Estimation

At each iteration n, we track:
- **acc**: Current approximation of exp(x)
- **err**: Next term in the series (x^n / n!)
- **error_term**: Bound on remaining series error

The algorithm stops when:
1. `cmp ≥ acc + error_term` → Conclusively ABOVE
2. `cmp < acc - error_term` → Conclusively BELOW
3. Max iterations reached → Inconclusive (MaxReached)

### Why This Works

The error term provides a conservative bound. If the comparison value is far enough from the approximation (outside the error bounds), we can conclude the result without computing more terms.

## Example Scenarios

### Scenario 1: High Stake, Low VRF Output
- σ = 0.5 (50% stake)
- VRF output = 0x0000...0001 (very low)
- f = 0.05 (5% active slot coefficient)

**Result**: Likely a leader (low VRF output means small p, high stake means large threshold)

### Scenario 2: Low Stake, High VRF Output
- σ = 0.001 (0.1% stake)
- VRF output = 0xFFFF...FFFF (very high)
- f = 0.05

**Result**: Not a leader (high VRF output means large p, low stake means small threshold)

### Scenario 3: Maximum Active Slot Coefficient
- f = 1.0
- Any σ, any VRF output

**Result**: Always a leader (degenerate test case)

## Usage Example

```rust
use num_rational::Rational64;

// VRF output from the VRF proof
let vrf_output: &[u8] = &[...]; // 32 bytes from VRF

// Pool has 5% of total stake
let sigma = Rational64::new(5, 100);

// Active slot coefficient of 0.05 (5% of slots have leaders on average)
let active_slot_coeff = ActiveSlotCoeff::new(0.05);

// Check if this pool is eligible to produce a block
let is_leader = check_leader_value(vrf_output, sigma, &active_slot_coeff);

if is_leader {
    println!("This pool can produce a block!");
} else {
    println!("This pool cannot produce a block in this slot.");
}
```

## Dependencies

```toml
[dependencies]
num-bigint = "0.4"
num-rational = "0.4"
num-traits = "0.2"
```

## References

- **Ouroboros Praos Paper**: "Ouroboros Praos: An adaptively-secure, semi-synchronous proof-of-stake protocol"
- **Cardano Ledger Specs**: https://github.com/IntersectMBO/cardano-ledger
- **Original Haskell Implementation**: `Cardano.Protocol.TPraos.BHeader.checkLeaderValue`

## Notes

- The bound parameter (3) in `taylor_exp_cmp` is empirically chosen for good performance
- For negative exponents (x < 0), exp(x) < 1, which helps with convergence
- The algorithm is designed to be constant-time with respect to sensitive values for security
