/// Example usage of the VRF seed construction

use vrf_seed::*;
use vrf_seed::universal_constants::*;

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn main() {
    println!("=== Cardano VRF Seed Construction (mkSeed) ===\n");

    // Example 1: Simple seed with just a slot number
    println!("Example 1: Seed with neutral nonces");
    println!("{}", "=".repeat(50));
    
    let slot = SlotNo(432000); // First slot of epoch 1
    let uc_nonce = Nonce::neutral();
    let e_nonce = Nonce::neutral();
    
    let seed = mk_seed(&uc_nonce, slot, &e_nonce);
    println!("Slot:              {}", slot.0);
    println!("Universal Const:   Neutral");
    println!("Epoch Nonce:       Neutral");
    println!("Result Seed:       {}\n", hex_encode(seed.as_bytes()));

    // Example 2: Seed with epoch nonce
    println!("Example 2: Seed with epoch nonce");
    println!("{}", "=".repeat(50));
    
    let slot = SlotNo(432000);
    let uc_nonce = Nonce::neutral();
    let e_nonce = Nonce::from_number(123); // Some epoch nonce
    
    let seed = mk_seed(&uc_nonce, slot, &e_nonce);
    println!("Slot:              {}", slot.0);
    println!("Universal Const:   Neutral");
    println!("Epoch Nonce:       {}", hex_encode(e_nonce.as_bytes().unwrap()));
    println!("Result Seed:       {}\n", hex_encode(seed.as_bytes()));

    // Example 3: Seed with seedEta universal constant (for randomness)
    println!("Example 3: Seed with seedEta constant (randomness/eta computation)");
    println!("{}", "=".repeat(50));
    
    let slot = SlotNo(432000);
    let uc_nonce = seed_eta(); // Domain separator for eta
    let e_nonce = Nonce::from_number(123);
    
    let seed = mk_seed(&uc_nonce, slot, &e_nonce);
    println!("Slot:              {}", slot.0);
    println!("Universal Const:   seedEta (0)");
    if let Nonce::Nonce(h) = &uc_nonce {
        println!("  Hash:            {}", hex_encode(h));
    }
    println!("Epoch Nonce:       {}", hex_encode(e_nonce.as_bytes().unwrap()));
    println!("Result Seed:       {}\n", hex_encode(seed.as_bytes()));

    // Example 4: Seed with seedL universal constant (for leader election)
    println!("Example 4: Seed with seedL constant (leader election computation)");
    println!("{}", "=".repeat(50));
    
    let slot = SlotNo(432000);
    let uc_nonce = seed_l(); // Domain separator for leader election
    let e_nonce = Nonce::from_number(123);
    
    let seed = mk_seed(&uc_nonce, slot, &e_nonce);
    println!("Slot:              {}", slot.0);
    println!("Universal Const:   seedL (1)");
    if let Nonce::Nonce(h) = &uc_nonce {
        println!("  Hash:            {}", hex_encode(h));
    }
    println!("Epoch Nonce:       {}", hex_encode(e_nonce.as_bytes().unwrap()));
    println!("Result Seed:       {}\n", hex_encode(seed.as_bytes()));

    // Example 5: Compare seeds with different universal constants
    println!("Example 5: Comparing eta vs leader seeds (same slot & epoch nonce)");
    println!("{}", "=".repeat(50));
    
    let slot = SlotNo(432000);
    let e_nonce = Nonce::from_number(123);
    
    let seed_eta_result = mk_seed(&seed_eta(), slot, &e_nonce);
    let seed_l_result = mk_seed(&seed_l(), slot, &e_nonce);
    
    println!("Seed with eta:     {}", hex_encode(seed_eta_result.as_bytes()));
    println!("Seed with L:       {}", hex_encode(seed_l_result.as_bytes()));
    println!("Are they equal?    {}\n", seed_eta_result == seed_l_result);

    // Example 6: Show how slot changes affect the seed
    println!("Example 6: Seeds for consecutive slots");
    println!("{}", "=".repeat(50));
    
    let e_nonce = Nonce::from_number(123);
    let uc_nonce = seed_l();
    
    for slot_num in 432000..432005 {
        let slot = SlotNo(slot_num);
        let seed = mk_seed(&uc_nonce, slot, &e_nonce);
        println!("Slot {}: {}", slot_num, hex_encode(seed.as_bytes()));
    }

    println!("\n=== Use Cases ===");
    println!("1. seedEta: Used to generate epoch nonce (randomness for next epoch)");
    println!("2. seedL:   Used for leader election (VRF proof that you're the slot leader)");
    println!("3. The seed is input to the VRF function to prove slot leadership");
    println!("4. Different universal constants ensure domain separation");
}

