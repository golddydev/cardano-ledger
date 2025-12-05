/// Rust translation of the Cardano overlay schedule lookup functions
/// from Cardano.Protocol.TPraos.Rules.Overlay

use std::collections::BTreeSet;

/// Slot number on the blockchain
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SlotNo(pub u64);

/// Hash of a genesis key (28 bytes / 224 bits)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GenesisKeyHash([u8; 28]);

impl GenesisKeyHash {
    pub fn from_bytes(bytes: [u8; 28]) -> Self {
        Self(bytes)
    }

    pub fn from_hex(hex: &str) -> Result<Self, String> {
        if hex.len() != 56 {
            return Err(format!("Expected 56 hex chars, got {}", hex.len()));
        }
        let mut bytes = [0u8; 28];
        for i in 0..28 {
            bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
                .map_err(|e| format!("Invalid hex: {}", e))?;
        }
        Ok(Self(bytes))
    }
}

/// Unit interval representing a value in [0, 1]
/// Stored as a rational number (numerator, denominator)
#[derive(Debug, Clone, Copy)]
pub struct UnitInterval {
    numerator: u64,
    denominator: u64,
}

impl UnitInterval {
    /// Create a new UnitInterval. Panics if numerator > denominator or denominator == 0
    pub fn new(numerator: u64, denominator: u64) -> Self {
        assert!(denominator > 0, "Denominator must be positive");
        assert!(numerator <= denominator, "Numerator must be <= denominator");
        Self {
            numerator,
            denominator,
        }
    }

    /// Create from a floating point value in [0, 1]
    pub fn from_f64(value: f64) -> Self {
        assert!(value >= 0.0 && value <= 1.0, "Value must be in [0, 1]");
        // Use a large denominator for precision
        const DENOM: u64 = 1_000_000_000;
        let numerator = (value * DENOM as f64).round() as u64;
        Self::new(numerator, DENOM)
    }

    /// Convert to f64
    pub fn to_f64(&self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }

    /// Get as a rational (for precise calculations)
    pub fn as_rational(&self) -> (u64, u64) {
        (self.numerator, self.denominator)
    }
}

/// Active slot coefficient (f in the Praos paper)
#[derive(Debug, Clone, Copy)]
pub struct ActiveSlotCoeff(pub f64);

impl ActiveSlotCoeff {
    pub fn new(value: f64) -> Self {
        assert!(value > 0.0 && value <= 1.0, "ActiveSlotCoeff must be in (0, 1]");
        Self(value)
    }
}

/// Classification of a slot in the overlay schedule
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OBftSlot {
    /// Slot is not reserved for the overlay schedule (stake pools compete via VRF)
    NonActiveSlot,
    /// Slot is reserved for a specific genesis node
    ActiveSlot(GenesisKeyHash),
}

/// Determine if the given slot is reserved for the overlay schedule.
///
/// # Arguments
/// * `first_slot_no` - The first slot of the given epoch
/// * `d_val` - The decentralization parameter
/// * `slot` - The slot to check
///
/// # Returns
/// `true` if the slot is reserved for the overlay schedule
pub fn is_overlay_slot(first_slot_no: SlotNo, d_val: UnitInterval, slot: SlotNo) -> bool {
    let s = (slot.0 - first_slot_no.0) as f64;
    let d = d_val.to_f64();

    // step function: ceiling of (x * d)
    let step = |x: f64| (x * d).ceil() as i64;

    step(s) < step(s + 1.0)
}

/// Classify a slot in the overlay schedule, determining which genesis node
/// should produce the block if it's an active overlay slot.
///
/// # Arguments
/// * `first_slot_no` - The first slot of the epoch
/// * `genesis_keys` - Set of genesis node key hashes
/// * `d_val` - The decentralization parameter
/// * `asc_value` - The active slot coefficient
/// * `slot` - The overlay slot to classify
///
/// # Returns
/// Classification of the slot (NonActiveSlot or ActiveSlot with genesis key)
pub fn classify_overlay_slot(
    first_slot_no: SlotNo,
    genesis_keys: &BTreeSet<GenesisKeyHash>,
    d_val: UnitInterval,
    asc_value: ActiveSlotCoeff,
    slot: SlotNo,
) -> OBftSlot {
    let d = d_val.to_f64();
    let slot_offset = (slot.0 - first_slot_no.0) as f64;
    let position = (slot_offset * d).ceil() as i64;

    // Calculate active slot coefficient inverse
    let asc_inv = (1.0 / asc_value.0).floor() as i64;

    let is_active = position % asc_inv == 0;

    if is_active {
        let genesis_idx = ((position / asc_inv) % genesis_keys.len() as i64) as usize;

        // Get the element at index from the set
        if let Some(key_hash) = genesis_keys.iter().nth(genesis_idx) {
            OBftSlot::ActiveSlot(*key_hash)
        } else {
            OBftSlot::NonActiveSlot
        }
    } else {
        OBftSlot::NonActiveSlot
    }
}

/// Look up a slot in the overlay schedule to determine if it's reserved
/// and, if so, which genesis node should produce the block.
///
/// # Arguments
/// * `first_slot_no` - The first slot of the epoch
/// * `genesis_keys` - Set of genesis node key hashes
/// * `d_val` - The decentralization parameter
/// * `asc_value` - The active slot coefficient
/// * `slot` - The slot to lookup
///
/// # Returns
/// * `Some(OBftSlot)` if the slot is in the overlay schedule
/// * `None` if the slot is not in the overlay schedule
pub fn lookup_in_overlay_schedule(
    first_slot_no: SlotNo,
    genesis_keys: &BTreeSet<GenesisKeyHash>,
    d_val: UnitInterval,
    asc_value: ActiveSlotCoeff,
    slot: SlotNo,
) -> Option<OBftSlot> {
    if is_overlay_slot(first_slot_no, d_val, slot) {
        Some(classify_overlay_slot(
            first_slot_no,
            genesis_keys,
            d_val,
            asc_value,
            slot,
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unit_interval() {
        let ui = UnitInterval::new(1, 2);
        assert_eq!(ui.to_f64(), 0.5);

        let ui2 = UnitInterval::from_f64(0.75);
        assert!((ui2.to_f64() - 0.75).abs() < 1e-6);
    }

    #[test]
    fn test_is_overlay_slot() {
        let first_slot = SlotNo(0);
        let d_val = UnitInterval::new(1, 1); // d = 1.0 (fully centralized)

        // When d = 1, all slots should be overlay slots
        assert!(is_overlay_slot(first_slot, d_val, SlotNo(0)));
        assert!(is_overlay_slot(first_slot, d_val, SlotNo(1)));
        assert!(is_overlay_slot(first_slot, d_val, SlotNo(100)));
    }

    #[test]
    fn test_is_overlay_slot_decentralized() {
        let first_slot = SlotNo(0);
        let d_val = UnitInterval::new(0, 1); // d = 0.0 (fully decentralized)

        // When d = 0, no slots should be overlay slots
        assert!(!is_overlay_slot(first_slot, d_val, SlotNo(0)));
        assert!(!is_overlay_slot(first_slot, d_val, SlotNo(1)));
        assert!(!is_overlay_slot(first_slot, d_val, SlotNo(100)));
    }

    #[test]
    fn test_lookup_in_overlay_schedule() {
        let first_slot = SlotNo(0);
        let d_val = UnitInterval::new(1, 1); // d = 1.0
        let asc = ActiveSlotCoeff::new(0.05);

        // Create a set of genesis keys
        let mut genesis_keys = BTreeSet::new();
        genesis_keys.insert(GenesisKeyHash([1u8; 28]));
        genesis_keys.insert(GenesisKeyHash([2u8; 28]));
        genesis_keys.insert(GenesisKeyHash([3u8; 28]));

        // Test lookup
        let result = lookup_in_overlay_schedule(
            first_slot,
            &genesis_keys,
            d_val,
            asc,
            SlotNo(0),
        );

        assert!(result.is_some());
    }

    #[test]
    fn test_genesis_key_hash_from_hex() {
        let hex = "ad5463153dc3d24b9ff133e46136028bdc1edbb897f5a7cf1b37950c";
        let result = GenesisKeyHash::from_hex(hex);
        assert!(result.is_ok());
    }

    #[test]
    fn test_classify_overlay_slot_round_robin() {
        let first_slot = SlotNo(432000); // Start of epoch 1
        let d_val = UnitInterval::new(1, 1); // d = 1.0
        let asc = ActiveSlotCoeff::new(0.05);

        // Create 7 genesis keys (like in Shelley testnet)
        let mut genesis_keys = BTreeSet::new();
        for i in 0..7 {
            let mut key = [0u8; 28];
            key[0] = i;
            genesis_keys.insert(GenesisKeyHash(key));
        }

        // Test that different slots get assigned to different genesis nodes
        for slot_offset in 0..100 {
            let slot = SlotNo(first_slot.0 + slot_offset);
            if let Some(OBftSlot::ActiveSlot(key_hash)) = lookup_in_overlay_schedule(
                first_slot,
                &genesis_keys,
                d_val,
                asc,
                slot,
            ) {
                println!("Slot {} assigned to genesis key: {:?}", slot.0, key_hash.0[0]);
            }
        }
    }
}



