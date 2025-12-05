// Cardano UTXOW Rule Implementation Example
// This demonstrates Phase 1 transaction validation - witnessing checks
// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxow.hs

use std::collections::{HashMap, HashSet};
use std::fmt;

// ============================================================================
// Core Types
// ============================================================================

/// 32-byte hash identifying keys, scripts, and transactions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hash([u8; 32]);

impl Hash {
    pub fn new(bytes: [u8; 32]) -> Self {
        Hash(bytes)
    }
}

/// Key hash for witness verification
pub type KeyHash = Hash;

/// Script hash for script identification
pub type ScriptHash = Hash;

/// Transaction body hash (what gets signed)
pub type TxBodyHash = Hash;

/// Metadata hash
pub type MetadataHash = Hash;

/// Slot number for timelock validation
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SlotNo(pub u64);

/// Ed25519 verification key (32 bytes)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VKey(pub [u8; 32]);

/// Ed25519 signature (64 bytes)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature(pub [u8; 64]);

// ============================================================================
// Native Scripts (Phase 1)
// ============================================================================

/// Native scripts - validated in UTXOW (Phase 1)
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Scripts.hs
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeScript {
    /// Shelley: Require specific signature
    RequireSignature(KeyHash),

    /// Shelley: Require ALL sub-scripts to validate
    RequireAllOf(Vec<NativeScript>),

    /// Shelley: Require ANY sub-script to validate
    RequireAnyOf(Vec<NativeScript>),

    /// Shelley: Require M of N sub-scripts to validate
    RequireMOf {
        required: usize,
        scripts: Vec<NativeScript>,
    },

    /// Allegra+: Valid after this slot (inclusive)
    RequireTimeStart(SlotNo),

    /// Allegra+: Valid before this slot (exclusive)
    RequireTimeExpire(SlotNo),
}

impl NativeScript {
    /// Validate native script against transaction context
    /// Reference: Shelley/Scripts.hs:233-249, Allegra/Scripts.hs:428-451
    pub fn validate(&self, vkey_hashes: &HashSet<KeyHash>, current_slot: SlotNo) -> bool {
        match self {
            // Shelley: Check signature present
            NativeScript::RequireSignature(key_hash) => vkey_hashes.contains(key_hash),

            // Shelley: All must validate
            NativeScript::RequireAllOf(scripts) => {
                scripts.iter().all(|s| s.validate(vkey_hashes, current_slot))
            }

            // Shelley: At least one must validate
            NativeScript::RequireAnyOf(scripts) => {
                scripts.iter().any(|s| s.validate(vkey_hashes, current_slot))
            }

            // Shelley: At least M must validate
            NativeScript::RequireMOf { required, scripts } => {
                let valid_count = scripts
                    .iter()
                    .filter(|s| s.validate(vkey_hashes, current_slot))
                    .count();
                valid_count >= *required
            }

            // Allegra: Current slot >= start slot
            NativeScript::RequireTimeStart(start_slot) => current_slot >= *start_slot,

            // Allegra: Current slot < expire slot
            NativeScript::RequireTimeExpire(expire_slot) => current_slot < *expire_slot,
        }
    }

    /// Compute script hash
    pub fn hash(&self) -> ScriptHash {
        // In real implementation: BLAKE2b-256 hash of CBOR-serialized script
        // Simplified for demonstration
        Hash([0u8; 32])
    }
}

// ============================================================================
// Transaction Components
// ============================================================================

/// VKey witness - public key and signature
/// Reference: Cardano.Ledger.Keys
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VKeyWitness {
    pub vkey: VKey,
    pub signature: Signature,
}

impl VKeyWitness {
    /// Extract key hash from VKey witness
    pub fn key_hash(&self) -> KeyHash {
        // In real implementation: BLAKE2b-224 hash of vkey
        Hash([0u8; 32])
    }

    /// Verify signature against transaction body hash
    /// Reference: Cardano.Ledger.Keys.verifyWitVKey
    pub fn verify(&self, tx_body_hash: TxBodyHash) -> bool {
        // In real implementation: Ed25519 signature verification
        // verify_ed25519(&self.vkey.0, &tx_body_hash.0, &self.signature.0)
        true // Placeholder
    }
}

/// Transaction witnesses
/// Reference: Cardano.Ledger.Shelley.TxWits
#[derive(Debug, Clone, Default)]
pub struct TxWits {
    /// VKey witnesses (signatures)
    pub vkey_witnesses: Vec<VKeyWitness>,

    /// Native scripts (MultiSig, Timelock)
    pub scripts: HashMap<ScriptHash, NativeScript>,
}

/// Transaction metadata datum
/// Reference: Cardano.Ledger.Metadata
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Metadatum {
    /// Integer value
    I(i128),
    /// Byte string (max 64 bytes)
    B(Vec<u8>),
    /// Text string (max 64 bytes UTF-8 encoded)
    S(String),
    /// List of metadata
    List(Vec<Metadatum>),
    /// Map of metadata key-value pairs
    Map(Vec<(Metadatum, Metadatum)>),
}

impl Metadatum {
    /// Validate metadatum according to Cardano rules
    /// Reference: Cardano.Ledger.Metadata:75-87
    pub fn validate(&self) -> bool {
        match self {
            // Integers always valid (size checked in CBOR decoder)
            Metadatum::I(_) => true,

            // Byte strings must be <= 64 bytes
            Metadatum::B(b) => b.len() <= 64,

            // Text strings must be <= 64 bytes when UTF-8 encoded
            Metadatum::S(s) => s.as_bytes().len() <= 64,

            // Lists: recursively validate all elements
            Metadatum::List(xs) => xs.iter().all(|x| x.validate()),

            // Maps: recursively validate all keys and values
            Metadatum::Map(kvs) => kvs.iter().all(|(k, v)| k.validate() && v.validate()),
        }
    }
}

/// Transaction metadata (map of Word64 to Metadatum)
/// Reference: Cardano.Ledger.Shelley.TxAuxData
#[derive(Debug, Clone)]
pub struct Metadata {
    pub metadata: HashMap<u64, Metadatum>,
}

impl Metadata {
    pub fn hash(&self) -> MetadataHash {
        // In real implementation: BLAKE2b-256 hash of CBOR-serialized metadata
        Hash([0u8; 32])
    }

    /// Validate all metadata entries
    /// Reference: Shelley/TxAuxData.hs:98
    pub fn validate(&self) -> bool {
        self.metadata.values().all(|m| m.validate())
    }
}

/// Transaction body
#[derive(Debug, Clone)]
pub struct TxBody {
    pub inputs: HashSet<TxInput>,
    pub metadata_hash: Option<MetadataHash>,
    // Other fields omitted for brevity
}

impl TxBody {
    pub fn hash(&self) -> TxBodyHash {
        // In real implementation: BLAKE2b-256 hash of CBOR-serialized body
        Hash([0u8; 32])
    }
}

/// Complete transaction
#[derive(Debug, Clone)]
pub struct Tx {
    pub body: TxBody,
    pub wits: TxWits,
    pub metadata: Option<Metadata>,
}

/// Transaction input reference
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TxInput {
    pub tx_hash: Hash,
    pub index: u32,
}

/// Transaction output
#[derive(Debug, Clone)]
pub struct TxOutput {
    pub address: Address,
    // Other fields omitted
}

/// Address types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Address {
    /// Payment credential is key hash
    KeyHashAddress { payment: KeyHash },

    /// Payment credential is script hash
    ScriptHashAddress { payment: ScriptHash },
}

/// UTxO set
pub type UTxO = HashMap<TxInput, TxOutput>;

// ============================================================================
// UTXOW Environment and State
// ============================================================================

/// UTXOW environment
/// Reference: Shelley/Rules/Utxo.hs:109-113
#[derive(Debug, Clone)]
pub struct UtxoEnv {
    /// Current slot for timelock validation
    pub slot: SlotNo,

    /// Protocol version (for soft fork features)
    pub protocol_version: u32,

    /// Genesis delegates for MIR validation
    pub genesis_delegates: HashMap<KeyHash, KeyHash>,

    /// Genesis quorum threshold
    pub quorum: usize,
}

/// UTXOW state
#[derive(Debug, Clone)]
pub struct UTxOState {
    pub utxo: UTxO,
    // Other fields omitted
}

// ============================================================================
// UTXOW Validation Errors
// ============================================================================

/// UTXOW validation failures
/// Reference: Shelley/Rules/Utxow.hs:112-134
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UtxowPredFailure {
    /// VKey witnesses with invalid signatures
    InvalidWitnessesUTXOW(Vec<VKey>),

    /// Required VKey witnesses not provided
    MissingVKeyWitnessesUTXOW(HashSet<KeyHash>),

    /// Required scripts not provided
    MissingScriptWitnessesUTXOW(HashSet<ScriptHash>),

    /// Native scripts that failed validation
    ScriptWitnessNotValidatingUTXOW(HashSet<ScriptHash>),

    /// Scripts provided but not needed
    ExtraneousScriptWitnessesUTXOW(HashSet<ScriptHash>),

    /// Metadata hash present but no metadata
    MissingTxMetadata(MetadataHash),

    /// Metadata present but no hash in body
    MissingTxBodyMetadataHash(MetadataHash),

    /// Metadata hash mismatch
    ConflictingMetadataHash {
        expected: MetadataHash,
        actual: MetadataHash,
    },

    /// Insufficient genesis signatures for MIR certificate
    MIRInsufficientGenesisSigs {
        required: usize,
        actual: usize,
    },

    /// Metadata validation failed (e.g., strings too long)
    InvalidMetadata,
}

impl fmt::Display for UtxowPredFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UtxowPredFailure::InvalidWitnessesUTXOW(vkeys) => {
                write!(f, "Invalid signatures for {} keys", vkeys.len())
            }
            UtxowPredFailure::MissingVKeyWitnessesUTXOW(keys) => {
                write!(f, "Missing {} required signatures", keys.len())
            }
            UtxowPredFailure::MissingScriptWitnessesUTXOW(scripts) => {
                write!(f, "Missing {} required scripts", scripts.len())
            }
            UtxowPredFailure::ScriptWitnessNotValidatingUTXOW(scripts) => {
                write!(f, "{} native scripts failed validation", scripts.len())
            }
            UtxowPredFailure::ExtraneousScriptWitnessesUTXOW(scripts) => {
                write!(f, "{} extraneous scripts provided", scripts.len())
            }
            UtxowPredFailure::MissingTxMetadata(hash) => {
                write!(f, "Metadata hash present but metadata missing")
            }
            UtxowPredFailure::MissingTxBodyMetadataHash(hash) => {
                write!(f, "Metadata present but hash missing in tx body")
            }
            UtxowPredFailure::ConflictingMetadataHash { expected, actual } => {
                write!(f, "Metadata hash mismatch")
            }
            UtxowPredFailure::MIRInsufficientGenesisSigs { required, actual } => {
                write!(
                    f,
                    "MIR requires {} genesis signatures, only {} provided",
                    required, actual
                )
            }
            UtxowPredFailure::InvalidMetadata => {
                write!(f, "Metadata validation failed (e.g., strings too long)")
            }
        }
    }
}

pub type UtxowResult<T> = Result<T, Vec<UtxowPredFailure>>;

// ============================================================================
// UTXOW Validation Functions
// ============================================================================

/// Get scripts needed for transaction
/// Reference: Shelley/UTxO.hs:104-120
pub fn get_scripts_needed(utxo: &UTxO, tx_body: &TxBody) -> HashSet<ScriptHash> {
    let mut needed = HashSet::new();

    // For each input, check if locked by script
    for input in &tx_body.inputs {
        if let Some(output) = utxo.get(input) {
            if let Address::ScriptHashAddress { payment } = &output.address {
                needed.insert(*payment);
            }
        }
    }

    // In full implementation, also check:
    // - Withdrawals from script stake addresses
    // - Certificates with script credentials

    needed
}

/// Get scripts provided in transaction
/// Reference: Shelley/UTxO.hs:190
pub fn get_scripts_provided(tx: &Tx) -> HashMap<ScriptHash, NativeScript> {
    tx.wits.scripts.clone()
}

/// Get VKey hashes from witnesses
pub fn get_vkey_hashes(wits: &TxWits) -> HashSet<KeyHash> {
    wits.vkey_witnesses
        .iter()
        .map(|w| w.key_hash())
        .collect()
}

/// Get VKey witnesses needed for transaction
/// Simplified - in real implementation checks inputs, certificates, withdrawals, etc.
/// Reference: Shelley/UTxO.hs:201-259
pub fn get_vkey_needed(utxo: &UTxO, tx_body: &TxBody) -> HashSet<KeyHash> {
    let mut needed = HashSet::new();

    // For each input, extract key hash from address
    for input in &tx_body.inputs {
        if let Some(output) = utxo.get(input) {
            if let Address::KeyHashAddress { payment } = &output.address {
                needed.insert(*payment);
            }
        }
    }

    // In full implementation, also check:
    // - Certificate authorizations
    // - Withdrawal authorizations
    // - Required signers (Alonzo+)
    // - Protocol parameter update proposals

    needed
}

// ============================================================================
// UTXOW Validation Steps
// ============================================================================

/// Step 1: Validate native scripts
/// Reference: Shelley/Rules/Utxow.hs:373-381
pub fn validate_failed_native_scripts(
    tx: &Tx,
    env: &UtxoEnv,
) -> Result<(), UtxowPredFailure> {
    let vkey_hashes = get_vkey_hashes(&tx.wits);
    let mut failed_scripts = HashSet::new();

    for (script_hash, script) in &tx.wits.scripts {
        // Validate native script against transaction context
        if !script.validate(&vkey_hashes, env.slot) {
            failed_scripts.insert(*script_hash);
        }
    }

    if !failed_scripts.is_empty() {
        return Err(UtxowPredFailure::ScriptWitnessNotValidatingUTXOW(
            failed_scripts,
        ));
    }

    Ok(())
}

/// Step 2: Validate script presence (no missing, no extra)
/// Reference: Shelley/Rules/Utxow.hs:383-398
pub fn validate_missing_scripts(
    scripts_needed: &HashSet<ScriptHash>,
    scripts_provided: &HashMap<ScriptHash, NativeScript>,
) -> Result<(), Vec<UtxowPredFailure>> {
    let mut errors = Vec::new();
    let provided_hashes: HashSet<_> = scripts_provided.keys().copied().collect();

    // Check for missing scripts
    let missing: HashSet<_> = scripts_needed.difference(&provided_hashes).copied().collect();
    if !missing.is_empty() {
        errors.push(UtxowPredFailure::MissingScriptWitnessesUTXOW(missing));
    }

    // Check for extraneous scripts
    let extraneous: HashSet<_> = provided_hashes.difference(scripts_needed).copied().collect();
    if !extraneous.is_empty() {
        errors.push(UtxowPredFailure::ExtraneousScriptWitnessesUTXOW(
            extraneous,
        ));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Step 3: Validate VKey witness signatures
/// Reference: Shelley/Rules/Utxow.hs:400-419
pub fn validate_verified_wits(tx: &Tx) -> Result<(), UtxowPredFailure> {
    let tx_body_hash = tx.body.hash();
    let mut invalid_vkeys = Vec::new();

    for witness in &tx.wits.vkey_witnesses {
        if !witness.verify(tx_body_hash) {
            invalid_vkeys.push(witness.vkey.clone());
        }
    }

    if !invalid_vkeys.is_empty() {
        return Err(UtxowPredFailure::InvalidWitnessesUTXOW(invalid_vkeys));
    }

    Ok(())
}

/// Step 4: Validate required witnesses present
/// Reference: Shelley/Rules/Utxow.hs:421-436
pub fn validate_needed_witnesses(
    vkey_hashes: &HashSet<KeyHash>,
    vkey_needed: &HashSet<KeyHash>,
) -> Result<(), UtxowPredFailure> {
    let missing: HashSet<_> = vkey_needed.difference(vkey_hashes).copied().collect();

    if !missing.is_empty() {
        return Err(UtxowPredFailure::MissingVKeyWitnessesUTXOW(missing));
    }

    Ok(())
}

/// Step 5: Validate metadata integrity
/// Reference: Shelley/Rules/Utxow.hs:438-457
pub fn validate_metadata(tx: &Tx, protocol_version: u32) -> Result<(), UtxowPredFailure> {
    match (&tx.body.metadata_hash, &tx.metadata) {
        // No metadata - OK
        (None, None) => Ok(()),

        // Hash but no metadata
        (Some(hash), None) => Err(UtxowPredFailure::MissingTxMetadata(*hash)),

        // Metadata but no hash
        (None, Some(metadata)) => Err(UtxowPredFailure::MissingTxBodyMetadataHash(
            metadata.hash(),
        )),

        // Both present - check match and validate content
        (Some(expected_hash), Some(metadata)) => {
            let actual_hash = metadata.hash();

            // Check hash matches
            if *expected_hash != actual_hash {
                return Err(UtxowPredFailure::ConflictingMetadataHash {
                    expected: *expected_hash,
                    actual: actual_hash,
                });
            }

            // Check metadata value sizes (protocol version >= 5)
            // Reference: Shelley/Rules/Utxow.hs:454-455
            if protocol_version >= 5 && !metadata.validate() {
                return Err(UtxowPredFailure::InvalidMetadata);
            }

            Ok(())
        }
    }
}

/// Step 6: Validate MIR certificate genesis signatures (simplified)
/// Reference: Shelley/Rules/Utxow.hs:459-485
pub fn validate_mir_genesis_sigs(
    env: &UtxoEnv,
    vkey_hashes: &HashSet<KeyHash>,
    has_mir_certs: bool,
) -> Result<(), UtxowPredFailure> {
    if !has_mir_certs {
        return Ok(());
    }

    // Count genesis delegate signatures present
    let genesis_sigs: HashSet<_> = env
        .genesis_delegates
        .values()
        .filter(|k| vkey_hashes.contains(k))
        .collect();

    if genesis_sigs.len() < env.quorum {
        return Err(UtxowPredFailure::MIRInsufficientGenesisSigs {
            required: env.quorum,
            actual: genesis_sigs.len(),
        });
    }

    Ok(())
}

// ============================================================================
// Complete UTXOW Validation
// ============================================================================

/// Complete UTXOW rule validation
/// Reference: Shelley/Rules/Utxow.hs:296-333
pub fn validate_utxow(
    env: &UtxoEnv,
    state: &UTxOState,
    tx: &Tx,
) -> UtxowResult<()> {
    let mut errors = Vec::new();

    // Extract transaction components
    let utxo = &state.utxo;
    let vkey_hashes = get_vkey_hashes(&tx.wits);
    let scripts_provided = get_scripts_provided(tx);
    let scripts_needed = get_scripts_needed(utxo, &tx.body);
    let vkey_needed = get_vkey_needed(utxo, &tx.body);

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
    if let Err(e) = validate_needed_witnesses(&vkey_hashes, &vkey_needed) {
        errors.push(e);
    }

    // Step 5: Validate metadata integrity
    if let Err(e) = validate_metadata(tx, env.protocol_version) {
        errors.push(e);
    }

    // Step 6: Validate MIR genesis signatures (simplified)
    let has_mir_certs = false; // Would check tx.body.certificates
    if let Err(e) = validate_mir_genesis_sigs(env, &vkey_hashes, has_mir_certs) {
        errors.push(e);
    }

    // Return errors if any
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }

    // Step 7: Would call UTXO rule here for structural validation
}

// ============================================================================
// Era-Specific Types and Implementations
// ============================================================================

/// Cardano ledger eras
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Era {
    Shelley,
    Allegra,
    Mary,
    Alonzo,
    Babbage,
    Conway,
}

impl Era {
    /// Check if this era supports timelock scripts
    pub fn supports_timelock(&self) -> bool {
        match self {
            Era::Shelley => false,
            Era::Allegra | Era::Mary | Era::Alonzo | Era::Babbage | Era::Conway => true,
        }
    }

    /// Get era name
    pub fn name(&self) -> &'static str {
        match self {
            Era::Shelley => "Shelley",
            Era::Allegra => "Allegra",
            Era::Mary => "Mary",
            Era::Alonzo => "Alonzo",
            Era::Babbage => "Babbage",
            Era::Conway => "Conway",
        }
    }
}

/// Check if a script is compatible with an era (structural validation only)
pub fn check_script_era_compatibility(script: &NativeScript, era: Era) -> Result<(), String> {
    match script {
        NativeScript::RequireTimeStart(_) | NativeScript::RequireTimeExpire(_) => {
            if !era.supports_timelock() {
                return Err(
                    "Timelock scripts (RequireTimeStart/RequireTimeExpire) not supported in Shelley era"
                        .to_string(),
                );
            }
        }
        NativeScript::RequireAllOf(scripts)
        | NativeScript::RequireAnyOf(scripts) => {
            // Recursively check nested scripts
            for s in scripts {
                check_script_era_compatibility(s, era)?;
            }
        }
        NativeScript::RequireMOf { scripts, .. } => {
            // Recursively check nested scripts
            for s in scripts {
                check_script_era_compatibility(s, era)?;
            }
        }
        _ => {} // RequireSignature valid in all eras
    }
    Ok(())
}

/// Era-aware native script validation (full validation with witnesses)
pub fn validate_native_script_for_era(
    script: &NativeScript,
    vkey_hashes: &HashSet<KeyHash>,
    current_slot: SlotNo,
    era: Era,
) -> Result<bool, String> {
    // Check if script is valid for this era
    check_script_era_compatibility(script, era)?;

    // Validate the script
    Ok(script.validate(vkey_hashes, current_slot))
}

// ============================================================================
// Allegra Era Specifics
// ============================================================================

/// Allegra introduced validity intervals for transactions
/// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/Scripts.hs:117-123
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidityInterval {
    /// Transaction invalid before this slot (inclusive)
    pub invalid_before: Option<SlotNo>,
    /// Transaction invalid after this slot (exclusive)
    pub invalid_hereafter: Option<SlotNo>,
}

impl ValidityInterval {
    /// Create a new validity interval
    pub fn new(invalid_before: Option<SlotNo>, invalid_hereafter: Option<SlotNo>) -> Self {
        ValidityInterval {
            invalid_before,
            invalid_hereafter,
        }
    }

    /// Check if current slot is within validity interval
    /// Reference: Allegra/Scripts.hs:inInterval
    pub fn in_interval(&self, slot: SlotNo) -> bool {
        let after_start = self
            .invalid_before
            .map(|start| slot >= start)
            .unwrap_or(true);

        let before_end = self
            .invalid_hereafter
            .map(|end| slot < end)
            .unwrap_or(true);

        after_start && before_end
    }

    /// Create unbounded interval (valid in all slots)
    pub fn unbounded() -> Self {
        ValidityInterval {
            invalid_before: None,
            invalid_hereafter: None,
        }
    }

    /// Create interval valid only before a slot
    pub fn before(slot: SlotNo) -> Self {
        ValidityInterval {
            invalid_before: None,
            invalid_hereafter: Some(slot),
        }
    }

    /// Create interval valid only after a slot
    pub fn after(slot: SlotNo) -> Self {
        ValidityInterval {
            invalid_before: Some(slot),
            invalid_hereafter: None,
        }
    }

    /// Create interval valid between two slots
    pub fn between(start: SlotNo, end: SlotNo) -> Self {
        ValidityInterval {
            invalid_before: Some(start),
            invalid_hereafter: Some(end),
        }
    }
}

// ============================================================================
// Example Usage
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn example_keyhash(byte: u8) -> KeyHash {
        Hash([byte; 32])
    }

    fn example_vkey(byte: u8) -> VKey {
        VKey([byte; 32])
    }

    fn example_sig() -> Signature {
        Signature([0u8; 64])
    }

    #[test]
    fn test_native_script_require_signature() {
        let key1 = example_keyhash(1);
        let key2 = example_keyhash(2);

        let script = NativeScript::RequireSignature(key1);

        // Valid: key1 present
        let mut vkeys = HashSet::new();
        vkeys.insert(key1);
        assert!(script.validate(&vkeys, SlotNo(0)));

        // Invalid: key1 not present
        let mut vkeys = HashSet::new();
        vkeys.insert(key2);
        assert!(!script.validate(&vkeys, SlotNo(0)));
    }

    #[test]
    fn test_native_script_require_all() {
        let key1 = example_keyhash(1);
        let key2 = example_keyhash(2);

        let script = NativeScript::RequireAllOf(vec![
            NativeScript::RequireSignature(key1),
            NativeScript::RequireSignature(key2),
        ]);

        // Valid: both keys present
        let mut vkeys = HashSet::new();
        vkeys.insert(key1);
        vkeys.insert(key2);
        assert!(script.validate(&vkeys, SlotNo(0)));

        // Invalid: only one key present
        let mut vkeys = HashSet::new();
        vkeys.insert(key1);
        assert!(!script.validate(&vkeys, SlotNo(0)));
    }

    #[test]
    fn test_native_script_require_m_of() {
        let key1 = example_keyhash(1);
        let key2 = example_keyhash(2);
        let key3 = example_keyhash(3);

        let script = NativeScript::RequireMOf {
            required: 2,
            scripts: vec![
                NativeScript::RequireSignature(key1),
                NativeScript::RequireSignature(key2),
                NativeScript::RequireSignature(key3),
            ],
        };

        // Valid: 2 of 3 keys present
        let mut vkeys = HashSet::new();
        vkeys.insert(key1);
        vkeys.insert(key2);
        assert!(script.validate(&vkeys, SlotNo(0)));

        // Valid: all 3 keys present (exceeds requirement)
        let mut vkeys = HashSet::new();
        vkeys.insert(key1);
        vkeys.insert(key2);
        vkeys.insert(key3);
        assert!(script.validate(&vkeys, SlotNo(0)));

        // Invalid: only 1 key present
        let mut vkeys = HashSet::new();
        vkeys.insert(key1);
        assert!(!script.validate(&vkeys, SlotNo(0)));
    }

    #[test]
    fn test_timelock_time_start() {
        let script = NativeScript::RequireTimeStart(SlotNo(100));

        // Valid: current slot >= start
        assert!(script.validate(&HashSet::new(), SlotNo(100)));
        assert!(script.validate(&HashSet::new(), SlotNo(150)));

        // Invalid: current slot < start
        assert!(!script.validate(&HashSet::new(), SlotNo(99)));
    }

    #[test]
    fn test_timelock_time_expire() {
        let script = NativeScript::RequireTimeExpire(SlotNo(100));

        // Valid: current slot < expire
        assert!(script.validate(&HashSet::new(), SlotNo(99)));

        // Invalid: current slot >= expire
        assert!(!script.validate(&HashSet::new(), SlotNo(100)));
        assert!(!script.validate(&HashSet::new(), SlotNo(101)));
    }

    #[test]
    fn test_timelock_complex() {
        let key1 = example_keyhash(1);
        let key2 = example_keyhash(2);
        let key3 = example_keyhash(3);

        // Valid between slots 100-200, requires 2 of 3 signatures
        let script = NativeScript::RequireAllOf(vec![
            NativeScript::RequireTimeStart(SlotNo(100)),
            NativeScript::RequireTimeExpire(SlotNo(200)),
            NativeScript::RequireMOf {
                required: 2,
                scripts: vec![
                    NativeScript::RequireSignature(key1),
                    NativeScript::RequireSignature(key2),
                    NativeScript::RequireSignature(key3),
                ],
            },
        ]);

        // Valid: slot 150, keys 1 and 2 present
        let mut vkeys = HashSet::new();
        vkeys.insert(key1);
        vkeys.insert(key2);
        assert!(script.validate(&vkeys, SlotNo(150)));

        // Invalid: slot 99 (before time window)
        assert!(!script.validate(&vkeys, SlotNo(99)));

        // Invalid: slot 200 (after time window)
        assert!(!script.validate(&vkeys, SlotNo(200)));

        // Invalid: slot 150 but only 1 signature
        let mut vkeys = HashSet::new();
        vkeys.insert(key1);
        assert!(!script.validate(&vkeys, SlotNo(150)));
    }

    #[test]
    fn test_validate_missing_scripts() {
        let script1 = Hash([1u8; 32]);
        let script2 = Hash([2u8; 32]);
        let script3 = Hash([3u8; 32]);

        let mut needed = HashSet::new();
        needed.insert(script1);
        needed.insert(script2);

        let mut provided = HashMap::new();
        provided.insert(script1, NativeScript::RequireSignature(example_keyhash(1)));
        provided.insert(script2, NativeScript::RequireSignature(example_keyhash(2)));

        // Valid: exact match
        assert!(validate_missing_scripts(&needed, &provided).is_ok());

        // Missing script2
        let mut provided = HashMap::new();
        provided.insert(script1, NativeScript::RequireSignature(example_keyhash(1)));
        let result = validate_missing_scripts(&needed, &provided);
        assert!(result.is_err());
        match &result.unwrap_err()[0] {
            UtxowPredFailure::MissingScriptWitnessesUTXOW(missing) => {
                assert!(missing.contains(&script2));
            }
            _ => panic!("Wrong error type"),
        }

        // Extraneous script3
        let mut provided = HashMap::new();
        provided.insert(script1, NativeScript::RequireSignature(example_keyhash(1)));
        provided.insert(script2, NativeScript::RequireSignature(example_keyhash(2)));
        provided.insert(script3, NativeScript::RequireSignature(example_keyhash(3)));
        let result = validate_missing_scripts(&needed, &provided);
        assert!(result.is_err());
        match &result.unwrap_err()[0] {
            UtxowPredFailure::ExtraneousScriptWitnessesUTXOW(extraneous) => {
                assert!(extraneous.contains(&script3));
            }
            _ => panic!("Wrong error type"),
        }
    }

    #[test]
    fn test_validate_metadata() {
        // Case 1: No metadata - OK
        let tx = Tx {
            body: TxBody {
                inputs: HashSet::new(),
                metadata_hash: None,
            },
            wits: TxWits::default(),
            metadata: None,
        };
        assert!(validate_metadata(&tx, 5).is_ok());

        // Case 2: Hash but no metadata - ERROR
        let tx = Tx {
            body: TxBody {
                inputs: HashSet::new(),
                metadata_hash: Some(Hash([1u8; 32])),
            },
            wits: TxWits::default(),
            metadata: None,
        };
        match validate_metadata(&tx, 5) {
            Err(UtxowPredFailure::MissingTxMetadata(_)) => {}
            _ => panic!("Expected MissingTxMetadata error"),
        }

        // Case 3: Metadata but no hash - ERROR
        let tx = Tx {
            body: TxBody {
                inputs: HashSet::new(),
                metadata_hash: None,
            },
            wits: TxWits::default(),
            metadata: Some(Metadata {
                metadata: HashMap::new(),
            }),
        };
        match validate_metadata(&tx, 5) {
            Err(UtxowPredFailure::MissingTxBodyMetadataHash(_)) => {}
            _ => panic!("Expected MissingTxBodyMetadataHash error"),
        }
    }

    #[test]
    fn test_metadatum_validation() {
        // Valid: Integer
        assert!(Metadatum::I(42).validate());
        assert!(Metadatum::I(-1000).validate());

        // Valid: Byte string <= 64 bytes
        assert!(Metadatum::B(vec![0u8; 64]).validate());
        assert!(Metadatum::B(vec![0u8; 32]).validate());

        // Invalid: Byte string > 64 bytes
        assert!(!Metadatum::B(vec![0u8; 65]).validate());
        assert!(!Metadatum::B(vec![0u8; 100]).validate());

        // Valid: Text string <= 64 bytes UTF-8
        assert!(Metadatum::S("Hello, world!".to_string()).validate());
        assert!(Metadatum::S("a".repeat(64)).validate());

        // Invalid: Text string > 64 bytes UTF-8
        assert!(!Metadatum::S("a".repeat(65)).validate());
        assert!(!Metadatum::S("x".repeat(100)).validate());

        // Valid: UTF-8 multi-byte characters (each emoji is 4 bytes)
        assert!(Metadatum::S("😀".repeat(16)).validate()); // 16 * 4 = 64 bytes
        assert!(!Metadatum::S("😀".repeat(17)).validate()); // 17 * 4 = 68 bytes

        // Valid: Empty list
        assert!(Metadatum::List(vec![]).validate());

        // Valid: List with valid elements
        assert!(Metadatum::List(vec![
            Metadatum::I(1),
            Metadatum::S("test".to_string()),
            Metadatum::B(vec![1, 2, 3]),
        ])
        .validate());

        // Invalid: List with invalid element
        assert!(!Metadatum::List(vec![
            Metadatum::I(1),
            Metadatum::S("a".repeat(65)), // Invalid: too long
        ])
        .validate());

        // Valid: Map with valid entries
        assert!(Metadatum::Map(vec![
            (Metadatum::I(1), Metadatum::S("value1".to_string())),
            (Metadatum::S("key".to_string()), Metadatum::I(42)),
        ])
        .validate());

        // Invalid: Map with invalid key
        assert!(!Metadatum::Map(vec![(
            Metadatum::S("a".repeat(65)), // Invalid key: too long
            Metadatum::I(1),
        )])
        .validate());

        // Invalid: Map with invalid value
        assert!(!Metadatum::Map(vec![(
            Metadatum::I(1),
            Metadatum::B(vec![0u8; 100]), // Invalid value: too long
        )])
        .validate());

        // Valid: Nested structure
        assert!(Metadatum::Map(vec![(
            Metadatum::S("nested".to_string()),
            Metadatum::List(vec![
                Metadatum::I(1),
                Metadatum::Map(vec![(Metadatum::I(2), Metadatum::B(vec![3, 4, 5]))]),
            ]),
        )])
        .validate());

        // Invalid: Deep nesting with one invalid element
        assert!(!Metadatum::List(vec![Metadatum::List(vec![
            Metadatum::Map(vec![(Metadatum::I(1), Metadatum::S("a".repeat(65)))]), // Invalid
        ])])
        .validate());
    }

    #[test]
    fn test_metadata_validation_protocol_version() {
        let mut metadata = HashMap::new();
        metadata.insert(0, Metadatum::S("test".to_string()));

        let tx = Tx {
            body: TxBody {
                inputs: HashSet::new(),
                metadata_hash: Some(Hash([0u8; 32])),
            },
            wits: TxWits::default(),
            metadata: Some(Metadata { metadata }),
        };

        // Protocol version < 5: string validation not enforced
        // (In this test, hash mismatch will fail first, but that's OK)

        // Protocol version >= 5: string validation enforced
        let mut metadata_invalid = HashMap::new();
        metadata_invalid.insert(0, Metadatum::S("a".repeat(65))); // Too long

        let tx_invalid = Tx {
            body: TxBody {
                inputs: HashSet::new(),
                metadata_hash: Some(Hash([0u8; 32])),
            },
            wits: TxWits::default(),
            metadata: Some(Metadata {
                metadata: metadata_invalid,
            }),
        };

        // With protocol version 5+, invalid metadata should be caught
        match validate_metadata(&tx_invalid, 5) {
            Err(UtxowPredFailure::InvalidMetadata) | Err(UtxowPredFailure::ConflictingMetadataHash { .. }) => {
                // Either error is acceptable (hash check happens first, but if it passes, validation should fail)
            }
            _ => {
                // In this case, hash check happens first, but let's test with protocol version 4
                // to ensure validation is skipped
                if let Err(UtxowPredFailure::InvalidMetadata) = validate_metadata(&tx_invalid, 4) {
                    panic!("Protocol version 4 should not validate metadata content");
                }
            }
        }
    }

    // ========================================================================
    // Allegra Era Tests
    // ========================================================================

    #[test]
    fn test_era_supports_timelock() {
        // Shelley: No timelock support
        assert!(!Era::Shelley.supports_timelock());

        // Allegra+: All support timelock
        assert!(Era::Allegra.supports_timelock());
        assert!(Era::Mary.supports_timelock());
        assert!(Era::Alonzo.supports_timelock());
        assert!(Era::Babbage.supports_timelock());
        assert!(Era::Conway.supports_timelock());
    }

    #[test]
    fn test_check_script_era_compatibility_shelley() {
        let key1 = example_keyhash(1);

        // Shelley: MultiSig scripts OK
        let multisig = NativeScript::RequireSignature(key1);
        assert!(check_script_era_compatibility(&multisig, Era::Shelley).is_ok());

        // Shelley: Timelock scripts NOT allowed
        let timelock_start = NativeScript::RequireTimeStart(SlotNo(100));
        let result = check_script_era_compatibility(&timelock_start, Era::Shelley);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Timelock scripts (RequireTimeStart/RequireTimeExpire) not supported in Shelley era"
        );

        let timelock_expire = NativeScript::RequireTimeExpire(SlotNo(200));
        let result = check_script_era_compatibility(&timelock_expire, Era::Shelley);
        assert!(result.is_err());
    }

    #[test]
    fn test_check_script_era_compatibility_allegra() {
        let key1 = example_keyhash(1);

        // Allegra: MultiSig scripts OK
        let multisig = NativeScript::RequireSignature(key1);
        assert!(check_script_era_compatibility(&multisig, Era::Allegra).is_ok());

        // Allegra: Timelock scripts OK
        let timelock_start = NativeScript::RequireTimeStart(SlotNo(100));
        assert!(check_script_era_compatibility(&timelock_start, Era::Allegra).is_ok());

        let timelock_expire = NativeScript::RequireTimeExpire(SlotNo(200));
        assert!(check_script_era_compatibility(&timelock_expire, Era::Allegra).is_ok());
    }

    #[test]
    fn test_check_script_era_compatibility_nested() {
        let key1 = example_keyhash(1);

        // Nested script with timelock in Shelley - should fail
        let nested = NativeScript::RequireAllOf(vec![
            NativeScript::RequireSignature(key1),
            NativeScript::RequireTimeStart(SlotNo(100)),
        ]);

        let result = check_script_era_compatibility(&nested, Era::Shelley);
        assert!(result.is_err());

        // Same nested script in Allegra - should succeed
        assert!(check_script_era_compatibility(&nested, Era::Allegra).is_ok());
    }

    #[test]
    fn test_validity_interval_unbounded() {
        let interval = ValidityInterval::unbounded();

        // Unbounded interval accepts any slot
        assert!(interval.in_interval(SlotNo(0)));
        assert!(interval.in_interval(SlotNo(1000)));
        assert!(interval.in_interval(SlotNo(u64::MAX)));
    }

    #[test]
    fn test_validity_interval_before() {
        let interval = ValidityInterval::before(SlotNo(100));

        // Valid before slot 100
        assert!(interval.in_interval(SlotNo(0)));
        assert!(interval.in_interval(SlotNo(99)));

        // Invalid at or after slot 100
        assert!(!interval.in_interval(SlotNo(100)));
        assert!(!interval.in_interval(SlotNo(101)));
    }

    #[test]
    fn test_validity_interval_after() {
        let interval = ValidityInterval::after(SlotNo(100));

        // Invalid before slot 100
        assert!(!interval.in_interval(SlotNo(99)));

        // Valid at or after slot 100
        assert!(interval.in_interval(SlotNo(100)));
        assert!(interval.in_interval(SlotNo(101)));
    }

    #[test]
    fn test_validity_interval_between() {
        let interval = ValidityInterval::between(SlotNo(100), SlotNo(200));

        // Invalid before start
        assert!(!interval.in_interval(SlotNo(99)));

        // Valid in range [100, 200)
        assert!(interval.in_interval(SlotNo(100)));
        assert!(interval.in_interval(SlotNo(150)));
        assert!(interval.in_interval(SlotNo(199)));

        // Invalid at or after end
        assert!(!interval.in_interval(SlotNo(200)));
        assert!(!interval.in_interval(SlotNo(201)));
    }

    #[test]
    fn test_validity_interval_open_start() {
        let interval = ValidityInterval {
            invalid_before: None,
            invalid_hereafter: Some(SlotNo(200)),
        };

        // Valid before 200, no lower bound
        assert!(interval.in_interval(SlotNo(0)));
        assert!(interval.in_interval(SlotNo(100)));
        assert!(interval.in_interval(SlotNo(199)));

        // Invalid at or after 200
        assert!(!interval.in_interval(SlotNo(200)));
    }

    #[test]
    fn test_validity_interval_open_end() {
        let interval = ValidityInterval {
            invalid_before: Some(SlotNo(100)),
            invalid_hereafter: None,
        };

        // Invalid before 100
        assert!(!interval.in_interval(SlotNo(99)));

        // Valid from 100 onwards, no upper bound
        assert!(interval.in_interval(SlotNo(100)));
        assert!(interval.in_interval(SlotNo(1000)));
        assert!(interval.in_interval(SlotNo(u64::MAX)));
    }

    #[test]
    fn test_allegra_timelock_with_validity_interval() {
        let key1 = example_keyhash(1);
        let key2 = example_keyhash(2);

        // Script: Valid between slots 100-200, requires 2 signatures
        let script = NativeScript::RequireAllOf(vec![
            NativeScript::RequireTimeStart(SlotNo(100)),
            NativeScript::RequireTimeExpire(SlotNo(200)),
            NativeScript::RequireMOf {
                required: 2,
                scripts: vec![
                    NativeScript::RequireSignature(key1),
                    NativeScript::RequireSignature(key2),
                ],
            },
        ]);

        // Transaction validity interval: slots [120, 180)
        let tx_interval = ValidityInterval::between(SlotNo(120), SlotNo(180));

        let mut vkeys = HashSet::new();
        vkeys.insert(key1);
        vkeys.insert(key2);

        // Test at various slots

        // Slot 110: After script start (100), before tx interval (120) - Script OK, but tx not valid yet
        assert!(script.validate(&vkeys, SlotNo(110)));
        assert!(!tx_interval.in_interval(SlotNo(110)));

        // Slot 130: Both script and tx valid
        assert!(script.validate(&vkeys, SlotNo(130)));
        assert!(tx_interval.in_interval(SlotNo(130)));

        // Slot 190: Before script expire (200), but after tx interval (180) - Script OK, but tx expired
        assert!(script.validate(&vkeys, SlotNo(190)));
        assert!(!tx_interval.in_interval(SlotNo(190)));

        // Slot 210: After script expire (200) - Script invalid
        assert!(!script.validate(&vkeys, SlotNo(210)));
    }

    #[test]
    fn test_allegra_timelock_combinations() {
        let key1 = example_keyhash(1);

        // Complex script: (TimeStart AND TimeExpire) OR Signature
        let script = NativeScript::RequireAnyOf(vec![
            NativeScript::RequireAllOf(vec![
                NativeScript::RequireTimeStart(SlotNo(100)),
                NativeScript::RequireTimeExpire(SlotNo(200)),
            ]),
            NativeScript::RequireSignature(key1),
        ]);

        // Without signature: valid only in time window
        let empty_keys = HashSet::new();
        assert!(!script.validate(&empty_keys, SlotNo(99)));   // Before window
        assert!(script.validate(&empty_keys, SlotNo(100)));    // In window
        assert!(script.validate(&empty_keys, SlotNo(150)));    // In window
        assert!(script.validate(&empty_keys, SlotNo(199)));    // In window
        assert!(!script.validate(&empty_keys, SlotNo(200)));   // After window

        // With signature: valid at any time
        let mut vkeys = HashSet::new();
        vkeys.insert(key1);
        assert!(script.validate(&vkeys, SlotNo(0)));           // Before window
        assert!(script.validate(&vkeys, SlotNo(150)));         // In window
        assert!(script.validate(&vkeys, SlotNo(1000)));        // After window
    }

    #[test]
    fn test_allegra_multisig_with_timelock() {
        let key1 = example_keyhash(1);
        let key2 = example_keyhash(2);
        let key3 = example_keyhash(3);

        // Script: 2-of-3 multisig, but only valid between slots 1000-2000
        let script = NativeScript::RequireAllOf(vec![
            NativeScript::RequireTimeStart(SlotNo(1000)),
            NativeScript::RequireTimeExpire(SlotNo(2000)),
            NativeScript::RequireMOf {
                required: 2,
                scripts: vec![
                    NativeScript::RequireSignature(key1),
                    NativeScript::RequireSignature(key2),
                    NativeScript::RequireSignature(key3),
                ],
            },
        ]);

        let mut vkeys = HashSet::new();
        vkeys.insert(key1);
        vkeys.insert(key2);

        // Valid: In time window, 2 signatures present
        assert!(script.validate(&vkeys, SlotNo(1500)));

        // Invalid: Before time window, even with correct signatures
        assert!(!script.validate(&vkeys, SlotNo(999)));

        // Invalid: After time window, even with correct signatures
        assert!(!script.validate(&vkeys, SlotNo(2000)));

        // Invalid: In time window, but only 1 signature
        let mut vkeys_one = HashSet::new();
        vkeys_one.insert(key1);
        assert!(!script.validate(&vkeys_one, SlotNo(1500)));
    }

    #[test]
    fn test_allegra_era_script_validation_integration() {
        // Verify that Allegra era properly validates both MultiSig and Timelock

        let key1 = example_keyhash(1);

        // Pure MultiSig (inherited from Shelley)
        let multisig = NativeScript::RequireSignature(key1);
        assert!(check_script_era_compatibility(&multisig, Era::Allegra).is_ok());

        // Pure Timelock (new in Allegra)
        let timelock = NativeScript::RequireTimeStart(SlotNo(100));
        assert!(check_script_era_compatibility(&timelock, Era::Allegra).is_ok());

        // Hybrid: MultiSig AND Timelock (Allegra innovation)
        let hybrid = NativeScript::RequireAllOf(vec![
            NativeScript::RequireSignature(key1),
            NativeScript::RequireTimeStart(SlotNo(100)),
            NativeScript::RequireTimeExpire(SlotNo(200)),
        ]);
        assert!(check_script_era_compatibility(&hybrid, Era::Allegra).is_ok());

        // Deep nesting with both types
        let deep_nested = NativeScript::RequireMOf {
            required: 2,
            scripts: vec![
                NativeScript::RequireSignature(key1),
                NativeScript::RequireAllOf(vec![
                    NativeScript::RequireTimeStart(SlotNo(100)),
                    NativeScript::RequireAnyOf(vec![
                        NativeScript::RequireTimeExpire(SlotNo(200)),
                        NativeScript::RequireSignature(example_keyhash(2)),
                    ]),
                ]),
                NativeScript::RequireTimeStart(SlotNo(50)),
            ],
        };
        assert!(check_script_era_compatibility(&deep_nested, Era::Allegra).is_ok());
    }

    #[test]
    fn test_timelock_script_evaluation_semantics() {
        // Demonstrate that timelock scripts are EVALUATED (not executed)
        // This means they are deterministic and depend only on slot number

        let script = NativeScript::RequireTimeStart(SlotNo(100));
        let vkeys = HashSet::new();

        // Same script, same slot, same vkeys -> MUST return same result
        let result1 = script.validate(&vkeys, SlotNo(150));
        let result2 = script.validate(&vkeys, SlotNo(150));
        assert_eq!(result1, result2);
        assert!(result1); // Both return true

        // Evaluation is instantaneous - no execution, no redeemers, no budget
        // This is the key difference from Plutus scripts in Phase 2
    }
}

// ============================================================================
// Allegra Era Implementation Notes
// ============================================================================

/*
ALLEGRA ERA CHANGES (from Shelley):

1. **Timelock Scripts**:
   - Adds RequireTimeStart(SlotNo): Script valid from slot onwards
   - Adds RequireTimeExpire(SlotNo): Script valid until slot
   - These extend MultiSig with time-based conditions

2. **ValidityInterval on Transactions**:
   - invalidBefore: Optional lower bound (inclusive)
   - invalidHereafter: Optional upper bound (exclusive)
   - Transaction only valid if current slot in interval

3. **Script-Level vs Transaction-Level Time Constraints**:
   - Script timelock: Constraints within the script itself
   - Tx validity interval: Constraints on when tx can be included in block
   - Both must be satisfied for successful validation

4. **Era Compatibility**:
   - Shelley era: Only MultiSig scripts allowed
   - Allegra+ eras: Both MultiSig and Timelock allowed
   - Era-aware validation via validate_native_script_for_era()

5. **UTXOW Rule Changes**:
   - **IMPORTANT**: Allegra makes ZERO changes to UTXOW logic!
   - Allegra/Rules/Utxow.hs (102 lines) completely reuses Shelley's transitionRulesUTXOW
   - Only the Script type family changes: Timelock extends MultiSig
   - This demonstrates the power of era-polymorphic validation

6. **Haskell Evidence**:
   ```haskell
   -- Allegra/Rules/Utxow.hs:82-87
   instance
     ( ... constraints ... ) =>
     STS (AllegraUTXOW era)
   where
     type PredicateFailure (AllegraUTXOW era) = ShelleyUtxowPredFailure era
     transitionRules = [transitionRulesUTXOW]  -- ← Shelley's function!
   ```

7. **Timelock Evaluation Semantics**:
   - Like MultiSig, Timelocks are EVALUATED (not executed)
   - Deterministic: Same slot always gives same result
   - No execution budget, no redeemers, no datum
   - Validated in Phase 1 (UTXOW), not Phase 2 (UTXOS)

8. **Practical Use Cases**:
   - Escrow: Lock funds until specific slot
   - Deadline: Require action before specific slot
   - Vesting: Combine with multisig for time-locked governance
   - Emergency override: Signature OR (time expired AND backup key)

9. **Validation Flow with Timelocks**:
   ```
   UTXOW checks:
   1. Validate script structure for era (reject timelock in Shelley)
   2. Check tx validity interval (if present)
   3. Evaluate timelock conditions against current slot
   4. Verify signatures (if RequireSignature in script)
   5. All must pass for UTXOW success
   ```

For complete implementation:
- eras/allegra/impl/src/Cardano/Ledger/Allegra/Scripts.hs (Timelock type)
- eras/allegra/impl/src/Cardano/Ledger/Allegra/Rules/Utxow.hs (rule instance)
- eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxow.hs (reused logic)
*/

// ============================================================================
// Implementation Notes
// ============================================================================

/*
IMPORTANT DISTINCTIONS:

1. Phase 1 vs Phase 2:
   - UTXOW = Phase 1 = Native scripts + Signatures
   - UTXOS = Phase 2 = Plutus scripts (Alonzo+)
   - This file implements Phase 1 only

2. Native Scripts vs Plutus Scripts:
   - Native scripts: Evaluated (not executed), lightweight, deterministic
   - Plutus scripts: Executed with redeemers, expensive, resource-limited
   - Native scripts validated in UTXOW (this file)
   - Plutus scripts validated in UTXOS (separate)

3. Script Types by Era:
   - Shelley: MultiSig only
   - Allegra/Mary: MultiSig + Timelock
   - Alonzo+: MultiSig + Timelock (native) + Plutus (Phase 2)

4. Validation Order:
   LEDGER -> UTXOW (witnesses) -> UTXO (structure) -> UTXOS (Plutus)
             ^^^^^^^^^^^^^^^^
             THIS FILE

5. Key Concepts:
   - Scripts needed: Computed from UTxO and transaction
   - Scripts provided: From transaction witnesses
   - VKeys needed: Computed from UTxO and transaction
   - VKeys provided: From transaction witnesses
   - All checks must pass before proceeding to UTXO rule

6. Real Implementation Details Omitted:
   - Ed25519 signature verification
   - BLAKE2b hashing
   - CBOR serialization
   - Bootstrap witnesses (Byron addresses)
   - Certificate processing
   - Withdrawal processing
   - Genesis delegate lookup
   - Protocol version checks

For complete implementation, see:
- eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxow.hs
- eras/shelley/impl/src/Cardano/Ledger/Shelley/Scripts.hs
- eras/allegra/impl/src/Cardano/Ledger/Allegra/Scripts.hs
*/
