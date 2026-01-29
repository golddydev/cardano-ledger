// Shelley Era UTXOW Rule Implementation
// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxow.hs
//
// This is a simplified educational implementation demonstrating the
// UTXOW (Unspent Transaction Output Witnessing) validation logic.

use std::collections::{HashMap, HashSet};

// ============================================================================
// Core Types
// ============================================================================

/// 32-byte hash (simplified)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hash([u8; 32]);

pub type KeyHash = Hash;
pub type ScriptHash = Hash;
pub type TxBodyHash = Hash;
pub type MetadataHash = Hash;

/// Ed25519 verification key (32 bytes)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VKey(pub [u8; 32]);

impl VKey {
    /// Hash to KeyHash (BLAKE2b-224 in real implementation)
    pub fn hash(&self) -> KeyHash {
        // Simplified - real impl uses BLAKE2b-224
        Hash([0u8; 32])
    }
}

/// Ed25519 signature (64 bytes)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature(pub [u8; 64]);

/// Slot number for timelock validation
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SlotNo(pub u64);

// ============================================================================
// Native Scripts (Shelley MultiSig)
// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Scripts.hs
// ============================================================================

/// Native script types
/// Reference: Scripts.hs:57-62
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeScript {
    /// Require signature from specific key
    RequireSignature(KeyHash),
    /// Require ALL sub-scripts to validate
    RequireAllOf(Vec<NativeScript>),
    /// Require ANY sub-script to validate
    RequireAnyOf(Vec<NativeScript>),
    /// Require at least M of N sub-scripts to validate
    RequireMOf { required: usize, scripts: Vec<NativeScript> },
}

impl NativeScript {
    /// Evaluate native script
    /// Reference: Scripts.hs:233-249 (evalMultiSig)
    ///
    /// # Arguments
    /// * `vkey_hashes` - Set of key hashes that have provided signatures
    ///
    /// # Returns
    /// * `true` if the script validates, `false` otherwise
    pub fn validate(&self, vkey_hashes: &HashSet<KeyHash>) -> bool {
        match self {
            // RequireSignature hk -> Set.member hk vhks
            NativeScript::RequireSignature(key_hash) => vkey_hashes.contains(key_hash),

            // RequireAllOf msigs -> all go msigs
            NativeScript::RequireAllOf(scripts) => {
                scripts.iter().all(|s| s.validate(vkey_hashes))
            }

            // RequireAnyOf msigs -> any go msigs
            NativeScript::RequireAnyOf(scripts) => {
                scripts.iter().any(|s| s.validate(vkey_hashes))
            }

            // RequireMOf m msigs -> m <= sum [if go msig then 1 else 0 | msig <- msigs]
            NativeScript::RequireMOf { required, scripts } => {
                let valid_count = scripts.iter().filter(|s| s.validate(vkey_hashes)).count();
                valid_count >= *required
            }
        }
    }

    /// Compute script hash (BLAKE2b-256 of CBOR in real implementation)
    pub fn hash(&self) -> ScriptHash {
        Hash([0u8; 32])
    }
}

// ============================================================================
// Transaction Types
// ============================================================================

/// Transaction input reference
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TxIn {
    pub tx_id: Hash,
    pub index: u32,
}

/// Transaction output
#[derive(Debug, Clone)]
pub struct TxOut {
    pub address: Address,
    pub value: u64, // Simplified - real impl uses multi-asset Value
}

/// Address with payment credential
#[derive(Debug, Clone)]
pub struct Address {
    pub payment: PaymentCredential,
    pub staking: Option<StakingCredential>,
}

/// Payment credential (key or script)
#[derive(Debug, Clone)]
pub enum PaymentCredential {
    KeyHash(KeyHash),
    ScriptHash(ScriptHash),
}

/// Staking credential
#[derive(Debug, Clone)]
pub enum StakingCredential {
    KeyHash(KeyHash),
    ScriptHash(ScriptHash),
}

/// Reward account for withdrawals
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RewardAccount {
    pub credential: StakingCredential,
}

/// Shelley certificate types (simplified)
#[derive(Debug, Clone)]
pub enum ShelleyTxCert {
    /// Register staking credential
    RegKey(StakingCredential),
    /// Deregister staking credential
    DeRegKey(StakingCredential),
    /// Delegate to pool
    Delegate { credential: StakingCredential, pool: KeyHash },
    /// Register stake pool
    RegPool(PoolParams),
    /// Retire stake pool
    RetirePool { pool_id: KeyHash, epoch: u64 },
    /// Move Instantaneous Rewards (requires genesis quorum)
    MIR { pot: MIRPot, amount: u64, target: KeyHash },
}

#[derive(Debug, Clone)]
pub struct PoolParams {
    pub pool_id: KeyHash,
    pub owners: HashSet<KeyHash>,
}

#[derive(Debug, Clone)]
pub enum MIRPot {
    Treasury,
    Reserves,
}

/// VKey witness (public key + signature)
/// Reference: Cardano.Ledger.Keys
#[derive(Debug, Clone)]
pub struct VKeyWitness {
    pub vkey: VKey,
    pub signature: Signature,
}

impl VKeyWitness {
    pub fn key_hash(&self) -> KeyHash {
        self.vkey.hash()
    }

    /// Verify signature against transaction body hash
    /// Reference: Cardano.Ledger.Keys.verifyWitVKey
    pub fn verify(&self, _tx_body_hash: TxBodyHash) -> bool {
        // Real implementation uses Ed25519 verification
        true
    }
}

/// Transaction body
#[derive(Debug, Clone)]
pub struct TxBody {
    pub inputs: HashSet<TxIn>,
    pub outputs: Vec<TxOut>,
    pub fee: u64,
    pub ttl: Option<SlotNo>,
    pub certificates: Vec<ShelleyTxCert>,
    pub withdrawals: HashMap<RewardAccount, u64>,
    pub update: Option<Update>,
    pub auxiliary_data_hash: Option<MetadataHash>,
}

/// Protocol parameter update
#[derive(Debug, Clone)]
pub struct Update {
    pub proposed_by: HashSet<KeyHash>, // Genesis delegates proposing
}

/// Transaction witnesses
#[derive(Debug, Clone, Default)]
pub struct TxWits {
    pub vkey_wits: Vec<VKeyWitness>,
    pub script_wits: HashMap<ScriptHash, NativeScript>,
}

/// Auxiliary data (metadata)
#[derive(Debug, Clone)]
pub struct AuxiliaryData {
    pub metadata: HashMap<u64, Vec<u8>>,
}

impl AuxiliaryData {
    pub fn hash(&self) -> MetadataHash {
        // Real implementation: BLAKE2b-256 of CBOR
        Hash([0u8; 32])
    }
}

/// Complete transaction
#[derive(Debug, Clone)]
pub struct Tx {
    pub body: TxBody,
    pub wits: TxWits,
    pub auxiliary_data: Option<AuxiliaryData>,
}

impl Tx {
    pub fn body_hash(&self) -> TxBodyHash {
        // Real implementation: BLAKE2b-256 of CBOR-encoded body
        Hash([0u8; 32])
    }
}

// ============================================================================
// UTxO State
// ============================================================================

/// Unspent Transaction Output set
pub type UTxO = HashMap<TxIn, TxOut>;

/// Genesis delegates configuration
#[derive(Debug, Clone, Default)]
pub struct GenDelegs {
    /// Maps genesis key hash -> delegate key hash
    pub delegates: HashMap<KeyHash, KeyHash>,
}

/// Certificate state (delegation state)
#[derive(Debug, Clone, Default)]
pub struct CertState {
    pub gen_delegs: GenDelegs,
}

// ============================================================================
// Predicate Failures (Errors)
// Reference: Utxow.hs:85-112
// ============================================================================

/// Shelley UTXOW predicate failures
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShelleyUtxowPredFailure {
    /// VKey signature verification failed
    /// Reference: Utxow.hs:87
    InvalidWitnessesUTXOW(Vec<KeyHash>),

    /// Required VKey witnesses not provided
    /// Reference: Utxow.hs:90
    MissingVKeyWitnessesUTXOW(HashSet<KeyHash>),

    /// Required scripts not provided
    /// Reference: Utxow.hs:93
    MissingScriptWitnessesUTXOW(HashSet<ScriptHash>),

    /// Native script evaluated to false
    /// Reference: Utxow.hs:96
    ScriptWitnessNotValidatingUTXOW(HashSet<ScriptHash>),

    /// Error from embedded UTXO rule
    /// Reference: Utxow.hs:99
    UtxoFailure(String), // Simplified - real impl uses nested error type

    /// Auxiliary data provided but no hash in body
    /// Reference: Utxow.hs:100
    MissingTxBodyMetadataHash(MetadataHash),

    /// Hash in body but no auxiliary data
    /// Reference: Utxow.hs:101
    MissingTxMetadata(MetadataHash),

    /// Auxiliary data hash doesn't match
    /// Reference: Utxow.hs:102
    ConflictingMetadataHash { expected: MetadataHash, actual: MetadataHash },

    /// Invalid auxiliary data (Shelley: never used)
    /// Reference: Utxow.hs:103
    InvalidMetadata,

    /// Scripts provided but not needed
    /// Reference: Utxow.hs:104
    ExtraneousScriptWitnessesUTXOW(HashSet<ScriptHash>),

    /// MIR certificate without enough genesis signatures
    /// Reference: Utxow.hs:105
    MIRInsufficientGenesisSigsUTXOW(HashSet<KeyHash>),
}

// ============================================================================
// Scripts Needed Computation
// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/UTxO.hs:103-119
// ============================================================================

/// Compute which script hashes are needed for this transaction
/// Reference: UTxO.hs:103-119 (getShelleyScriptsNeeded)
///
/// Collects scripts from:
/// - Inputs locked by script addresses
/// - Withdrawals from script-locked reward accounts
/// - Certificates authorized by scripts
pub fn get_shelley_scripts_needed(utxo: &UTxO, tx_body: &TxBody) -> HashSet<ScriptHash> {
    let mut needed = HashSet::new();

    // 1. Scripts from inputs
    // scriptHashes = txinsScriptHashes (txBody ^. inputsTxBodyL) u
    for txin in &tx_body.inputs {
        if let Some(txout) = utxo.get(txin) {
            if let PaymentCredential::ScriptHash(sh) = &txout.address.payment {
                needed.insert(*sh);
            }
        }
    }

    // 2. Scripts from withdrawals
    // [sh | w <- withdrawals, Just sh <- [credScriptHash (raCredential w)]]
    for reward_account in tx_body.withdrawals.keys() {
        if let StakingCredential::ScriptHash(sh) = &reward_account.credential {
            needed.insert(*sh);
        }
    }

    // 3. Scripts from certificates
    // [sh | c <- certificates, Just sh <- [getScriptWitnessTxCert c]]
    for cert in &tx_body.certificates {
        if let Some(sh) = get_script_witness_tx_cert(cert) {
            needed.insert(sh);
        }
    }

    needed
}

/// Get script hash from certificate if it requires script authorization
fn get_script_witness_tx_cert(cert: &ShelleyTxCert) -> Option<ScriptHash> {
    match cert {
        ShelleyTxCert::DeRegKey(StakingCredential::ScriptHash(sh)) => Some(*sh),
        ShelleyTxCert::Delegate { credential: StakingCredential::ScriptHash(sh), .. } => Some(*sh),
        _ => None,
    }
}

// ============================================================================
// VKey Witnesses Needed Computation
// Reference: UTxO.hs:223-280
// ============================================================================

/// Compute which key hashes must sign this transaction
/// Reference: UTxO.hs:270-280 (getShelleyWitsVKeyNeeded)
pub fn get_shelley_wits_vkey_needed(
    cert_state: &CertState,
    utxo: &UTxO,
    tx_body: &TxBody,
) -> HashSet<KeyHash> {
    let mut needed = get_shelley_wits_vkey_needed_no_gov(utxo, tx_body);

    // Add genesis delegate witnesses for protocol updates
    // witsVKeyNeededGenDelegs txBody (dsGenDelegs (certState ^. certDStateL))
    needed.extend(wits_vkey_needed_gen_delegs(tx_body, &cert_state.gen_delegs));

    needed
}

/// VKey witnesses needed (excluding governance)
/// Reference: UTxO.hs:223-268 (getShelleyWitsVKeyNeededNoGov)
pub fn get_shelley_wits_vkey_needed_no_gov(utxo: &UTxO, tx_body: &TxBody) -> HashSet<KeyHash> {
    let mut needed = HashSet::new();

    // 1. Input authors - keys owning UTxO inputs
    for txin in &tx_body.inputs {
        if let Some(txout) = utxo.get(txin) {
            if let PaymentCredential::KeyHash(kh) = &txout.address.payment {
                needed.insert(*kh);
            }
        }
    }

    // 2. Withdrawal authors - keys authorizing withdrawals
    for reward_account in tx_body.withdrawals.keys() {
        if let StakingCredential::KeyHash(kh) = &reward_account.credential {
            needed.insert(*kh);
        }
    }

    // 3. Certificate authors - keys authorizing certificates
    for cert in &tx_body.certificates {
        if let Some(kh) = get_vkey_witness_tx_cert(cert) {
            needed.insert(kh);
        }
    }

    // 4. Pool owners - for pool registration
    for cert in &tx_body.certificates {
        if let ShelleyTxCert::RegPool(params) = cert {
            needed.extend(params.owners.iter().copied());
        }
    }

    needed
}

/// Get VKey witness from certificate
fn get_vkey_witness_tx_cert(cert: &ShelleyTxCert) -> Option<KeyHash> {
    match cert {
        ShelleyTxCert::DeRegKey(StakingCredential::KeyHash(kh)) => Some(*kh),
        ShelleyTxCert::Delegate { credential: StakingCredential::KeyHash(kh), .. } => Some(*kh),
        ShelleyTxCert::RetirePool { pool_id, .. } => Some(*pool_id),
        _ => None,
    }
}

/// Genesis delegate witnesses needed for protocol updates
/// Reference: UTxO.hs:206-219 (witsVKeyNeededGenDelegs)
fn wits_vkey_needed_gen_delegs(tx_body: &TxBody, gen_delegs: &GenDelegs) -> HashSet<KeyHash> {
    if let Some(update) = &tx_body.update {
        // Proposed updates require genesis delegate signatures
        update
            .proposed_by
            .iter()
            .filter_map(|genesis_key| gen_delegs.delegates.get(genesis_key).copied())
            .collect()
    } else {
        HashSet::new()
    }
}

// ============================================================================
// Validation Functions
// Reference: Utxow.hs:184-289
// ============================================================================

/// Validate failed native scripts
/// Reference: Utxow.hs:184-197 (validateFailedNativeScripts)
///
/// Evaluates all native scripts. Fails if any returns false.
pub fn validate_failed_native_scripts(
    scripts_provided: &HashMap<ScriptHash, NativeScript>,
    tx: &Tx,
) -> Result<(), ShelleyUtxowPredFailure> {
    // Get key hashes from VKey witnesses
    let vkey_hashes: HashSet<KeyHash> = tx.wits.vkey_wits.iter().map(|w| w.key_hash()).collect();

    // Find scripts that fail validation
    let failed_scripts: HashSet<ScriptHash> = scripts_provided
        .iter()
        .filter(|(_, script)| !script.validate(&vkey_hashes))
        .map(|(hash, _)| *hash)
        .collect();

    if failed_scripts.is_empty() {
        Ok(())
    } else {
        Err(ShelleyUtxowPredFailure::ScriptWitnessNotValidatingUTXOW(
            failed_scripts,
        ))
    }
}

/// Validate missing scripts
/// Reference: Utxow.hs:382-389 (validateMissingScripts)
///
/// Checks that exactly the needed scripts are provided.
pub fn validate_missing_scripts(
    scripts_needed: &HashSet<ScriptHash>,
    scripts_provided: &HashMap<ScriptHash, NativeScript>,
) -> Result<(), ShelleyUtxowPredFailure> {
    let scripts_received: HashSet<ScriptHash> = scripts_provided.keys().copied().collect();

    // Missing = needed - received
    let missing: HashSet<ScriptHash> = scripts_needed
        .difference(&scripts_received)
        .copied()
        .collect();

    // Extra = received - needed
    let extra: HashSet<ScriptHash> = scripts_received
        .difference(scripts_needed)
        .copied()
        .collect();

    if !missing.is_empty() {
        Err(ShelleyUtxowPredFailure::MissingScriptWitnessesUTXOW(missing))
    } else if !extra.is_empty() {
        Err(ShelleyUtxowPredFailure::ExtraneousScriptWitnessesUTXOW(extra))
    } else {
        Ok(())
    }
}

/// Validate verified witnesses
/// Reference: Utxow.hs:210-226 (validateVerifiedWits)
///
/// Cryptographically verifies all VKey signatures.
pub fn validate_verified_wits(tx: &Tx) -> Result<(), ShelleyUtxowPredFailure> {
    let tx_body_hash = tx.body_hash();

    // Find witnesses where verification fails
    let failed_wits: Vec<KeyHash> = tx
        .wits
        .vkey_wits
        .iter()
        .filter(|wit| !wit.verify(tx_body_hash))
        .map(|wit| wit.key_hash())
        .collect();

    if failed_wits.is_empty() {
        Ok(())
    } else {
        Err(ShelleyUtxowPredFailure::InvalidWitnessesUTXOW(failed_wits))
    }
}

/// Validate needed witnesses
/// Reference: Utxow.hs:422-434 (validateNeededWitnesses)
///
/// Checks that all required VKey witnesses are present.
pub fn validate_needed_witnesses(
    wits_key_hashes: &HashSet<KeyHash>,
    cert_state: &CertState,
    utxo: &UTxO,
    tx_body: &TxBody,
) -> Result<(), ShelleyUtxowPredFailure> {
    let needed = get_shelley_wits_vkey_needed(cert_state, utxo, tx_body);
    let missing: HashSet<KeyHash> = needed.difference(wits_key_hashes).copied().collect();

    if missing.is_empty() {
        Ok(())
    } else {
        Err(ShelleyUtxowPredFailure::MissingVKeyWitnessesUTXOW(missing))
    }
}

/// Validate metadata
/// Reference: Utxow.hs:234-261 (validateMetadata)
///
/// Checks metadata hash consistency.
pub fn validate_metadata(tx: &Tx) -> Result<(), ShelleyUtxowPredFailure> {
    match (&tx.body.auxiliary_data_hash, &tx.auxiliary_data) {
        // No hash, no data: OK
        (None, None) => Ok(()),

        // Hash but no data: Error
        (Some(hash), None) => Err(ShelleyUtxowPredFailure::MissingTxMetadata(*hash)),

        // Data but no hash: Error
        (None, Some(aux_data)) => Err(ShelleyUtxowPredFailure::MissingTxBodyMetadataHash(
            aux_data.hash(),
        )),

        // Both present: Check match
        (Some(body_hash), Some(aux_data)) => {
            let computed_hash = aux_data.hash();
            if *body_hash == computed_hash {
                Ok(())
            } else {
                Err(ShelleyUtxowPredFailure::ConflictingMetadataHash {
                    expected: *body_hash,
                    actual: computed_hash,
                })
            }
        }
    }
}

/// Validate MIR insufficient genesis signatures
/// Reference: Utxow.hs:267-288 (validateMIRInsufficientGenesisSigs)
///
/// Checks that MIR certificates have enough genesis signatures.
pub fn validate_mir_insufficient_genesis_sigs(
    gen_delegs: &GenDelegs,
    quorum: u64,
    wits_key_hashes: &HashSet<KeyHash>,
    tx: &Tx,
) -> Result<(), ShelleyUtxowPredFailure> {
    // Check if tx contains MIR certificates
    let has_mir = tx.body.certificates.iter().any(|cert| matches!(cert, ShelleyTxCert::MIR { .. }));

    if !has_mir {
        return Ok(());
    }

    // Count genesis delegate signatures
    let gen_sigs: HashSet<KeyHash> = gen_delegs
        .delegates
        .values()
        .filter(|delegate| wits_key_hashes.contains(*delegate))
        .copied()
        .collect();

    if gen_sigs.len() as u64 >= quorum {
        Ok(())
    } else {
        Err(ShelleyUtxowPredFailure::MIRInsufficientGenesisSigsUTXOW(
            gen_sigs,
        ))
    }
}

// ============================================================================
// Main UTXOW Transition Function
// Reference: Utxow.hs:296-333 (transitionRulesUTXOW)
// ============================================================================

/// UTXOW environment
pub struct UtxoEnv {
    pub slot: SlotNo,
    pub quorum: u64,
}

/// Shelley UTXOW validation
/// Reference: Utxow.hs:296-333 (transitionRulesUTXOW)
///
/// This is the main entry point for Phase 1 witness validation.
/// After this passes, the transaction proceeds to UTXO structural validation.
pub fn shelley_utxow_transition(
    env: &UtxoEnv,
    cert_state: &CertState,
    utxo: &UTxO,
    tx: &Tx,
) -> Result<(), ShelleyUtxowPredFailure> {
    // Extract witness key hashes
    // witsKeyHashes := { hashKey vk | vk ∈ dom(txwitsVKey txw) }
    let wits_key_hashes: HashSet<KeyHash> =
        tx.wits.vkey_wits.iter().map(|w| w.key_hash()).collect();

    // Get scripts provided
    let scripts_provided = &tx.wits.script_wits;

    // Step 1: Validate native scripts (line 308)
    // ∀ s ∈ range(txscripts txw) ∩ Scriptnative, runNativeScript s tx
    validate_failed_native_scripts(scripts_provided, tx)?;

    // Step 2: Check script presence (line 311)
    // { s | (_,s) ∈ scriptsNeeded utxo tx} = dom(txscripts txw)
    let scripts_needed = get_shelley_scripts_needed(utxo, &tx.body);
    validate_missing_scripts(&scripts_needed, scripts_provided)?;

    // Step 3: Verify VKey signatures (line 316)
    // ∀ (vk ↦ σ) ∈ (txwitsVKey txw), V_vk⟦ txBodyHash ⟧_σ
    validate_verified_wits(tx)?;

    // Step 4: Check required witnesses (line 319)
    // witsVKeyNeeded utxo tx genDelegs ⊆ witsKeyHashes
    validate_needed_witnesses(&wits_key_hashes, cert_state, utxo, &tx.body)?;

    // Step 5: Validate metadata (line 323)
    // (adh = ◇ ∧ ad = ◇) ∨ (adh = hashAD ad)
    validate_metadata(tx)?;

    // Step 6: Check MIR genesis signatures (line 328)
    // { c ∈ txcerts txb ∩ TxCert_mir } ≠ ∅ ⇒ |genSig| ≥ Quorum
    validate_mir_insufficient_genesis_sigs(
        &cert_state.gen_delegs,
        env.quorum,
        &wits_key_hashes,
        tx,
    )?;

    // Step 7: Call UTXO rule (line 333)
    // In real implementation, this would call the UTXO transition function
    // trans @(EraRule "UTXO" era) $ TRC (utxoEnv, u, tx)

    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_key_hash(id: u8) -> KeyHash {
        let mut bytes = [0u8; 32];
        bytes[0] = id;
        Hash(bytes)
    }

    fn make_script_hash(id: u8) -> ScriptHash {
        let mut bytes = [0u8; 32];
        bytes[0] = id;
        Hash(bytes)
    }

    #[test]
    fn test_native_script_require_signature() {
        let key1 = make_key_hash(1);
        let key2 = make_key_hash(2);

        let script = NativeScript::RequireSignature(key1);

        // With key1 present: validates
        let mut keys = HashSet::new();
        keys.insert(key1);
        assert!(script.validate(&keys));

        // With different key: fails
        let mut keys = HashSet::new();
        keys.insert(key2);
        assert!(!script.validate(&keys));
    }

    #[test]
    fn test_native_script_require_all_of() {
        let key1 = make_key_hash(1);
        let key2 = make_key_hash(2);

        let script = NativeScript::RequireAllOf(vec![
            NativeScript::RequireSignature(key1),
            NativeScript::RequireSignature(key2),
        ]);

        // With both keys: validates
        let mut keys = HashSet::new();
        keys.insert(key1);
        keys.insert(key2);
        assert!(script.validate(&keys));

        // With only key1: fails
        let mut keys = HashSet::new();
        keys.insert(key1);
        assert!(!script.validate(&keys));
    }

    #[test]
    fn test_native_script_require_m_of_n() {
        let key1 = make_key_hash(1);
        let key2 = make_key_hash(2);
        let key3 = make_key_hash(3);

        // 2-of-3 multisig
        let script = NativeScript::RequireMOf {
            required: 2,
            scripts: vec![
                NativeScript::RequireSignature(key1),
                NativeScript::RequireSignature(key2),
                NativeScript::RequireSignature(key3),
            ],
        };

        // With 2 keys: validates
        let mut keys = HashSet::new();
        keys.insert(key1);
        keys.insert(key3);
        assert!(script.validate(&keys));

        // With 1 key: fails
        let mut keys = HashSet::new();
        keys.insert(key1);
        assert!(!script.validate(&keys));
    }

    #[test]
    fn test_validate_missing_scripts() {
        let sh1 = make_script_hash(1);
        let sh2 = make_script_hash(2);

        let mut needed = HashSet::new();
        needed.insert(sh1);

        // Script provided: OK
        let mut provided = HashMap::new();
        provided.insert(sh1, NativeScript::RequireSignature(make_key_hash(1)));
        assert!(validate_missing_scripts(&needed, &provided).is_ok());

        // Script missing: Error
        let provided = HashMap::new();
        let result = validate_missing_scripts(&needed, &provided);
        assert!(matches!(
            result,
            Err(ShelleyUtxowPredFailure::MissingScriptWitnessesUTXOW(_))
        ));

        // Extra script: Error
        let mut provided = HashMap::new();
        provided.insert(sh1, NativeScript::RequireSignature(make_key_hash(1)));
        provided.insert(sh2, NativeScript::RequireSignature(make_key_hash(2)));
        let result = validate_missing_scripts(&needed, &provided);
        assert!(matches!(
            result,
            Err(ShelleyUtxowPredFailure::ExtraneousScriptWitnessesUTXOW(_))
        ));
    }
}
