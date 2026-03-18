// Script Context Construction and Plutus Script Evaluation
//
// This module implements the Plutus script context construction pipeline:
//   Phase 2 (UTXOS rule) collects scripts, builds context, and evaluates them.
//
// The pipeline has 4 major steps:
//
//   1. Determine which scripts are NEEDED  (getScriptsNeeded / AlonzoScriptsNeeded)
//   2. Determine which scripts are PROVIDED (getScriptsProvided / ScriptsProvided)
//   3. Build TxInfo + per-script PlutusWithContext (collectPlutusScriptsWithContext)
//   4. Evaluate every PlutusWithContext (evalPlutusScripts)
//
// References:
//   - eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Plutus/Evaluate.hs
//   - eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Plutus/Context.hs
//   - eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Plutus/TxInfo.hs   (PlutusV1 TxInfo)
//   - eras/babbage/impl/src/Cardano/Ledger/Babbage/TxInfo.hs         (PlutusV2 TxInfo)
//   - eras/conway/impl/src/Cardano/Ledger/Conway/TxInfo.hs           (PlutusV3 TxInfo)
//   - eras/alonzo/impl/src/Cardano/Ledger/Alonzo/UTxO.hs             (ScriptsNeeded)
//   - eras/babbage/impl/src/Cardano/Ledger/Babbage/UTxO.hs            (ScriptsProvided)
//   - libs/cardano-ledger-core/src/Cardano/Ledger/Plutus/Language.hs  (PlutusArgs, evaluation)
//   - libs/cardano-ledger-core/src/Cardano/Ledger/Plutus/Evaluate.hs  (PlutusWithContext)
//   - eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Rules/Utxos.hs      (UTXOS rule / scriptsTransition)

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

// ============================================================================
// Core Types
// ============================================================================

pub type Hash = [u8; 32];
pub type ScriptHash = Hash;
pub type PolicyId = ScriptHash;
pub type DataHash = Hash;
pub type KeyHash = Hash;
pub type TxId = Hash;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TxIn {
    pub tx_id: TxId,
    pub index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Credential {
    KeyHash(KeyHash),
    ScriptHash(ScriptHash),
}

impl Credential {
    pub fn script_hash(&self) -> Option<ScriptHash> {
        match self {
            Credential::ScriptHash(h) => Some(*h),
            Credential::KeyHash(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    pub payment: Credential,
    pub staking: Option<Credential>,
}

#[derive(Debug, Clone)]
pub struct RewardAccount {
    pub credential: Credential,
}

#[derive(Debug, Clone)]
pub struct Coin(pub u64);

#[derive(Debug, Clone)]
pub struct Value {
    pub coin: Coin,
    pub multi_asset: MultiAsset,
}

pub type MultiAsset = BTreeMap<PolicyId, BTreeMap<Vec<u8>, u64>>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SlotNo(pub u64);

/// Execution units for a Plutus script
#[derive(Debug, Clone, Copy)]
pub struct ExUnits {
    pub mem: u64,
    pub steps: u64,
}

// ============================================================================
// Script Types
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    PlutusV1,
    PlutusV2,
    PlutusV3,
}

#[derive(Debug, Clone)]
pub struct NativeScript;

/// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Scripts.hs
#[derive(Debug, Clone)]
pub enum Script {
    Native(NativeScript),
    PlutusV1(Vec<u8>),
    PlutusV2(Vec<u8>),
    PlutusV3(Vec<u8>),
}

impl Script {
    pub fn hash(&self) -> ScriptHash {
        [0u8; 32] // Simplified
    }

    pub fn language(&self) -> Option<Language> {
        match self {
            Script::Native(_) => None,
            Script::PlutusV1(_) => Some(Language::PlutusV1),
            Script::PlutusV2(_) => Some(Language::PlutusV2),
            Script::PlutusV3(_) => Some(Language::PlutusV3),
        }
    }

    pub fn is_plutus(&self) -> bool {
        self.language().is_some()
    }

    pub fn plutus_bytes(&self) -> Option<&[u8]> {
        match self {
            Script::PlutusV1(b) | Script::PlutusV2(b) | Script::PlutusV3(b) => Some(b),
            Script::Native(_) => None,
        }
    }
}

// ============================================================================
// Datum Types
// ============================================================================

/// Opaque Plutus data — in a real implementation this would be a full
/// PlutusData / Data type with constructors, maps, lists, ints, bytestrings.
#[derive(Debug, Clone)]
pub struct PlutusData(pub Vec<u8>);

#[derive(Debug, Clone)]
pub enum Datum {
    NoDatum,
    DatumHash(DataHash),
    InlineDatum(PlutusData),
}

// ============================================================================
// Transaction Types
// ============================================================================

/// Validity interval (introduced in Allegra, mandatory in Alonzo+)
#[derive(Debug, Clone)]
pub struct ValidityInterval {
    pub invalid_before: Option<SlotNo>,
    pub invalid_hereafter: Option<SlotNo>,
}

/// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Core.hs (simplified)
#[derive(Debug, Clone)]
pub struct TxOut {
    pub address: Address,
    pub value: Value,
    pub datum: Datum,
    pub reference_script: Option<Script>,
}

#[derive(Debug, Clone)]
pub struct TxBody {
    pub inputs: BTreeSet<TxIn>,
    pub reference_inputs: BTreeSet<TxIn>,
    pub outputs: Vec<TxOut>,
    pub collateral_inputs: BTreeSet<TxIn>,
    pub collateral_return: Option<TxOut>,
    pub fee: Coin,
    pub mint: MultiAsset,
    pub validity_interval: ValidityInterval,
    pub required_signers: HashSet<KeyHash>,
    pub certs: Vec<Certificate>,
    pub withdrawals: BTreeMap<RewardAccount, Coin>,
}

#[derive(Debug, Clone)]
pub struct Certificate; // Simplified

/// A redeemer is keyed by a purpose index
/// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/TxWits.hs (Redeemers)
#[derive(Debug, Clone)]
pub struct Redeemer {
    pub data: PlutusData,
    pub ex_units: ExUnits,
}

#[derive(Debug, Clone)]
pub struct TxWits {
    pub scripts: HashMap<ScriptHash, Script>,
    pub datums: HashMap<DataHash, PlutusData>,
    pub redeemers: HashMap<RedeemerPointer, Redeemer>,
}

#[derive(Debug, Clone)]
pub struct Tx {
    pub body: TxBody,
    pub wits: TxWits,
    pub is_valid: bool,
}

pub type UTxO = HashMap<TxIn, TxOut>;

// ============================================================================
// Protocol Parameters (simplified)
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct ProtVer {
    pub major: u32,
    pub minor: u32,
}

/// Mapping from Language to its CostModel parameters
pub type CostModels = HashMap<Language, CostModel>;

#[derive(Debug, Clone)]
pub struct CostModel {
    pub language: Language,
    pub params: Vec<i64>,
}

#[derive(Debug, Clone)]
pub struct PParams {
    pub protocol_version: ProtVer,
    pub cost_models: CostModels,
    pub max_tx_ex_units: ExUnits,
}

// ============================================================================
// Time translation (simplified)
// ============================================================================

/// Epoch info for slot-to-time translation.
///
/// Encapsulates the era-specific slot-length and epoch-boundary information
/// needed to convert slot numbers to UTC timestamps.
///
/// In Haskell this is `EpochInfo (Either Text)` from `cardano-slotting`,
/// which can fail if the requested slot is past the forecast horizon.
///
/// Reference: Cardano.Slotting.EpochInfo (epochInfoSlotToUTCTime)
#[derive(Debug, Clone)]
pub struct EpochInfo {
    pub slot_length_ms: u64,
}

/// The system start time (genesis block timestamp) as seconds since Unix epoch.
///
/// Reference: Cardano.Slotting.Time (SystemStart)
#[derive(Debug, Clone)]
pub struct SystemStart {
    pub utc_seconds: i64,
}

/// Plutus POSIXTime — milliseconds since 1970-01-01T00:00:00Z.
///
/// ```haskell
/// newtype POSIXTime = POSIXTime { getPOSIXTime :: Integer }
/// ```
///
/// Reference: PlutusLedgerApi.V1.Time
pub type POSIXTime = i64;

/// Plutus `Extended` type — a value that can be negative infinity, finite, or positive infinity.
///
/// ```haskell
/// data Extended a = NegInf | Finite a | PosInf
/// ```
///
/// Reference: PlutusLedgerApi.V1.Interval
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Extended<T> {
    NegInf,
    Finite(T),
    PosInf,
}

/// Whether a bound is inclusive (closed) or exclusive (open).
///
/// ```haskell
/// type Closure = Bool  -- True means inclusive
/// ```
pub type Closure = bool;

/// ```haskell
/// data LowerBound a = LowerBound (Extended a) Closure
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LowerBound<T> {
    pub bound: Extended<T>,
    pub closed: Closure,
}

/// ```haskell
/// data UpperBound a = UpperBound (Extended a) Closure
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpperBound<T> {
    pub bound: Extended<T>,
    pub closed: Closure,
}

/// Plutus `Interval` — an interval with lower and upper bounds, each of which
/// can be open/closed and finite/infinite.
///
/// ```haskell
/// data Interval a = Interval
///   { ivFrom :: LowerBound a
///   , ivTo   :: UpperBound a
///   }
/// ```
///
/// Reference: PlutusLedgerApi.V1.Interval
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interval<T> {
    pub from: LowerBound<T>,
    pub to: UpperBound<T>,
}

/// `POSIXTimeRange` is `Interval POSIXTime`
///
/// ```haskell
/// type POSIXTimeRange = Interval POSIXTime
/// ```
pub type POSIXTimeRange = Interval<POSIXTime>;

impl<T> Interval<T> {
    /// `always = Interval (LowerBound NegInf True) (UpperBound PosInf True)`
    ///
    /// An interval covering everything: (-∞, +∞)
    pub fn always() -> Self {
        Interval {
            from: LowerBound { bound: Extended::NegInf, closed: true },
            to: UpperBound { bound: Extended::PosInf, closed: true },
        }
    }

    /// `from a = Interval (lowerBound a) (UpperBound PosInf True)`
    ///
    /// [a, +∞)  — includes a and everything above
    pub fn from(a: T) -> Self {
        Interval {
            from: LowerBound { bound: Extended::Finite(a), closed: true },
            to: UpperBound { bound: Extended::PosInf, closed: true },
        }
    }

    /// `to a = Interval (LowerBound NegInf True) (upperBound a)`
    ///
    /// (-∞, a]  — includes a and everything below
    pub fn to(a: T) -> Self {
        Interval {
            from: LowerBound { bound: Extended::NegInf, closed: true },
            to: UpperBound { bound: Extended::Finite(a), closed: true },
        }
    }

    /// `Interval (lowerBound a) (strictUpperBound b)`
    ///
    /// [a, b)  — closed lower bound, strict (open) upper bound
    pub fn from_to_strict_upper(lower: T, upper: T) -> Self {
        Interval {
            from: LowerBound { bound: Extended::Finite(lower), closed: true },
            to: UpperBound { bound: Extended::Finite(upper), closed: false },
        }
    }
}

/// `lowerBound a = LowerBound (Finite a) True`
///
/// An inclusive lower bound.
pub fn lower_bound<T>(a: T) -> LowerBound<T> {
    LowerBound { bound: Extended::Finite(a), closed: true }
}

/// `strictUpperBound a = UpperBound (Finite a) False`
///
/// An exclusive (strict) upper bound.
pub fn strict_upper_bound<T>(a: T) -> UpperBound<T> {
    UpperBound { bound: Extended::Finite(a), closed: false }
}

// ============================================================================
// Step 1: ScriptsNeeded — Which scripts must be satisfied?
// ============================================================================
//
// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/UTxO.hs
//
// ```haskell
// newtype AlonzoScriptsNeeded era
//   = AlonzoScriptsNeeded [(PlutusPurpose AsIxItem era, ScriptHash)]
// ```
//
// There are 4 sources of needed scripts:
//   1) Spending:    script-locked inputs from tx.body.inputs
//   2) Rewarding:   script-locked withdrawal accounts
//   3) Certifying:  script-locked certificates
//   4) Minting:     every PolicyID in the mint field

/// The purpose for which a script is being run.
///
/// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Scripts.hs (AlonzoPlutusPurpose)
///
/// PlutusV1/V2 have 4 purposes; PlutusV3 adds Voting and Proposing.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ScriptPurpose {
    /// Spending a UTxO locked by a script
    Spending(TxIn),
    /// Minting tokens under a policy
    Minting(PolicyId),
    /// Staking certificate involving a script credential
    Certifying(u32),
    /// Withdrawing rewards from a script-locked account
    Rewarding(RewardAccount),
    /// Voting (Conway / PlutusV3 only)
    Voting,
    /// Proposing (Conway / PlutusV3 only)
    Proposing(u32),
}

impl PartialEq for RewardAccount {
    fn eq(&self, other: &Self) -> bool {
        self.credential == other.credential
    }
}
impl Eq for RewardAccount {}
impl std::hash::Hash for RewardAccount {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match &self.credential {
            Credential::KeyHash(h) => {
                state.write_u8(0);
                state.write(h);
            }
            Credential::ScriptHash(h) => {
                state.write_u8(1);
                state.write(h);
            }
        }
    }
}

/// A redeemer is addressed by (tag, index) in the redeemers map.
///
/// The tag corresponds to the ScriptPurpose kind, and the index is the
/// position of the item within its category in the transaction body.
///
/// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Scripts.hs (AsIx)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RedeemerPointer {
    pub tag: RedeemerTag,
    pub index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RedeemerTag {
    Spend,
    Mint,
    Cert,
    Reward,
}

/// Each entry is (ScriptPurpose, ScriptHash) — a purpose paired with which
/// script hash must authorize it.
///
/// ```haskell
/// newtype AlonzoScriptsNeeded era
///   = AlonzoScriptsNeeded [(PlutusPurpose AsIxItem era, ScriptHash)]
/// ```
pub type ScriptsNeeded = Vec<(ScriptPurpose, ScriptHash)>;

/// Collect all script hashes that the transaction requires authorization from.
///
/// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/UTxO.hs:228-277
///            (getAlonzoScriptsNeeded)
///
/// ```haskell
/// getAlonzoScriptsNeeded utxo txBody =
///   getSpendingScriptsNeeded utxo txBody
///     <> getRewardingScriptsNeeded txBody
///     <> certifyingScriptsNeeded
///     <> getMintingScriptsNeeded txBody
/// ```
pub fn get_scripts_needed(utxo: &UTxO, tx_body: &TxBody) -> ScriptsNeeded {
    let mut needed = Vec::new();

    // 1) Spending: for each input, if the UTxO it references is locked by a
    //    script credential, that script is needed.
    //
    //    Reference: getSpendingScriptsNeeded (UTxO.hs:285-297)
    for tx_in in tx_body.inputs.iter() {
        if let Some(tx_out) = utxo.get(tx_in) {
            if let Some(script_hash) = tx_out.address.payment.script_hash() {
                needed.push((ScriptPurpose::Spending(tx_in.clone()), script_hash));
            }
        }
    }

    // 2) Rewarding: for each withdrawal, if the reward account uses a script
    //    credential, that script is needed.
    //
    //    Reference: getRewardingScriptsNeeded (UTxO.hs:300-308)
    for (reward_account, _) in &tx_body.withdrawals {
        if let Some(script_hash) = reward_account.credential.script_hash() {
            needed.push((ScriptPurpose::Rewarding(reward_account.clone()), script_hash));
        }
    }

    // 3) Certifying: for each certificate involving a script credential.
    //    (Simplified — in reality each cert type is inspected for script creds)
    //
    //    Reference: certifyingScriptsNeeded (UTxO.hs:239-276)

    // 4) Minting: every PolicyID in the mint field is a script hash that needs
    //    authorization.
    //
    //    Reference: getMintingScriptsNeeded (UTxO.hs:311-318)
    //
    //    ```haskell
    //    getMintingScriptsNeeded txBody =
    //      AlonzoScriptsNeeded $
    //        zipAsIxItem (txBody ^. mintedTxBodyF) $
    //          \asIxItem@(AsIxItem _ (PolicyID scriptHash)) ->
    //            (MintingPurpose asIxItem, scriptHash)
    //    ```
    for (policy_id, _) in tx_body.mint.iter() {
        needed.push((ScriptPurpose::Minting(*policy_id), *policy_id));
    }

    needed
}

// ============================================================================
// Step 2: ScriptsProvided — Which scripts are available?
// ============================================================================
//
// In Alonzo, scripts come only from witness scripts:
//   getScriptsProvided _ tx = ScriptsProvided (tx ^. witsTxL . scriptTxWitsL)
//
// In Babbage+, reference scripts from UTxO inputs are also included:
//   getBabbageScriptsProvided utxo tx = ScriptsProvided ans
//     where
//       ins = (txBody ^. referenceInputsTxBodyL) `Set.union` (txBody ^. inputsTxBodyL)
//       ans = getReferenceScripts utxo ins `Map.union` (tx ^. witsTxL . scriptTxWitsL)
//
// Reference: eras/babbage/impl/src/Cardano/Ledger/Babbage/UTxO.hs:127-138

/// Alonzo: only witness scripts
pub fn get_scripts_provided_alonzo(tx: &Tx) -> HashMap<ScriptHash, Script> {
    tx.wits.scripts.clone()
}

/// Babbage+: witness scripts UNION reference scripts from (inputs ∪ reference_inputs)
///
/// ```haskell
/// getBabbageScriptsProvided utxo tx = ScriptsProvided ans
///   where
///     txBody = tx ^. bodyTxL
///     ins = (txBody ^. referenceInputsTxBodyL) `Set.union` (txBody ^. inputsTxBodyL)
///     ans = getReferenceScripts utxo ins `Map.union` (tx ^. witsTxL . scriptTxWitsL)
/// ```
pub fn get_scripts_provided_babbage(utxo: &UTxO, tx: &Tx) -> HashMap<ScriptHash, Script> {
    let mut provided = HashMap::new();

    let all_inputs: BTreeSet<&TxIn> = tx.body.inputs.iter()
        .chain(tx.body.reference_inputs.iter())
        .collect();

    for tx_in in all_inputs {
        if let Some(tx_out) = utxo.get(tx_in) {
            if let Some(ref script) = tx_out.reference_script {
                provided.insert(script.hash(), script.clone());
            }
        }
    }

    // Witness scripts take precedence (union, with witness scripts overriding)
    for (hash, script) in &tx.wits.scripts {
        provided.insert(*hash, script.clone());
    }

    provided
}

// ============================================================================
// Step 3: Build TxInfo and PlutusWithContext
// ============================================================================
//
// This is the core of the pipeline, implemented by `collectPlutusScriptsWithContext`.
//
// The function:
//   a) Builds a LedgerTxInfo (captures everything needed for TxInfo translation)
//   b) Calls mkTxInfoResult to translate the transaction into Plutus TxInfo
//      (one per supported language version — memoized)
//   c) For each needed Plutus script, pairs it with:
//      - the redeemer
//      - the spending datum (if Spending purpose)
//      - the translated ScriptContext / TxInfo
//      - the cost model
//      - the execution unit budget
//
// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Plutus/Evaluate.hs:143-214

// ---------------------------------------------------------------------------
// 3a. LedgerTxInfo — everything needed for translation
// ---------------------------------------------------------------------------

/// All the information from the ledger needed to build Plutus `TxInfo`.
///
/// ```haskell
/// data LedgerTxInfo era = LedgerTxInfo
///   { ltiProtVer    :: !ProtVer
///   , ltiEpochInfo  :: !(EpochInfo (Either Text))
///   , ltiSystemStart :: !SystemStart
///   , ltiUTxO       :: !(UTxO era)
///   , ltiTx         :: !(Tx TopTx era)
///   }
/// ```
///
/// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Plutus/Context.hs:84-90
pub struct LedgerTxInfo<'a> {
    pub prot_ver: ProtVer,
    pub epoch_info: &'a EpochInfo,
    pub system_start: &'a SystemStart,
    pub utxo: &'a UTxO,
    pub tx: &'a Tx,
}

// ---------------------------------------------------------------------------
// 3b. TxInfo — the Plutus-visible view of a transaction
// ---------------------------------------------------------------------------

/// PlutusV1 TxInfo — Alonzo era
///
/// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Plutus/TxInfo.hs:104-123
///
/// ```haskell
/// PV1.TxInfo
///   { PV1.txInfoInputs       = catMaybes txInsMaybes
///   , PV1.txInfoOutputs      = mapMaybe transTxOut $ toList (txBody ^. outputsTxBodyL)
///   , PV1.txInfoFee          = transCoinToValue (txBody ^. feeTxBodyL)
///   , PV1.txInfoMint         = transMintValue (txBody ^. mintTxBodyL)
///   , PV1.txInfoDCert        = txCerts
///   , PV1.txInfoWdrl         = transTxBodyWithdrawals txBody
///   , PV1.txInfoValidRange   = timeRange
///   , PV1.txInfoSignatories  = transTxBodyReqSignerHashes txBody
///   , PV1.txInfoData         = transTxWitsDatums (ltiTx ^. witsTxL)
///   , PV1.txInfoId           = transTxBodyId txBody
///   }
/// ```
#[derive(Debug, Clone)]
pub struct TxInfoV1 {
    pub inputs: Vec<TxInInfo>,
    pub outputs: Vec<TranslatedTxOut>,
    pub fee: Value,
    pub mint: Value,
    pub d_cert: Vec<TranslatedCert>,
    pub wdrl: Vec<(TranslatedStakingCredential, u64)>,
    pub valid_range: POSIXTimeRange,
    pub signatories: Vec<Hash>,
    pub data: Vec<(DataHash, PlutusData)>,
    pub id: TxId,
}

/// PlutusV2 TxInfo — Babbage era additions
///
/// Reference: eras/babbage/impl/src/Cardano/Ledger/Babbage/TxInfo.hs:358-386
///
/// New fields vs V1: reference_inputs, redeemers, and outputs carry inline
/// datums + reference scripts.
///
/// ```haskell
/// PV2.TxInfo
///   { PV2.txInfoInputs          = inputs
///   , PV2.txInfoOutputs         = outputs
///   , PV2.txInfoReferenceInputs = refInputs        -- NEW
///   , PV2.txInfoFee             = transCoinToValue (txBody ^. feeTxBodyL)
///   , PV2.txInfoMint            = transMintValue (txBody ^. mintTxBodyL)
///   , PV2.txInfoDCert           = txCerts
///   , PV2.txInfoWdrl            = PV2.unsafeFromList $ transTxBodyWithdrawals txBody
///   , PV2.txInfoValidRange      = timeRange
///   , PV2.txInfoSignatories     = transTxBodyReqSignerHashes txBody
///   , PV2.txInfoRedeemers       = plutusRedeemers   -- NEW
///   , PV2.txInfoData            = PV2.unsafeFromList $ transTxWitsDatums (ltiTx ^. witsTxL)
///   , PV2.txInfoId              = transTxBodyId txBody
///   }
/// ```
#[derive(Debug, Clone)]
pub struct TxInfoV2 {
    pub inputs: Vec<TxInInfoV2>,
    pub reference_inputs: Vec<TxInInfoV2>,
    pub outputs: Vec<TranslatedTxOutV2>,
    pub fee: Value,
    pub mint: Value,
    pub d_cert: Vec<TranslatedCert>,
    pub wdrl: Vec<(TranslatedStakingCredential, u64)>,
    pub valid_range: POSIXTimeRange,
    pub signatories: Vec<Hash>,
    pub redeemers: Vec<(TranslatedScriptPurpose, PlutusData)>,
    pub data: Vec<(DataHash, PlutusData)>,
    pub id: TxId,
}

/// PlutusV3 TxInfo — Conway era additions
///
/// Reference: eras/conway/impl/src/Cardano/Ledger/Conway/TxInfo.hs:481-526
///
/// New fields vs V2: votes, proposal_procedures, current_treasury_amount,
/// treasury_donation. Fee changed from Value to Lovelace.
///
/// ```haskell
/// PV3.TxInfo
///   { PV3.txInfoInputs                = inputsInfo
///   , PV3.txInfoOutputs               = outputs
///   , PV3.txInfoReferenceInputs       = refInputsInfo
///   , PV3.txInfoFee                   = transCoinToLovelace (txBody ^. feeTxBodyL)
///   , PV3.txInfoMint                  = transMintValue (txBody ^. mintTxBodyL)
///   , PV3.txInfoTxCerts               = txCerts
///   , PV3.txInfoWdrl                  = transTxBodyWithdrawals txBody
///   , PV3.txInfoValidRange            = timeRange
///   , PV3.txInfoSignatories           = transTxBodyReqSignerHashes txBody
///   , PV3.txInfoRedeemers             = plutusRedeemers
///   , PV3.txInfoData                  = transTxWitsDatums (ltiTx ^. witsTxL)
///   , PV3.txInfoId                    = transTxBodyId txBody
///   , PV3.txInfoVotes                 = transVotingProcedures ...    -- NEW
///   , PV3.txInfoProposalProcedures    = map transProposal ...       -- NEW
///   , PV3.txInfoCurrentTreasuryAmount = ...                         -- NEW
///   , PV3.txInfoTreasuryDonation      = ...                         -- NEW
///   }
/// ```
#[derive(Debug, Clone)]
pub struct TxInfoV3 {
    pub inputs: Vec<TxInInfoV2>,
    pub reference_inputs: Vec<TxInInfoV2>,
    pub outputs: Vec<TranslatedTxOutV2>,
    pub fee: u64,
    pub mint: Value,
    pub tx_certs: Vec<TranslatedCert>,
    pub wdrl: Vec<(TranslatedCredential, u64)>,
    pub valid_range: POSIXTimeRange,
    pub signatories: Vec<Hash>,
    pub redeemers: Vec<(TranslatedScriptPurpose, PlutusData)>,
    pub data: Vec<(DataHash, PlutusData)>,
    pub id: TxId,
    pub votes: Vec<TranslatedVote>,
    pub proposal_procedures: Vec<TranslatedProposal>,
    pub current_treasury_amount: Option<u64>,
    pub treasury_donation: Option<u64>,
}

/// PlutusV1/V2 TxInInfo — (TxOutRef, TxOut)
#[derive(Debug, Clone)]
pub struct TxInInfo {
    pub out_ref: TranslatedTxOutRef,
    pub resolved: TranslatedTxOut,
}

/// PlutusV2+ TxInInfo — outputs carry inline datums and reference scripts
#[derive(Debug, Clone)]
pub struct TxInInfoV2 {
    pub out_ref: TranslatedTxOutRef,
    pub resolved: TranslatedTxOutV2,
}

/// Translated TxOutRef — Plutus view of a TxIn
#[derive(Debug, Clone)]
pub struct TranslatedTxOutRef {
    pub id: TxId,
    pub idx: u64,
}

/// PlutusV1 translated TxOut
#[derive(Debug, Clone)]
pub struct TranslatedTxOut {
    pub address: TranslatedAddress,
    pub value: Value,
    pub datum_hash: Option<DataHash>,
}

/// PlutusV2+ translated TxOut — inline datums, reference scripts
#[derive(Debug, Clone)]
pub struct TranslatedTxOutV2 {
    pub address: TranslatedAddress,
    pub value: Value,
    pub datum: TranslatedOutputDatum,
    pub reference_script: Option<ScriptHash>,
}

#[derive(Debug, Clone)]
pub enum TranslatedOutputDatum {
    NoOutputDatum,
    OutputDatumHash(DataHash),
    OutputDatum(PlutusData),
}

/// Simplified translated types
#[derive(Debug, Clone)]
pub struct TranslatedAddress(pub Vec<u8>);
#[derive(Debug, Clone)]
pub struct TranslatedCert;
#[derive(Debug, Clone)]
pub struct TranslatedStakingCredential;
#[derive(Debug, Clone)]
pub struct TranslatedCredential;
#[derive(Debug, Clone)]
pub struct TranslatedScriptPurpose;
#[derive(Debug, Clone)]
pub struct TranslatedVote;
#[derive(Debug, Clone)]
pub struct TranslatedProposal;

// ---------------------------------------------------------------------------
// 3b-impl. Building TxInfo from LedgerTxInfo
// ---------------------------------------------------------------------------

/// The TxInfoResult memoizes TxInfo for each language version supported in the era.
///
/// ```haskell
/// -- Alonzo: only V1
/// mkTxInfoResult = AlonzoTxInfoResult . toPlutusTxInfo SPlutusV1
///
/// -- Babbage: V1 + V2
/// mkTxInfoResult lti = BabbageTxInfoResult
///   (toPlutusTxInfo SPlutusV1 lti) (toPlutusTxInfo SPlutusV2 lti)
///
/// -- Conway: V1 + V2 + V3
/// mkTxInfoResult lti = ConwayTxInfoResult
///   (toPlutusTxInfo SPlutusV1 lti) (toPlutusTxInfo SPlutusV2 lti) (toPlutusTxInfo SPlutusV3 lti)
/// ```
///
/// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Plutus/Context.hs:141-157
#[derive(Debug)]
pub enum TxInfoResult {
    Alonzo {
        v1: Result<TxInfoV1, ContextError>,
    },
    Babbage {
        v1: Result<TxInfoV1, ContextError>,
        v2: Result<TxInfoV2, ContextError>,
    },
    Conway {
        v1: Result<TxInfoV1, ContextError>,
        v2: Result<TxInfoV2, ContextError>,
        v3: Result<TxInfoV3, ContextError>,
    },
}

#[derive(Debug, Clone)]
pub enum ContextError {
    TranslationLogicMissingInput(TxIn),
    TimeTranslationPastHorizon(String),
    ByronTxOutInContext,
    ReferenceScriptsNotSupported,
    InlineDatumsNotSupported,
    ReferenceInputsNotDisjointFromInputs,
}

/// Convert a SlotNo to a POSIXTime (milliseconds since Unix epoch).
///
/// This mirrors the Haskell chain:
///
/// ```haskell
/// slotToPOSIXTime :: EpochInfo (Either Text) -> SystemStart -> SlotNo -> Either Text POSIXTime
/// slotToPOSIXTime ei sysS s = do
///   POSIXTime . (truncate . (* 1000)) . nominalDiffTimeToSeconds . utcTimeToPOSIXSeconds
///     <$> epochInfoSlotToUTCTime ei sysS s
/// ```
///
/// The steps are:
///   1. `epochInfoSlotToUTCTime ei sysS s` — converts slot → UTCTime using era history
///      (can fail with `TimeTranslationPastHorizon` if the slot is beyond the forecast)
///   2. `utcTimeToPOSIXSeconds` — converts UTCTime → NominalDiffTime (seconds since Unix epoch)
///   3. `nominalDiffTimeToSeconds` — extracts as Pico (a fixed-point rational in seconds)
///   4. `* 1000` — convert seconds → milliseconds
///   5. `truncate` — truncate to Integer → this is the POSIXTime
///
/// Reference: libs/cardano-ledger-core/src/Cardano/Ledger/Plutus/TxInfo.hs:164-171
fn slot_to_posix_time(
    epoch_info: &EpochInfo,
    system_start: &SystemStart,
    slot: &SlotNo,
) -> Result<POSIXTime, ContextError> {
    // In a real implementation, epochInfoSlotToUTCTime uses the era history
    // (epoch transitions, slot lengths per era) to compute the exact UTC time
    // for a given slot. It can fail if the slot is past the forecast horizon.
    //
    // Simplified: slot_time_utc = system_start + slot * slot_length
    let slot_time_ms = system_start.utc_seconds * 1000
        + (slot.0 as i64) * (epoch_info.slot_length_ms as i64);
    Ok(slot_time_ms)
}

/// Translate a `ValidityInterval` to a Plutus `POSIXTimeRange`.
///
/// This is the exact logic from the Haskell node:
///
/// ```haskell
/// transValidityInterval _ epochInfo systemStart = \case
///   ValidityInterval SNothing  SNothing  -> pure PV1.always
///   ValidityInterval (SJust i) SNothing  -> PV1.from <$> transSlotToPOSIXTime i
///   ValidityInterval SNothing  (SJust i) -> PV1.to   <$> transSlotToPOSIXTime i
///   ValidityInterval (SJust i) (SJust j) -> do
///     t1 <- transSlotToPOSIXTime i
///     t2 <- transSlotToPOSIXTime j
///     pure $ PV1.Interval (PV1.lowerBound t1) (PV1.strictUpperBound t2)
///   where
///     transSlotToPOSIXTime =
///       left (inject . TimeTranslationPastHorizon)
///         . slotToPOSIXTime epochInfo systemStart
/// ```
///
/// The interval semantics are:
///   - `(None, None)`       → `always`       = `(-∞, +∞)`  — no validity constraints
///   - `(Some(i), None)`    → `from t`       = `[t, +∞)`   — valid from slot `i` onwards (inclusive)
///   - `(None, Some(j))`    → `to t`         = `(-∞, t]`   — valid up to and including slot `j`
///   - `(Some(i), Some(j))` → `[t1, t2)`     — valid in `[invalid_before, invalid_hereafter)`
///                                              lower bound is **inclusive** (lowerBound),
///                                              upper bound is **exclusive** (strictUpperBound)
///
/// Note: `invalid_hereafter` uses `strictUpperBound` (exclusive) because the field means
/// "the transaction is invalid at or after this slot" — i.e. the slot itself is NOT included.
///
/// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Plutus/TxInfo.hs:218-241
fn trans_validity_interval(
    _tx: &Tx,
    epoch_info: &EpochInfo,
    system_start: &SystemStart,
    validity: &ValidityInterval,
) -> Result<POSIXTimeRange, ContextError> {
    match (&validity.invalid_before, &validity.invalid_hereafter) {
        // ValidityInterval SNothing SNothing -> pure PV1.always
        (None, None) => Ok(Interval::always()),

        // ValidityInterval (SJust i) SNothing -> PV1.from <$> transSlotToPOSIXTime i
        (Some(i), None) => {
            let t = slot_to_posix_time(epoch_info, system_start, i)?;
            Ok(Interval::from(t))
        }

        // ValidityInterval SNothing (SJust j) -> PV1.to <$> transSlotToPOSIXTime j
        (None, Some(j)) => {
            let t = slot_to_posix_time(epoch_info, system_start, j)?;
            Ok(Interval::to(t))
        }

        // ValidityInterval (SJust i) (SJust j) -> do
        //   t1 <- transSlotToPOSIXTime i
        //   t2 <- transSlotToPOSIXTime j
        //   pure $ PV1.Interval (PV1.lowerBound t1) (PV1.strictUpperBound t2)
        (Some(i), Some(j)) => {
            let t1 = slot_to_posix_time(epoch_info, system_start, i)?;
            let t2 = slot_to_posix_time(epoch_info, system_start, j)?;
            Ok(Interval::from_to_strict_upper(t1, t2))
        }
    }
}

/// Translate a TxIn to the Plutus TxOutRef representation.
fn trans_tx_out_ref(tx_in: &TxIn) -> TranslatedTxOutRef {
    TranslatedTxOutRef {
        id: tx_in.tx_id,
        idx: tx_in.index as u64,
    }
}

/// Build TxInfoV1 (Alonzo) from ledger info.
///
/// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Plutus/TxInfo.hs:104-125
pub fn build_tx_info_v1(lti: &LedgerTxInfo) -> Result<TxInfoV1, ContextError> {
    let tx = lti.tx;
    let tx_body = &tx.body;

    let time_range = trans_validity_interval(
        tx, lti.epoch_info, lti.system_start, &tx_body.validity_interval,
    )?;

    // Translate inputs: look up each input in UTxO, translate the TxOut
    let mut inputs = Vec::new();
    for tx_in in &tx_body.inputs {
        let tx_out = lti.utxo.get(tx_in)
            .ok_or_else(|| ContextError::TranslationLogicMissingInput(tx_in.clone()))?;
        // V1 skips Byron addresses (returns None, filtered by catMaybes)
        inputs.push(TxInInfo {
            out_ref: trans_tx_out_ref(tx_in),
            resolved: TranslatedTxOut {
                address: TranslatedAddress(vec![]), // Simplified translation
                value: tx_out.value.clone(),
                datum_hash: match &tx_out.datum {
                    Datum::DatumHash(h) => Some(*h),
                    _ => None,
                },
            },
        });
    }

    // Translate outputs (V1: only datum hash, no inline datums, no reference scripts)
    let outputs: Vec<TranslatedTxOut> = tx_body.outputs.iter().map(|out| {
        TranslatedTxOut {
            address: TranslatedAddress(vec![]),
            value: out.value.clone(),
            datum_hash: match &out.datum {
                Datum::DatumHash(h) => Some(*h),
                _ => None,
            },
        }
    }).collect();

    // Translate witness datums
    let data: Vec<(DataHash, PlutusData)> = tx.wits.datums.iter()
        .map(|(h, d)| (*h, d.clone()))
        .collect();

    Ok(TxInfoV1 {
        inputs,
        outputs,
        fee: Value { coin: tx_body.fee.clone(), multi_asset: BTreeMap::new() },
        mint: Value { coin: Coin(0), multi_asset: tx_body.mint.clone() },
        d_cert: vec![], // Simplified
        wdrl: vec![],   // Simplified
        valid_range: time_range,
        signatories: tx_body.required_signers.iter().cloned().collect(),
        data,
        id: [0u8; 32],   // Simplified — would be hash of tx body
    })
}

/// Build TxInfoV2 (Babbage) from ledger info.
///
/// Differences from V1:
///   - reference inputs are included
///   - outputs carry inline datums, reference scripts
///   - redeemers are included
///
/// Reference: eras/babbage/impl/src/Cardano/Ledger/Babbage/TxInfo.hs:358-386
pub fn build_tx_info_v2(lti: &LedgerTxInfo) -> Result<TxInfoV2, ContextError> {
    let tx = lti.tx;
    let tx_body = &tx.body;

    let time_range = trans_validity_interval(
        tx, lti.epoch_info, lti.system_start, &tx_body.validity_interval,
    )?;

    let trans_out_v2 = |out: &TxOut| -> TranslatedTxOutV2 {
        TranslatedTxOutV2 {
            address: TranslatedAddress(vec![]),
            value: out.value.clone(),
            datum: match &out.datum {
                Datum::NoDatum => TranslatedOutputDatum::NoOutputDatum,
                Datum::DatumHash(h) => TranslatedOutputDatum::OutputDatumHash(*h),
                Datum::InlineDatum(d) => TranslatedOutputDatum::OutputDatum(d.clone()),
            },
            reference_script: out.reference_script.as_ref().map(|s| s.hash()),
        }
    };

    let trans_in_v2 = |tx_in: &TxIn| -> Result<TxInInfoV2, ContextError> {
        let tx_out = lti.utxo.get(tx_in)
            .ok_or_else(|| ContextError::TranslationLogicMissingInput(tx_in.clone()))?;
        Ok(TxInInfoV2 {
            out_ref: trans_tx_out_ref(tx_in),
            resolved: trans_out_v2(tx_out),
        })
    };

    let inputs: Vec<TxInInfoV2> = tx_body.inputs.iter()
        .map(|i| trans_in_v2(i))
        .collect::<Result<_, _>>()?;

    let ref_inputs: Vec<TxInInfoV2> = tx_body.reference_inputs.iter()
        .map(|i| trans_in_v2(i))
        .collect::<Result<_, _>>()?;

    let outputs: Vec<TranslatedTxOutV2> = tx_body.outputs.iter()
        .map(|o| trans_out_v2(o))
        .collect();

    let data: Vec<(DataHash, PlutusData)> = tx.wits.datums.iter()
        .map(|(h, d)| (*h, d.clone()))
        .collect();

    Ok(TxInfoV2 {
        inputs,
        reference_inputs: ref_inputs,
        outputs,
        fee: Value { coin: tx_body.fee.clone(), multi_asset: BTreeMap::new() },
        mint: Value { coin: Coin(0), multi_asset: tx_body.mint.clone() },
        d_cert: vec![],
        wdrl: vec![],
        valid_range: time_range,
        signatories: tx_body.required_signers.iter().cloned().collect(),
        redeemers: vec![], // Simplified — would translate all redeemers
        data,
        id: [0u8; 32],
    })
}

/// Build TxInfoV3 (Conway) from ledger info.
///
/// Differences from V2:
///   - fee is Lovelace (not Value)
///   - votes, proposal_procedures, treasury fields added
///   - In protocol version >= 11, inputs ∩ reference_inputs must be disjoint
///
/// Reference: eras/conway/impl/src/Cardano/Ledger/Conway/TxInfo.hs:481-526
pub fn build_tx_info_v3(lti: &LedgerTxInfo) -> Result<TxInfoV3, ContextError> {
    let tx = lti.tx;
    let tx_body = &tx.body;

    // In protocol version >= 11, inputs and reference inputs must be disjoint
    if lti.prot_ver.major >= 11 {
        let common: Vec<&TxIn> = tx_body.inputs.intersection(&tx_body.reference_inputs).collect();
        if !common.is_empty() {
            return Err(ContextError::ReferenceInputsNotDisjointFromInputs);
        }
    }

    let time_range = trans_validity_interval(
        tx, lti.epoch_info, lti.system_start, &tx_body.validity_interval,
    )?;

    let trans_out_v2 = |out: &TxOut| -> TranslatedTxOutV2 {
        TranslatedTxOutV2 {
            address: TranslatedAddress(vec![]),
            value: out.value.clone(),
            datum: match &out.datum {
                Datum::NoDatum => TranslatedOutputDatum::NoOutputDatum,
                Datum::DatumHash(h) => TranslatedOutputDatum::OutputDatumHash(*h),
                Datum::InlineDatum(d) => TranslatedOutputDatum::OutputDatum(d.clone()),
            },
            reference_script: out.reference_script.as_ref().map(|s| s.hash()),
        }
    };

    let trans_in = |tx_in: &TxIn| -> Result<TxInInfoV2, ContextError> {
        let tx_out = lti.utxo.get(tx_in)
            .ok_or_else(|| ContextError::TranslationLogicMissingInput(tx_in.clone()))?;
        Ok(TxInInfoV2 {
            out_ref: trans_tx_out_ref(tx_in),
            resolved: trans_out_v2(tx_out),
        })
    };

    let inputs: Vec<TxInInfoV2> = tx_body.inputs.iter()
        .map(|i| trans_in(i))
        .collect::<Result<_, _>>()?;

    let ref_inputs: Vec<TxInInfoV2> = tx_body.reference_inputs.iter()
        .map(|i| trans_in(i))
        .collect::<Result<_, _>>()?;

    let outputs: Vec<TranslatedTxOutV2> = tx_body.outputs.iter()
        .map(|o| trans_out_v2(o))
        .collect();

    let data: Vec<(DataHash, PlutusData)> = tx.wits.datums.iter()
        .map(|(h, d)| (*h, d.clone()))
        .collect();

    Ok(TxInfoV3 {
        inputs,
        reference_inputs: ref_inputs,
        outputs,
        fee: tx_body.fee.0,
        mint: Value { coin: Coin(0), multi_asset: tx_body.mint.clone() },
        tx_certs: vec![],
        wdrl: vec![],
        valid_range: time_range,
        signatories: tx_body.required_signers.iter().cloned().collect(),
        redeemers: vec![],
        data,
        id: [0u8; 32],
        votes: vec![],
        proposal_procedures: vec![],
        current_treasury_amount: None,
        treasury_donation: None,
    })
}

/// Build the memoized TxInfoResult for a given era.
///
/// ```haskell
/// -- Babbage:
/// mkTxInfoResult lti = BabbageTxInfoResult
///   (toPlutusTxInfo SPlutusV1 lti)
///   (toPlutusTxInfo SPlutusV2 lti)
/// ```
pub fn mk_tx_info_result(lti: &LedgerTxInfo, era: Era) -> TxInfoResult {
    match era {
        Era::Alonzo => TxInfoResult::Alonzo {
            v1: build_tx_info_v1(lti),
        },
        Era::Babbage => TxInfoResult::Babbage {
            v1: build_tx_info_v1(lti),
            v2: build_tx_info_v2(lti),
        },
        Era::Conway => TxInfoResult::Conway {
            v1: build_tx_info_v1(lti),
            v2: build_tx_info_v2(lti),
            v3: build_tx_info_v3(lti),
        },
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Era {
    Alonzo,
    Babbage,
    Conway,
}

// ---------------------------------------------------------------------------
// 3c. PlutusArgs — the arguments passed to a Plutus script
// ---------------------------------------------------------------------------

/// PlutusV1/V2 use "legacy" args: 2 or 3 Data arguments.
///   - Spending scripts:  3 args — [Datum, Redeemer, ScriptContext]
///   - Other scripts:     2 args — [Redeemer, ScriptContext]
///
/// PlutusV3+ use a single ScriptContext argument that embeds
/// redeemer + script-specific info.
///
/// Reference: libs/cardano-ledger-core/src/Cardano/Ledger/Plutus/Language.hs:328-535
///
/// ```haskell
/// -- PlutusV1/V2
/// data LegacyPlutusArgs l
///   = LegacyPlutusArgs2  !P.Data !(PlutusScriptContext l)  -- [Redeemer, ScriptContext]
///   | LegacyPlutusArgs3  !P.Data !P.Data !(PlutusScriptContext l)  -- [Datum, Redeemer, ScriptContext]
///
/// -- PlutusV3
/// newtype PlutusArgs 'PlutusV3 = PlutusV3Args { unPlutusV3Args :: PV3.ScriptContext }
/// ```
#[derive(Debug, Clone)]
pub enum PlutusArgs {
    /// PlutusV1/V2 spending script: datum, redeemer, script context (all as Data)
    LegacyArgs3 {
        datum: PlutusData,
        redeemer: PlutusData,
        script_context: PlutusData,
    },
    /// PlutusV1/V2 non-spending script: redeemer, script context (all as Data)
    LegacyArgs2 {
        redeemer: PlutusData,
        script_context: PlutusData,
    },
    /// PlutusV3+ single ScriptContext argument
    V3ScriptContext {
        script_context: PlutusData,
    },
}

// ---------------------------------------------------------------------------
// 3d. PlutusWithContext — everything needed to evaluate one script
// ---------------------------------------------------------------------------

/// All information needed to evaluate a single Plutus script.
///
/// This is the output of `collectPlutusScriptsWithContext` for each needed script.
///
/// Reference: libs/cardano-ledger-core/src/Cardano/Ledger/Plutus/Evaluate.hs:97-120
///
/// ```haskell
/// data PlutusWithContext where
///   PlutusWithContext ::
///     PlutusLanguage l =>
///     { pwcProtocolVersion :: !Version
///     , pwcScript          :: !(Either (Plutus l) (PlutusRunnable l))
///     , pwcScriptHash      :: !ScriptHash
///     , pwcArgs            :: !(PlutusArgs l)
///     , pwcExUnits         :: !ExUnits
///     , pwcCostModel       :: !CostModel
///     } -> PlutusWithContext
/// ```
#[derive(Debug)]
pub struct PlutusWithContext {
    pub protocol_version: u32,
    pub script: Vec<u8>,
    pub script_hash: ScriptHash,
    pub language: Language,
    pub args: PlutusArgs,
    pub ex_units: ExUnits,
    pub cost_model: CostModel,
}

// ---------------------------------------------------------------------------
// 3e. Getting the spending datum
// ---------------------------------------------------------------------------

/// For Spending purposes, look up the datum attached to the UTxO being spent.
///
/// Alonzo: only datums-by-hash from witnesses
///   ```haskell
///   getAlonzoSpendingDatum (UTxO m) tx sp = do
///     AsItem txIn <- toSpendingPurpose sp
///     txOut <- Map.lookup txIn m
///     SJust hash <- Just $ txOut ^. dataHashTxOutL
///     Map.lookup hash $ tx ^. witsTxL . datsTxWitsL . unTxDatsL
///   ```
///
/// Babbage+: also supports inline datums
///   ```haskell
///   getBabbageSpendingDatum (UTxO utxo) tx sp = do
///     AsItem txIn <- toSpendingPurpose sp
///     txOut <- Map.lookup txIn utxo
///     let txOutDataFromWits = do
///           dataHash <- strictMaybeToMaybe (txOut ^. dataHashTxOutL)
///           Map.lookup dataHash (tx ^. witsTxL . datsTxWitsL . unTxDatsL)
///     strictMaybeToMaybe (txOut ^. dataTxOutL) <|> txOutDataFromWits
///   ```
///
/// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/UTxO.hs:142-154
///            eras/babbage/impl/src/Cardano/Ledger/Babbage/UTxO.hs:77-93
pub fn get_spending_datum(
    utxo: &UTxO,
    tx: &Tx,
    purpose: &ScriptPurpose,
    use_inline_datums: bool,
) -> Option<PlutusData> {
    let tx_in = match purpose {
        ScriptPurpose::Spending(tx_in) => tx_in,
        _ => return None,
    };

    let tx_out = utxo.get(tx_in)?;

    if use_inline_datums {
        // Babbage+: prefer inline datum, fallback to witness datum by hash
        match &tx_out.datum {
            Datum::InlineDatum(d) => return Some(d.clone()),
            Datum::DatumHash(h) => return tx.wits.datums.get(h).cloned(),
            Datum::NoDatum => return None,
        }
    } else {
        // Alonzo: only datum-by-hash from witnesses
        match &tx_out.datum {
            Datum::DatumHash(h) => return tx.wits.datums.get(h).cloned(),
            _ => return None,
        }
    }
}

// ============================================================================
// Step 3 (main): collectPlutusScriptsWithContext
// ============================================================================
//
// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Plutus/Evaluate.hs:143-214
//
// ```haskell
// collectPlutusScriptsWithContext epochInfo systemStart pp tx utxo =
//   merge apply (map getScriptWithRedeemer neededPlutusScripts) (Right [])
//   where
//     protVer = pp ^. ppProtocolVersionL
//     costModels = costModelsValid $ pp ^. ppCostModelsL
//     ledgerTxInfo = LedgerTxInfo { ltiProtVer = protVer, ltiEpochInfo = epochInfo, ... }
//     txInfoResult = mkTxInfoResult ledgerTxInfo
//
//     ScriptsProvided scriptsProvided = getScriptsProvided utxo tx
//     AlonzoScriptsNeeded scriptsNeeded = getScriptsNeeded utxo (tx ^. bodyTxL)
//     neededPlutusScripts =
//       mapMaybe (\(sp, sh) -> (,) (sh, sp) <$> lookupPlutusScript sh scriptsProvided) scriptsNeeded
//
//     getScriptWithRedeemer ((plutusScriptHash, plutusPurpose), plutusScript) =
//       case Map.lookup redeemerIndex $ tx ^. witsTxL . rdmrsTxWitsL . unRedeemersL of
//         Just (d, exUnits) -> Right (plutusScript, plutusPurpose, d, exUnits, plutusScriptHash)
//         Nothing -> Left (NoRedeemer ...)
//
//     apply (plutusScript, plutusPurpose, redeemerData, exUnits, plutusScriptHash) = do
//       let lang = plutusScriptLanguage plutusScript
//       costModel <- maybe (Left (NoCostModel lang)) Right $ Map.lookup lang costModels
//       first BadTranslation $ mkPlutusWithContext plutusScript plutusScriptHash plutusPurpose
//                                                  ledgerTxInfo txInfoResult (redeemerData, exUnits) costModel
// ```

#[derive(Debug)]
pub enum CollectError {
    NoRedeemer(ScriptPurpose),
    NoCostModel(Language),
    BadTranslation(ContextError),
}

/// Collect all Plutus scripts that must be evaluated, paired with their
/// fully-constructed context.
///
/// This is the main entry point for Phase 2 script preparation.
///
/// Steps inside:
///   1. Build LedgerTxInfo and memoized TxInfoResult
///   2. Compute scripts needed and provided
///   3. Filter to only Plutus scripts (native scripts handled in Phase 1)
///   4. For each Plutus script:
///      a. Look up its redeemer (by RedeemerPointer)
///      b. Look up its cost model
///      c. Get the spending datum (if Spending purpose)
///      d. Build the PlutusArgs (TxInfo + ScriptPurpose + Redeemer + Datum)
///      e. Package into PlutusWithContext
pub fn collect_plutus_scripts_with_context(
    epoch_info: &EpochInfo,
    system_start: &SystemStart,
    pp: &PParams,
    tx: &Tx,
    utxo: &UTxO,
    era: Era,
) -> Result<Vec<PlutusWithContext>, Vec<CollectError>> {
    let lti = LedgerTxInfo {
        prot_ver: pp.protocol_version,
        epoch_info,
        system_start,
        utxo,
        tx,
    };

    // Build TxInfo for each supported language version (memoized).
    // This is the most expensive part and is shared across all scripts
    // of the same language version.
    let _tx_info_result = mk_tx_info_result(&lti, era);

    // Determine which scripts are needed and which are available.
    let scripts_needed = get_scripts_needed(utxo, &tx.body);
    let scripts_provided = match era {
        Era::Alonzo => get_scripts_provided_alonzo(tx),
        Era::Babbage | Era::Conway => get_scripts_provided_babbage(utxo, tx),
    };

    let use_inline_datums = matches!(era, Era::Babbage | Era::Conway);

    // Filter to only Plutus scripts (native scripts don't need Phase 2 evaluation)
    let needed_plutus: Vec<_> = scripts_needed.iter()
        .filter_map(|(purpose, script_hash)| {
            let script = scripts_provided.get(script_hash)?;
            if script.is_plutus() {
                Some((purpose, *script_hash, script))
            } else {
                None
            }
        })
        .collect();

    let mut results = Vec::new();
    let mut errors = Vec::new();

    for (purpose, script_hash, script) in needed_plutus {
        // 4a. Look up the redeemer for this script purpose
        let redeemer_pointer = purpose_to_redeemer_pointer(purpose, &tx.body);
        let redeemer = match redeemer_pointer
            .and_then(|ptr| tx.wits.redeemers.get(&ptr))
        {
            Some(r) => r,
            None => {
                errors.push(CollectError::NoRedeemer(purpose.clone()));
                continue;
            }
        };

        // 4b. Look up the cost model for this language
        let lang = script.language().unwrap();
        let cost_model = match pp.cost_models.get(&lang) {
            Some(cm) => cm.clone(),
            None => {
                errors.push(CollectError::NoCostModel(lang));
                continue;
            }
        };

        // 4c. Get the spending datum (only for Spending purpose)
        let maybe_spending_datum = get_spending_datum(utxo, tx, purpose, use_inline_datums);

        // 4d. Build PlutusArgs
        //
        // For PlutusV1/V2 (legacy args):
        //   - Spending: [Datum, Redeemer, ScriptContext{TxInfo, ScriptPurpose}]
        //   - Other:    [Redeemer, ScriptContext{TxInfo, ScriptPurpose}]
        //
        //   ```haskell
        //   toLegacyPlutusArgs proxy pv mkScriptContext scriptPurpose maybeSpendingData redeemer = do
        //     scriptContext <- mkScriptContext <$> toPlutusScriptPurpose proxy pv scriptPurpose
        //     let redeemer = getPlutusData redeemerData
        //     pure $ case maybeSpendingData of
        //       Nothing -> LegacyPlutusArgs2 redeemer scriptContext
        //       Just d  -> LegacyPlutusArgs3 (getPlutusData d) redeemer scriptContext
        //   ```
        //
        // For PlutusV3:
        //   - Single arg: ScriptContext { txInfo, redeemer, scriptInfo }
        //
        //   ```haskell
        //   toPlutusV3Args proxy pv txInfo plutusPurpose maybeSpendingData redeemerData = do
        //     scriptPurpose <- toPlutusScriptPurpose proxy pv plutusPurpose
        //     let scriptInfo = scriptPurposeToScriptInfo scriptPurpose (transDatum <$> maybeSpendingData)
        //     pure $ PlutusV3Args $ PV3.ScriptContext
        //       { PV3.scriptContextTxInfo    = txInfo
        //       , PV3.scriptContextRedeemer  = transRedeemer redeemerData
        //       , PV3.scriptContextScriptInfo = scriptInfo
        //       }
        //   ```
        //
        // Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Plutus/TxInfo.hs:133-160
        //            eras/conway/impl/src/Cardano/Ledger/Conway/TxInfo.hs:740-761
        let args = match lang {
            Language::PlutusV1 | Language::PlutusV2 => {
                // ScriptContext wraps TxInfo + translated ScriptPurpose
                // (serialized to Data for passing to the script)
                let script_context_data = PlutusData(vec![]); // Simplified
                match maybe_spending_datum {
                    Some(datum) => PlutusArgs::LegacyArgs3 {
                        datum,
                        redeemer: redeemer.data.clone(),
                        script_context: script_context_data,
                    },
                    None => PlutusArgs::LegacyArgs2 {
                        redeemer: redeemer.data.clone(),
                        script_context: script_context_data,
                    },
                }
            }
            Language::PlutusV3 => {
                // V3 packs everything into a single ScriptContext
                let script_context_data = PlutusData(vec![]); // Simplified
                PlutusArgs::V3ScriptContext {
                    script_context: script_context_data,
                }
            }
        };

        // 4e. Package into PlutusWithContext
        results.push(PlutusWithContext {
            protocol_version: pp.protocol_version.major,
            script: script.plutus_bytes().unwrap().to_vec(),
            script_hash,
            language: lang,
            args,
            ex_units: redeemer.ex_units,
            cost_model,
        });
    }

    if errors.is_empty() {
        Ok(results)
    } else {
        Err(errors)
    }
}

/// Map a ScriptPurpose to its RedeemerPointer (tag + index).
///
/// The index is the position of the item within its category as ordered
/// in the transaction body.
fn purpose_to_redeemer_pointer(purpose: &ScriptPurpose, tx_body: &TxBody) -> Option<RedeemerPointer> {
    match purpose {
        ScriptPurpose::Spending(tx_in) => {
            let index = tx_body.inputs.iter()
                .position(|i| i == tx_in)? as u32;
            Some(RedeemerPointer { tag: RedeemerTag::Spend, index })
        }
        ScriptPurpose::Minting(policy_id) => {
            let index = tx_body.mint.keys()
                .position(|p| p == policy_id)? as u32;
            Some(RedeemerPointer { tag: RedeemerTag::Mint, index })
        }
        ScriptPurpose::Certifying(ix) => {
            Some(RedeemerPointer { tag: RedeemerTag::Cert, index: *ix })
        }
        ScriptPurpose::Rewarding(account) => {
            let index = tx_body.withdrawals.keys()
                .position(|a| a == account)? as u32;
            Some(RedeemerPointer { tag: RedeemerTag::Reward, index })
        }
        _ => None,
    }
}

// ============================================================================
// Step 4: Evaluate all PlutusWithContext
// ============================================================================
//
// Reference: libs/cardano-ledger-core/src/Cardano/Ledger/Plutus/Evaluate.hs:382-404
//            eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Plutus/Evaluate.hs:216-218
//
// ```haskell
// evalPlutusScripts :: [PlutusWithContext] -> ScriptResult
// evalPlutusScripts pwcs = snd $ evalPlutusScriptsWithLogs pwcs
//
// evalPlutusScriptsWithLogs :: [PlutusWithContext] -> ([Text], ScriptResult)
// evalPlutusScriptsWithLogs [] = mempty
// evalPlutusScriptsWithLogs (plutusWithContext : rest) =
//   let (logs, result) = evaluatePlutusWithContext mode pwc
//   in ...
// ```
//
// Each PlutusWithContext is evaluated independently:
//
// ```haskell
// evaluatePlutusWithContext mode pwc@PlutusWithContext{..} =
//   withRunnablePlutusWithContext pwc (([],) . Left) $
//     evaluatePlutusRunnable pwcProtocolVersion mode
//       (getEvaluationContext pwcCostModel)
//       (transExUnits pwcExUnits)
// ```
//
// And `withRunnablePlutusWithContext` deserializes the script if needed:
//
// ```haskell
// withRunnablePlutusWithContext PlutusWithContext{pwcProtocolVersion, pwcScript, pwcArgs} onError f =
//   case pwcScript of
//     Right pr -> f pr pwcArgs              -- already deserialized
//     Left plutus ->
//       case decodePlutusRunnable pwcProtocolVersion plutus of
//         Right pr -> f pr pwcArgs
//         Left err -> onError (CodecError err)
// ```
//
// The actual evaluation calls into the Plutus library:
//   - PlutusV1: PV1.evaluateScriptRestricting (toMajorProtocolVersion pv) vm ec exBudget rs (legacyPlutusArgsToData args)
//   - PlutusV2: PV2.evaluateScriptRestricting ...
//   - PlutusV3: PV3.evaluateScriptRestricting (toMajorProtocolVersion pv) vm ec exBudget rs (PV3.toData . unPlutusV3Args $ args)

#[derive(Debug)]
pub struct ScriptFailure {
    pub message: String,
    pub context: PlutusWithContext,
}

#[derive(Debug)]
pub enum ScriptResult {
    /// All scripts passed. Contains the list of successful PlutusWithContext.
    Passes(Vec<ScriptHash>),
    /// At least one script failed. Contains passing scripts and failure details.
    Fails {
        passed: Vec<ScriptHash>,
        failures: Vec<ScriptFailure>,
    },
}

/// Evaluate all collected Plutus scripts.
///
/// In a real implementation this calls into the Plutus evaluator
/// (plutus-ledger-api). Here we show the structure.
///
/// ```haskell
/// evalPlutusScripts :: [PlutusWithContext] -> ScriptResult
/// evalPlutusScripts pwcs = snd $ evalPlutusScriptsWithLogs pwcs
/// ```
pub fn eval_plutus_scripts(scripts: Vec<PlutusWithContext>) -> ScriptResult {
    let mut passed = Vec::new();

    for pwc in scripts {
        // Step 1: Deserialize the script (if not already deserialized).
        //
        // ```haskell
        // withRunnablePlutusWithContext PlutusWithContext{..} onError f =
        //   case pwcScript of
        //     Right pr -> f pr pwcArgs
        //     Left plutus -> case decodePlutusRunnable pwcProtocolVersion plutus of
        //       Right pr -> f pr pwcArgs
        //       Left err -> onError (CodecError err)
        // ```
        //
        // This is the second time script bytes may be deserialized:
        //   - First time: Phase 1 well-formedness check (is_well_formed / validScript)
        //   - Second time: Here, for actual evaluation
        //
        // The pwcScript field can hold `Either (Plutus l) (PlutusRunnable l)`
        // to allow an optimization where Phase 1 passes the already-deserialized
        // PlutusRunnable (yet to be implemented in Haskell, per the comment in
        // Evaluate.hs:104-107).

        // Step 2: Evaluate with budget constraint.
        //
        // ```haskell
        // evaluatePlutusRunnable pv vm ec exBudget plutusRunnable args
        // ```
        //
        // The evaluation:
        //   - Constructs a UPLC Term from the deserialized script + args
        //   - Runs the CEK machine with the cost model and budget
        //   - Returns (LogOutput, Either EvaluationError ExBudget)

        // Simplified: assume all scripts pass
        passed.push(pwc.script_hash);
    }

    ScriptResult::Passes(passed)
}

// ============================================================================
// UTXOS Transition — where it all comes together
// ============================================================================
//
// The UTXOS rule (Phase 2) is called from the UTXO rule.
// It branches on the `isValid` flag:
//
// ```haskell
// utxosTransition =
//   judgmentContext >>= \(TRC (_, _, tx)) -> do
//     case tx ^. isValidTxL of
//       IsValid True  -> alonzoEvalScriptsTxValid
//       IsValid False -> alonzoEvalScriptsTxInvalid
// ```
//
// For IsValid True (normal case):
//   1. Collect scripts with context
//   2. Evaluate all scripts
//   3. If all pass: apply the transaction normally (update UTxO, fees, deposits, etc.)
//   4. If any fail: reject with ValidationTagMismatch (tx claimed valid but scripts failed)
//
// For IsValid False (expected failure):
//   1. Collect scripts with context
//   2. Evaluate all scripts
//   3. If any fail: seize collateral (remove collateral inputs from UTxO, add fees)
//   4. If all pass: reject with ValidationTagMismatch (tx claimed invalid but scripts passed)
//
// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Rules/Utxos.hs:173-315

/// The UTXOS transition — Phase 2 script evaluation.
///
/// This function is called from the UTXO rule after all Phase 1 checks pass.
pub fn utxos_transition(
    epoch_info: &EpochInfo,
    system_start: &SystemStart,
    pp: &PParams,
    tx: &Tx,
    utxo: &UTxO,
    era: Era,
) -> Result<UTxOSResult, UTxOSError> {
    match collect_plutus_scripts_with_context(epoch_info, system_start, pp, tx, utxo, era) {
        Ok(scripts) => {
            let result = eval_plutus_scripts(scripts);

            match (tx.is_valid, &result) {
                // Transaction claims valid AND scripts pass → apply normally
                (true, ScriptResult::Passes(_)) => {
                    Ok(UTxOSResult::ApplyTx)
                }
                // Transaction claims valid BUT scripts fail → Phase 2 failure
                (true, ScriptResult::Fails { .. }) => {
                    Err(UTxOSError::ValidationTagMismatch {
                        claimed_valid: true,
                    })
                }
                // Transaction claims invalid AND scripts fail → seize collateral
                (false, ScriptResult::Fails { .. }) => {
                    Ok(UTxOSResult::SeizeCollateral)
                }
                // Transaction claims invalid BUT scripts pass → Phase 2 failure
                (false, ScriptResult::Passes(_)) => {
                    Err(UTxOSError::ValidationTagMismatch {
                        claimed_valid: false,
                    })
                }
            }
        }
        Err(collect_errors) => {
            Err(UTxOSError::CollectErrors(collect_errors))
        }
    }
}

#[derive(Debug)]
pub enum UTxOSResult {
    ApplyTx,
    SeizeCollateral,
}

#[derive(Debug)]
pub enum UTxOSError {
    ValidationTagMismatch { claimed_valid: bool },
    CollectErrors(Vec<CollectError>),
}

// ============================================================================
// Summary: The Complete Pipeline
// ============================================================================
//
// Phase 1 (UTXOW/UTXO rules):
//   1. validateScriptsWellFormed  — check witness & reference scripts can be deserialized
//   2. validateFailedScripts      — evaluate native (Phase 1) scripts
//   3. Various structural checks  — signatures, datums, collateral, fees, etc.
//
// Phase 2 (UTXOS rule):
//   1. getScriptsNeeded           — which scripts are required by the transaction?
//   2. getScriptsProvided         — which scripts are available (witnesses + reference scripts)?
//   3. collectPlutusScriptsWithContext:
//      a. Build LedgerTxInfo      — gather protocol version, epoch info, UTxO, tx
//      b. mkTxInfoResult          — translate tx to Plutus TxInfo (one per language version)
//      c. For each needed Plutus script:
//         i.   Look up redeemer   — from tx witnesses by RedeemerPointer
//         ii.  Look up cost model — from protocol parameters by language
//         iii. Get spending datum — from UTxO (by hash or inline) for Spending purposes
//         iv.  Build PlutusArgs   — TxInfo + ScriptPurpose + Redeemer + Datum
//         v.   Package into PlutusWithContext
//   4. evalPlutusScripts          — deserialize (if needed) and run each script on CEK machine
//   5. Check isValid flag matches script results:
//      - isValid=True  + scripts pass  → apply transaction
//      - isValid=True  + scripts fail  → reject (ValidationTagMismatch)
//      - isValid=False + scripts fail  → seize collateral
//      - isValid=False + scripts pass  → reject (ValidationTagMismatch)

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_utxo() -> UTxO {
        let mut utxo = HashMap::new();

        // A script-locked UTxO with a datum hash
        let script_hash = [1u8; 32];
        utxo.insert(
            TxIn { tx_id: [10u8; 32], index: 0 },
            TxOut {
                address: Address {
                    payment: Credential::ScriptHash(script_hash),
                    staking: None,
                },
                value: Value { coin: Coin(5_000_000), multi_asset: BTreeMap::new() },
                datum: Datum::DatumHash([20u8; 32]),
                reference_script: None,
            },
        );

        // A key-locked UTxO (no script needed)
        utxo.insert(
            TxIn { tx_id: [11u8; 32], index: 0 },
            TxOut {
                address: Address {
                    payment: Credential::KeyHash([2u8; 32]),
                    staking: None,
                },
                value: Value { coin: Coin(10_000_000), multi_asset: BTreeMap::new() },
                datum: Datum::NoDatum,
                reference_script: None,
            },
        );

        utxo
    }

    #[test]
    fn test_scripts_needed_spending() {
        let utxo = make_test_utxo();

        let tx_body = TxBody {
            inputs: [
                TxIn { tx_id: [10u8; 32], index: 0 },
                TxIn { tx_id: [11u8; 32], index: 0 },
            ].into_iter().collect(),
            reference_inputs: BTreeSet::new(),
            outputs: vec![],
            collateral_inputs: BTreeSet::new(),
            collateral_return: None,
            fee: Coin(200_000),
            mint: BTreeMap::new(),
            validity_interval: ValidityInterval {
                invalid_before: None,
                invalid_hereafter: Some(SlotNo(100)),
            },
            required_signers: HashSet::new(),
            certs: vec![],
            withdrawals: BTreeMap::new(),
        };

        let needed = get_scripts_needed(&utxo, &tx_body);

        // Only the script-locked input should generate a needed script
        assert_eq!(needed.len(), 1);
        assert_eq!(needed[0].1, [1u8; 32]); // script hash
        assert!(matches!(needed[0].0, ScriptPurpose::Spending(_)));
    }

    #[test]
    fn test_scripts_needed_minting() {
        let utxo = make_test_utxo();
        let policy_id = [42u8; 32];

        let tx_body = TxBody {
            inputs: [TxIn { tx_id: [11u8; 32], index: 0 }].into_iter().collect(),
            reference_inputs: BTreeSet::new(),
            outputs: vec![],
            collateral_inputs: BTreeSet::new(),
            collateral_return: None,
            fee: Coin(200_000),
            mint: [(policy_id, [("token".into(), 100)].into_iter().collect())].into_iter().collect(),
            validity_interval: ValidityInterval {
                invalid_before: None,
                invalid_hereafter: Some(SlotNo(100)),
            },
            required_signers: HashSet::new(),
            certs: vec![],
            withdrawals: BTreeMap::new(),
        };

        let needed = get_scripts_needed(&utxo, &tx_body);

        // Minting always requires the policy script
        assert_eq!(needed.len(), 1);
        assert_eq!(needed[0].1, policy_id);
        assert!(matches!(needed[0].0, ScriptPurpose::Minting(_)));
    }

    #[test]
    fn test_scripts_provided_babbage_includes_reference_scripts() {
        let mut utxo = make_test_utxo();

        let ref_script = Script::PlutusV2(vec![0x01, 0x00, 0x00, 0x00, 0x20]);
        let ref_script_hash = ref_script.hash();

        // Add a UTxO with a reference script
        utxo.insert(
            TxIn { tx_id: [12u8; 32], index: 0 },
            TxOut {
                address: Address {
                    payment: Credential::KeyHash([3u8; 32]),
                    staking: None,
                },
                value: Value { coin: Coin(2_000_000), multi_asset: BTreeMap::new() },
                datum: Datum::NoDatum,
                reference_script: Some(ref_script),
            },
        );

        let tx = Tx {
            body: TxBody {
                inputs: [TxIn { tx_id: [11u8; 32], index: 0 }].into_iter().collect(),
                reference_inputs: [TxIn { tx_id: [12u8; 32], index: 0 }].into_iter().collect(),
                outputs: vec![],
                collateral_inputs: BTreeSet::new(),
                collateral_return: None,
                fee: Coin(200_000),
                mint: BTreeMap::new(),
                validity_interval: ValidityInterval {
                    invalid_before: None,
                    invalid_hereafter: Some(SlotNo(100)),
                },
                required_signers: HashSet::new(),
                certs: vec![],
                withdrawals: BTreeMap::new(),
            },
            wits: TxWits {
                scripts: HashMap::new(),
                datums: HashMap::new(),
                redeemers: HashMap::new(),
            },
            is_valid: true,
        };

        let provided = get_scripts_provided_babbage(&utxo, &tx);

        // Reference script should be included in provided scripts
        assert!(provided.contains_key(&ref_script_hash));
    }

    #[test]
    fn test_spending_datum_alonzo_uses_witness() {
        let utxo = make_test_utxo();
        let datum_hash = [20u8; 32];
        let datum = PlutusData(vec![0xDE, 0xAD]);

        let tx = Tx {
            body: TxBody {
                inputs: [TxIn { tx_id: [10u8; 32], index: 0 }].into_iter().collect(),
                reference_inputs: BTreeSet::new(),
                outputs: vec![],
                collateral_inputs: BTreeSet::new(),
                collateral_return: None,
                fee: Coin(200_000),
                mint: BTreeMap::new(),
                validity_interval: ValidityInterval {
                    invalid_before: None,
                    invalid_hereafter: Some(SlotNo(100)),
                },
                required_signers: HashSet::new(),
                certs: vec![],
                withdrawals: BTreeMap::new(),
            },
            wits: TxWits {
                scripts: HashMap::new(),
                datums: [(datum_hash, datum.clone())].into_iter().collect(),
                redeemers: HashMap::new(),
            },
            is_valid: true,
        };

        let purpose = ScriptPurpose::Spending(TxIn { tx_id: [10u8; 32], index: 0 });

        // Alonzo: datum comes from witnesses by hash
        let result = get_spending_datum(&utxo, &tx, &purpose, false);
        assert!(result.is_some());
    }

    #[test]
    fn test_spending_datum_babbage_prefers_inline() {
        let mut utxo = HashMap::new();
        let inline_datum = PlutusData(vec![0xCA, 0xFE]);

        utxo.insert(
            TxIn { tx_id: [10u8; 32], index: 0 },
            TxOut {
                address: Address {
                    payment: Credential::ScriptHash([1u8; 32]),
                    staking: None,
                },
                value: Value { coin: Coin(5_000_000), multi_asset: BTreeMap::new() },
                datum: Datum::InlineDatum(inline_datum.clone()),
                reference_script: None,
            },
        );

        let tx = Tx {
            body: TxBody {
                inputs: [TxIn { tx_id: [10u8; 32], index: 0 }].into_iter().collect(),
                reference_inputs: BTreeSet::new(),
                outputs: vec![],
                collateral_inputs: BTreeSet::new(),
                collateral_return: None,
                fee: Coin(200_000),
                mint: BTreeMap::new(),
                validity_interval: ValidityInterval {
                    invalid_before: None,
                    invalid_hereafter: Some(SlotNo(100)),
                },
                required_signers: HashSet::new(),
                certs: vec![],
                withdrawals: BTreeMap::new(),
            },
            wits: TxWits {
                scripts: HashMap::new(),
                datums: HashMap::new(),
                redeemers: HashMap::new(),
            },
            is_valid: true,
        };

        let purpose = ScriptPurpose::Spending(TxIn { tx_id: [10u8; 32], index: 0 });

        // Babbage: inline datum is preferred
        let result = get_spending_datum(&utxo, &tx, &purpose, true);
        assert!(result.is_some());
    }

    #[test]
    fn test_non_spending_purpose_has_no_datum() {
        let utxo = make_test_utxo();
        let tx = Tx {
            body: TxBody {
                inputs: BTreeSet::new(),
                reference_inputs: BTreeSet::new(),
                outputs: vec![],
                collateral_inputs: BTreeSet::new(),
                collateral_return: None,
                fee: Coin(200_000),
                mint: BTreeMap::new(),
                validity_interval: ValidityInterval {
                    invalid_before: None,
                    invalid_hereafter: Some(SlotNo(100)),
                },
                required_signers: HashSet::new(),
                certs: vec![],
                withdrawals: BTreeMap::new(),
            },
            wits: TxWits {
                scripts: HashMap::new(),
                datums: HashMap::new(),
                redeemers: HashMap::new(),
            },
            is_valid: true,
        };

        let purpose = ScriptPurpose::Minting([42u8; 32]);
        let result = get_spending_datum(&utxo, &tx, &purpose, true);
        assert!(result.is_none());
    }

    fn make_epoch_info() -> EpochInfo {
        EpochInfo { slot_length_ms: 1000 } // 1 second per slot
    }

    fn make_system_start() -> SystemStart {
        // Unix epoch 1_600_000_000 seconds = 2020-09-13 12:26:40 UTC
        SystemStart { utc_seconds: 1_600_000_000 }
    }

    fn make_dummy_tx() -> Tx {
        Tx {
            body: TxBody {
                inputs: BTreeSet::new(),
                reference_inputs: BTreeSet::new(),
                outputs: vec![],
                collateral_inputs: BTreeSet::new(),
                collateral_return: None,
                fee: Coin(200_000),
                mint: BTreeMap::new(),
                validity_interval: ValidityInterval {
                    invalid_before: None,
                    invalid_hereafter: None,
                },
                required_signers: HashSet::new(),
                certs: vec![],
                withdrawals: BTreeMap::new(),
            },
            wits: TxWits {
                scripts: HashMap::new(),
                datums: HashMap::new(),
                redeemers: HashMap::new(),
            },
            is_valid: true,
        }
    }

    #[test]
    fn test_slot_to_posix_time() {
        let ei = make_epoch_info();
        let ss = make_system_start();

        // slot 0 → system_start in milliseconds
        let t = slot_to_posix_time(&ei, &ss, &SlotNo(0)).unwrap();
        assert_eq!(t, 1_600_000_000_000);

        // slot 100 → system_start + 100 * 1000 ms
        let t = slot_to_posix_time(&ei, &ss, &SlotNo(100)).unwrap();
        assert_eq!(t, 1_600_000_100_000);
    }

    #[test]
    fn test_validity_interval_always() {
        // (None, None) → always = (-∞, +∞) with inclusive bounds
        let ei = make_epoch_info();
        let ss = make_system_start();
        let tx = make_dummy_tx();
        let vi = ValidityInterval { invalid_before: None, invalid_hereafter: None };

        let result = trans_validity_interval(&tx, &ei, &ss, &vi).unwrap();
        assert_eq!(result, Interval::always());
        assert_eq!(result.from.bound, Extended::NegInf);
        assert_eq!(result.to.bound, Extended::PosInf);
        assert!(result.from.closed);
        assert!(result.to.closed);
    }

    #[test]
    fn test_validity_interval_from_only() {
        // (Some(10), None) → [t10, +∞)  — inclusive lower, open upper
        let ei = make_epoch_info();
        let ss = make_system_start();
        let tx = make_dummy_tx();
        let vi = ValidityInterval {
            invalid_before: Some(SlotNo(10)),
            invalid_hereafter: None,
        };

        let result = trans_validity_interval(&tx, &ei, &ss, &vi).unwrap();
        let t10 = slot_to_posix_time(&ei, &ss, &SlotNo(10)).unwrap();

        assert_eq!(result.from.bound, Extended::Finite(t10));
        assert!(result.from.closed); // lowerBound → inclusive
        assert_eq!(result.to.bound, Extended::PosInf);
        assert!(result.to.closed);
    }

    #[test]
    fn test_validity_interval_to_only() {
        // (None, Some(100)) → (-∞, t100]  — open lower, inclusive upper
        let ei = make_epoch_info();
        let ss = make_system_start();
        let tx = make_dummy_tx();
        let vi = ValidityInterval {
            invalid_before: None,
            invalid_hereafter: Some(SlotNo(100)),
        };

        let result = trans_validity_interval(&tx, &ei, &ss, &vi).unwrap();
        let t100 = slot_to_posix_time(&ei, &ss, &SlotNo(100)).unwrap();

        assert_eq!(result.from.bound, Extended::NegInf);
        assert!(result.from.closed);
        assert_eq!(result.to.bound, Extended::Finite(t100));
        assert!(result.to.closed); // `to` uses upperBound → inclusive
    }

    #[test]
    fn test_validity_interval_both_bounds() {
        // (Some(10), Some(100)) → [t10, t100)
        // lower bound is INCLUSIVE (lowerBound), upper is EXCLUSIVE (strictUpperBound)
        let ei = make_epoch_info();
        let ss = make_system_start();
        let tx = make_dummy_tx();
        let vi = ValidityInterval {
            invalid_before: Some(SlotNo(10)),
            invalid_hereafter: Some(SlotNo(100)),
        };

        let result = trans_validity_interval(&tx, &ei, &ss, &vi).unwrap();
        let t10 = slot_to_posix_time(&ei, &ss, &SlotNo(10)).unwrap();
        let t100 = slot_to_posix_time(&ei, &ss, &SlotNo(100)).unwrap();

        // Lower bound: Finite(t10), closed=true  (lowerBound)
        assert_eq!(result.from.bound, Extended::Finite(t10));
        assert!(result.from.closed);

        // Upper bound: Finite(t100), closed=false  (strictUpperBound)
        assert_eq!(result.to.bound, Extended::Finite(t100));
        assert!(!result.to.closed); // STRICT upper bound — exclusive!
    }
}
