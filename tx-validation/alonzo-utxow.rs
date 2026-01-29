// Alonzo Era UTXOW Rule Implementation
// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Rules/Utxow.hs
//
// Alonzo adds Plutus script support. UTXOW gains Phase 1 setup checks:
// - Datum validation for Plutus spending inputs
// - Redeemer matching for all Plutus scripts
// - Script integrity hash verification

use std::collections::{HashMap, HashSet};

// ============================================================================
// Re-export Shelley types (Alonzo builds on Shelley)
// ============================================================================

// In a real implementation, these would be imported from shelley_utxow
pub type Hash = [u8; 32];
pub type KeyHash = Hash;
pub type ScriptHash = Hash;
pub type TxBodyHash = Hash;
pub type DataHash = Hash;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SlotNo(pub u64);

// ============================================================================
// Plutus-Specific Types
// Reference: Cardano.Ledger.Alonzo.Scripts, Cardano.Ledger.Alonzo.Data
// ============================================================================

/// Plutus language version
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Language {
    PlutusV1,
    PlutusV2,
    PlutusV3, // Added in Conway, but defined here for forward compat
}

/// Script purpose - what the script is authorizing
/// Reference: Cardano.Ledger.Alonzo.Tx
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PlutusPurpose {
    /// Spending a UTxO input (index into inputs)
    Spending(u32),
    /// Minting/burning tokens (index into minted policies)
    Minting(u32),
    /// Withdrawing rewards (index into withdrawals)
    Rewarding(u32),
    /// Authorizing a certificate (index into certificates)
    Certifying(u32),
}

/// Execution units for Plutus script execution
/// Reference: Cardano.Ledger.Alonzo.Scripts
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExUnits {
    pub mem: u64,   // Memory units
    pub steps: u64, // CPU steps
}

/// Redeemer data for Plutus script
/// Reference: Cardano.Ledger.Alonzo.TxWits
#[derive(Debug, Clone)]
pub struct Redeemer {
    pub data: Vec<u8>, // CBOR-encoded PlutusData
    pub ex_units: ExUnits,
}

/// Datum for Plutus script
/// Reference: Cardano.Ledger.Alonzo.Data
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Datum {
    pub data: Vec<u8>, // CBOR-encoded PlutusData
}

impl Datum {
    pub fn hash(&self) -> DataHash {
        // Real implementation: BLAKE2b-256 of CBOR
        [0u8; 32]
    }
}

/// How datum is attached to a TxOut
/// Reference: Cardano.Ledger.Alonzo.TxOut
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatumOption {
    /// No datum (native scripts or key-locked)
    NoDatum,
    /// Datum hash only (actual datum in witnesses)
    DatumHash(DataHash),
    // InlineDatum added in Babbage
}

/// Script integrity hash - commits to Plutus execution context
/// Reference: Cardano.Ledger.Alonzo.TxBody
pub type ScriptIntegrityHash = Hash;

// ============================================================================
// Transaction Types (Alonzo Extensions)
// ============================================================================

/// Transaction input reference
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TxIn {
    pub tx_id: Hash,
    pub index: u32,
}

/// Address credential
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Credential {
    KeyHash(KeyHash),
    ScriptHash(ScriptHash),
}

/// Alonzo TxOut with datum support
#[derive(Debug, Clone)]
pub struct AlonzoTxOut {
    pub address: Credential,
    pub value: u64,
    pub datum: DatumOption,
}

/// Reward account
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RewardAccount {
    pub credential: Credential,
}

/// Certificate (simplified)
#[derive(Debug, Clone)]
pub enum TxCert {
    Delegate { credential: Credential, pool: KeyHash },
    DeRegKey(Credential),
    // ... other cert types
}

/// Minting policy ID (same as script hash)
pub type PolicyId = ScriptHash;

/// Alonzo transaction body
#[derive(Debug, Clone)]
pub struct AlonzoTxBody {
    pub inputs: Vec<TxIn>,
    pub outputs: Vec<AlonzoTxOut>,
    pub fee: u64,
    pub certificates: Vec<TxCert>,
    pub withdrawals: HashMap<RewardAccount, u64>,
    pub mint: HashSet<PolicyId>,
    pub script_integrity_hash: Option<ScriptIntegrityHash>,
    pub required_signers: HashSet<KeyHash>,
}

/// Script types
#[derive(Debug, Clone)]
pub enum Script {
    Native(NativeScript),
    PlutusV1(Vec<u8>), // Serialized Plutus script
    PlutusV2(Vec<u8>),
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
        }
    }
}

/// Native script (from Shelley)
#[derive(Debug, Clone)]
pub enum NativeScript {
    RequireSignature(KeyHash),
    RequireAllOf(Vec<NativeScript>),
    RequireAnyOf(Vec<NativeScript>),
    RequireMOf { required: usize, scripts: Vec<NativeScript> },
    RequireTimeStart(SlotNo),
    RequireTimeExpire(SlotNo),
}

/// VKey witness
#[derive(Debug, Clone)]
pub struct VKeyWitness {
    pub vkey: [u8; 32],
    pub signature: [u8; 64],
}

impl VKeyWitness {
    pub fn key_hash(&self) -> KeyHash {
        // Real: BLAKE2b-224 of vkey
        [0u8; 32]
    }
}

/// Alonzo transaction witnesses
#[derive(Debug, Clone, Default)]
pub struct AlonzoTxWits {
    pub vkey_wits: Vec<VKeyWitness>,
    pub scripts: HashMap<ScriptHash, Script>,
    pub datums: HashMap<DataHash, Datum>,
    pub redeemers: HashMap<PlutusPurpose, Redeemer>,
}

/// Complete Alonzo transaction
#[derive(Debug, Clone)]
pub struct AlonzoTx {
    pub body: AlonzoTxBody,
    pub wits: AlonzoTxWits,
}

impl AlonzoTx {
    pub fn body_hash(&self) -> TxBodyHash {
        [0u8; 32]
    }
}

/// UTxO set
pub type UTxO = HashMap<TxIn, AlonzoTxOut>;

// ============================================================================
// Predicate Failures (Errors)
// Reference: Utxow.hs:97-129
// ============================================================================

/// Shelley UTXOW failures (simplified)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShelleyUtxowPredFailure {
    InvalidWitnessesUTXOW(Vec<KeyHash>),
    MissingVKeyWitnessesUTXOW(HashSet<KeyHash>),
    MissingScriptWitnessesUTXOW(HashSet<ScriptHash>),
    ScriptWitnessNotValidatingUTXOW(HashSet<ScriptHash>),
}

/// Alonzo UTXOW predicate failures
/// Reference: Utxow.hs:97-129
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlonzoUtxowPredFailure {
    /// Wrapped Shelley error
    ShelleyInAlonzoUtxowPredFailure(ShelleyUtxowPredFailure),

    /// Plutus scripts missing redeemers
    /// Reference: Utxow.hs:99
    MissingRedeemers(Vec<(PlutusPurpose, ScriptHash)>),

    /// Required datums not provided in witnesses
    /// Reference: Utxow.hs:100
    MissingRequiredDatums {
        missing: HashSet<DataHash>,
        provided: HashSet<DataHash>,
    },

    /// Extra datums that aren't allowed
    /// Reference: Utxow.hs:101-104
    NotAllowedSupplementalDatums {
        unallowed: HashSet<DataHash>,
        allowed: HashSet<DataHash>,
    },

    /// UTxO inputs locked by Plutus but missing datum hash
    /// Reference: Utxow.hs:118-120
    UnspendableUTxONoDatumHash(HashSet<TxIn>),

    /// Extra redeemers for non-existent scripts
    /// Reference: Utxow.hs:122-123
    ExtraRedeemers(Vec<PlutusPurpose>),

    /// Script integrity hash mismatch
    /// Reference: Utxow.hs:125-127
    ScriptIntegrityHashMismatch {
        expected: Option<ScriptIntegrityHash>,
        actual: Option<ScriptIntegrityHash>,
    },
}

// ============================================================================
// Scripts Needed (Alonzo Style with Purpose)
// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/UTxO.hs:228-320
// ============================================================================

/// Scripts needed with their purposes
/// Reference: UTxO.hs:73-75 (AlonzoScriptsNeeded)
pub struct AlonzoScriptsNeeded {
    pub needed: Vec<(PlutusPurpose, ScriptHash)>,
}

/// Get all scripts needed for a transaction
/// Reference: UTxO.hs:228-277 (getAlonzoScriptsNeeded)
pub fn get_alonzo_scripts_needed(utxo: &UTxO, tx_body: &AlonzoTxBody) -> AlonzoScriptsNeeded {
    let mut needed = Vec::new();

    // 1. Spending scripts (from inputs)
    // Reference: UTxO.hs:285-298 (getSpendingScriptsNeeded)
    for (idx, txin) in tx_body.inputs.iter().enumerate() {
        if let Some(txout) = utxo.get(txin) {
            if let Credential::ScriptHash(sh) = &txout.address {
                needed.push((PlutusPurpose::Spending(idx as u32), *sh));
            }
        }
    }

    // 2. Rewarding scripts (from withdrawals)
    // Reference: UTxO.hs:300-310 (getRewardingScriptsNeeded)
    for (idx, reward_account) in tx_body.withdrawals.keys().enumerate() {
        if let Credential::ScriptHash(sh) = &reward_account.credential {
            needed.push((PlutusPurpose::Rewarding(idx as u32), *sh));
        }
    }

    // 3. Certifying scripts (from certificates)
    // Reference: UTxO.hs:239-276 (certifyingScriptsNeeded)
    for (idx, cert) in tx_body.certificates.iter().enumerate() {
        if let Some(sh) = get_script_witness_cert(cert) {
            needed.push((PlutusPurpose::Certifying(idx as u32), sh));
        }
    }

    // 4. Minting scripts (from minted policies)
    // Reference: UTxO.hs:312-320 (getMintingScriptsNeeded)
    for (idx, policy_id) in tx_body.mint.iter().enumerate() {
        needed.push((PlutusPurpose::Minting(idx as u32), *policy_id));
    }

    AlonzoScriptsNeeded { needed }
}

fn get_script_witness_cert(cert: &TxCert) -> Option<ScriptHash> {
    match cert {
        TxCert::DeRegKey(Credential::ScriptHash(sh)) => Some(*sh),
        TxCert::Delegate { credential: Credential::ScriptHash(sh), .. } => Some(*sh),
        _ => None,
    }
}

/// Get just the script hashes (for Shelley-style validation)
pub fn get_scripts_hashes_needed(scripts_needed: &AlonzoScriptsNeeded) -> HashSet<ScriptHash> {
    scripts_needed.needed.iter().map(|(_, sh)| *sh).collect()
}

// ============================================================================
// Datum Validation
// Reference: Utxow.hs:229-257, UTxO.hs:165-194
// ============================================================================

/// Result of analyzing input datum requirements
pub struct InputDataAnalysis {
    /// Datum hashes from inputs that have them
    pub input_hashes: HashSet<DataHash>,
    /// Inputs locked by Plutus V1/V2 that have NO datum hash (unspendable!)
    pub inputs_no_datum: HashSet<TxIn>,
}

/// Analyze inputs for datum requirements
/// Reference: UTxO.hs:165-194 (getInputDataHashesTxBody)
///
/// For each spending input locked by a Plutus script:
/// - If it has a DatumHash: collect the hash
/// - If it has NoDatum AND is PlutusV1/V2: mark as unspendable
/// - If it has NoDatum AND is PlutusV3: OK (CIP-0069)
pub fn get_input_data_hashes(
    utxo: &UTxO,
    tx_body: &AlonzoTxBody,
    scripts_provided: &HashMap<ScriptHash, Script>,
) -> InputDataAnalysis {
    let mut input_hashes = HashSet::new();
    let mut inputs_no_datum = HashSet::new();

    for txin in &tx_body.inputs {
        if let Some(txout) = utxo.get(txin) {
            if let Credential::ScriptHash(sh) = &txout.address {
                // Check if it's a Plutus script
                if let Some(script) = scripts_provided.get(sh) {
                    if let Some(lang) = script.language() {
                        match &txout.datum {
                            DatumOption::NoDatum => {
                                // PlutusV3 doesn't require datum (CIP-0069)
                                // PlutusV1/V2 require datum
                                if lang < Language::PlutusV3 {
                                    inputs_no_datum.insert(txin.clone());
                                }
                            }
                            DatumOption::DatumHash(hash) => {
                                input_hashes.insert(*hash);
                            }
                        }
                    }
                }
            }
        }
    }

    InputDataAnalysis {
        input_hashes,
        inputs_no_datum,
    }
}

/// Validate missing required datums
/// Reference: Utxow.hs:229-257 (missingRequiredDatums)
pub fn validate_missing_required_datums(
    utxo: &UTxO,
    tx: &AlonzoTx,
) -> Result<(), AlonzoUtxowPredFailure> {
    let scripts_provided = &tx.wits.scripts;
    let analysis = get_input_data_hashes(utxo, &tx.body, scripts_provided);

    // Check 1: No inputs should be missing datum hash (for V1/V2)
    // failureUnless (Set.null txInsNoDataHash) (UnspendableUTxONoDatumHash txInsNoDataHash)
    if !analysis.inputs_no_datum.is_empty() {
        return Err(AlonzoUtxowPredFailure::UnspendableUTxONoDatumHash(
            analysis.inputs_no_datum,
        ));
    }

    // Check 2: All required datum hashes must have datums in witnesses
    // failureUnless (Set.null unmatchedDatumHashes) (MissingRequiredDatums ...)
    let tx_datum_hashes: HashSet<DataHash> = tx.wits.datums.keys().copied().collect();
    let missing: HashSet<DataHash> = analysis
        .input_hashes
        .difference(&tx_datum_hashes)
        .copied()
        .collect();

    if !missing.is_empty() {
        return Err(AlonzoUtxowPredFailure::MissingRequiredDatums {
            missing,
            provided: tx_datum_hashes,
        });
    }

    // Check 3: Supplemental datums must be allowed (for outputs or ref inputs)
    // In Alonzo, only output datum hashes are allowed as supplemental
    let output_datum_hashes: HashSet<DataHash> = tx
        .body
        .outputs
        .iter()
        .filter_map(|out| match &out.datum {
            DatumOption::DatumHash(h) => Some(*h),
            _ => None,
        })
        .collect();

    let supplemental: HashSet<DataHash> = tx_datum_hashes
        .difference(&analysis.input_hashes)
        .copied()
        .collect();

    let unallowed: HashSet<DataHash> = supplemental
        .difference(&output_datum_hashes)
        .copied()
        .collect();

    if !unallowed.is_empty() {
        return Err(AlonzoUtxowPredFailure::NotAllowedSupplementalDatums {
            unallowed,
            allowed: output_datum_hashes,
        });
    }

    Ok(())
}

// ============================================================================
// Redeemer Validation
// Reference: Utxow.hs:262-285
// ============================================================================

/// Validate exact set of redeemers
/// Reference: Utxow.hs:262-285 (hasExactSetOfRedeemers)
///
/// Every Plutus script needs exactly one redeemer.
/// Native scripts don't need redeemers.
pub fn validate_exact_redeemers(
    tx: &AlonzoTx,
    scripts_needed: &AlonzoScriptsNeeded,
) -> Result<(), AlonzoUtxowPredFailure> {
    // Compute which redeemers are needed (only for Plutus scripts)
    let redeemers_needed: Vec<(PlutusPurpose, ScriptHash)> = scripts_needed
        .needed
        .iter()
        .filter(|(_, sh)| {
            tx.wits
                .scripts
                .get(sh)
                .map(|s| !s.is_native())
                .unwrap_or(false)
        })
        .cloned()
        .collect();

    // Get provided redeemer purposes
    let redeemers_provided: HashSet<&PlutusPurpose> = tx.wits.redeemers.keys().collect();

    // Find missing redeemers
    let missing: Vec<(PlutusPurpose, ScriptHash)> = redeemers_needed
        .iter()
        .filter(|(purpose, _)| !redeemers_provided.contains(purpose))
        .cloned()
        .collect();

    if !missing.is_empty() {
        return Err(AlonzoUtxowPredFailure::MissingRedeemers(missing));
    }

    // Find extra redeemers
    let needed_purposes: HashSet<&PlutusPurpose> =
        redeemers_needed.iter().map(|(p, _)| p).collect();

    let extra: Vec<PlutusPurpose> = redeemers_provided
        .iter()
        .filter(|p| !needed_purposes.contains(*p))
        .map(|p| (*p).clone())
        .collect();

    if !extra.is_empty() {
        return Err(AlonzoUtxowPredFailure::ExtraRedeemers(extra));
    }

    Ok(())
}

// ============================================================================
// Script Integrity Hash
// Reference: Alonzo/Tx.hs (ScriptIntegrity, originalBytes, hashScriptIntegrity, mkScriptIntegrity)
//            Alonzo/TxWits.hs (Redeemers, TxDats encoding)
//            Alonzo/PParams.hs (getLanguageView, encodeLangViews)
//            Alonzo/Rules/Utxow.hs (checkScriptIntegrityHash)
// ============================================================================
//
// Script integrity = hash(redeemers_cbor ++ datums_cbor ++ lang_views_cbor).
// - Redeemers: CBOR map (PlutusPurpose -> [Data, ExUnits]), keys in ascending order.
// - Datums: if non-empty, CBOR tag 258 (set) then array of datum CBOR bytes.
// - Lang views: CBOR map (tag_bs -> params_bs), sorted by shortLex(tag).
// Hash algorithm: BLAKE2b-256 (Cardano ledger standard).
// Requires: blake2 = "0.10", digest = "0.10" in Cargo.toml when compiling this module.

/// BLAKE2b-256 hash for script integrity (Cardano ledger standard).
/// Reference: Cardano.Ledger.Hashes (hashAnnotated uses Hash.hashWith originalBytes)
fn blake2b_256(data: &[u8]) -> [u8; 32] {
    use digest::Digest;
    let hash = blake2::Blake2b256::digest(data);
    let mut out = [0u8; 32];
    out.copy_from_slice(&hash);
    out
}

// --- CBOR encoding helpers (match Alonzo ledger encoding) ---

fn cbor_encode_unsigned(n: u64, buf: &mut Vec<u8>) {
    if n <= 23 {
        buf.push(n as u8);
    } else if n <= 0xff {
        buf.push(0x18);
        buf.push(n as u8);
    } else if n <= 0xffff {
        buf.push(0x19);
        buf.extend_from_slice(&(n as u16).to_be_bytes());
    } else if n <= 0xffff_ffff {
        buf.push(0x1a);
        buf.extend_from_slice(&(n as u32).to_be_bytes());
    } else {
        buf.push(0x1b);
        buf.extend_from_slice(&n.to_be_bytes());
    }
}

fn cbor_encode_byte_string(bytes: &[u8], buf: &mut Vec<u8>) {
    if bytes.len() <= 23 {
        buf.push(0x40 + bytes.len() as u8);
    } else {
        buf.push(0x58);
        buf.push(bytes.len() as u8);
    }
    buf.extend_from_slice(bytes);
}

/// Encode redeemers as CBOR map (version >= 9 style): map of key -> [data, ex_units].
/// Reference: TxWits.hs encCBOR RedeemersRaw (encCBOR rs for Map), keyValueEncoder.
fn encode_redeemers_cbor(redeemers: &HashMap<PlutusPurpose, Redeemer>) -> Vec<u8> {
    if redeemers.is_empty() {
        return vec![];
    }
    let mut pairs: Vec<_> = redeemers.iter().collect();
    pairs.sort_by(|a, b| plutus_purpose_cmp(a.0, b.0));

    let mut buf = Vec::new();
    if pairs.len() <= 23 {
        buf.push(0xa0 + pairs.len() as u8);
    } else {
        buf.push(0xb8);
        buf.push(pairs.len() as u8);
    }
    for (purpose, redeemer) in pairs {
        encode_plutus_purpose_cbor(purpose, &mut buf);
        // Value: list of [data, ex_units]. encCBOR dats <> encCBOR exs -> list len 2
        buf.push(0x82);
        cbor_encode_byte_string(&redeemer.data, &mut buf);
        buf.push(0x82);
        cbor_encode_unsigned(redeemer.ex_units.mem, &mut buf);
        cbor_encode_unsigned(redeemer.ex_units.steps, &mut buf);
    }
    buf
}

fn plutus_purpose_cmp(a: &PlutusPurpose, b: &PlutusPurpose) -> std::cmp::Ordering {
    let (ta, ia) = purpose_tag_index(a);
    let (tb, ib) = purpose_tag_index(b);
    (ta, ia).cmp(&(tb, ib))
}

fn purpose_tag_index(p: &PlutusPurpose) -> (u8, u32) {
    match p {
        PlutusPurpose::Spending(i) => (0, *i),
        PlutusPurpose::Minting(i) => (1, *i),
        PlutusPurpose::Certifying(i) => (2, *i),
        PlutusPurpose::Rewarding(i) => (3, *i),
    }
}

fn encode_plutus_purpose_cbor(p: &PlutusPurpose, buf: &mut Vec<u8>) {
    let (tag, index) = purpose_tag_index(p);
    buf.push(0x82);
    cbor_encode_unsigned(tag as u64, buf);
    cbor_encode_unsigned(index as u64, buf);
}

/// Encode datums as CBOR tag 258 (set) then array of Data encodings (raw CBOR per datum).
/// Reference: TxWits.hs encodeWithSetTag . Map.elems . unTxDatsRaw
fn encode_datums_cbor(datums: &HashMap<DataHash, Datum>) -> Vec<u8> {
    if datums.is_empty() {
        return vec![];
    }
    let mut elems: Vec<&[u8]> = datums.values().map(|d| d.data.as_slice()).collect();
    elems.sort_by(|a, b| cbor_byte_string_short_lex_cmp(a, b));

    let mut buf = Vec::new();
    buf.push(0xd9);
    buf.push(0x01);
    buf.push(0x02);
    if elems.len() <= 23 {
        buf.push(0x80 + elems.len() as u8);
    } else {
        buf.push(0x98);
        buf.push(elems.len() as u8);
    }
    for d in elems {
        buf.extend_from_slice(d);
    }
    buf
}

fn cbor_byte_string_short_lex_cmp(a: &[u8], b: &[u8]) -> std::cmp::Ordering {
    if a.len() != b.len() {
        return a.len().cmp(&b.len());
    }
    a.cmp(b)
}

/// LangDepView: (tag: ByteString, params: ByteString). Sorted by shortLex(tag).
/// Reference: PParams.hs encodeLangViews, getLanguageView
fn encode_lang_views_cbor(lang_views: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
    if lang_views.is_empty() {
        return vec![];
    }
    let mut views: Vec<_> = lang_views.iter().collect();
    views.sort_by(|a, b| short_lex_cmp(&a.0, &b.0));

    let mut buf = Vec::new();
    if views.len() <= 23 {
        buf.push(0xa0 + views.len() as u8);
    } else {
        buf.push(0xb8);
        buf.push(views.len() as u8);
    }
    for (tag, params) in views {
        cbor_encode_byte_string(tag, &mut buf);
        cbor_encode_byte_string(params, &mut buf);
    }
    buf
}

fn short_lex_cmp(a: &[u8], b: &[u8]) -> std::cmp::Ordering {
    if a.len() != b.len() {
        return a.len().cmp(&b.len());
    }
    a.cmp(b)
}

/// Build language view (tag, params) for a language from protocol params.
/// Reference: PParams.hs getLanguageView. PlutusV1 uses "double bagging" for tag.
fn get_language_view(
    lang: Language,
    cost_models: &HashMap<Language, Vec<u64>>,
    _protocol_version_major: u32,
) -> (Vec<u8>, Vec<u8>) {
    let tag = match lang {
        Language::PlutusV1 => {
            let inner: u8 = 0;
            let inner_encoded = vec![inner];
            let mut outer = Vec::new();
            cbor_encode_byte_string(&inner_encoded, &mut outer);
            outer
        }
        Language::PlutusV2 | Language::PlutusV3 => {
            let tag_byte: u8 = match lang {
                Language::PlutusV1 => 0,
                Language::PlutusV2 => 1,
                Language::PlutusV3 => 2,
            };
            let mut buf = Vec::new();
            buf.push(tag_byte);
            buf
        }
    };
    let params = match cost_models.get(&lang) {
        Some(params) => {
            let mut buf = Vec::new();
            buf.push(0x9f);
            for &p in params {
                cbor_encode_unsigned(p, &mut buf);
            }
            buf.push(0xff);
            buf
        }
        None => vec![0xf6],
    };
    (tag, params)
}

/// Build script integrity input bytes: redeemers ++ (if datums non-empty then datums else []) ++ lang_views.
/// Reference: Tx.hs originalBytes (ScriptIntegrity m d l)
fn script_integrity_original_bytes(
    redeemers: &HashMap<PlutusPurpose, Redeemer>,
    datums: &HashMap<DataHash, Datum>,
    lang_views: &[(Vec<u8>, Vec<u8>)],
) -> Vec<u8> {
    let r = encode_redeemers_cbor(redeemers);
    let d = if datums.is_empty() {
        vec![]
    } else {
        encode_datums_cbor(datums)
    };
    let l = encode_lang_views_cbor(lang_views);
    let mut out = Vec::with_capacity(r.len() + d.len() + l.len());
    out.extend_from_slice(&r);
    out.extend_from_slice(&d);
    out.extend_from_slice(&l);
    out
}

/// Compute script integrity value (optional) from tx and scripts needed.
/// Reference: Tx.hs mkScriptIntegrity
fn mk_script_integrity(
    tx: &AlonzoTx,
    scripts_needed: &AlonzoScriptsNeeded,
    cost_models: &HashMap<Language, Vec<u64>>,
    protocol_version_major: u32,
) -> Option<(Vec<u8>, ScriptIntegrityHash)> {
    let scripts_hashes_needed: HashSet<ScriptHash> =
        scripts_needed.needed.iter().map(|(_, h)| *h).collect();
    let scripts_used: Vec<&Script> = tx
        .wits
        .scripts
        .iter()
        .filter(|(h, _)| scripts_hashes_needed.contains(*h))
        .map(|(_, s)| s)
        .collect();
    let langs: Vec<Language> = scripts_used
        .iter()
        .filter_map(|s| s.language())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let tx_redeemers = &tx.wits.redeemers;
    let tx_dats = &tx.wits.datums;
    let lang_views: Vec<(Vec<u8>, Vec<u8>)> = langs
        .iter()
        .map(|&lang| get_language_view(lang, cost_models, protocol_version_major))
        .collect();

    let empty_rdmrs = tx_redeemers.is_empty();
    let empty_dats = tx_dats.is_empty();
    let empty_langs = lang_views.is_empty();

    if empty_rdmrs && empty_dats && empty_langs {
        return None;
    }

    let original_bytes =
        script_integrity_original_bytes(tx_redeemers, tx_dats, &lang_views);
    let hash = blake2b_256(&original_bytes);
    Some((original_bytes, hash))
}

/// Compute script integrity hash for a transaction (what should appear in body).
/// Reference: Tx.hs hashScriptIntegrity, mkScriptIntegrity
pub fn compute_script_integrity_hash(
    tx: &AlonzoTx,
    scripts_needed: &AlonzoScriptsNeeded,
    cost_models: &HashMap<Language, Vec<u64>>,
    protocol_version_major: u32,
) -> Option<ScriptIntegrityHash> {
    mk_script_integrity(tx, scripts_needed, cost_models, protocol_version_major)
        .map(|(_, hash)| hash)
}

/// Validate script integrity hash against body.
/// Reference: Utxow.hs:289-308 (checkScriptIntegrityHash)
/// - If supplied != computed: fail.
/// - If pvMajor < 11: use PPViewHashesDontMatch; else ScriptIntegrityHashMismatch with expected bytes.
pub fn validate_script_integrity_hash(
    tx: &AlonzoTx,
    scripts_needed: &AlonzoScriptsNeeded,
    env: &AlonzoUtxoEnv,
) -> Result<(), AlonzoUtxowPredFailure> {
    let cost_models = &env.cost_models;
    let pv_major = env.protocol_version_major;

    let computed_opt = mk_script_integrity(
        tx,
        scripts_needed,
        cost_models,
        pv_major,
    );
    let supplied = tx.body.script_integrity_hash;

    let computed_hash = computed_opt.as_ref().map(|(_, h)| *h);
    if supplied != computed_hash {
        let expected = computed_hash;
        let actual = supplied;
        if pv_major < 11 {
            return Err(AlonzoUtxowPredFailure::PPViewHashesDontMatch {
                supplied: actual,
                expected,
            });
        }
        let expected_script_integrity_bytes =
            computed_opt.map(|(bytes, _)| bytes);
        return Err(AlonzoUtxowPredFailure::ScriptIntegrityHashMismatch {
            expected,
            actual,
            expected_script_integrity_bytes,
        });
    }

    Ok(())
}

// ============================================================================
// VKey Witnesses Needed (Alonzo)
// Reference: UTxO.hs:322-338
// ============================================================================

/// Get VKey witnesses needed (Alonzo style)
/// Reference: UTxO.hs:322-338 (getAlonzoWitsVKeyNeeded)
///
/// Adds required signers to Shelley's computation.
pub fn get_alonzo_wits_vkey_needed(
    utxo: &UTxO,
    tx_body: &AlonzoTxBody,
) -> HashSet<KeyHash> {
    let mut needed = HashSet::new();

    // 1. Input authors (keys owning inputs)
    for txin in &tx_body.inputs {
        if let Some(txout) = utxo.get(txin) {
            if let Credential::KeyHash(kh) = &txout.address {
                needed.insert(*kh);
            }
        }
    }

    // 2. Withdrawal authors
    for reward_account in tx_body.withdrawals.keys() {
        if let Credential::KeyHash(kh) = &reward_account.credential {
            needed.insert(*kh);
        }
    }

    // 3. Certificate authors
    for cert in &tx_body.certificates {
        if let Some(kh) = get_vkey_witness_cert(cert) {
            needed.insert(kh);
        }
    }

    // 4. Required signers (NEW in Alonzo)
    // txBody ^. reqSignerHashesTxBodyG
    needed.extend(&tx_body.required_signers);

    needed
}

fn get_vkey_witness_cert(cert: &TxCert) -> Option<KeyHash> {
    match cert {
        TxCert::DeRegKey(Credential::KeyHash(kh)) => Some(*kh),
        TxCert::Delegate { credential: Credential::KeyHash(kh), .. } => Some(*kh),
        _ => None,
    }
}

// ============================================================================
// Main UTXOW Transition Function
// Reference: Utxow.hs:340-396 (alonzoStyleWitness)
// ============================================================================

/// Alonzo UTXOW environment
/// Reference: Utxow.hs (UtxoEnv: slot, pp, certState)
pub struct AlonzoUtxoEnv {
    pub slot: SlotNo,
    /// Protocol version major (used for script integrity error: pv < 11 => PPViewHashesDontMatch)
    /// Reference: Utxow.hs:306-308
    pub protocol_version_major: u32,
    pub cost_models: HashMap<Language, Vec<u64>>,
}

/// Alonzo UTXOW validation
/// Reference: Utxow.hs:340-396 (alonzoStyleWitness)
///
/// Adds datum, redeemer, and script integrity validation to Shelley.
pub fn alonzo_utxow_transition(
    env: &AlonzoUtxoEnv,
    utxo: &UTxO,
    tx: &AlonzoTx,
) -> Result<(), AlonzoUtxowPredFailure> {
    // Get key hashes from witnesses
    let wits_key_hashes: HashSet<KeyHash> =
        tx.wits.vkey_wits.iter().map(|w| w.key_hash()).collect();

    let scripts_needed = get_alonzo_scripts_needed(utxo, &tx.body);

    // Step 1: Validate native scripts (REUSED from Shelley)
    // runTestOnSignal $ Shelley.validateFailedNativeScripts scriptsProvided tx
    // (Simplified - would call Shelley function)

    // Step 2: Check script presence (REUSED from Shelley)
    // runTest $ Shelley.validateMissingScripts shelleyScriptsNeeded scriptsProvided
    let scripts_hashes_needed = get_scripts_hashes_needed(&scripts_needed);
    let scripts_provided: HashSet<ScriptHash> = tx.wits.scripts.keys().copied().collect();

    let missing_scripts: HashSet<ScriptHash> = scripts_hashes_needed
        .difference(&scripts_provided)
        .copied()
        .collect();

    if !missing_scripts.is_empty() {
        return Err(AlonzoUtxowPredFailure::ShelleyInAlonzoUtxowPredFailure(
            ShelleyUtxowPredFailure::MissingScriptWitnessesUTXOW(missing_scripts),
        ));
    }

    // Step 3: Check required datums (NEW in Alonzo)
    // runTest $ missingRequiredDatums utxo tx
    validate_missing_required_datums(utxo, tx)?;

    // Step 4: Check exact set of redeemers (NEW in Alonzo)
    // runTest $ hasExactSetOfRedeemers tx scriptsProvided scriptsNeeded
    validate_exact_redeemers(tx, &scripts_needed)?;

    // Step 5: Verify VKey signatures (REUSED from Shelley)
    // runTestOnSignal $ Shelley.validateVerifiedWits tx
    // (Simplified - would verify Ed25519 signatures)

    // Step 6: Check required witnesses (REUSED from Shelley with Alonzo extension)
    // runTest $ validateNeededWitnesses witsKeyHashes certState utxo txBody
    let needed_witnesses = get_alonzo_wits_vkey_needed(utxo, &tx.body);
    let missing_witnesses: HashSet<KeyHash> = needed_witnesses
        .difference(&wits_key_hashes)
        .copied()
        .collect();

    if !missing_witnesses.is_empty() {
        return Err(AlonzoUtxowPredFailure::ShelleyInAlonzoUtxowPredFailure(
            ShelleyUtxowPredFailure::MissingVKeyWitnessesUTXOW(missing_witnesses),
        ));
    }

    // Step 7: MIR signatures (REUSED from Shelley)
    // (Simplified - would check genesis quorum for MIR certs)

    // Step 8: Metadata validation (REUSED from Shelley)
    // (Simplified - would validate auxiliary data hash)

    // Step 9: Script integrity hash (NEW in Alonzo)
    // runTest $ checkScriptIntegrityHash tx pp scriptIntegrity
    validate_script_integrity_hash(tx, &scripts_needed, env)?;

    // Step 10: Call UTXO rule
    // trans @(EraRule "UTXO" era) $ TRC (utxoEnv, u, tx)

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
    fn test_inputs_no_datum_v1_fails() {
        let mut utxo = UTxO::new();
        let txin = make_txin(1, 0);
        let script_hash = make_hash(10);

        // UTxO locked by PlutusV1 with NO datum
        utxo.insert(
            txin.clone(),
            AlonzoTxOut {
                address: Credential::ScriptHash(script_hash),
                value: 1000,
                datum: DatumOption::NoDatum, // No datum!
            },
        );

        let mut scripts = HashMap::new();
        scripts.insert(script_hash, Script::PlutusV1(vec![]));

        let analysis = get_input_data_hashes(
            &utxo,
            &AlonzoTxBody {
                inputs: vec![txin.clone()],
                outputs: vec![],
                fee: 0,
                certificates: vec![],
                withdrawals: HashMap::new(),
                mint: HashSet::new(),
                script_integrity_hash: None,
                required_signers: HashSet::new(),
            },
            &scripts,
        );

        // Should flag this input as unspendable
        assert!(analysis.inputs_no_datum.contains(&txin));
    }

    #[test]
    fn test_inputs_with_datum_hash_ok() {
        let mut utxo = UTxO::new();
        let txin = make_txin(1, 0);
        let script_hash = make_hash(10);
        let datum_hash = make_hash(20);

        // UTxO locked by PlutusV1 WITH datum hash
        utxo.insert(
            txin.clone(),
            AlonzoTxOut {
                address: Credential::ScriptHash(script_hash),
                value: 1000,
                datum: DatumOption::DatumHash(datum_hash),
            },
        );

        let mut scripts = HashMap::new();
        scripts.insert(script_hash, Script::PlutusV1(vec![]));

        let analysis = get_input_data_hashes(
            &utxo,
            &AlonzoTxBody {
                inputs: vec![txin.clone()],
                outputs: vec![],
                fee: 0,
                certificates: vec![],
                withdrawals: HashMap::new(),
                mint: HashSet::new(),
                script_integrity_hash: None,
                required_signers: HashSet::new(),
            },
            &scripts,
        );

        // Should collect the datum hash, not flag as unspendable
        assert!(analysis.inputs_no_datum.is_empty());
        assert!(analysis.input_hashes.contains(&datum_hash));
    }

    #[test]
    fn test_scripts_needed_all_purposes() {
        let mut utxo = UTxO::new();
        let txin = make_txin(1, 0);
        let spending_script = make_hash(10);
        let minting_script = make_hash(11);

        utxo.insert(
            txin.clone(),
            AlonzoTxOut {
                address: Credential::ScriptHash(spending_script),
                value: 1000,
                datum: DatumOption::DatumHash(make_hash(99)),
            },
        );

        let mut mint = HashSet::new();
        mint.insert(minting_script);

        let tx_body = AlonzoTxBody {
            inputs: vec![txin],
            outputs: vec![],
            fee: 0,
            certificates: vec![],
            withdrawals: HashMap::new(),
            mint,
            script_integrity_hash: None,
            required_signers: HashSet::new(),
        };

        let scripts_needed = get_alonzo_scripts_needed(&utxo, &tx_body);

        // Should have both spending and minting purposes
        assert_eq!(scripts_needed.needed.len(), 2);

        let purposes: Vec<_> = scripts_needed.needed.iter().map(|(p, _)| p).collect();
        assert!(purposes.iter().any(|p| matches!(p, PlutusPurpose::Spending(_))));
        assert!(purposes.iter().any(|p| matches!(p, PlutusPurpose::Minting(_))));
    }

    #[test]
    fn test_script_integrity_env_and_none_when_no_plutus() {
        let mut utxo = UTxO::new();
        let txin = make_txin(1, 0);
        utxo.insert(
            txin.clone(),
            AlonzoTxOut {
                address: Credential::KeyHash(make_hash(5)),
                value: 1000,
                datum: DatumOption::NoDatum,
            },
        );
        let env = AlonzoUtxoEnv {
            slot: SlotNo(100),
            protocol_version_major: 11,
            cost_models: HashMap::new(),
        };
        let tx = AlonzoTx {
            body: AlonzoTxBody {
                inputs: vec![txin],
                outputs: vec![],
                fee: 10,
                certificates: vec![],
                withdrawals: HashMap::new(),
                mint: HashSet::new(),
                script_integrity_hash: None,
                required_signers: HashSet::new(),
            },
            wits: AlonzoTxWits::default(),
        };
        let scripts_needed = get_alonzo_scripts_needed(&utxo, &tx.body);
        assert!(scripts_needed.needed.is_empty());
        let hash_opt = compute_script_integrity_hash(
            &tx,
            &scripts_needed,
            &env.cost_models,
            env.protocol_version_major,
        );
        assert!(hash_opt.is_none());
        let r = validate_script_integrity_hash(&tx, &scripts_needed, &env);
        assert!(r.is_ok());
    }
}
