// Babbage Era UTXOW Rule Implementation
// Reference: eras/babbage/impl/src/Cardano/Ledger/Babbage/Rules/Utxow.hs
//
// Babbage adds reference scripts, inline datums, and script well-formedness checks.

use std::collections::{HashMap, HashSet};

// ============================================================================
// Core Types (from previous eras)
// ============================================================================

pub type Hash = [u8; 32];
pub type KeyHash = Hash;
pub type ScriptHash = Hash;
pub type DataHash = Hash;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SlotNo(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TxIn {
    pub tx_id: Hash,
    pub index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Credential {
    KeyHash(KeyHash),
    ScriptHash(ScriptHash),
}

// ============================================================================
// Babbage-Specific Types
// ============================================================================

/// Datum options (extended in Babbage)
/// Reference: eras/babbage/impl/src/Cardano/Ledger/Babbage/TxOut.hs
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Datum {
    /// No datum
    NoDatum,
    /// Datum hash (actual datum in witnesses)
    DatumHash(DataHash),
    /// Inline datum (actual datum embedded in output) - NEW in Babbage
    InlineDatum(Vec<u8>),
}

impl Datum {
    /// Get datum hash if applicable
    pub fn hash(&self) -> Option<DataHash> {
        match self {
            Datum::NoDatum => None,
            Datum::DatumHash(h) => Some(*h),
            Datum::InlineDatum(_data) => {
                // Hash the inline datum
                Some([0u8; 32]) // Simplified
            }
        }
    }

    /// Check if this is an inline datum
    pub fn is_inline(&self) -> bool {
        matches!(self, Datum::InlineDatum(_))
    }
}

/// Babbage TxOut with inline datum and reference script support
/// Reference: Babbage/TxOut.hs
#[derive(Debug, Clone)]
pub struct BabbageTxOut {
    pub address: Credential,
    pub value: u64,
    pub datum: Datum,
    pub reference_script: Option<Script>, // NEW in Babbage
}

/// Plutus language version
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Language {
    PlutusV1,
    PlutusV2,
}

/// Script types
#[derive(Debug, Clone)]
pub enum Script {
    Native(NativeScript),
    PlutusV1(Vec<u8>),
    PlutusV2(Vec<u8>),
}

impl Script {
    pub fn is_native(&self) -> bool {
        matches!(self, Script::Native(_))
    }

    pub fn hash(&self) -> ScriptHash {
        [0u8; 32] // Simplified
    }

    /// Check if Plutus script is well-formed (valid CBOR)
    /// Reference: Babbage/Rules/Utxow.hs:248-277
    pub fn is_well_formed(&self) -> bool {
        match self {
            Script::Native(_) => true, // Native scripts always well-formed
            Script::PlutusV1(bytes) | Script::PlutusV2(bytes) => {
                // Real implementation: validate CBOR structure
                // Check for proper CBOR encoding, correct script format, etc.
                !bytes.is_empty()
            }
        }
    }
}

/// Native script (simplified)
#[derive(Debug, Clone)]
pub enum NativeScript {
    RequireSignature(KeyHash),
    RequireAllOf(Vec<NativeScript>),
    RequireAnyOf(Vec<NativeScript>),
    RequireMOf { required: usize, scripts: Vec<NativeScript> },
}

/// Script purpose
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PlutusPurpose {
    Spending(u32),
    Minting(u32),
    Rewarding(u32),
    Certifying(u32),
}

/// Redeemer
#[derive(Debug, Clone)]
pub struct Redeemer {
    pub data: Vec<u8>,
    pub ex_units: (u64, u64),
}

/// Actual datum data
#[derive(Debug, Clone)]
pub struct DatumData {
    pub data: Vec<u8>,
}

/// VKey witness
#[derive(Debug, Clone)]
pub struct VKeyWitness {
    pub vkey: [u8; 32],
    pub signature: [u8; 64],
}

impl VKeyWitness {
    pub fn key_hash(&self) -> KeyHash {
        [0u8; 32]
    }
}

/// Babbage transaction body
/// Reference: Babbage/TxBody.hs
#[derive(Debug, Clone)]
pub struct BabbageTxBody {
    pub inputs: Vec<TxIn>,
    pub reference_inputs: HashSet<TxIn>, // NEW in Babbage
    pub outputs: Vec<BabbageTxOut>,
    pub fee: u64,
    pub mint: HashSet<ScriptHash>,
    pub script_integrity_hash: Option<Hash>,
    pub required_signers: HashSet<KeyHash>,
}

/// Babbage transaction witnesses
#[derive(Debug, Clone, Default)]
pub struct BabbageTxWits {
    pub vkey_wits: Vec<VKeyWitness>,
    pub scripts: HashMap<ScriptHash, Script>,
    pub datums: HashMap<DataHash, DatumData>,
    pub redeemers: HashMap<PlutusPurpose, Redeemer>,
}

/// Complete Babbage transaction
#[derive(Debug, Clone)]
pub struct BabbageTx {
    pub body: BabbageTxBody,
    pub wits: BabbageTxWits,
}

/// UTxO set
pub type UTxO = HashMap<TxIn, BabbageTxOut>;

// ============================================================================
// Predicate Failures (Errors)
// Reference: Utxow.hs:77-109
// ============================================================================

/// Alonzo errors (wrapped)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlonzoUtxowPredFailure {
    MissingScriptWitnessesUTXOW(HashSet<ScriptHash>),
    ExtraneousScriptWitnessesUTXOW(HashSet<ScriptHash>),
    MissingRedeemers(Vec<(PlutusPurpose, ScriptHash)>),
    MissingRequiredDatums { missing: HashSet<DataHash>, provided: HashSet<DataHash> },
    UnspendableUTxONoDatumHash(HashSet<TxIn>),
    ExtraRedeemers(Vec<PlutusPurpose>),
    /// Supplemental datums not allowed (not referenced by outputs or reference inputs)
    /// Reference: Alonzo/Rules/Utxow.hs:254-256
    NotAllowedSupplementalDatums {
        unallowed: HashSet<DataHash>,
        allowed: HashSet<DataHash>,
    },
}

/// Babbage UTXOW predicate failures
/// Reference: Utxow.hs:77-109
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BabbageUtxowPredFailure {
    /// Wrapped Alonzo error
    AlonzoInBabbageUtxowPredFailure(AlonzoUtxowPredFailure),

    /// Scripts in witnesses have invalid CBOR structure
    /// Reference: Utxow.hs:99-102
    MalformedScriptWitnesses(HashSet<ScriptHash>),

    /// Scripts in reference UTxOs have invalid CBOR structure
    /// Reference: Utxow.hs:103-106
    MalformedReferenceScripts(HashSet<ScriptHash>),

    /// Script integrity hash mismatch
    /// Reference: Utxow.hs:107-109
    ScriptIntegrityHashMismatch {
        expected: Option<Hash>,
        actual: Option<Hash>,
    },
}

// ============================================================================
// Scripts Provided (Babbage - includes reference scripts)
// Reference: eras/babbage/impl/src/Cardano/Ledger/Babbage/UTxO.hs:63-81
// ============================================================================

/// Get all scripts available for this transaction
/// Reference: UTxO.hs:63-81 (getBabbageScriptsProvided)
///
/// Collects scripts from:
/// 1. Transaction witnesses (same as Alonzo)
/// 2. Reference scripts in UTxOs (NEW in Babbage)
pub fn get_babbage_scripts_provided(
    utxo: &UTxO,
    tx: &BabbageTx,
) -> HashMap<ScriptHash, Script> {
    let mut provided = HashMap::new();

    // 1. Scripts from witnesses
    // tx ^. witsTxL . scriptTxWitsL
    for (hash, script) in &tx.wits.scripts {
        provided.insert(*hash, script.clone());
    }

    // 2. Reference scripts from UTxOs (NEW in Babbage)
    // Both regular inputs and reference inputs can provide scripts
    let all_inputs: HashSet<&TxIn> = tx
        .body
        .inputs
        .iter()
        .chain(tx.body.reference_inputs.iter())
        .collect();

    for txin in all_inputs {
        if let Some(txout) = utxo.get(txin) {
            if let Some(ref script) = txout.reference_script {
                provided.insert(script.hash(), script.clone());
            }
        }
    }

    provided
}

/// Partition scripts by source (witness vs reference)
pub fn partition_scripts_by_source(
    witness_scripts: &HashMap<ScriptHash, Script>,
    all_provided: &HashMap<ScriptHash, Script>,
) -> (HashSet<ScriptHash>, HashSet<ScriptHash>) {
    let witness_hashes: HashSet<ScriptHash> = witness_scripts.keys().copied().collect();
    let all_hashes: HashSet<ScriptHash> = all_provided.keys().copied().collect();
    let reference_hashes: HashSet<ScriptHash> = all_hashes
        .difference(&witness_hashes)
        .copied()
        .collect();

    (witness_hashes, reference_hashes)
}

// ============================================================================
// Scripts Needed
// ============================================================================

pub struct BabbageScriptsNeeded {
    pub needed: Vec<(PlutusPurpose, ScriptHash)>,
}

/// Get scripts needed (same as Alonzo)
pub fn get_babbage_scripts_needed(
    utxo: &UTxO,
    tx_body: &BabbageTxBody,
) -> BabbageScriptsNeeded {
    let mut needed = Vec::new();

    // Spending scripts
    for (idx, txin) in tx_body.inputs.iter().enumerate() {
        if let Some(txout) = utxo.get(txin) {
            if let Credential::ScriptHash(sh) = &txout.address {
                needed.push((PlutusPurpose::Spending(idx as u32), *sh));
            }
        }
    }

    // Minting scripts
    for (idx, policy) in tx_body.mint.iter().enumerate() {
        needed.push((PlutusPurpose::Minting(idx as u32), *policy));
    }

    // Note: Reference inputs do NOT require authorization - they're read-only

    BabbageScriptsNeeded { needed }
}

pub fn get_scripts_hashes_needed(needed: &BabbageScriptsNeeded) -> HashSet<ScriptHash> {
    needed.needed.iter().map(|(_, sh)| *sh).collect()
}

// ============================================================================
// Script Validation (Babbage-specific)
// Reference: Utxow.hs:208-233, 248-277
// ============================================================================

/// Validate missing scripts (Babbage version)
/// Reference: Utxow.hs:208-233 (babbageMissingScripts)
///
/// Key difference from Alonzo: Reference scripts are allowed to be "extra"
pub fn validate_babbage_missing_scripts(
    scripts_provided: &HashMap<ScriptHash, Script>,
    scripts_needed: &HashSet<ScriptHash>,
    witness_scripts: &HashMap<ScriptHash, Script>,
) -> Result<(), BabbageUtxowPredFailure> {
    let scripts_received: HashSet<ScriptHash> = scripts_provided.keys().copied().collect();
    let witness_hashes: HashSet<ScriptHash> = witness_scripts.keys().copied().collect();

    // Missing = needed but not received (from any source)
    let missing: HashSet<ScriptHash> = scripts_needed
        .difference(&scripts_received)
        .copied()
        .collect();

    if !missing.is_empty() {
        return Err(BabbageUtxowPredFailure::AlonzoInBabbageUtxowPredFailure(
            AlonzoUtxowPredFailure::MissingScriptWitnessesUTXOW(missing),
        ));
    }

    // Extra = received as WITNESS but not needed
    // Reference scripts are allowed to be extra (they're on-chain anyway)
    let extra_witness: HashSet<ScriptHash> = witness_hashes
        .difference(scripts_needed)
        .copied()
        .collect();

    if !extra_witness.is_empty() {
        return Err(BabbageUtxowPredFailure::AlonzoInBabbageUtxowPredFailure(
            AlonzoUtxowPredFailure::ExtraneousScriptWitnessesUTXOW(extra_witness),
        ));
    }

    Ok(())
}

/// Validate scripts well-formed (NEW in Babbage)
/// Reference: Utxow.hs:248-277 (validateScriptsWellFormed)
///
/// Checks that all Plutus scripts have valid CBOR structure.
pub fn validate_scripts_well_formed(
    scripts_provided: &HashMap<ScriptHash, Script>,
    witness_scripts: &HashMap<ScriptHash, Script>,
) -> Result<(), BabbageUtxowPredFailure> {
    // Check witness scripts
    let malformed_witnesses: HashSet<ScriptHash> = witness_scripts
        .iter()
        .filter(|(_, script)| !script.is_native() && !script.is_well_formed())
        .map(|(hash, _)| *hash)
        .collect();

    if !malformed_witnesses.is_empty() {
        return Err(BabbageUtxowPredFailure::MalformedScriptWitnesses(
            malformed_witnesses,
        ));
    }

    // Check reference scripts (scripts in provided but not in witnesses)
    let witness_hashes: HashSet<ScriptHash> = witness_scripts.keys().copied().collect();
    let reference_scripts: HashMap<&ScriptHash, &Script> = scripts_provided
        .iter()
        .filter(|(hash, _)| !witness_hashes.contains(*hash))
        .collect();

    let malformed_references: HashSet<ScriptHash> = reference_scripts
        .iter()
        .filter(|(_, script)| !script.is_native() && !script.is_well_formed())
        .map(|(hash, _)| **hash)
        .collect();

    if !malformed_references.is_empty() {
        return Err(BabbageUtxowPredFailure::MalformedReferenceScripts(
            malformed_references,
        ));
    }

    Ok(())
}

// ============================================================================
// Datum Handling (Babbage - with inline datums)
// Reference: eras/babbage/impl/src/Cardano/Ledger/Babbage/UTxO.hs:83-92
// ============================================================================

/// Get supplemental data hashes (Babbage version)
/// Reference: UTxO.hs:65-75 (getBabbageSupplementalDataHashes)
///
/// Supplemental datums can be for:
/// 1. Outputs (same as Alonzo) - ONLY DatumHash, NOT inline datums
/// 2. Reference inputs (NEW in Babbage) - ONLY DatumHash, NOT inline datums
///
/// Note: Inline datums do NOT count as supplemental datum hashes.
/// The Haskell uses `dataHashTxOutL` which returns SNothing for inline datums.
/// See: Babbage/TxOut.hs:714-717 (getDataHashBabbageTxOut)
pub fn get_babbage_supplemental_data_hashes(
    utxo: &UTxO,
    tx_body: &BabbageTxBody,
) -> HashSet<DataHash> {
    let mut hashes = HashSet::new();

    // Output datum hashes (only DatumHash, not inline datums)
    // Haskell: newOuts = map sizedValue $ toList $ txBody ^. allSizedOutputsTxBodyF
    for out in &tx_body.outputs {
        if let Datum::DatumHash(h) = &out.datum {
            hashes.insert(*h);
        }
        // Note: InlineDatum does NOT contribute to supplemental hashes
    }

    // Reference input datum hashes (NEW in Babbage)
    // Haskell: referencedOuts = Map.elems $ Map.restrictKeys utxo (txBody ^. referenceInputsTxBodyL)
    // Only DatumHash, not inline datums!
    for txin in &tx_body.reference_inputs {
        if let Some(txout) = utxo.get(txin) {
            // Only DatumHash counts, not InlineDatum
            // Haskell: SJust dh <- [txOut ^. dataHashTxOutL]
            // dataHashTxOutL returns SNothing for inline datums
            if let Datum::DatumHash(h) = &txout.datum {
                hashes.insert(*h);
            }
        }
    }

    hashes
}

/// Analyze input datum requirements (Babbage version)
/// Reference: Similar to Alonzo but handles inline datums
pub fn get_babbage_input_data_hashes(
    utxo: &UTxO,
    tx_body: &BabbageTxBody,
    scripts_provided: &HashMap<ScriptHash, Script>,
) -> (HashSet<DataHash>, HashSet<TxIn>) {
    let mut input_hashes = HashSet::new();
    let mut inputs_no_datum = HashSet::new();

    for txin in &tx_body.inputs {
        if let Some(txout) = utxo.get(txin) {
            if let Credential::ScriptHash(sh) = &txout.address {
                if let Some(script) = scripts_provided.get(sh) {
                    if !script.is_native() {
                        match &txout.datum {
                            Datum::NoDatum => {
                                // PlutusV2 still requires datum (V3 added in Conway)
                                inputs_no_datum.insert(txin.clone());
                            }
                            Datum::DatumHash(hash) => {
                                input_hashes.insert(*hash);
                            }
                            Datum::InlineDatum(_) => {
                                // Inline datum - no hash needed in witnesses
                                // The datum is already in the UTxO!
                            }
                        }
                    }
                }
            }
        }
    }

    (input_hashes, inputs_no_datum)
}

// ============================================================================
// Main UTXOW Transition Function
// Reference: Utxow.hs:315-401 (babbageUtxowTransition)
// ============================================================================

/// Babbage UTXOW environment
pub struct BabbageUtxoEnv {
    pub slot: SlotNo,
    pub cost_models: HashMap<Language, Vec<u64>>,
}

/// Babbage UTXOW validation
/// Reference: Utxow.hs:315-401 (babbageUtxowTransition)
pub fn babbage_utxow_transition(
    _env: &BabbageUtxoEnv,
    utxo: &UTxO,
    tx: &BabbageTx,
) -> Result<(), BabbageUtxowPredFailure> {
    // Get scripts from witnesses AND reference UTxOs
    let scripts_provided = get_babbage_scripts_provided(utxo, tx);
    let witness_scripts = &tx.wits.scripts;

    let scripts_needed = get_babbage_scripts_needed(utxo, &tx.body);
    let script_hashes_needed = get_scripts_hashes_needed(&scripts_needed);

    // Step 1: Validate native scripts (same as Alonzo)
    // (Simplified)

    // Step 2: Check script presence (CHANGED: reference scripts allowed)
    validate_babbage_missing_scripts(&scripts_provided, &script_hashes_needed, witness_scripts)?;

    // Step 3: Check required datums (handles inline datums)
    let (input_hashes, inputs_no_datum) =
        get_babbage_input_data_hashes(utxo, &tx.body, &scripts_provided);

    if !inputs_no_datum.is_empty() {
        return Err(BabbageUtxowPredFailure::AlonzoInBabbageUtxowPredFailure(
            AlonzoUtxowPredFailure::UnspendableUTxONoDatumHash(inputs_no_datum),
        ));
    }

    // Check datums provided in witnesses
    // Reference: Alonzo/Rules/Utxow.hs:237-257 (missingRequiredDatums)
    let tx_datum_hashes: HashSet<DataHash> = tx.wits.datums.keys().copied().collect();

    // Check 1: All required datum hashes must have datums in witnesses
    // Haskell: unmatchedDatumHashes = Set.difference inputHashes txHashes
    let missing_datums: HashSet<DataHash> = input_hashes
        .difference(&tx_datum_hashes)
        .copied()
        .collect();

    if !missing_datums.is_empty() {
        return Err(BabbageUtxowPredFailure::AlonzoInBabbageUtxowPredFailure(
            AlonzoUtxowPredFailure::MissingRequiredDatums {
                missing: missing_datums,
                provided: tx_datum_hashes.clone(),
            },
        ));
    }

    // Check 2: Supplemental datums must be allowed (for outputs or ref inputs)
    // Reference: Alonzo/Rules/Utxow.hs:243-256
    // Haskell: allowedSupplementalDataHashes = getSupplementalDataHashes utxo txBody
    //          supplimentalDatumHashes = Set.difference txHashes inputHashes
    //          notOkSupplimentalDHs = filter (not in allowed) supplimentalDatumHashes
    let allowed_supplemental = get_babbage_supplemental_data_hashes(utxo, &tx.body);

    // Supplemental = provided but not required by inputs
    let supplemental_datums: HashSet<DataHash> = tx_datum_hashes
        .difference(&input_hashes)
        .copied()
        .collect();

    // Not allowed = supplemental but not in allowed set
    let not_allowed: HashSet<DataHash> = supplemental_datums
        .difference(&allowed_supplemental)
        .copied()
        .collect();

    if !not_allowed.is_empty() {
        // ok_supplemental = supplemental ∩ allowed
        let ok_supplemental: HashSet<DataHash> = supplemental_datums
            .intersection(&allowed_supplemental)
            .copied()
            .collect();
        return Err(BabbageUtxowPredFailure::AlonzoInBabbageUtxowPredFailure(
            AlonzoUtxowPredFailure::NotAllowedSupplementalDatums {
                unallowed: not_allowed,
                allowed: ok_supplemental,
            },
        ));
    }

    // Step 4: Check exact redeemers (same as Alonzo)
    // (Simplified)

    // Step 5-8: VKey, witnesses, MIR, metadata (same as Shelley)
    // Metadata: Shelley.validateMetadata pp tx (hash + when pv > (2,0) validMetadatum).
    // See shelley-utxow.rs validate_metadata(tx, protocol_version).

    // Step 9: Validate script well-formedness (NEW in Babbage)
    validate_scripts_well_formed(&scripts_provided, witness_scripts)?;

    // Step 10: Script integrity hash (same as Alonzo)
    // (Simplified)

    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_hash(id: u8) -> Hash {
        let mut h = [0u8; 32];
        h[0] = id;
        h
    }

    fn make_txin(id: u8, idx: u32) -> TxIn {
        TxIn {
            tx_id: make_hash(id),
            index: idx,
        }
    }

    #[test]
    fn test_reference_scripts_provided() {
        let mut utxo = UTxO::new();
        let ref_input = make_txin(1, 0);
        let script = Script::PlutusV2(vec![1, 2, 3]);
        let script_hash = script.hash();

        // UTxO with reference script
        utxo.insert(
            ref_input.clone(),
            BabbageTxOut {
                address: Credential::KeyHash(make_hash(99)),
                value: 1000,
                datum: Datum::NoDatum,
                reference_script: Some(script),
            },
        );

        let tx = BabbageTx {
            body: BabbageTxBody {
                inputs: vec![],
                reference_inputs: [ref_input].into_iter().collect(),
                outputs: vec![],
                fee: 0,
                mint: HashSet::new(),
                script_integrity_hash: None,
                required_signers: HashSet::new(),
            },
            wits: BabbageTxWits::default(),
        };

        let provided = get_babbage_scripts_provided(&utxo, &tx);

        // Reference script should be available
        assert!(provided.contains_key(&script_hash));
    }

    #[test]
    fn test_inline_datum_no_witness_needed() {
        let mut utxo = UTxO::new();
        let txin = make_txin(1, 0);
        let script_hash = make_hash(10);
        let inline_datum = vec![1, 2, 3, 4];

        // UTxO with inline datum
        utxo.insert(
            txin.clone(),
            BabbageTxOut {
                address: Credential::ScriptHash(script_hash),
                value: 1000,
                datum: Datum::InlineDatum(inline_datum),
                reference_script: None,
            },
        );

        let mut scripts = HashMap::new();
        scripts.insert(script_hash, Script::PlutusV2(vec![]));

        let tx_body = BabbageTxBody {
            inputs: vec![txin.clone()],
            reference_inputs: HashSet::new(),
            outputs: vec![],
            fee: 0,
            mint: HashSet::new(),
            script_integrity_hash: None,
            required_signers: HashSet::new(),
        };

        let (input_hashes, inputs_no_datum) =
            get_babbage_input_data_hashes(&utxo, &tx_body, &scripts);

        // Inline datum: no hash needed, not flagged as missing
        assert!(input_hashes.is_empty());
        assert!(inputs_no_datum.is_empty());
    }

    #[test]
    fn test_malformed_script_detection() {
        let well_formed = Script::PlutusV2(vec![1, 2, 3]);
        let malformed = Script::PlutusV2(vec![]); // Empty = malformed in our simplified check

        assert!(well_formed.is_well_formed());
        assert!(!malformed.is_well_formed());
    }

    // ========================================================================
    // Supplemental Datum Hash Tests
    // Reference: Babbage/UTxO.hs:65-75 (getBabbageSupplementalDataHashes)
    // ========================================================================

    #[test]
    fn test_supplemental_hashes_from_outputs() {
        let utxo = UTxO::new();
        let datum_hash = make_hash(42);

        let tx_body = BabbageTxBody {
            inputs: vec![],
            reference_inputs: HashSet::new(),
            outputs: vec![BabbageTxOut {
                address: Credential::KeyHash(make_hash(1)),
                value: 1000,
                datum: Datum::DatumHash(datum_hash),
                reference_script: None,
            }],
            fee: 0,
            mint: HashSet::new(),
            script_integrity_hash: None,
            required_signers: HashSet::new(),
        };

        let hashes = get_babbage_supplemental_data_hashes(&utxo, &tx_body);

        // Output datum hash should be in supplemental
        assert!(hashes.contains(&datum_hash));
        assert_eq!(hashes.len(), 1);
    }

    #[test]
    fn test_supplemental_hashes_from_reference_inputs() {
        let mut utxo = UTxO::new();
        let ref_input = make_txin(1, 0);
        let datum_hash = make_hash(42);

        // Reference input with datum hash
        utxo.insert(
            ref_input.clone(),
            BabbageTxOut {
                address: Credential::KeyHash(make_hash(99)),
                value: 1000,
                datum: Datum::DatumHash(datum_hash),
                reference_script: None,
            },
        );

        let tx_body = BabbageTxBody {
            inputs: vec![],
            reference_inputs: [ref_input].into_iter().collect(),
            outputs: vec![],
            fee: 0,
            mint: HashSet::new(),
            script_integrity_hash: None,
            required_signers: HashSet::new(),
        };

        let hashes = get_babbage_supplemental_data_hashes(&utxo, &tx_body);

        // Reference input datum hash should be in supplemental
        assert!(hashes.contains(&datum_hash));
        assert_eq!(hashes.len(), 1);
    }

    #[test]
    fn test_inline_datum_not_in_supplemental_hashes() {
        let mut utxo = UTxO::new();
        let ref_input = make_txin(1, 0);

        // Reference input with INLINE datum (not datum hash)
        utxo.insert(
            ref_input.clone(),
            BabbageTxOut {
                address: Credential::KeyHash(make_hash(99)),
                value: 1000,
                datum: Datum::InlineDatum(vec![1, 2, 3, 4]),
                reference_script: None,
            },
        );

        let tx_body = BabbageTxBody {
            inputs: vec![],
            reference_inputs: [ref_input].into_iter().collect(),
            outputs: vec![
                // Output with inline datum
                BabbageTxOut {
                    address: Credential::KeyHash(make_hash(1)),
                    value: 1000,
                    datum: Datum::InlineDatum(vec![5, 6, 7, 8]),
                    reference_script: None,
                },
            ],
            fee: 0,
            mint: HashSet::new(),
            script_integrity_hash: None,
            required_signers: HashSet::new(),
        };

        let hashes = get_babbage_supplemental_data_hashes(&utxo, &tx_body);

        // Inline datums should NOT contribute to supplemental hashes
        // This is the key Babbage behavior - inline datums don't count
        assert!(hashes.is_empty());
    }

    #[test]
    fn test_supplemental_datum_allowed_for_output() {
        let utxo = UTxO::new();
        let datum_hash = make_hash(42);

        let tx_body = BabbageTxBody {
            inputs: vec![],
            reference_inputs: HashSet::new(),
            outputs: vec![BabbageTxOut {
                address: Credential::KeyHash(make_hash(1)),
                value: 1000,
                datum: Datum::DatumHash(datum_hash),
                reference_script: None,
            }],
            fee: 0,
            mint: HashSet::new(),
            script_integrity_hash: None,
            required_signers: HashSet::new(),
        };

        // Witness provides datum for output datum hash
        let mut datums = HashMap::new();
        datums.insert(datum_hash, DatumData { data: vec![1, 2, 3] });

        let tx = BabbageTx {
            body: tx_body,
            wits: BabbageTxWits {
                datums,
                ..Default::default()
            },
        };

        let env = BabbageUtxoEnv {
            slot: SlotNo(0),
            cost_models: HashMap::new(),
        };

        // Should succeed - supplemental datum is allowed (for output)
        let result = babbage_utxow_transition(&env, &utxo, &tx);
        assert!(result.is_ok());
    }

    #[test]
    fn test_supplemental_datum_not_allowed() {
        let utxo = UTxO::new();
        let datum_hash = make_hash(42);
        let random_hash = make_hash(99); // Not in any output or ref input

        let tx_body = BabbageTxBody {
            inputs: vec![],
            reference_inputs: HashSet::new(),
            outputs: vec![BabbageTxOut {
                address: Credential::KeyHash(make_hash(1)),
                value: 1000,
                datum: Datum::DatumHash(datum_hash),
                reference_script: None,
            }],
            fee: 0,
            mint: HashSet::new(),
            script_integrity_hash: None,
            required_signers: HashSet::new(),
        };

        // Witness provides EXTRA datum that's not referenced anywhere
        let mut datums = HashMap::new();
        datums.insert(datum_hash, DatumData { data: vec![1, 2, 3] });
        datums.insert(random_hash, DatumData { data: vec![4, 5, 6] }); // Not allowed!

        let tx = BabbageTx {
            body: tx_body,
            wits: BabbageTxWits {
                datums,
                ..Default::default()
            },
        };

        let env = BabbageUtxoEnv {
            slot: SlotNo(0),
            cost_models: HashMap::new(),
        };

        // Should fail - random datum is not allowed as supplemental
        let result = babbage_utxow_transition(&env, &utxo, &tx);
        assert!(matches!(
            result,
            Err(BabbageUtxowPredFailure::AlonzoInBabbageUtxowPredFailure(
                AlonzoUtxowPredFailure::NotAllowedSupplementalDatums { .. }
            ))
        ));
    }

    #[test]
    fn test_supplemental_datum_allowed_for_reference_input() {
        let mut utxo = UTxO::new();
        let ref_input = make_txin(1, 0);
        let datum_hash = make_hash(42);

        // Reference input with datum hash
        utxo.insert(
            ref_input.clone(),
            BabbageTxOut {
                address: Credential::KeyHash(make_hash(99)),
                value: 1000,
                datum: Datum::DatumHash(datum_hash),
                reference_script: None,
            },
        );

        let tx_body = BabbageTxBody {
            inputs: vec![],
            reference_inputs: [ref_input].into_iter().collect(),
            outputs: vec![],
            fee: 0,
            mint: HashSet::new(),
            script_integrity_hash: None,
            required_signers: HashSet::new(),
        };

        // Witness provides datum for reference input's datum hash
        let mut datums = HashMap::new();
        datums.insert(datum_hash, DatumData { data: vec![1, 2, 3] });

        let tx = BabbageTx {
            body: tx_body,
            wits: BabbageTxWits {
                datums,
                ..Default::default()
            },
        };

        let env = BabbageUtxoEnv {
            slot: SlotNo(0),
            cost_models: HashMap::new(),
        };

        // Should succeed - supplemental datum is allowed (for reference input)
        let result = babbage_utxow_transition(&env, &utxo, &tx);
        assert!(result.is_ok());
    }

    #[test]
    fn test_same_datum_hash_in_input_and_output() {
        // Test from earlier discussion: same datum hash can appear in both input and output
        let mut utxo = UTxO::new();
        let txin = make_txin(1, 0);
        let script_hash = make_hash(10);
        let datum_hash = make_hash(42);

        // Input UTxO with datum hash (locked by script)
        utxo.insert(
            txin.clone(),
            BabbageTxOut {
                address: Credential::ScriptHash(script_hash),
                value: 1000,
                datum: Datum::DatumHash(datum_hash),
                reference_script: None,
            },
        );

        let tx_body = BabbageTxBody {
            inputs: vec![txin.clone()],
            reference_inputs: HashSet::new(),
            outputs: vec![
                // Output with SAME datum hash
                BabbageTxOut {
                    address: Credential::ScriptHash(script_hash),
                    value: 1000,
                    datum: Datum::DatumHash(datum_hash), // Same hash!
                    reference_script: None,
                },
            ],
            fee: 0,
            mint: HashSet::new(),
            script_integrity_hash: None,
            required_signers: HashSet::new(),
        };

        let mut scripts = HashMap::new();
        scripts.insert(script_hash, Script::PlutusV2(vec![1, 2, 3]));

        let (input_hashes, _) =
            get_babbage_input_data_hashes(&utxo, &tx_body, &scripts);

        // Input requires the datum hash
        assert!(input_hashes.contains(&datum_hash));

        let supplemental = get_babbage_supplemental_data_hashes(&utxo, &tx_body);

        // Output also allows it as supplemental
        assert!(supplemental.contains(&datum_hash));

        // Key insight: the datum is required by input, so it's NOT supplemental
        // in the validation logic. The output just happens to reference the same hash.
        let mut datums = HashMap::new();
        datums.insert(datum_hash, DatumData { data: vec![1, 2, 3] });

        let tx_datum_hashes: HashSet<DataHash> = datums.keys().copied().collect();

        // Supplemental = provided - required_by_inputs
        let supplemental_datums: HashSet<DataHash> = tx_datum_hashes
            .difference(&input_hashes)
            .copied()
            .collect();

        // Since datum is required by input, it's NOT in supplemental set
        assert!(supplemental_datums.is_empty());
    }
}
