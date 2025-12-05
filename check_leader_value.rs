//! VRF Leader Election Validation
//!
//! This module implements the Praos leader election check, which determines
//! whether a stake pool is eligible to produce a block in a given slot based
//! on their VRF output and stake proportion.

use num_bigint::BigUint;
use num_rational::Rational64;
use num_traits::{ToPrimitive, Zero, One};

/// Result of comparing a value to the Taylor expansion of an exponential function
#[derive(Debug, Clone, PartialEq)]
pub enum CompareResult {
    /// The reference value is below the exponential
    Below { approximation: f64, iterations: usize },
    /// The reference value is above the exponential
    Above { approximation: f64, iterations: usize },
    /// Maximum iterations reached without conclusive result
    MaxReached { iterations: usize },
}

/// A natural number bounded by a maximum value
#[derive(Debug, Clone)]
pub struct BoundedNatural {
    /// Maximum bound (inclusive)
    pub max_value: BigUint,
    /// Actual value (must be <= max_value)
    pub value: BigUint,
}

impl BoundedNatural {
    /// Create a new bounded natural, panicking if value > max_value
    pub fn new(max_value: BigUint, value: BigUint) -> Self {
        assert!(
            value <= max_value,
            "Value {} exceeds maximum {}",
            value,
            max_value
        );
        BoundedNatural { max_value, value }
    }
}

/// Active slot coefficient with its logarithm
///
/// Represents the parameter 'f' in the Praos protocol, which is the probability
/// that any given slot will have at least one eligible block producer.
#[derive(Debug, Clone)]
pub struct ActiveSlotCoeff {
    /// The active slot coefficient value (must be in range (0, 1])
    pub value: f64,
    /// Natural logarithm of (1 - value), used in the leader check calculation
    /// For value = 1.0, this is undefined, so we use 0.0 as a special marker
    pub log_value: f64,
}

impl ActiveSlotCoeff {
    /// Create a new active slot coefficient
    ///
    /// # Arguments
    /// * `value` - The active slot coefficient, must be in (0, 1]
    ///
    /// # Panics
    /// Panics if value is not in the valid range (0, 1]
    pub fn new(value: f64) -> Self {
        assert!(
            value > 0.0 && value <= 1.0,
            "Active slot coefficient must be in (0, 1], got {}",
            value
        );

        let log_value = if (value - 1.0).abs() < f64::EPSILON {
            // Special case: when f = 1, ln(1-f) is undefined
            // We use 0.0 as a marker for this degenerate case
            0.0
        } else {
            (1.0 - value).ln()
        };

        ActiveSlotCoeff { value, log_value }
    }

    /// Check if this is the degenerate case (f = 1.0)
    pub fn is_max(&self) -> bool {
        (self.value - 1.0).abs() < f64::EPSILON
    }
}

/// Compare a reference value to exp(x) using Taylor expansion
///
/// Computes the Taylor series expansion of exp(x) and compares it to `cmp`,
/// stopping early when the error bounds allow a conclusive determination.
///
/// # Arguments
/// * `bound_x` - Bound on the absolute error term (typically 3 for the Praos check)
/// * `cmp` - The reference value to compare against exp(x)
/// * `x` - The exponent
///
/// # Returns
/// * `Below` if cmp < exp(x)
/// * `Above` if cmp >= exp(x)
/// * `MaxReached` if inconclusive after max iterations
///
/// # Algorithm
/// Uses the Taylor series: exp(x) = 1 + x + x²/2! + x³/3! + ...
/// Tracks error bounds to stop early when the comparison result is certain.
pub fn taylor_exp_cmp(bound_x: usize, cmp: f64, x: f64) -> CompareResult {
    const MAX_ITERATIONS: usize = 1000;

    let mut n = 0;
    let mut err = x;
    let mut acc = 1.0;
    let mut divisor = 1.0;

    while n < MAX_ITERATIONS {
        let next_x = err;
        let acc_prime = acc + next_x;
        let divisor_prime = divisor + 1.0;
        let err_prime = (err * x) / divisor_prime;

        // Error bound for the Taylor series approximation
        let error_term = (err_prime * bound_x as f64).abs();

        // Check if we can conclusively determine the comparison
        if cmp >= acc_prime + error_term {
            return CompareResult::Above {
                approximation: acc_prime,
                iterations: n + 1,
            };
        }

        if cmp < acc_prime - error_term {
            return CompareResult::Below {
                approximation: acc_prime,
                iterations: n + 1,
            };
        }

        // Continue iteration
        n += 1;
        err = err_prime;
        acc = acc_prime;
        divisor = divisor_prime;
    }

    CompareResult::MaxReached {
        iterations: MAX_ITERATIONS,
    }
}

/// Check if a certified VRF output indicates slot leadership eligibility
///
/// This is the core Praos leader election check. Given:
/// - A VRF output value (as a bounded natural)
/// - The stake proportion σ (sigma) of the stake pool
/// - The active slot coefficient f
///
/// Determines if: p < 1 - (1 - f)^σ
///
/// where p = cert_nat / cert_nat_max is the "randomness" derived from the VRF.
///
/// # Arguments
/// * `bounded_nat` - The VRF output as a bounded natural number
/// * `sigma` - Stake proportion of the pool (rational in [0, 1])
/// * `active_slot_coeff` - Active slot coefficient f
///
/// # Returns
/// `true` if the pool is eligible to produce a block, `false` otherwise
///
/// # Algorithm
/// Uses the optimization:
/// - Let q = 1 - p and c = ln(1 - f)
/// - Then: p < 1 - (1 - f)^σ  ⟺  1/q < exp(-σ · c)
/// - Where: 1/q = cert_nat_max / (cert_nat_max - cert_nat)
///
/// This avoids computing large powers and uses efficient Taylor expansion comparison.
pub fn check_leader_nat_value(
    bounded_nat: &BoundedNatural,
    sigma: Rational64,
    active_slot_coeff: &ActiveSlotCoeff,
) -> bool {
    // Special case: if active slot coefficient is 1, always return true
    // This is a degenerate case for testing where every pool can produce blocks
    if active_slot_coeff.is_max() {
        return true;
    }

    let cert_nat_max = &bounded_nat.max_value;
    let cert_nat = &bounded_nat.value;

    // Calculate c = ln(1 - f)
    let c = active_slot_coeff.log_value;

    // Calculate recip_q = cert_nat_max / (cert_nat_max - cert_nat)
    let denominator = cert_nat_max - cert_nat;

    let recip_q = if denominator.is_zero() {
        // Edge case: cert_nat = cert_nat_max, so p = 1
        // This means recip_q would be infinite (1/0)
        // In practice this should never be a leader
        cert_nat_max.to_f64().unwrap_or(f64::MAX)
    } else {
        // recip_q = cert_nat_max / (cert_nat_max - cert_nat)
        let numerator = cert_nat_max.to_f64().unwrap_or(f64::MAX);
        let denom = denominator.to_f64().unwrap_or(f64::MIN_POSITIVE);
        numerator / denom
    };

    // Calculate x = -σ · c
    let sigma_f64 = (*sigma.numer() as f64) / (*sigma.denom() as f64);
    let x = -sigma_f64 * c;

    // Compare: recip_q < exp(x) ?
    // If BELOW, then recip_q < exp(x), which means the pool IS a leader
    // If ABOVE or MaxReached, the pool is NOT a leader
    match taylor_exp_cmp(3, recip_q, x) {
        CompareResult::Below { .. } => true,
        CompareResult::Above { .. } => false,
        CompareResult::MaxReached { .. } => false,
    }
}

/// Check if a VRF output indicates slot leadership eligibility
///
/// High-level wrapper that converts a VRF output byte array to a bounded natural
/// and checks for leader eligibility.
///
/// # Arguments
/// * `vrf_output` - The VRF output as a byte array
/// * `sigma` - Stake proportion of the pool (rational in [0, 1])
/// * `active_slot_coeff` - Active slot coefficient f
///
/// # Returns
/// `true` if the pool is eligible to produce a block, `false` otherwise
///
/// # Algorithm
/// 1. Converts VRF output bytes to a natural number (big-endian)
/// 2. Calculates cert_nat_max = 2^(8 * output_size_bytes)
/// 3. Creates a bounded natural and calls check_leader_nat_value
pub fn check_leader_value(
    vrf_output: &[u8],
    sigma: Rational64,
    active_slot_coeff: &ActiveSlotCoeff,
) -> bool {
    // Convert VRF output to natural number (big-endian)
    let cert_nat = BigUint::from_bytes_be(vrf_output);

    // Calculate maximum value: 2^(8 * size_in_bytes)
    let output_size_bits = vrf_output.len() * 8;
    let cert_nat_max = BigUint::one() << output_size_bits;

    // Create bounded natural
    let bounded_nat = BoundedNatural::new(cert_nat_max, cert_nat);

    // Check leader value
    check_leader_nat_value(&bounded_nat, sigma, active_slot_coeff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_taylor_exp_cmp_basic() {
        // Test exp(0) = 1
        let result = taylor_exp_cmp(3, 1.0, 0.0);
        match result {
            CompareResult::Below { .. } => {}
            _ => panic!("Expected Below for cmp=1.0, x=0.0"),
        }
    }

    #[test]
    fn test_active_slot_coeff_normal() {
        let asc = ActiveSlotCoeff::new(0.05);
        assert!(!asc.is_max());
        assert!(asc.log_value < 0.0); // ln(1 - 0.05) = ln(0.95) < 0
    }

    #[test]
    fn test_active_slot_coeff_max() {
        let asc = ActiveSlotCoeff::new(1.0);
        assert!(asc.is_max());
        assert_eq!(asc.log_value, 0.0);
    }

    #[test]
    fn test_check_leader_value_degenerate_case() {
        // When f = 1.0, should always be leader
        let vrf_output = vec![0x42; 32];
        let sigma = Rational64::new(1, 10); // 10% stake
        let asc = ActiveSlotCoeff::new(1.0);

        assert!(check_leader_value(&vrf_output, sigma, &asc));
    }

    #[test]
    fn test_check_leader_value_low_vrf_high_stake() {
        // Low VRF output + high stake = likely leader
        let vrf_output = vec![0x00; 31].into_iter().chain(vec![0x01]).collect::<Vec<_>>();
        let sigma = Rational64::new(5, 10); // 50% stake
        let asc = ActiveSlotCoeff::new(0.05);

        // With very low VRF output and high stake, should be leader
        assert!(check_leader_value(&vrf_output, sigma, &asc));
    }

    #[test]
    fn test_check_leader_value_high_vrf_low_stake() {
        // High VRF output + low stake = unlikely leader
        let vrf_output = vec![0xFF; 32];
        let sigma = Rational64::new(1, 1000); // 0.1% stake
        let asc = ActiveSlotCoeff::new(0.05);

        // With very high VRF output and low stake, should not be leader
        assert!(!check_leader_value(&vrf_output, sigma, &asc));
    }

    #[test]
    fn test_bounded_natural() {
        let max = BigUint::from(1000u32);
        let val = BigUint::from(500u32);
        let bn = BoundedNatural::new(max.clone(), val.clone());

        assert_eq!(bn.max_value, max);
        assert_eq!(bn.value, val);
    }

    #[test]
    #[should_panic(expected = "exceeds maximum")]
    fn test_bounded_natural_exceeds_max() {
        let max = BigUint::from(100u32);
        let val = BigUint::from(200u32);
        BoundedNatural::new(max, val);
    }
}
