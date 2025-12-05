/// Rust implementation of VRF seed construction from Cardano TPraos
/// Original: Cardano.Protocol.TPraos.BHeader.mkSeed

use blake2::{Blake2b256, Digest};

/// A 32-byte hash (Blake2b_256)
pub type Hash32 = [u8; 32];

/// Slot number on the blockchain
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotNo(pub u64);

/// Nonce - either contains a hash or is neutral (identity element)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Nonce {
    /// Nonce containing a hash value
    Nonce(Hash32),
    /// Neutral/identity nonce (no value)
    NeutralNonce,
}

impl Nonce {
    /// Create a nonce from a number (for universal constants)
    pub fn from_number(n: u64) -> Self {
        let mut hasher = Blake2b256::new();
        hasher.update(&n.to_be_bytes());
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        Nonce::Nonce(hash)
    }

    /// Create neutral nonce
    pub fn neutral() -> Self {
        Nonce::NeutralNonce
    }

    /// Get the hash bytes if not neutral
    pub fn as_bytes(&self) -> Option<&[u8; 32]> {
        match self {
            Nonce::Nonce(h) => Some(h),
            Nonce::NeutralNonce => None,
        }
    }
}

/// Seed for VRF computation (wrapped Blake2b_256 hash)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seed(pub Hash32);

impl Seed {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Universal constant nonces used as domain separators in VRF computation
pub mod universal_constants {
    use super::Nonce;

    /// Seed constant for eta (randomness/entropy) computation
    /// Used when generating the epoch nonce
    pub fn seed_eta() -> Nonce {
        Nonce::from_number(0)
    }

    /// Seed constant for leader (L) computation  
    /// Used when determining if a stake pool is the slot leader
    pub fn seed_l() -> Nonce {
        Nonce::from_number(1)
    }
}

/// XOR two 32-byte hashes
fn xor_hash(a: &Hash32, b: &Hash32) -> Hash32 {
    let mut result = [0u8; 32];
    for i in 0..32 {
        result[i] = a[i] ^ b[i];
    }
    result
}

/// Construct a seed to use in the VRF computation.
///
/// This seed is used for VRF proofs in the Praos consensus protocol.
/// It combines the slot number and epoch nonce, optionally with a
/// universal constant for domain separation.
///
/// # Arguments
///
/// * `uc_nonce` - Universal constant nonce (domain separator)
///   - Use `seed_eta()` for randomness/eta computation
///   - Use `seed_l()` for leader election computation  
///   - Use `NeutralNonce` for no domain separation
/// * `slot` - The slot number
/// * `e_nonce` - The epoch nonce (randomness from the epoch)
///
/// # Returns
///
/// A `Seed` that can be used for VRF computation
///
/// # Algorithm
///
/// 1. Serialize slot number as 8-byte big-endian u64
/// 2. Append epoch nonce hash (32 bytes) if not neutral
/// 3. Hash the concatenated bytes with Blake2b_256
/// 4. XOR with universal constant if provided
/// 5. Wrap result in Seed type
pub fn mk_seed(uc_nonce: &Nonce, slot: SlotNo, e_nonce: &Nonce) -> Seed {
    // Step 1 & 2: Build the byte buffer
    // 8 bytes for slot + optionally 32 bytes for epoch nonce
    let mut buffer = Vec::with_capacity(8 + 32);
    
    // Add slot number as big-endian u64 (8 bytes)
    buffer.extend_from_slice(&slot.0.to_be_bytes());
    
    // Add epoch nonce hash if not neutral (32 bytes)
    if let Some(e_hash) = e_nonce.as_bytes() {
        buffer.extend_from_slice(e_hash);
    }
    
    // Step 3: Hash the buffer with Blake2b_256
    let mut hasher = Blake2b256::new();
    hasher.update(&buffer);
    let hash_result = hasher.finalize();
    
    let mut seed_hash = [0u8; 32];
    seed_hash.copy_from_slice(&hash_result);
    
    // Step 4: XOR with universal constant if provided
    let final_hash = match uc_nonce {
        Nonce::NeutralNonce => seed_hash,
        Nonce::Nonce(uc_hash) => xor_hash(&seed_hash, uc_hash),
    };
    
    // Step 5: Wrap in Seed
    Seed(final_hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::universal_constants::*;

    #[test]
    fn test_nonce_creation() {
        let nonce = Nonce::from_number(42);
        assert!(matches!(nonce, Nonce::Nonce(_)));
        
        let neutral = Nonce::neutral();
        assert!(matches!(neutral, Nonce::NeutralNonce));
    }

    #[test]
    fn test_universal_constants() {
        let eta = seed_eta();
        let l = seed_l();
        
        // They should be different
        assert_ne!(eta, l);
        
        // Both should be non-neutral
        assert!(matches!(eta, Nonce::Nonce(_)));
        assert!(matches!(l, Nonce::Nonce(_)));
    }

    #[test]
    fn test_mk_seed_with_neutral_nonces() {
        let slot = SlotNo(12345);
        let uc_nonce = Nonce::neutral();
        let e_nonce = Nonce::neutral();
        
        let seed = mk_seed(&uc_nonce, slot, &e_nonce);
        
        // Should produce a valid 32-byte seed
        assert_eq!(seed.as_bytes().len(), 32);
    }

    #[test]
    fn test_mk_seed_with_epoch_nonce() {
        let slot = SlotNo(12345);
        let uc_nonce = Nonce::neutral();
        let e_nonce = Nonce::from_number(100);
        
        let seed = mk_seed(&uc_nonce, slot, &e_nonce);
        
        // Should produce a valid 32-byte seed
        assert_eq!(seed.as_bytes().len(), 32);
    }

    #[test]
    fn test_mk_seed_with_universal_constant() {
        let slot = SlotNo(12345);
        let uc_nonce = seed_eta(); // Use eta constant
        let e_nonce = Nonce::from_number(100);
        
        let seed = mk_seed(&uc_nonce, slot, &e_nonce);
        
        // Should produce a valid 32-byte seed
        assert_eq!(seed.as_bytes().len(), 32);
    }

    #[test]
    fn test_mk_seed_deterministic() {
        let slot = SlotNo(12345);
        let uc_nonce = seed_l();
        let e_nonce = Nonce::from_number(100);
        
        let seed1 = mk_seed(&uc_nonce, slot, &e_nonce);
        let seed2 = mk_seed(&uc_nonce, slot, &e_nonce);
        
        // Same inputs should produce same output
        assert_eq!(seed1, seed2);
    }

    #[test]
    fn test_mk_seed_different_for_different_slots() {
        let slot1 = SlotNo(12345);
        let slot2 = SlotNo(12346);
        let uc_nonce = seed_l();
        let e_nonce = Nonce::from_number(100);
        
        let seed1 = mk_seed(&uc_nonce, slot1, &e_nonce);
        let seed2 = mk_seed(&uc_nonce, slot2, &e_nonce);
        
        // Different slots should produce different seeds
        assert_ne!(seed1, seed2);
    }

    #[test]
    fn test_mk_seed_different_for_different_constants() {
        let slot = SlotNo(12345);
        let e_nonce = Nonce::from_number(100);
        
        let seed_with_eta = mk_seed(&seed_eta(), slot, &e_nonce);
        let seed_with_l = mk_seed(&seed_l(), slot, &e_nonce);
        
        // Different universal constants should produce different seeds
        assert_ne!(seed_with_eta, seed_with_l);
    }

    #[test]
    fn test_xor_hash() {
        let hash1 = [0xFFu8; 32];
        let hash2 = [0x00u8; 32];
        let result = xor_hash(&hash1, &hash2);
        
        assert_eq!(result, hash1);
        
        let hash3 = [0xAAu8; 32];
        let hash4 = [0x55u8; 32];
        let result2 = xor_hash(&hash3, &hash4);
        
        assert_eq!(result2, [0xFFu8; 32]);
    }

    #[test]
    fn test_seed_size_with_neutral_epoch_nonce() {
        // When epoch nonce is neutral, buffer should be 8 bytes (just slot)
        let slot = SlotNo(12345);
        let seed = mk_seed(&Nonce::neutral(), slot, &Nonce::neutral());
        
        // Result should still be 32 bytes (hash output size)
        assert_eq!(seed.as_bytes().len(), 32);
    }

    #[test]
    fn test_seed_size_with_epoch_nonce() {
        // When epoch nonce is present, buffer should be 40 bytes (8 + 32)
        let slot = SlotNo(12345);
        let e_nonce = Nonce::from_number(100);
        let seed = mk_seed(&Nonce::neutral(), slot, &e_nonce);
        
        // Result should be 32 bytes (hash output size)
        assert_eq!(seed.as_bytes().len(), 32);
    }
}

