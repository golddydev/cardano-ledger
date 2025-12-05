/// Example usage of the overlay schedule lookup functions

mod overlay_schedule;

use overlay_schedule::*;
use std::collections::BTreeSet;

fn main() {
    println!("=== Cardano Overlay Schedule Example ===\n");

    // Simulate Shelley mainnet genesis configuration
    let epoch_length = 432000; // slots per epoch
    let first_slot_of_epoch_1 = SlotNo(epoch_length);

    // Decentralization parameter (d = 1.0 means fully centralized)
    let d_val = UnitInterval::new(1, 1);

    // Active slot coefficient (f = 0.05 means 5% of slots can have blocks)
    let asc = ActiveSlotCoeff::new(0.05);

    // Create genesis keys from the example genesis JSON
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
        genesis_keys.insert(
            GenesisKeyHash::from_hex(hex).expect("Invalid genesis key hex")
        );
    }

    println!("Genesis nodes count: {}", genesis_keys.len());
    println!("Decentralization parameter (d): {}", d_val.to_f64());
    println!("Active slot coefficient (f): {}\n", asc.0);

    // Check first 50 slots of epoch 1
    println!("Checking first 50 slots of epoch 1:");
    println!("{:<10} {:<15} {:<20}", "Slot", "Overlay?", "Genesis Node");
    println!("{:-<50}", "");

    for offset in 0..50 {
        let slot = SlotNo(first_slot_of_epoch_1.0 + offset);

        match lookup_in_overlay_schedule(
            first_slot_of_epoch_1,
            &genesis_keys,
            d_val,
            asc,
            slot,
        ) {
            Some(OBftSlot::ActiveSlot(key_hash)) => {
                // Find which genesis node this is
                let index = genesis_keys
                    .iter()
                    .position(|k| k == &key_hash)
                    .unwrap_or(999);
                println!(
                    "{:<10} {:<15} Genesis #{:<10}",
                    slot.0, "Yes (Active)", index
                );
            }
            Some(OBftSlot::NonActiveSlot) => {
                println!("{:<10} {:<15} {:<20}", slot.0, "Yes (Inactive)", "-");
            }
            None => {
                println!("{:<10} {:<15} {:<20}", slot.0, "No", "-");
            }
        }
    }

    println!("\n=== Testing with partial decentralization (d = 0.5) ===\n");

    let d_val_half = UnitInterval::from_f64(0.5);

    println!("Checking first 50 slots with d = 0.5:");
    println!("{:<10} {:<15} {:<20}", "Slot", "Overlay?", "Genesis Node");
    println!("{:-<50}", "");

    let mut overlay_count = 0;
    let mut non_overlay_count = 0;

    for offset in 0..50 {
        let slot = SlotNo(first_slot_of_epoch_1.0 + offset);

        match lookup_in_overlay_schedule(
            first_slot_of_epoch_1,
            &genesis_keys,
            d_val_half,
            asc,
            slot,
        ) {
            Some(OBftSlot::ActiveSlot(key_hash)) => {
                let index = genesis_keys
                    .iter()
                    .position(|k| k == &key_hash)
                    .unwrap_or(999);
                println!(
                    "{:<10} {:<15} Genesis #{:<10}",
                    slot.0, "Yes (Active)", index
                );
                overlay_count += 1;
            }
            Some(OBftSlot::NonActiveSlot) => {
                println!("{:<10} {:<15} {:<20}", slot.0, "Yes (Inactive)", "-");
                overlay_count += 1;
            }
            None => {
                println!("{:<10} {:<15} {:<20}", slot.0, "No", "Any pool");
                non_overlay_count += 1;
            }
        }
    }

    println!("\nSummary:");
    println!("  Overlay slots: {} ({:.1}%)", overlay_count, overlay_count as f64 / 50.0 * 100.0);
    println!("  Non-overlay slots: {} ({:.1}%)", non_overlay_count, non_overlay_count as f64 / 50.0 * 100.0);
    println!("\nNote: With d = 0.5, approximately 50% of slots are reserved for genesis nodes");
}



