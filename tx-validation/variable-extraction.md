# UTXOW Variable Extraction and Construction

**Reference**: `Shelley/Rules/Utxow.hs:296-311`

This document explains how the UTXOW rule extracts and constructs variables from the transaction context before validation begins. These variables are fundamental to understanding how the validation rule works.

## Input Context

The UTXOW rule receives a **Transition Rule Context (TRC)** containing:

**Haskell** (`Utxow.hs:296`):
```haskell
transitionRulesUTXOW = do
  (TRC (utxoEnv@(UtxoEnv _ pp certState), u, tx)) <- judgmentContext
```

**Components**:
1. **`utxoEnv`** - Environment containing:
   - Slot number (for timelock validation)
   - Protocol parameters (`pp`)
   - Certificate state (`certState`) including genesis delegates
2. **`u`** - UTxO state containing the current UTxO set
3. **`tx`** - The transaction to validate

---

## Variable 1: `utxo` - The UTxO Set

**Formal Specification** (`Utxow.hs:298`):
```
(utxo, _, _, _) := utxoSt
```

**Haskell** (`Utxow.hs:300`):
```haskell
let utxo = utxosUtxo u
```

**Rust Implementation** (`utxow.rs:989`):
```rust
let utxo = &state.utxo;
```

### What It Is

- The **Unspent Transaction Output** set
- Maps `TxInput` (transaction hash + output index) to `TxOutput` (address, value, datum)
- Represents all currently spendable outputs on the blockchain

### Type

- **Haskell**: `UTxO era = Map TxIn (TxOut era)`
- **Rust**: `HashMap<TxInput, TxOutput>`

### Why Needed

- Look up inputs being spent to extract their addresses
- Determine if inputs are locked by keys or scripts
- Extract payment credentials for witness validation

### Example

```rust
// UTxO set
{
    TxInput { hash: 0xabc..., index: 0 } => TxOutput {
        address: KeyHashAddress { payment: K1, stake: Some(S1) },
        value: 1000000,  // 1 ADA
    },
    TxInput { hash: 0xdef..., index: 1 } => TxOutput {
        address: ScriptHashAddress { payment: Script123, stake: None },
        value: 5000000,  // 5 ADA
    },
}
```

---

## Variable 2: `witsKeyHashes` - Provided Key Hash Witnesses

**Formal Specification** (`Utxow.hs:299`):
```
witsKeyHashes := { hashKey vk | vk ∈ dom(txwitsVKey txw) }
```

This reads as: "The set of key hashes from all verification keys in the transaction witnesses"

**Haskell** (`Utxow.hs:301`):
```haskell
witsKeyHashes = keyHashWitnessesTxWits (tx ^. witsTxL)
```

### Construction Function

**Haskell** (`Core.hs:505-511`):
```haskell
keyHashWitnessesTxWits ::
  EraTxWits era =>
  TxWits era ->
  Set (KeyHash Witness)
keyHashWitnessesTxWits txWits =
  Set.map witVKeyHash (txWits ^. addrTxWitsL)         -- Regular VKey witnesses
    `Set.union` Set.map bootstrapWitKeyHash (txWits ^. bootAddrTxWitsL)  -- Bootstrap witnesses
```

**Rust Implementation** (`utxow.rs:621-630`):
```rust
pub fn get_vkey_hashes(wits: &TxWits) -> HashSet<KeyHash> {
    let mut hashes = HashSet::new();

    // Extract key hashes from regular VKey witnesses
    for witness in &wits.vkey_witnesses {
        hashes.insert(witness.key_hash());
    }

    // Extract key hashes from bootstrap witnesses
    for bootstrap_witness in &wits.bootstrap_witnesses {
        hashes.insert(bootstrap_witness.vkey_hash());
    }

    hashes
}
```

### What It Is

- Set of all **key hashes** from witnesses provided in the transaction
- Includes both **regular VKey witnesses** (Shelley) and **bootstrap witnesses** (Byron)
- These are the signatures that **are present** in the transaction

### Why Two Types of Witnesses?

1. **Regular VKey Witnesses** (`addrTxWitsL`):
   - For Shelley-era addresses
   - Simple: `VKey → Signature`
   - Hash the VKey to get the KeyHash

2. **Bootstrap Witnesses** (`bootAddrTxWitsL`):
   - For Byron-era addresses
   - Complex: `VKey → Signature → ChainCode → Attributes`
   - Hash the VKey to get the KeyHash

### Step-by-Step Construction

**Step 1**: Get transaction witnesses
```rust
let tx_wits = &tx.wits;
// TxWits {
//     vkey_witnesses: [VKeyWitness1, VKeyWitness2],
//     bootstrap_witnesses: [BootstrapWitness1],
//     scripts: {...}
// }
```

**Step 2**: Extract key hashes from VKey witnesses
```haskell
-- Haskell
Set.map witVKeyHash (txWits ^. addrTxWitsL)

-- For each VKeyWitness { vkey, signature }:
--   witVKeyHash extracts: hash(vkey) → KeyHash
```

```rust
// Rust
for witness in &tx_wits.vkey_witnesses {
    let key_hash = witness.key_hash();  // BLAKE2b-224(vkey)
    hashes.insert(key_hash);
}
```

**Step 3**: Extract key hashes from bootstrap witnesses
```haskell
-- Haskell
Set.map bootstrapWitKeyHash (txWits ^. bootAddrTxWitsL)

-- For each BootstrapWitness { vkey, signature, chainCode, attributes }:
--   bootstrapWitKeyHash extracts: hash(vkey) → KeyHash
```

```rust
// Rust
for bootstrap_witness in &tx_wits.bootstrap_witnesses {
    let key_hash = bootstrap_witness.vkey_hash();  // BLAKE2b-224(vkey)
    hashes.insert(key_hash);
}
```

**Step 4**: Union both sets
```haskell
-- Haskell: automatically done by Set.union
regularKeyHashes `Set.union` bootstrapKeyHashes
```

```rust
// Rust: automatically done by HashSet insertion
// All key hashes are inserted into the same HashSet
```

### Complete Example

**Transaction Witnesses**:
```rust
TxWits {
    vkey_witnesses: [
        VKeyWitness { vkey: VK1, signature: Sig1 },  // hash(VK1) = KeyHash1
        VKeyWitness { vkey: VK2, signature: Sig2 },  // hash(VK2) = KeyHash2
    ],
    bootstrap_witnesses: [
        BootstrapWitness {
            vkey: VK3,           // hash(VK3) = KeyHash3
            signature: Sig3,
            chain_code: [0x11...],
            attributes: []
        }
    ],
    scripts: {...}
}
```

**Result**:
```rust
witsKeyHashes = { KeyHash1, KeyHash2, KeyHash3 }
```

### Usage in Validation

```rust
// Later in validation (Step 4):
let needed = get_shelley_wits_vkey_needed(utxo, &tx.body, gen_delegs);
// needed = { KeyHash1, KeyHash2, KeyHash4 }

let missing = needed.difference(&witsKeyHashes);
// missing = { KeyHash4 }  ← ERROR! KeyHash4 is required but not provided
```

---

## Variable 3: `scriptsProvided` - Provided Script Witnesses

**Haskell** (`Utxow.hs:302`):
```haskell
scriptsProvided = getScriptsProvided utxo tx
```

**Rust Implementation** (`utxow.rs:615-619`):
```rust
pub fn get_scripts_provided(tx: &Tx) -> HashMap<ScriptHash, NativeScript> {
    tx.wits.scripts.clone()
}
```

### What It Is

- Map of **script hash** to **script code** provided in transaction witnesses
- Contains all scripts the transaction claims to satisfy
- Indexed by script hash for O(1) lookup

### Why Needed

- Validate that required scripts are provided (Step 2)
- Execute native scripts to check they evaluate to true (Step 1)

### Example

```rust
scriptsProvided = {
    ScriptHash(0xabc...) => NativeScript::RequireSignature(KeyHash1),
    ScriptHash(0xdef...) => NativeScript::RequireAllOf(vec![
        NativeScript::RequireSignature(KeyHash2),
        NativeScript::RequireSignature(KeyHash3),
    ]),
}
```

---

## Variable 4: `scriptsNeeded` - Required Script Witnesses

**Haskell** (`Utxow.hs:310`):
```haskell
let scriptsNeeded = getScriptsNeeded utxo (tx ^. bodyTxL)
```

**Rust Implementation** (`utxow.rs:571-612`):
```rust
pub fn get_scripts_needed(utxo: &UTxO, tx_body: &TxBody) -> HashSet<ScriptHash> {
    let mut needed = HashSet::new();

    // 1. Input scripts
    for input in &tx_body.inputs {
        if let Some(output) = utxo.get(input) {
            if let Address::ScriptHashAddress { payment, .. } = &output.address {
                needed.insert(*payment);
            }
        }
    }

    // 2. Withdrawal scripts
    for reward_account in tx_body.withdrawals.keys() {
        if let Some(script_hash) = reward_account.credential.script_hash() {
            needed.insert(script_hash);
        }
    }

    // 3. Certificate scripts
    for cert in &tx_body.certificates {
        if let Some(script_hash) = cert.get_script_witness() {
            needed.insert(script_hash);
        }
    }

    needed
}
```

### What It Is

- Set of **script hashes** that must be provided as witnesses
- Determined by examining transaction inputs, withdrawals, and certificates
- These are the scripts that **must be present** to authorize the transaction

### Three Sources

1. **Input Scripts**: Inputs locked by script addresses
2. **Withdrawal Scripts**: Reward accounts with script credentials
3. **Certificate Scripts**: Certificates with script credentials

### Example

```rust
// Transaction tries to spend these inputs:
inputs = {
    TxIn(0xabc..., 0),  // Locked by Script1
    TxIn(0xdef..., 1),  // Locked by KeyHash (no script needed)
}

// And has this withdrawal:
withdrawals = {
    RewardAccount(ScriptCredential(Script2)): 1000000
}

// Result:
scriptsNeeded = { Script1, Script2 }
```

### Validation Check

```rust
// Step 2 validates:
scriptsNeeded == scriptsProvided.keys()

// If not equal → error:
// - Missing: scriptsNeeded - scriptsProvided → MissingScriptWitnessesUTXOW
// - Extra: scriptsProvided - scriptsNeeded → ExtraneousScriptWitnessesUTXOW
```

---

## Variable 5: `genDelegs` - Genesis Delegates

**Haskell** (`Utxow.hs:327`):
```haskell
let genDelegs = dsGenDelegs (certState ^. certDStateL)
```

**Rust Implementation** (passed via `UtxoEnv`):
```rust
let gen_delegs = &env.genesis_delegates;
// GenDelegs {
//     mapping: HashMap<KeyHash, KeyHash>  // GenesisKey → DelegateKey
// }
```

### What It Is

- Mapping from **genesis key hash** to **genesis delegate key hash**
- Part of the delegation state maintained by the ledger
- Used for protocol parameter update validation

### Why Needed

- Protocol updates are proposed by genesis keys
- But must be signed by their delegates (for security)
- This mapping tells us which delegate corresponds to which genesis key

### Example

```rust
genDelegs = {
    GenesisKey1 → DelegateKey1,
    GenesisKey2 → DelegateKey2,
    GenesisKey3 → DelegateKey3,
}

// If transaction proposes update with GenesisKey1:
tx.body.update = Some(Update {
    proposals: { GenesisKey1: UpdateParams }
})

// Then DelegateKey1 must be in witsKeyHashes
```

---

## Variable 6: `coreNodeQuorum` - Genesis Quorum Threshold

**Haskell** (`Utxow.hs:328`):
```haskell
coreNodeQuorum <- liftSTS $ asks quorum
```

**Rust Implementation** (passed via `UtxoEnv`):
```rust
let quorum = env.quorum;  // usize
```

### What It Is

- Minimum number of genesis delegate signatures required for MIR certificates
- Typically set to a majority of genesis delegates (e.g., 5 out of 7)
- Hardcoded in genesis config

### Why Needed

- MIR certificates move funds from reserves/treasury
- Require quorum of genesis delegates to prevent single-party control
- Validated in Step 6

### Example

```rust
quorum = 5  // Need at least 5 genesis delegate signatures

// Transaction has MIR certificate
tx.body.certificates = [ShelleyTxCert::MIR]

// Count genesis delegate signatures in witsKeyHashes
genDelegs.values() = { D1, D2, D3, D4, D5, D6, D7 }
witsKeyHashes = { D1, D2, D3, D4 }  // Only 4 delegates signed

genSig = genDelegs.values() ∩ witsKeyHashes = { D1, D2, D3, D4 }
|genSig| = 4 < quorum (5)  → ERROR!
```

---

## Summary Table

| Variable | Type | Source | Purpose |
|----------|------|--------|---------|
| **`utxo`** | `HashMap<TxInput, TxOutput>` | State | Look up inputs being spent |
| **`witsKeyHashes`** | `HashSet<KeyHash>` | `tx.wits` | Key hashes of provided signatures |
| **`scriptsProvided`** | `HashMap<ScriptHash, NativeScript>` | `tx.wits` | Scripts provided in witnesses |
| **`scriptsNeeded`** | `HashSet<ScriptHash>` | `utxo` + `tx.body` | Scripts required for authorization |
| **`genDelegs`** | `HashMap<KeyHash, KeyHash>` | `certState` | Genesis key → Delegate mapping |
| **`coreNodeQuorum`** | `usize` | Environment | Minimum genesis sigs for MIR |

---

## Complete Variable Extraction in Rust

**Implementation** (`utxow.rs:981-992`):
```rust
pub fn validate_utxow(
    env: &UtxoEnv,
    state: &UTxOState,
    tx: &Tx,
) -> UtxowResult<()> {
    let mut errors = Vec::new();

    // Variable 1: UTxO set
    let utxo = &state.utxo;

    // Variable 2: Provided key hash witnesses
    let vkey_hashes = get_vkey_hashes(&tx.wits);

    // Variable 3: Provided script witnesses
    let scripts_provided = get_scripts_provided(tx);

    // Variable 4: Required script witnesses
    let scripts_needed = get_scripts_needed(utxo, &tx.body);

    // Variable 5 & 6: Genesis delegates and quorum (from environment)
    let gen_delegs = &env.genesis_delegates;
    let quorum = env.quorum;

    // Step 1: Validate native scripts
    if let Err(e) = validate_failed_native_scripts(tx, env) {
        errors.push(e);
    }

    // Step 2: Validate script presence
    if let Err(mut e) = validate_missing_scripts(&scripts_needed, &scripts_provided) {
        errors.append(&mut e);
    }

    // Step 3: Validate VKey witness signatures
    if let Err(e) = validate_verified_wits(tx) {
        errors.push(e);
    }

    // Step 4: Validate required witnesses present
    // Uses: utxo, tx.body, gen_delegs
    if let Err(e) = validate_needed_witnesses(&vkey_hashes, utxo, &tx.body, gen_delegs) {
        errors.push(e);
    }

    // Step 5: Validate metadata integrity
    if let Err(e) = validate_metadata(tx, env.protocol_version) {
        errors.push(e);
    }

    // Step 6: Validate MIR genesis signatures
    // Uses: gen_delegs, quorum, vkey_hashes, tx.body
    if let Err(e) = validate_mir_insufficient_genesis_sigs(
        gen_delegs,
        quorum,
        &vkey_hashes,
        &tx.body,
    ) {
        errors.push(e);
    }

    // Return errors if any
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
```

---

## Key Insights

1. **Variable extraction is deterministic** - Same inputs always produce same variables
2. **Variables are immutable** - Once extracted, never modified during validation
3. **Variables serve specific purposes** - Each used by one or more validation steps
4. **Separation of concerns** - Extraction separate from validation logic
5. **Performance consideration** - Sets and maps allow O(1) lookups during validation

This variable extraction phase is **critical** - it transforms the raw transaction, state, and environment into the specific data structures needed for each validation step.
