// Conway Era UTXOW Rule Implementation
// Reference: eras/conway/impl/src/Cardano/Ledger/Conway/Rules/Utxow.hs
//
// Conway introduces:
// - Flattened error hierarchy
// - Governance witnesses (voters)
// - New script purposes (Voting, Proposing)
// - PlutusV3 optional datums (CIP-0069)
// - MIR removal

use std::collections::{HashMap, HashSet};

// ============================================================================
// Core Types
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Credential {
    KeyHash(KeyHash),
    ScriptHash(ScriptHash),
}

impl Credential {
    pub fn key_hash(&self) -> Option<KeyHash> {
        match self {
            Credential::KeyHash(kh) => Some(*kh),
            Credential::ScriptHash(_) => None,
        }
    }

    pub fn script_hash(&self) -> Option<ScriptHash> {
        match self {
            Credential::ScriptHash(sh) => Some(*sh),
            Credential::KeyHash(_) => None,
        }
    }
}

// ============================================================================
// Conway-Specific Types
// ============================================================================

/// Plutus language version (now includes V3)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Language {
    PlutusV1,
    PlutusV2,
    PlutusV3, // NEW in Conway
}

/// Datum options
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Datum {
    NoDatum,
    DatumHash(DataHash),
    InlineDatum(Vec<u8>),
}

/// Script types (now includes PlutusV3)
#[derive(Debug, Clone)]
pub enum Script {
    Native(NativeScript),
    PlutusV1(Vec<u8>),
    PlutusV2(Vec<u8>),
    PlutusV3(Vec<u8>), // NEW in Conway
}

impl Script {
    pub fn is_native(&self) -> bool {
        matches!(self, Script::Native(_))
    }

    pub fn language(&self) -> Option<Language> {
        match self {
            Script::Native(_) => None,
            Script::PlutusV1(_) => Some(Language::PlutusV1),
            Script::PlutusV2(_) => Some(Language::PlutusV2),
            Script::PlutusV3(_) => Some(Language::PlutusV3),
        }
    }

    pub fn hash(&self) -> ScriptHash {
        [0u8; 32]
    }

    pub fn is_well_formed(&self) -> bool {
        match self {
            Script::Native(_) => true,
            Script::PlutusV1(b) | Script::PlutusV2(b) | Script::PlutusV3(b) => !b.is_empty(),
        }
    }
}

/// Native script (simplified)
#[derive(Debug, Clone)]
pub enum NativeScript {
    RequireSignature(KeyHash),
    RequireAllOf(Vec<NativeScript>),
}

/// Script purpose (extended in Conway)
/// Reference: Cardano.Ledger.Conway.Scripts
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PlutusPurpose {
    Spending(u32),
    Minting(u32),
    Rewarding(u32),
    Certifying(u32),
    Voting(u32),     // NEW in Conway
    Proposing(u32),  // NEW in Conway
}

/// Voter types (CIP-1694)
/// Reference: Cardano.Ledger.Conway.Governance
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Voter {
    CommitteeVoter(Credential),
    DRepVoter(Credential),
    StakePoolVoter(KeyHash),
}

impl Voter {
    /// Get script hash if voter uses a script
    pub fn script_hash(&self) -> Option<ScriptHash> {
        match self {
            Voter::CommitteeVoter(cred) => cred.script_hash(),
            Voter::DRepVoter(cred) => cred.script_hash(),
            Voter::StakePoolVoter(_) => None, // SPOs use key witnesses
        }
    }

    /// Get key hash if voter uses a key
    pub fn key_hash(&self) -> Option<KeyHash> {
        match self {
            Voter::CommitteeVoter(cred) => cred.key_hash(),
            Voter::DRepVoter(cred) => cred.key_hash(),
            Voter::StakePoolVoter(pool_id) => Some(*pool_id),
        }
    }
}

/// Governance action types
#[derive(Debug, Clone)]
pub enum GovAction {
    ParameterChange {
        guardrails_script: Option<ScriptHash>,
    },
    TreasuryWithdrawals {
        guardrails_script: Option<ScriptHash>,
    },
    HardFork,
    NoConfidence,
    UpdateCommittee,
    NewConstitution,
    Info,
}

/// Proposal procedure
#[derive(Debug, Clone)]
pub struct ProposalProcedure {
    pub gov_action: GovAction,
    pub deposit: u64,
    pub return_addr: Credential,
}

impl ProposalProcedure {
    /// Get guardrails script if needed
    pub fn guardrails_script(&self) -> Option<ScriptHash> {
        match &self.gov_action {
            GovAction::ParameterChange { guardrails_script } => *guardrails_script,
            GovAction::TreasuryWithdrawals { guardrails_script } => *guardrails_script,
            _ => None,
        }
    }
}

/// Voting procedure
#[derive(Debug, Clone)]
pub struct VotingProcedure {
    pub vote: Vote,
    pub anchor: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Vote {
    Yes,
    No,
    Abstain,
}

/// Conway TxOut
#[derive(Debug, Clone)]
pub struct ConwayTxOut {
    pub address: Credential,
    pub value: u64,
    pub datum: Datum,
    pub reference_script: Option<Script>,
}

/// Conway transaction body
/// Reference: Cardano.Ledger.Conway.TxBody
#[derive(Debug, Clone)]
pub struct ConwayTxBody {
    pub inputs: Vec<TxIn>,
    pub reference_inputs: HashSet<TxIn>,
    pub outputs: Vec<ConwayTxOut>,
    pub fee: u64,
    pub mint: HashSet<ScriptHash>,
    pub certificates: Vec<ConwayCert>,
    pub withdrawals: HashMap<Credential, u64>,
    pub voting_procedures: HashMap<Voter, Vec<VotingProcedure>>,  // NEW
    pub proposal_procedures: Vec<ProposalProcedure>,              // NEW
    pub script_integrity_hash: Option<Hash>,
    pub required_signers: HashSet<KeyHash>,
}

/// Conway certificate types (simplified)
#[derive(Debug, Clone)]
pub enum ConwayCert {
    RegDRep(Credential),
    UnRegDRep(Credential),
    Delegate { credential: Credential, pool: KeyHash },
    // Note: MIR certificate is NOT in Conway!
}

/// Redeemer
#[derive(Debug, Clone)]
pub struct Redeemer {
    pub data: Vec<u8>,
    pub ex_units: (u64, u64),
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

/// Conway transaction witnesses
#[derive(Debug, Clone, Default)]
pub struct ConwayTxWits {
    pub vkey_wits: Vec<VKeyWitness>,
    pub scripts: HashMap<ScriptHash, Script>,
    pub datums: HashMap<DataHash, Vec<u8>>,
    pub redeemers: HashMap<PlutusPurpose, Redeemer>,
}

/// Complete Conway transaction
#[derive(Debug, Clone)]
pub struct ConwayTx {
    pub body: ConwayTxBody,
    pub wits: ConwayTxWits,
}

/// UTxO set
pub type UTxO = HashMap<TxIn, ConwayTxOut>;

// ============================================================================
// Predicate Failures (Flattened in Conway)
// Reference: Utxow.hs:75-136
// ============================================================================

/// Conway UTXOW predicate failures (FLATTENED - no nesting!)
/// Reference: Utxow.hs:75-136
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConwayUtxowPredFailure {
    // Shelley errors (directly embedded):
    InvalidWitnessesUTXOW(Vec<KeyHash>),
    MissingVKeyWitnessesUTXOW(HashSet<KeyHash>),
    MissingScriptWitnessesUTXOW(HashSet<ScriptHash>),
    ScriptWitnessNotValidatingUTXOW(HashSet<ScriptHash>),
    MissingTxBodyMetadataHash(Hash),
    MissingTxMetadata(Hash),
    ConflictingMetadataHash { expected: Hash, actual: Hash },
    InvalidMetadata,
    ExtraneousScriptWitnessesUTXOW(HashSet<ScriptHash>),
    // NOTE: MIRInsufficientGenesisSigsUTXOW is REMOVED!

    // Alonzo errors (directly embedded):
    MissingRedeemers(Vec<(PlutusPurpose, ScriptHash)>),
    MissingRequiredDatums { missing: HashSet<DataHash>, provided: HashSet<DataHash> },
    NotAllowedSupplementalDatums { unallowed: HashSet<DataHash>, allowed: HashSet<DataHash> },
    UnspendableUTxONoDatumHash(HashSet<TxIn>),
    ExtraRedeemers(Vec<PlutusPurpose>),

    // Babbage errors (directly embedded):
    MalformedScriptWitnesses(HashSet<ScriptHash>),
    MalformedReferenceScripts(HashSet<ScriptHash>),

    /// PPViewHashesDontMatch - script integrity hash in tx doesn't match computed
    /// Reference: Alonzo/Rules/Utxow.hs (ppViewHashesDontMatch)
    PPViewHashesDontMatch { expected: Option<Hash>, actual: Option<Hash> },

    /// ScriptIntegrityHashMismatch - more detailed version with original bytes
    /// Reference: Babbage/Rules/Utxow.hs
    ScriptIntegrityHashMismatch {
        expected: Option<Hash>,
        actual: Option<Hash>,
        original_bytes: Option<Vec<u8>>,
    },

    // UTXO rule failure
    UtxoFailure(String),
}

// ============================================================================
// Error Conversion Functions
// Reference: Utxow.hs:306-345
// ============================================================================

/// Alonzo errors (for conversion)
/// Reference: Alonzo/Rules/Utxow.hs:87-112
#[derive(Debug, Clone)]
pub enum AlonzoUtxowPredFailure {
    ShelleyInAlonzoUtxowPredFailure(ShelleyUtxowPredFailure),
    MissingRedeemers(Vec<(PlutusPurpose, ScriptHash)>),
    MissingRequiredDatums { missing: HashSet<DataHash>, provided: HashSet<DataHash> },
    NotAllowedSupplementalDatums { unallowed: HashSet<DataHash>, allowed: HashSet<DataHash> },
    PPViewHashesDontMatch { expected: Option<Hash>, actual: Option<Hash> },
    UnspendableUTxONoDatumHash(HashSet<TxIn>),
    ExtraRedeemers(Vec<PlutusPurpose>),
    ScriptIntegrityHashMismatch {
        expected: Option<Hash>,
        actual: Option<Hash>,
        original_bytes: Option<Vec<u8>>,
    },
}

/// Shelley errors (for conversion)
#[derive(Debug, Clone)]
pub enum ShelleyUtxowPredFailure {
    InvalidWitnessesUTXOW(Vec<KeyHash>),
    MissingVKeyWitnessesUTXOW(HashSet<KeyHash>),
    MissingScriptWitnessesUTXOW(HashSet<ScriptHash>),
    MIRInsufficientGenesisSigsUTXOW(HashSet<KeyHash>), // Still exists for conversion!
}

/// Babbage errors (for conversion)
/// Reference: Babbage/Rules/Utxow.hs:93-109
#[derive(Debug, Clone)]
pub enum BabbageUtxowPredFailure {
    AlonzoInBabbageUtxowPredFailure(AlonzoUtxowPredFailure),
    UtxoFailure(String),
    MalformedScriptWitnesses(HashSet<ScriptHash>),
    MalformedReferenceScripts(HashSet<ScriptHash>),
    ScriptIntegrityHashMismatch {
        expected: Option<Hash>,
        actual: Option<Hash>,
        original_bytes: Option<Vec<u8>>,
    },
}

/// Convert Shelley error to Conway
/// Reference: Utxow.hs:327-345 (shelleyToConwayUtxowPredFailure)
pub fn shelley_to_conway(err: ShelleyUtxowPredFailure) -> ConwayUtxowPredFailure {
    match err {
        ShelleyUtxowPredFailure::InvalidWitnessesUTXOW(xs) => {
            ConwayUtxowPredFailure::InvalidWitnessesUTXOW(xs)
        }
        ShelleyUtxowPredFailure::MissingVKeyWitnessesUTXOW(xs) => {
            ConwayUtxowPredFailure::MissingVKeyWitnessesUTXOW(xs)
        }
        ShelleyUtxowPredFailure::MissingScriptWitnessesUTXOW(xs) => {
            ConwayUtxowPredFailure::MissingScriptWitnessesUTXOW(xs)
        }
        ShelleyUtxowPredFailure::MIRInsufficientGenesisSigsUTXOW(_) => {
            // This should never happen in Conway - MIR is gone!
            panic!("Impossible: MIR has been removed in Conway")
        }
    }
}

/// Convert Alonzo error to Conway
/// Reference: Utxow.hs:317-329 (alonzoToConwayUtxowPredFailure)
pub fn alonzo_to_conway(err: AlonzoUtxowPredFailure) -> ConwayUtxowPredFailure {
    match err {
        AlonzoUtxowPredFailure::ShelleyInAlonzoUtxowPredFailure(f) => shelley_to_conway(f),
        AlonzoUtxowPredFailure::MissingRedeemers(rs) => {
            ConwayUtxowPredFailure::MissingRedeemers(rs)
        }
        AlonzoUtxowPredFailure::MissingRequiredDatums { missing, provided } => {
            ConwayUtxowPredFailure::MissingRequiredDatums { missing, provided }
        }
        AlonzoUtxowPredFailure::NotAllowedSupplementalDatums { unallowed, allowed } => {
            ConwayUtxowPredFailure::NotAllowedSupplementalDatums { unallowed, allowed }
        }
        AlonzoUtxowPredFailure::PPViewHashesDontMatch { expected, actual } => {
            ConwayUtxowPredFailure::PPViewHashesDontMatch { expected, actual }
        }
        AlonzoUtxowPredFailure::UnspendableUTxONoDatumHash(ins) => {
            ConwayUtxowPredFailure::UnspendableUTxONoDatumHash(ins)
        }
        AlonzoUtxowPredFailure::ExtraRedeemers(xs) => {
            ConwayUtxowPredFailure::ExtraRedeemers(xs)
        }
        AlonzoUtxowPredFailure::ScriptIntegrityHashMismatch { expected, actual, original_bytes } => {
            ConwayUtxowPredFailure::ScriptIntegrityHashMismatch { expected, actual, original_bytes }
        }
    }
}

/// Convert Babbage error to Conway
/// Reference: Utxow.hs:306-316 (babbageToConwayUtxowPredFailure)
pub fn babbage_to_conway(err: BabbageUtxowPredFailure) -> ConwayUtxowPredFailure {
    match err {
        BabbageUtxowPredFailure::AlonzoInBabbageUtxowPredFailure(x) => alonzo_to_conway(x),
        BabbageUtxowPredFailure::UtxoFailure(x) => ConwayUtxowPredFailure::UtxoFailure(x),
        BabbageUtxowPredFailure::MalformedScriptWitnesses(xs) => {
            ConwayUtxowPredFailure::MalformedScriptWitnesses(xs)
        }
        BabbageUtxowPredFailure::MalformedReferenceScripts(xs) => {
            ConwayUtxowPredFailure::MalformedReferenceScripts(xs)
        }
        BabbageUtxowPredFailure::ScriptIntegrityHashMismatch { expected, actual, original_bytes } => {
            ConwayUtxowPredFailure::ScriptIntegrityHashMismatch { expected, actual, original_bytes }
        }
    }
}

// ============================================================================
// Scripts Needed (Conway - with Voting and Proposing)
// Reference: eras/conway/impl/src/Cardano/Ledger/Conway/UTxO.hs:59-102
// ============================================================================

pub struct ConwayScriptsNeeded {
    pub needed: Vec<(PlutusPurpose, ScriptHash)>,
}

/// Get scripts needed for Conway transaction
/// Reference: UTxO.hs:59-102 (getConwayScriptsNeeded)
pub fn get_conway_scripts_needed(
    utxo: &UTxO,
    tx_body: &ConwayTxBody,
) -> ConwayScriptsNeeded {
    let mut needed = Vec::new();

    // 1. Spending scripts (same as previous eras)
    for (idx, txin) in tx_body.inputs.iter().enumerate() {
        if let Some(txout) = utxo.get(txin) {
            if let Credential::ScriptHash(sh) = &txout.address {
                needed.push((PlutusPurpose::Spending(idx as u32), *sh));
            }
        }
    }

    // 2. Minting scripts (same as previous eras)
    for (idx, policy) in tx_body.mint.iter().enumerate() {
        needed.push((PlutusPurpose::Minting(idx as u32), *policy));
    }

    // 3. Rewarding scripts (same as previous eras)
    for (idx, cred) in tx_body.withdrawals.keys().enumerate() {
        if let Credential::ScriptHash(sh) = cred {
            needed.push((PlutusPurpose::Rewarding(idx as u32), *sh));
        }
    }

    // 4. Certifying scripts (same as previous eras)
    for (idx, cert) in tx_body.certificates.iter().enumerate() {
        if let Some(sh) = get_cert_script_hash(cert) {
            needed.push((PlutusPurpose::Certifying(idx as u32), sh));
        }
    }

    // 5. Voting scripts (NEW in Conway)
    // Reference: UTxO.hs:79-89 (votingScriptsNeeded)
    for (idx, voter) in tx_body.voting_procedures.keys().enumerate() {
        if let Some(sh) = voter.script_hash() {
            needed.push((PlutusPurpose::Voting(idx as u32), sh));
        }
    }

    // 6. Proposing scripts (NEW in Conway)
    // Reference: UTxO.hs:91-102 (proposingScriptsNeeded)
    for (idx, proposal) in tx_body.proposal_procedures.iter().enumerate() {
        if let Some(sh) = proposal.guardrails_script() {
            needed.push((PlutusPurpose::Proposing(idx as u32), sh));
        }
    }

    ConwayScriptsNeeded { needed }
}

fn get_cert_script_hash(cert: &ConwayCert) -> Option<ScriptHash> {
    match cert {
        ConwayCert::UnRegDRep(Credential::ScriptHash(sh)) => Some(*sh),
        ConwayCert::Delegate { credential: Credential::ScriptHash(sh), .. } => Some(*sh),
        _ => None,
    }
}

// ============================================================================
// VKey Witnesses Needed (Conway)
// Reference: eras/conway/impl/src/Cardano/Ledger/Conway/UTxO.hs:177-199
// ============================================================================

/// Get VKey witnesses needed for Conway transaction
/// Reference: UTxO.hs:177-199 (getConwayWitsVKeyNeeded)
///
/// Key differences from Babbage:
/// - Uses getShelleyWitsVKeyNeededNoGov (no genesis delegates!)
/// - Adds voter key witnesses
pub fn get_conway_wits_vkey_needed(
    utxo: &UTxO,
    tx_body: &ConwayTxBody,
) -> HashSet<KeyHash> {
    let mut needed = HashSet::new();

    // 1. Input authors (same as Shelley)
    for txin in &tx_body.inputs {
        if let Some(txout) = utxo.get(txin) {
            if let Credential::KeyHash(kh) = &txout.address {
                needed.insert(*kh);
            }
        }
    }

    // 2. Withdrawal authors (same as Shelley)
    for cred in tx_body.withdrawals.keys() {
        if let Credential::KeyHash(kh) = cred {
            needed.insert(*kh);
        }
    }

    // 3. Certificate authors (same as Shelley)
    for cert in &tx_body.certificates {
        if let Some(kh) = get_cert_key_hash(cert) {
            needed.insert(kh);
        }
    }

    // 4. Required signers (same as Alonzo)
    needed.extend(&tx_body.required_signers);

    // 5. Voter witnesses (NEW in Conway)
    // Reference: UTxO.hs:187-199 (voterWitnesses)
    for voter in tx_body.voting_procedures.keys() {
        if let Some(kh) = voter.key_hash() {
            needed.insert(kh);
        }
    }

    // NOTE: No genesis delegate witnesses! (unlike Shelley/Alonzo/Babbage)
    // Governance is on-chain now via CIP-1694

    needed
}

fn get_cert_key_hash(cert: &ConwayCert) -> Option<KeyHash> {
    match cert {
        ConwayCert::UnRegDRep(Credential::KeyHash(kh)) => Some(*kh),
        ConwayCert::Delegate { credential: Credential::KeyHash(kh), .. } => Some(*kh),
        _ => None,
    }
}

// ============================================================================
// Datum Validation (Conway - PlutusV3 optional datums)
// Reference: Alonzo/UTxO.hs:165-194 (getInputDataHashesTxBody)
// ============================================================================

/// Analyze input datum requirements (Conway version)
/// Reference: Alonzo/UTxO.hs:184-188
///
/// Key difference: PlutusV3 does NOT require datums for spending! (CIP-0069)
pub fn get_conway_input_data_hashes(
    utxo: &UTxO,
    tx_body: &ConwayTxBody,
    scripts_provided: &HashMap<ScriptHash, Script>,
) -> (HashSet<DataHash>, HashSet<TxIn>) {
    let mut input_hashes = HashSet::new();
    let mut inputs_no_datum = HashSet::new();

    for txin in &tx_body.inputs {
        if let Some(txout) = utxo.get(txin) {
            if let Credential::ScriptHash(sh) = &txout.address {
                if let Some(script) = scripts_provided.get(sh) {
                    if let Some(lang) = script.language() {
                        match &txout.datum {
                            Datum::NoDatum => {
                                // CIP-0069: PlutusV3 doesn't require datum!
                                // Haskell: lang < PlutusV3 -> require datum
                                if lang < Language::PlutusV3 {
                                    inputs_no_datum.insert(txin.clone());
                                }
                                // PlutusV3: OK without datum
                            }
                            Datum::DatumHash(hash) => {
                                input_hashes.insert(*hash);
                            }
                            Datum::InlineDatum(_) => {
                                // Inline datum - OK, no witness needed
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
// Supplemental Data Hashes (Conway - same as Babbage)
// Reference: Conway/UTxO.hs:149-151 (uses getBabbageSupplementalDataHashes)
// ============================================================================

/// Get supplemental data hashes for Conway
/// Reference: Conway/UTxO.hs:150 - Conway uses getBabbageSupplementalDataHashes
///
/// Supplemental datums can be for:
/// 1. Outputs - ONLY DatumHash, NOT inline datums
/// 2. Reference inputs - ONLY DatumHash, NOT inline datums
pub fn get_conway_supplemental_data_hashes(
    utxo: &UTxO,
    tx_body: &ConwayTxBody,
) -> HashSet<DataHash> {
    let mut hashes = HashSet::new();

    // Output datum hashes (only DatumHash, not inline datums)
    for out in &tx_body.outputs {
        if let Datum::DatumHash(h) = &out.datum {
            hashes.insert(*h);
        }
        // Note: InlineDatum does NOT contribute to supplemental hashes
    }

    // Reference input datum hashes
    // Only DatumHash counts, not InlineDatum
    for txin in &tx_body.reference_inputs {
        if let Some(txout) = utxo.get(txin) {
            if let Datum::DatumHash(h) = &txout.datum {
                hashes.insert(*h);
            }
        }
    }

    hashes
}

// ============================================================================
// Main UTXOW Transition Function
// Reference: Utxow.hs:199-226
// ============================================================================

/// Conway UTXOW environment
pub struct ConwayUtxoEnv {
    pub slot: SlotNo,
    pub cost_models: HashMap<Language, Vec<u64>>,
}

/// Conway UTXOW validation
/// Reference: Utxow.hs:199-226
///
/// Note: Conway REUSES Babbage's transition logic (babbageUtxowTransition)
/// The only changes are:
/// - Error type is flattened
/// - Witness computation uses getConwayWitsVKeyNeeded (with voters, no genesis)
/// - Datum validation allows PlutusV3 without datums (CIP-0069)
pub fn conway_utxow_transition(
    _env: &ConwayUtxoEnv,
    utxo: &UTxO,
    tx: &ConwayTx,
) -> Result<(), ConwayUtxowPredFailure> {
    // Get scripts from witnesses and reference UTxOs
    let scripts_provided = get_scripts_provided(utxo, tx);
    let witness_scripts = &tx.wits.scripts;
    let scripts_needed = get_conway_scripts_needed(utxo, &tx.body);
    let script_hashes_needed: HashSet<_> = scripts_needed.needed.iter().map(|(_, sh)| *sh).collect();

    // Step 1: Validate native scripts
    // (Simplified - same as Babbage)

    // Step 2: Check script presence (Babbage style - reference scripts allowed as extra)
    // Reference: Babbage/Rules/Utxow.hs:208-226 (babbageMissingScripts)
    let scripts_received: HashSet<_> = scripts_provided.keys().copied().collect();
    let witness_hashes: HashSet<_> = witness_scripts.keys().copied().collect();

    // Get reference script hashes
    let ref_script_hashes: HashSet<ScriptHash> = get_reference_script_hashes(utxo, tx);

    // neededNonRefs = needed - refs (scripts that must come from witnesses)
    let needed_non_refs: HashSet<_> = script_hashes_needed
        .difference(&ref_script_hashes)
        .copied()
        .collect();

    // missing = neededNonRefs - received
    let missing_scripts: HashSet<_> = needed_non_refs
        .difference(&scripts_received)
        .copied()
        .collect();

    if !missing_scripts.is_empty() {
        return Err(ConwayUtxowPredFailure::MissingScriptWitnessesUTXOW(missing_scripts));
    }

    // extra = received as WITNESS - neededNonRefs
    let extra_witness_scripts: HashSet<_> = witness_hashes
        .difference(&needed_non_refs)
        .copied()
        .collect();

    if !extra_witness_scripts.is_empty() {
        return Err(ConwayUtxowPredFailure::ExtraneousScriptWitnessesUTXOW(extra_witness_scripts));
    }

    // Step 3: Check required datums (PlutusV3 can skip! CIP-0069)
    // Reference: Alonzo/Rules/Utxow.hs:237-257 (missingRequiredDatums)
    let (input_hashes, inputs_no_datum) =
        get_conway_input_data_hashes(utxo, &tx.body, &scripts_provided);

    if !inputs_no_datum.is_empty() {
        return Err(ConwayUtxowPredFailure::UnspendableUTxONoDatumHash(inputs_no_datum));
    }

    // Check datums provided in witnesses
    let tx_datum_hashes: HashSet<DataHash> = tx.wits.datums.keys().copied().collect();

    // Check 1: All required datum hashes must have datums in witnesses
    let missing_datums: HashSet<DataHash> = input_hashes
        .difference(&tx_datum_hashes)
        .copied()
        .collect();

    if !missing_datums.is_empty() {
        return Err(ConwayUtxowPredFailure::MissingRequiredDatums {
            missing: missing_datums,
            provided: tx_datum_hashes.clone(),
        });
    }

    // Check 2: Supplemental datums must be allowed (for outputs or ref inputs)
    // Reference: Alonzo/Rules/Utxow.hs:243-256
    let allowed_supplemental = get_conway_supplemental_data_hashes(utxo, &tx.body);

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
        let ok_supplemental: HashSet<DataHash> = supplemental_datums
            .intersection(&allowed_supplemental)
            .copied()
            .collect();
        return Err(ConwayUtxowPredFailure::NotAllowedSupplementalDatums {
            unallowed: not_allowed,
            allowed: ok_supplemental,
        });
    }

    // Step 4: Check exact redeemers
    // (Simplified - same as Babbage but with new purposes: Voting, Proposing)

    // Step 5: Verify VKey signatures
    // (Simplified)

    // Step 6: Check required witnesses (with voter witnesses!)
    let wits_key_hashes: HashSet<_> = tx.wits.vkey_wits.iter().map(|w| w.key_hash()).collect();
    let needed_witnesses = get_conway_wits_vkey_needed(utxo, &tx.body);
    let missing_witnesses: HashSet<_> = needed_witnesses.difference(&wits_key_hashes).copied().collect();

    if !missing_witnesses.is_empty() {
        return Err(ConwayUtxowPredFailure::MissingVKeyWitnessesUTXOW(missing_witnesses));
    }

    // Step 7: MIR signatures - REMOVED in Conway!
    // No more genesis delegate checks (governance is on-chain now via CIP-1694)

    // Step 8: Metadata validation (REUSED from Shelley)
    // Shelley.validateMetadata pp tx: hash consistency + when pv > (2,0) validMetadatum (InvalidMetadata).
    // See shelley-utxow.rs validate_metadata(tx, protocol_version).

    // Step 9: Script well-formedness
    // (Simplified - same as Babbage)

    // Step 10: Script integrity hash
    // (Simplified)

    Ok(())
}

/// Get reference script hashes from UTxO
fn get_reference_script_hashes(utxo: &UTxO, tx: &ConwayTx) -> HashSet<ScriptHash> {
    let all_inputs: HashSet<_> = tx.body.inputs.iter()
        .chain(tx.body.reference_inputs.iter())
        .collect();

    let mut hashes = HashSet::new();
    for txin in all_inputs {
        if let Some(txout) = utxo.get(txin) {
            if let Some(ref script) = txout.reference_script {
                hashes.insert(script.hash());
            }
        }
    }
    hashes
}

/// Get scripts provided from witnesses and reference UTxOs
fn get_scripts_provided(utxo: &UTxO, tx: &ConwayTx) -> HashMap<ScriptHash, Script> {
    let mut provided = tx.wits.scripts.clone();

    // Add reference scripts
    let all_inputs: HashSet<_> = tx.body.inputs.iter()
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
        TxIn { tx_id: make_hash(id), index: idx }
    }

    #[test]
    fn test_plutus_v3_no_datum_ok() {
        let mut utxo = UTxO::new();
        let txin = make_txin(1, 0);
        let script_hash = make_hash(10);

        // UTxO locked by PlutusV3 with NO datum
        utxo.insert(txin.clone(), ConwayTxOut {
            address: Credential::ScriptHash(script_hash),
            value: 1000,
            datum: Datum::NoDatum,
            reference_script: None,
        });

        let mut scripts = HashMap::new();
        scripts.insert(script_hash, Script::PlutusV3(vec![1, 2, 3]));

        let tx_body = ConwayTxBody {
            inputs: vec![txin.clone()],
            reference_inputs: HashSet::new(),
            outputs: vec![],
            fee: 0,
            mint: HashSet::new(),
            certificates: vec![],
            withdrawals: HashMap::new(),
            voting_procedures: HashMap::new(),
            proposal_procedures: vec![],
            script_integrity_hash: None,
            required_signers: HashSet::new(),
        };

        let (_, inputs_no_datum) = get_conway_input_data_hashes(&utxo, &tx_body, &scripts);

        // PlutusV3: No datum required!
        assert!(inputs_no_datum.is_empty());
    }

    #[test]
    fn test_plutus_v2_no_datum_fails() {
        let mut utxo = UTxO::new();
        let txin = make_txin(1, 0);
        let script_hash = make_hash(10);

        // UTxO locked by PlutusV2 with NO datum
        utxo.insert(txin.clone(), ConwayTxOut {
            address: Credential::ScriptHash(script_hash),
            value: 1000,
            datum: Datum::NoDatum,
            reference_script: None,
        });

        let mut scripts = HashMap::new();
        scripts.insert(script_hash, Script::PlutusV2(vec![1, 2, 3]));

        let tx_body = ConwayTxBody {
            inputs: vec![txin.clone()],
            reference_inputs: HashSet::new(),
            outputs: vec![],
            fee: 0,
            mint: HashSet::new(),
            certificates: vec![],
            withdrawals: HashMap::new(),
            voting_procedures: HashMap::new(),
            proposal_procedures: vec![],
            script_integrity_hash: None,
            required_signers: HashSet::new(),
        };

        let (_, inputs_no_datum) = get_conway_input_data_hashes(&utxo, &tx_body, &scripts);

        // PlutusV2: Still requires datum!
        assert!(inputs_no_datum.contains(&txin));
    }

    #[test]
    fn test_voter_witnesses_needed() {
        let utxo = UTxO::new();
        let drep_key = make_hash(50);
        let spo_key = make_hash(51);

        let mut voting_procedures = HashMap::new();
        voting_procedures.insert(
            Voter::DRepVoter(Credential::KeyHash(drep_key)),
            vec![VotingProcedure { vote: Vote::Yes, anchor: None }],
        );
        voting_procedures.insert(
            Voter::StakePoolVoter(spo_key),
            vec![VotingProcedure { vote: Vote::No, anchor: None }],
        );

        let tx_body = ConwayTxBody {
            inputs: vec![],
            reference_inputs: HashSet::new(),
            outputs: vec![],
            fee: 0,
            mint: HashSet::new(),
            certificates: vec![],
            withdrawals: HashMap::new(),
            voting_procedures,
            proposal_procedures: vec![],
            script_integrity_hash: None,
            required_signers: HashSet::new(),
        };

        let needed = get_conway_wits_vkey_needed(&utxo, &tx_body);

        // Both voters should require key witnesses
        assert!(needed.contains(&drep_key));
        assert!(needed.contains(&spo_key));
    }

    #[test]
    fn test_voting_scripts_needed() {
        let utxo = UTxO::new();
        let script_hash = make_hash(60);

        let mut voting_procedures = HashMap::new();
        voting_procedures.insert(
            Voter::DRepVoter(Credential::ScriptHash(script_hash)),
            vec![VotingProcedure { vote: Vote::Yes, anchor: None }],
        );

        let tx_body = ConwayTxBody {
            inputs: vec![],
            reference_inputs: HashSet::new(),
            outputs: vec![],
            fee: 0,
            mint: HashSet::new(),
            certificates: vec![],
            withdrawals: HashMap::new(),
            voting_procedures,
            proposal_procedures: vec![],
            script_integrity_hash: None,
            required_signers: HashSet::new(),
        };

        let scripts_needed = get_conway_scripts_needed(&utxo, &tx_body);

        // Should have VotingPurpose for script DRep
        assert!(scripts_needed.needed.iter().any(|(p, sh)| {
            matches!(p, PlutusPurpose::Voting(_)) && *sh == script_hash
        }));
    }

    // ========================================================================
    // Supplemental Datum Hash Tests (same logic as Babbage)
    // ========================================================================

    #[test]
    fn test_supplemental_hashes_from_outputs() {
        let utxo = UTxO::new();
        let datum_hash = make_hash(42);

        let tx_body = ConwayTxBody {
            inputs: vec![],
            reference_inputs: HashSet::new(),
            outputs: vec![ConwayTxOut {
                address: Credential::KeyHash(make_hash(1)),
                value: 1000,
                datum: Datum::DatumHash(datum_hash),
                reference_script: None,
            }],
            fee: 0,
            mint: HashSet::new(),
            certificates: vec![],
            withdrawals: HashMap::new(),
            voting_procedures: HashMap::new(),
            proposal_procedures: vec![],
            script_integrity_hash: None,
            required_signers: HashSet::new(),
        };

        let hashes = get_conway_supplemental_data_hashes(&utxo, &tx_body);

        assert!(hashes.contains(&datum_hash));
        assert_eq!(hashes.len(), 1);
    }

    #[test]
    fn test_inline_datum_not_in_supplemental_hashes() {
        let mut utxo = UTxO::new();
        let ref_input = make_txin(1, 0);

        // Reference input with INLINE datum
        utxo.insert(
            ref_input.clone(),
            ConwayTxOut {
                address: Credential::KeyHash(make_hash(99)),
                value: 1000,
                datum: Datum::InlineDatum(vec![1, 2, 3, 4]),
                reference_script: None,
            },
        );

        let tx_body = ConwayTxBody {
            inputs: vec![],
            reference_inputs: [ref_input].into_iter().collect(),
            outputs: vec![
                ConwayTxOut {
                    address: Credential::KeyHash(make_hash(1)),
                    value: 1000,
                    datum: Datum::InlineDatum(vec![5, 6, 7, 8]),
                    reference_script: None,
                },
            ],
            fee: 0,
            mint: HashSet::new(),
            certificates: vec![],
            withdrawals: HashMap::new(),
            voting_procedures: HashMap::new(),
            proposal_procedures: vec![],
            script_integrity_hash: None,
            required_signers: HashSet::new(),
        };

        let hashes = get_conway_supplemental_data_hashes(&utxo, &tx_body);

        // Inline datums should NOT contribute to supplemental hashes
        assert!(hashes.is_empty());
    }

    #[test]
    fn test_supplemental_datum_not_allowed_error() {
        let utxo = UTxO::new();
        let datum_hash = make_hash(42);
        let random_hash = make_hash(99); // Not in any output or ref input

        let tx_body = ConwayTxBody {
            inputs: vec![],
            reference_inputs: HashSet::new(),
            outputs: vec![ConwayTxOut {
                address: Credential::KeyHash(make_hash(1)),
                value: 1000,
                datum: Datum::DatumHash(datum_hash),
                reference_script: None,
            }],
            fee: 0,
            mint: HashSet::new(),
            certificates: vec![],
            withdrawals: HashMap::new(),
            voting_procedures: HashMap::new(),
            proposal_procedures: vec![],
            script_integrity_hash: None,
            required_signers: HashSet::new(),
        };

        // Witness provides EXTRA datum not referenced anywhere
        let mut datums = HashMap::new();
        datums.insert(datum_hash, vec![1, 2, 3]);
        datums.insert(random_hash, vec![4, 5, 6]); // Not allowed!

        let tx = ConwayTx {
            body: tx_body,
            wits: ConwayTxWits {
                datums,
                ..Default::default()
            },
        };

        let env = ConwayUtxoEnv {
            slot: SlotNo(0),
            cost_models: HashMap::new(),
        };

        let result = conway_utxow_transition(&env, &utxo, &tx);
        assert!(matches!(
            result,
            Err(ConwayUtxowPredFailure::NotAllowedSupplementalDatums { .. })
        ));
    }

    #[test]
    fn test_extra_witness_script_error() {
        let utxo = UTxO::new();
        let extra_script_hash = make_hash(42);

        let tx_body = ConwayTxBody {
            inputs: vec![],
            reference_inputs: HashSet::new(),
            outputs: vec![],
            fee: 0,
            mint: HashSet::new(), // No minting - script not needed
            certificates: vec![],
            withdrawals: HashMap::new(),
            voting_procedures: HashMap::new(),
            proposal_procedures: vec![],
            script_integrity_hash: None,
            required_signers: HashSet::new(),
        };

        // Provide an unnecessary script in witnesses
        let mut scripts = HashMap::new();
        scripts.insert(extra_script_hash, Script::PlutusV3(vec![1, 2, 3]));

        let tx = ConwayTx {
            body: tx_body,
            wits: ConwayTxWits {
                scripts,
                ..Default::default()
            },
        };

        let env = ConwayUtxoEnv {
            slot: SlotNo(0),
            cost_models: HashMap::new(),
        };

        let result = conway_utxow_transition(&env, &utxo, &tx);
        assert!(matches!(
            result,
            Err(ConwayUtxowPredFailure::ExtraneousScriptWitnessesUTXOW(_))
        ));
    }

    #[test]
    fn test_proposing_scripts_needed() {
        let utxo = UTxO::new();
        let guardrails_hash = make_hash(70);

        let tx_body = ConwayTxBody {
            inputs: vec![],
            reference_inputs: HashSet::new(),
            outputs: vec![],
            fee: 0,
            mint: HashSet::new(),
            certificates: vec![],
            withdrawals: HashMap::new(),
            voting_procedures: HashMap::new(),
            proposal_procedures: vec![
                ProposalProcedure {
                    gov_action: GovAction::ParameterChange {
                        guardrails_script: Some(guardrails_hash),
                    },
                    deposit: 1000,
                    return_addr: Credential::KeyHash(make_hash(1)),
                },
            ],
            script_integrity_hash: None,
            required_signers: HashSet::new(),
        };

        let scripts_needed = get_conway_scripts_needed(&utxo, &tx_body);

        // Should have ProposingPurpose for guardrails script
        assert!(scripts_needed.needed.iter().any(|(p, sh)| {
            matches!(p, PlutusPurpose::Proposing(_)) && *sh == guardrails_hash
        }));
    }

    #[test]
    fn test_error_conversion_completeness() {
        // Test that all Alonzo errors convert correctly
        let alonzo_err = AlonzoUtxowPredFailure::MissingRequiredDatums {
            missing: HashSet::new(),
            provided: HashSet::new(),
        };
        let conway_err = alonzo_to_conway(alonzo_err);
        assert!(matches!(conway_err, ConwayUtxowPredFailure::MissingRequiredDatums { .. }));

        let alonzo_err = AlonzoUtxowPredFailure::NotAllowedSupplementalDatums {
            unallowed: HashSet::new(),
            allowed: HashSet::new(),
        };
        let conway_err = alonzo_to_conway(alonzo_err);
        assert!(matches!(conway_err, ConwayUtxowPredFailure::NotAllowedSupplementalDatums { .. }));

        let alonzo_err = AlonzoUtxowPredFailure::PPViewHashesDontMatch {
            expected: None,
            actual: None,
        };
        let conway_err = alonzo_to_conway(alonzo_err);
        assert!(matches!(conway_err, ConwayUtxowPredFailure::PPViewHashesDontMatch { .. }));
    }
}
