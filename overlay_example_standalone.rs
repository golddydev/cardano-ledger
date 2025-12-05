/// Standalone example with all code in one file for easy compilation

use std::collections::BTreeSet;

/// Slot number on the blockchain
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SlotNo(pub u64);

/// Hash of a genesis key (28 bytes / 224 bits)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GenesisKeyHash([u8; 28]);

impl GenesisKeyHash {
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
#[derive(Debug, Clone, Copy)]
pub struct UnitInterval {
    numerator: u64,
    denominator: u64,
}

impl UnitInterval {
    pub fn new(numerator: u64, denominator: u64) -> Self {
        assert!(denominator > 0, "Denominator must be positive");
        assert!(numerator <= denominator, "Numerator must be <= denominator");
        Self { numerator, denominator }
    }

    pub fn from_f64(value: f64) -> Self {
        assert!(value >= 0.0 && value <= 1.0, "Value must be in [0, 1]");
        const DENOM: u64 = 1_000_000_000;
        let numerator = (value * DENOM as f64).round() as u64;
        Self::new(numerator, DENOM)
    }

    pub fn to_f64(&self) -> f64 {
        self.numerator as f64 / self.denominator as f64
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
    NonActiveSlot,
    ActiveSlot(GenesisKeyHash),
}

/// Determine if the given slot is reserved for the overlay schedule.
pub fn is_overlay_slot(first_slot_no: SlotNo, d_val: UnitInterval, slot: SlotNo) -> bool {
    let s = (slot.0 - first_slot_no.0) as f64;
    let d = d_val.to_f64();
    let step = |x: f64| (x * d).ceil() as i64;
    step(s) < step(s + 1.0)
}

/// Classify a slot in the overlay schedule
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
    let asc_inv = (1.0 / asc_value.0).floor() as i64;
    let is_active = position % asc_inv == 0;

    if is_active {
        let genesis_idx = ((position / asc_inv) % genesis_keys.len() as i64) as usize;
        if let Some(key_hash) = genesis_keys.iter().nth(genesis_idx) {
            OBftSlot::ActiveSlot(*key_hash)
        } else {
            OBftSlot::NonActiveSlot
        }
    } else {
        OBftSlot::NonActiveSlot
    }
}

/// Look up a slot in the overlay schedule
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

fn main() {
    println!("=== Cardano Overlay Schedule Lookup (Rust) ===\n");

    // Shelley mainnet configuration
    let epoch_length = 432000;
    let first_slot_of_epoch_1 = SlotNo(epoch_length);
    let d_val = UnitInterval::new(1, 1); // d = 1.0 (fully centralized)
    let asc = ActiveSlotCoeff::new(0.05); // f = 0.05

    // Genesis keys from Shelley genesis JSON
    let genesis_key_hexs = vec![
        "ad5463153dc3d24b9ff133e46136028bdc1edbb897f5a7cf1b37950c",
        "b9547b8a57656539a8d9bc42c008e38d9c8bd9c8adbb1e73ad529497",
        "60baee25cbc90047e83fd01e1e57dc0b06d3d0cb150d0ab40bbfead1",
        "f7b341c14cd58fca4195a9b278cce1ef402dc0e06deb77e543cd1757",
        "162f94554ac8c225383a2248c245659eda870eaa82d0ef25fc7dcd82",
        "2075a095b3c844a29c24317a94a643ab8e22d54a3a3a72a420260af6",
        "268cfc0b89e910ead22e0ade91493d8212f53f3e2164b2e4bef0819b",
    ];

    let mut genesis_keys = BTreeSet::new();
    for hex in genesis_key_hexs {
        genesis_keys.insert(GenesisKeyHash::from_hex(hex).expect("Invalid genesis key hex"));
    }

    println!("Genesis nodes: {}", genesis_keys.len());
    println!("Decentralization (d): {}", d_val.to_f64());
    println!("Active slot coeff (f): {}\n", asc.0);

    println!("First 20 slots of epoch 1 (d=1.0, fully centralized):");
    println!("{:<10} {:<20}", "Slot", "Assigned Genesis Node");
    println!("{:-<35}", "");

    for offset in 0..20 {
        let slot = SlotNo(first_slot_of_epoch_1.0 + offset);
        match lookup_in_overlay_schedule(first_slot_of_epoch_1, &genesis_keys, d_val, asc, slot) {
            Some(OBftSlot::ActiveSlot(key_hash)) => {
                let index = genesis_keys.iter().position(|k| k == &key_hash).unwrap_or(999);
                println!("{:<10} Genesis #{}", slot.0, index);
            }
            Some(OBftSlot::NonActiveSlot) => {
                println!("{:<10} (Non-active slot)", slot.0);
            }
            None => {
                println!("{:<10} Any stake pool", slot.0);
            }
        }
    }

    println!("\n=== With d=0.5 (50% decentralized) ===\n");
    let d_val_half = UnitInterval::from_f64(0.5);

    let mut overlay = 0;
    let mut non_overlay = 0;

    for offset in 0..100 {
        let slot = SlotNo(first_slot_of_epoch_1.0 + offset);
        match lookup_in_overlay_schedule(first_slot_of_epoch_1, &genesis_keys, d_val_half, asc, slot) {
            Some(_) => overlay += 1,
            None => non_overlay += 1,
        }
    }

    println!("Out of 100 slots:");
    println!("  Overlay slots (genesis): {} ({:.0}%)", overlay, overlay as f64);
    println!("  Regular slots (pools):   {} ({:.0}%)", non_overlay, non_overlay as f64);
}



