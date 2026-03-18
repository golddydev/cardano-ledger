// Babbage Era UTXO Validation
//
// This module implements the Babbage UTXO rule, which introduces:
// - Reference inputs (read UTxOs without spending)
// - Inline datums (store datums directly in outputs)
// - Reference scripts (reuse scripts via reference)
// - Collateral return (get change back from collateral)
//
// Reference: eras/babbage/impl/src/Cardano/Ledger/Babbage/Rules/Utxo.hs
//
// ============================================================================
// BABBAGE UTXO RULE SUMMARY
// ============================================================================
//
// Key additions from Alonzo:
// - Reference inputs validation (disjoint from spending inputs)
// - Collateral return output support
// - Relaxed collateral token rules
// - totalCollateral field validation
// - coinsPerUTxOByte (more accurate than coinsPerUTxOWord)
// - All outputs (including collateral return) are validated
//
// ============================================================================

use std::collections::{HashMap, HashSet};

// Import types from previous eras
pub use super::alonzo_utxo::{
    AlonzoTxOut, AlonzoTxWits, AlonzoUTxO, AlonzoUtxoPredFailure, Datum, ExUnits,
    PlutusScript, PlutusVersion, Redeemer, ScriptPurpose, ValidityInterval, Value,
    VKeyWitness, BootstrapWitness,
};
pub use super::shelley_utxo::{
    Addr, Certificate, CertState, Coin, Credential, Network, NativeScript,
    PoolParams, RewardAccount, SlotNo, TxIn, TxSize, Update,
};

// ============================================================================
// Babbage-Specific Type Definitions
// ============================================================================

/// Babbage transaction output with inline datum and reference script support
#[derive(Debug, Clone)]
pub struct BabbageTxOut {
    pub address: Addr,
    pub value: Value,
    /// Datum can be: None, Hash only, or Inline (full datum)
    pub datum: BabbageDatum,
    /// Optional reference script
    pub reference_script: Option<BabbageScript>,
}

impl BabbageTxOut {
    /// Check if output is VKey-locked
    pub fn is_vkey_locked(&self) -> bool {
        match &self.address {
            Addr::Base { payment_credential, .. } => {
                matches!(payment_credential, Credential::KeyHash(_))
            }
            Addr::Enterprise { payment_credential, .. } => {
                matches!(payment_credential, Credential::KeyHash(_))
            }
            Addr::Pointer { payment_credential, .. } => {
                matches!(payment_credential, Credential::KeyHash(_))
            }
            Addr::Bootstrap { .. } => true,
            Addr::Reward { .. } => true,
        }
    }

    /// Calculate serialized size (for minUTxO calculation)
    pub fn serialized_size(&self) -> usize {
        let mut size = 0;

        // Address size (varies by type)
        size += 57; // Base address is typically 57 bytes

        // Value size
        size += 8; // Coin
        for (_, assets) in &self.value.multi_asset {
            size += 28; // Policy ID
            for (name, _) in assets {
                size += name.len() + 8;
            }
        }

        // Datum size
        size += match &self.datum {
            BabbageDatum::None => 0,
            BabbageDatum::Hash(_) => 32,
            BabbageDatum::Inline(data) => data.len(),
        };

        // Reference script size
        if let Some(script) = &self.reference_script {
            size += match script {
                BabbageScript::Native(s) => s.bytes.len(),
                BabbageScript::PlutusV1(s) => s.bytes.len(),
                BabbageScript::PlutusV2(s) => s.bytes.len(),
            };
        }

        size
    }
}

/// Babbage datum types
#[derive(Debug, Clone)]
pub enum BabbageDatum {
    /// No datum
    None,
    /// Datum hash only (Alonzo style)
    Hash([u8; 32]),
    /// Inline datum (Babbage feature)
    Inline(Vec<u8>),
}

/// Babbage script types
#[derive(Debug, Clone)]
pub enum BabbageScript {
    Native(NativeScriptBytes),
    PlutusV1(PlutusScript),
    PlutusV2(PlutusScript),
}

/// Native script with bytes
#[derive(Debug, Clone)]
pub struct NativeScriptBytes {
    pub script: NativeScript,
    pub bytes: Vec<u8>,
}

/// Sized output (pre-computed size for efficiency)
#[derive(Debug, Clone)]
pub struct SizedTxOut {
    pub output: BabbageTxOut,
    pub size: usize,
}

impl SizedTxOut {
    pub fn new(output: BabbageTxOut) -> Self {
        let size = output.serialized_size();
        SizedTxOut { output, size }
    }
}

/// Babbage Protocol Parameters
#[derive(Debug, Clone)]
pub struct BabbagePParams {
    // Inherited
    pub min_fee_a: u64,
    pub min_fee_b: u64,
    pub max_tx_size: u32,
    pub key_deposit: Coin,
    pub pool_deposit: Coin,
    pub collateral_percentage: u32,
    pub max_collateral_inputs: u32,
    pub max_tx_ex_units: ExUnits,
    pub max_val_size: u32,

    // Babbage: replaces coinsPerUTxOWord
    /// Coins per byte for minUTxO calculation (more accurate than per word)
    pub coins_per_utxo_byte: Coin,

    /// Protocol version
    pub protocol_version: (u32, u32),
}

/// Babbage Transaction Body
#[derive(Debug, Clone)]
pub struct BabbageTxBody {
    /// Spending inputs
    pub inputs: HashSet<TxIn>,

    /// Collateral inputs
    pub collateral_inputs: HashSet<TxIn>,

    /// Reference inputs (NEW in Babbage - read without spending)
    pub reference_inputs: HashSet<TxIn>,

    /// Regular outputs
    pub outputs: Vec<BabbageTxOut>,

    /// Collateral return output (NEW in Babbage)
    pub collateral_return: Option<BabbageTxOut>,

    /// Explicit total collateral (NEW in Babbage)
    pub total_collateral: Option<Coin>,

    /// Transaction fee
    pub fee: Coin,

    /// Validity interval
    pub validity_interval: ValidityInterval,

    /// Withdrawals
    pub withdrawals: HashMap<RewardAccount, Coin>,

    /// Certificates
    pub certificates: Vec<Certificate>,

    /// Minting
    pub mint: HashMap<[u8; 28], HashMap<Vec<u8>, i64>>,

    /// Script data hash
    pub script_data_hash: Option<[u8; 32]>,

    /// Network ID
    pub network_id: Option<Network>,
}

impl BabbageTxBody {
    /// Get all inputs (spending + collateral + reference)
    pub fn all_inputs(&self) -> HashSet<TxIn> {
        let mut all = self.inputs.clone();
        all.extend(self.collateral_inputs.clone());
        all.extend(self.reference_inputs.clone());
        all
    }

    /// Get all outputs including collateral return
    pub fn all_outputs(&self) -> Vec<&BabbageTxOut> {
        let mut all: Vec<&BabbageTxOut> = self.outputs.iter().collect();
        if let Some(ref ret) = self.collateral_return {
            all.push(ret);
        }
        all
    }

    /// Get all sized outputs
    pub fn all_sized_outputs(&self) -> Vec<SizedTxOut> {
        let mut all: Vec<SizedTxOut> = self.outputs.iter().map(|o| SizedTxOut::new(o.clone())).collect();
        if let Some(ref ret) = self.collateral_return {
            all.push(SizedTxOut::new(ret.clone()));
        }
        all
    }
}

/// Babbage Transaction
#[derive(Debug, Clone)]
pub struct BabbageTx {
    pub body: BabbageTxBody,
    pub wits: AlonzoTxWits,
    pub is_valid: bool,
    pub auxiliary_data: Option<Vec<u8>>,
}

impl BabbageTx {
    pub fn size(&self) -> TxSize {
        0 // Simplified
    }
}

/// Babbage UTxO
pub struct BabbageUTxO {
    pub utxo: HashMap<TxIn, BabbageTxOut>,
}

impl BabbageUTxO {
    pub fn get(&self, input: &TxIn) -> Option<&BabbageTxOut> {
        self.utxo.get(input)
    }

    pub fn contains(&self, input: &TxIn) -> bool {
        self.utxo.contains_key(input)
    }

    pub fn restrict(&self, inputs: &HashSet<TxIn>) -> HashMap<TxIn, BabbageTxOut> {
        inputs
            .iter()
            .filter_map(|input| self.utxo.get(input).map(|out| (input.clone(), out.clone())))
            .collect()
    }

    /// Sum coin values for a set of inputs
    pub fn balance_coin(&self, inputs: &HashSet<TxIn>) -> Coin {
        inputs
            .iter()
            .filter_map(|input| self.utxo.get(input))
            .map(|out| out.value.coin)
            .sum()
    }

    /// Sum all values for a set of inputs
    pub fn balance(&self, inputs: &HashSet<TxIn>) -> Value {
        let mut total = Value::default();
        for input in inputs {
            if let Some(out) = self.utxo.get(input) {
                total.coin += out.value.coin;
                // Would merge multi_asset in real implementation
            }
        }
        total
    }
}

// ============================================================================
// Error Types
// ============================================================================

/// Babbage UTXO predicate failures
///
/// Reference: eras/babbage/impl/src/Cardano/Ledger/Babbage/Rules/Utxo.hs:68-79
///
/// ```haskell
/// data BabbageUtxoPredFailure era
///   = AlonzoInBabbageUtxoPredFailure (AlonzoUtxoPredFailure era)
///   | IncorrectTotalCollateralField DeltaCoin Coin
///   | BabbageOutputTooSmallUTxO [(TxOut era, Coin)]
///   | BabbageNonDisjointRefInputs (NonEmpty TxIn)
/// ```
#[derive(Debug, Clone)]
pub enum BabbageUtxoPredFailure {
    /// Wrapped Alonzo error (all Alonzo errors are valid Babbage errors)
    AlonzoInBabbage(AlonzoUtxoPredFailure),

    /// totalCollateral field doesn't match computed balance
    /// Formal: (txcoll tx ≠ ◇) ⇒ balance = txcoll tx
    IncorrectTotalCollateralField {
        computed: i64,    // DeltaCoin
        declared: Coin,
    },

    /// Outputs below minimum with the minimum value included
    /// Different from Alonzo: includes the minimum for each output
    BabbageOutputTooSmallUTxO(Vec<(BabbageTxOut, Coin)>),

    /// Reference inputs overlap with spending inputs
    /// Formal: inputs ∩ refInputs = ∅
    BabbageNonDisjointRefInputs(Vec<TxIn>),
}

// ============================================================================
// Validation Functions
// ============================================================================

/// Validate reference inputs are disjoint from spending inputs
///
/// Reference: eras/babbage/impl/src/Cardano/Ledger/Babbage/Rules/Utxo.hs:225-239
///
/// ```haskell
/// disjointRefInputs ::
///   forall era.
///   EraPParams era =>
///   PParams era ->
///   Set TxIn ->
///   Set TxIn ->
///   Test (BabbageUtxoPredFailure era)
/// disjointRefInputs pp inputs refInputs =
///   when (protocolVersion check)
///     (failureOnNonEmpty common BabbageNonDisjointRefInputs)
///   where
///     common = inputs `Set.intersection` refInputs
/// ```
///
/// Formal specification: inputs ∩ refInputs = ∅
///
/// # Why Reference Inputs Must Be Disjoint:
/// - Reference inputs are READ-ONLY (not consumed)
/// - Spending inputs ARE consumed (removed from UTxO)
/// - Can't both read and spend the same UTxO
/// - Prevents ambiguous transaction semantics
pub fn validate_disjoint_ref_inputs(
    inputs: &HashSet<TxIn>,
    ref_inputs: &HashSet<TxIn>,
) -> Result<(), BabbageUtxoPredFailure> {
    let common: Vec<TxIn> = inputs.intersection(ref_inputs).cloned().collect();

    if common.is_empty() {
        Ok(())
    } else {
        Err(BabbageUtxoPredFailure::BabbageNonDisjointRefInputs(common))
    }
}

/// Validate all inputs exist (spending + collateral + reference)
///
/// In Babbage, we check ALL three input types exist in UTxO
pub fn validate_bad_inputs(
    utxo: &BabbageUTxO,
    inputs: &HashSet<TxIn>,
    collateral_inputs: &HashSet<TxIn>,
    reference_inputs: &HashSet<TxIn>,
) -> Result<(), BabbageUtxoPredFailure> {
    // Combine all input types
    let all_inputs: HashSet<TxIn> = inputs
        .union(collateral_inputs)
        .cloned()
        .chain(reference_inputs.iter().cloned())
        .collect();

    let bad_inputs: HashSet<TxIn> = all_inputs
        .iter()
        .filter(|input| !utxo.contains(input))
        .cloned()
        .collect();

    if bad_inputs.is_empty() {
        Ok(())
    } else {
        Err(BabbageUtxoPredFailure::AlonzoInBabbage(
            AlonzoUtxoPredFailure::BadInputsUTxO(bad_inputs),
        ))
    }
}

/// Validate collateral contains only ADA (Babbage relaxed version)
///
/// Reference: eras/babbage/impl/src/Cardano/Ledger/Babbage/Rules/Utxo.hs:278-317
///
/// ```haskell
/// validateCollateralContainsNonADA ::
///   BabbageEraTxBody era =>
///   TxBody TopTx era ->
///   Map.Map TxIn (TxOut era) ->
///   Test (AlonzoUtxoPredFailure era)
/// validateCollateralContainsNonADA txBody utxoCollateral =
///   failureUnless onlyAdaInCollateral $ CollateralContainsNonADA valueWithNonAda
///   where
///     onlyAdaInCollateral =
///       utxoCollateralAndReturnHaveOnlyAda || allNonAdaIsConsumedByReturn
///     allNonAdaIsConsumedByReturn = Val.isAdaOnly totalCollateralBalance
///     totalCollateralBalance = case txBody ^. collateralReturnTxBodyL of
///       SNothing -> collateralBalance
///       SJust retTxOut -> collateralBalance <-> (retTxOut ^. valueTxOutL)
/// ```
///
/// # Babbage Improvement:
/// Collateral inputs CAN contain native tokens as long as the
/// collateral return output "absorbs" them. Only the NET collateral
/// (what would be taken if scripts fail) must be ADA-only.
///
/// # Example:
/// ```text
/// Collateral Input:  10 ADA + 100 TokenX
/// Collateral Return:  8 ADA + 100 TokenX
/// Net Collateral:     2 ADA (ADA-only!) ✓
/// ```
pub fn validate_collateral_contains_non_ada(
    collateral_utxo: &HashMap<TxIn, BabbageTxOut>,
    collateral_return: Option<&BabbageTxOut>,
) -> Result<(), BabbageUtxoPredFailure> {
    // Sum all collateral input values
    let mut collateral_value = Value::default();
    for out in collateral_utxo.values() {
        collateral_value.coin += out.value.coin;
        // Would merge multi_asset in real implementation
    }

    // Check if inputs are already ADA-only
    let inputs_ada_only = collateral_utxo.values().all(|out| out.value.is_ada_only());
    if inputs_ada_only {
        return Ok(());
    }

    // If not, check if collateral return absorbs all non-ADA
    if let Some(return_out) = collateral_return {
        // Net collateral = inputs - return
        // Only the NET must be ADA-only
        let net_value = Value {
            coin: collateral_value.coin.saturating_sub(return_out.value.coin),
            multi_asset: HashMap::new(), // Would compute difference
        };

        if net_value.is_ada_only() {
            return Ok(());
        }
    }

    // Neither condition met - fail
    Err(BabbageUtxoPredFailure::AlonzoInBabbage(
        AlonzoUtxoPredFailure::CollateralContainsNonADA(collateral_value),
    ))
}

/// Validate totalCollateral field matches computed balance
///
/// Reference: eras/babbage/impl/src/Cardano/Ledger/Babbage/Rules/Utxo.hs:320-325
///
/// ```haskell
/// validateCollateralEqBalance ::
///   DeltaCoin -> StrictMaybe Coin -> Validation (NonEmpty (BabbageUtxoPredFailure era)) ()
/// validateCollateralEqBalance bal txcoll =
///   case txcoll of
///     SNothing -> pure ()
///     SJust tc -> failureUnless (bal == toDeltaCoin tc) (IncorrectTotalCollateralField bal tc)
/// ```
///
/// Formal specification: (txcoll tx ≠ ◇) ⇒ balance = txcoll tx
///
/// # Why This Field Exists:
/// - Wallets can specify exact collateral amount upfront
/// - Provides certainty about maximum loss if scripts fail
/// - Protects against wallet calculation bugs
/// - Optional for backward compatibility
pub fn validate_collateral_eq_balance(
    computed_balance: i64,  // Can be negative (DeltaCoin)
    declared_total: Option<Coin>,
) -> Result<(), BabbageUtxoPredFailure> {
    match declared_total {
        None => Ok(()), // Not specified, no check needed
        Some(tc) => {
            if computed_balance == tc as i64 {
                Ok(())
            } else {
                Err(BabbageUtxoPredFailure::IncorrectTotalCollateralField {
                    computed: computed_balance,
                    declared: tc,
                })
            }
        }
    }
}

/// Calculate minimum UTxO value (Babbage uses coinsPerUTxOByte)
///
/// Babbage formula: serSize(txOut) * coinsPerUTxOByte
///
/// This is more accurate than Alonzo's word-based calculation.
pub fn get_min_coin_sized_tx_out(pp: &BabbagePParams, sized_out: &SizedTxOut) -> Coin {
    (sized_out.size as u64) * pp.coins_per_utxo_byte
}

/// Validate outputs meet minimum value (Babbage version)
///
/// Reference: eras/babbage/impl/src/Cardano/Ledger/Babbage/Rules/Utxo.hs:328-348
///
/// ```haskell
/// validateOutputTooSmallUTxO ::
///   forall era f.
///   (BabbageEraTxOut era, Foldable f) =>
///   PParams era ->
///   f (Sized (TxOut era)) ->
///   Test (BabbageUtxoPredFailure era)
/// validateOutputTooSmallUTxO pp outputs =
///   failureUnless (null outputsTooSmall) $
///     BabbageOutputTooSmallUTxO (map (\(out, Coin minSize) -> ...) outputsTooSmall)
/// ```
///
/// # Key Differences from Alonzo:
/// 1. Checks ALL outputs including collateral return
/// 2. Uses coinsPerUTxOByte (more accurate)
/// 3. Error includes the minimum value (helps wallets fix the issue)
pub fn validate_output_too_small(
    pp: &BabbagePParams,
    outputs: &[SizedTxOut],
) -> Result<(), BabbageUtxoPredFailure> {
    let too_small: Vec<(BabbageTxOut, Coin)> = outputs
        .iter()
        .filter_map(|sized| {
            let min_coin = get_min_coin_sized_tx_out(pp, sized);
            if sized.output.value.coin < min_coin {
                Some((sized.output.clone(), min_coin))
            } else {
                None
            }
        })
        .collect();

    if too_small.is_empty() {
        Ok(())
    } else {
        Err(BabbageUtxoPredFailure::BabbageOutputTooSmallUTxO(too_small))
    }
}

/// Comprehensive Babbage fee and collateral validation
///
/// Reference: eras/babbage/impl/src/Cardano/Ledger/Babbage/Rules/Utxo.hs:136-219
pub fn fees_ok(
    pp: &BabbagePParams,
    tx: &BabbageTx,
    utxo: &BabbageUTxO,
) -> Result<(), Vec<BabbageUtxoPredFailure>> {
    let mut errors = Vec::new();

    // Part 1: Check fee is sufficient
    let min_fee = calculate_min_fee(pp, tx.size());
    if tx.body.fee < min_fee {
        errors.push(BabbageUtxoPredFailure::AlonzoInBabbage(
            AlonzoUtxoPredFailure::FeeTooSmallUTxO {
                supplied_fee: tx.body.fee,
                minimum_fee: min_fee,
            },
        ));
    }

    // Part 2: If redeemers present, validate collateral
    if tx.wits.has_redeemers() {
        let collateral_utxo = utxo.restrict(&tx.body.collateral_inputs);

        // Validate collateral (Babbage version)
        if let Err(mut coll_errors) = validate_babbage_collateral(
            pp,
            &tx.body,
            &collateral_utxo,
        ) {
            errors.append(&mut coll_errors);
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn calculate_min_fee(pp: &BabbagePParams, tx_size: TxSize) -> Coin {
    (tx_size as u64 * pp.min_fee_a) + pp.min_fee_b
}

/// Babbage collateral validation
fn validate_babbage_collateral(
    pp: &BabbagePParams,
    tx_body: &BabbageTxBody,
    collateral_utxo: &HashMap<TxIn, BabbageTxOut>,
) -> Result<(), Vec<BabbageUtxoPredFailure>> {
    let mut errors = Vec::new();

    // Check collateral not empty
    if collateral_utxo.is_empty() {
        errors.push(BabbageUtxoPredFailure::AlonzoInBabbage(
            AlonzoUtxoPredFailure::NoCollateralInputs,
        ));
    }

    // Check all collateral is VKey-locked
    let script_locked: HashMap<TxIn, BabbageTxOut> = collateral_utxo
        .iter()
        .filter(|(_, out)| !out.is_vkey_locked())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    if !script_locked.is_empty() {
        // Convert to Alonzo format for error
        let alonzo_script_locked: HashMap<TxIn, AlonzoTxOut> = script_locked
            .into_iter()
            .map(|(k, v)| (k, convert_to_alonzo_out(&v)))
            .collect();
        errors.push(BabbageUtxoPredFailure::AlonzoInBabbage(
            AlonzoUtxoPredFailure::ScriptsNotPaidUTxO(alonzo_script_locked),
        ));
    }

    // Calculate collateral balance
    let collateral_coin: Coin = collateral_utxo.values().map(|out| out.value.coin).sum();
    let return_coin: Coin = tx_body.collateral_return.as_ref().map(|o| o.value.coin).unwrap_or(0);
    let net_collateral = (collateral_coin as i64) - (return_coin as i64);

    // Check sufficient collateral
    let required = (tx_body.fee * pp.collateral_percentage as u64 + 99) / 100;
    if net_collateral < required as i64 {
        errors.push(BabbageUtxoPredFailure::AlonzoInBabbage(
            AlonzoUtxoPredFailure::InsufficientCollateral {
                provided: net_collateral,
                required,
            },
        ));
    }

    // Check collateral contains only ADA (Babbage relaxed version)
    if let Err(e) = validate_collateral_contains_non_ada(
        collateral_utxo,
        tx_body.collateral_return.as_ref(),
    ) {
        errors.push(e);
    }

    // Check totalCollateral field matches (if specified)
    if let Err(e) = validate_collateral_eq_balance(net_collateral, tx_body.total_collateral) {
        errors.push(e);
    }

    // Check collateral input count
    if collateral_utxo.len() as u32 > pp.max_collateral_inputs {
        errors.push(BabbageUtxoPredFailure::AlonzoInBabbage(
            AlonzoUtxoPredFailure::TooManyCollateralInputs {
                supplied: collateral_utxo.len() as u32,
                maximum: pp.max_collateral_inputs,
            },
        ));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Convert Babbage output to Alonzo format (for error compatibility)
fn convert_to_alonzo_out(out: &BabbageTxOut) -> AlonzoTxOut {
    AlonzoTxOut {
        address: out.address.clone(),
        value: out.value.clone(),
        datum_hash: match &out.datum {
            BabbageDatum::Hash(h) => Some(*h),
            _ => None,
        },
    }
}

// ============================================================================
// Main Transition Rule
// ============================================================================

/// Babbage UTXO Transition Rule
///
/// Reference: eras/babbage/impl/src/Cardano/Ledger/Babbage/Rules/Utxo.hs:350-465
pub fn babbage_utxo_transition(
    env: &BabbageUtxoEnv,
    state: &BabbageUTxOState,
    tx: &BabbageTx,
) -> Result<BabbageUTxOState, Vec<BabbageUtxoPredFailure>> {
    let mut errors: Vec<BabbageUtxoPredFailure> = Vec::new();
    let tx_body = &tx.body;

    // Step 1: inputs ∩ refInputs = ∅
    if let Err(e) = validate_disjoint_ref_inputs(&tx_body.inputs, &tx_body.reference_inputs) {
        errors.push(e);
    }

    // Step 2: ininterval slot (txvld txb)
    if !tx_body.validity_interval.contains(env.slot) {
        errors.push(BabbageUtxoPredFailure::AlonzoInBabbage(
            AlonzoUtxoPredFailure::OutsideValidityIntervalUTxO {
                validity_interval: tx_body.validity_interval,
                current_slot: env.slot,
            },
        ));
    }

    // Step 3: epochInfoSlotToUTCTime ≠ ◇
    // Omitted for brevity

    // Step 4: txins txb ≠ ∅
    if tx_body.inputs.is_empty() {
        errors.push(BabbageUtxoPredFailure::AlonzoInBabbage(
            AlonzoUtxoPredFailure::InputSetEmptyUTxO,
        ));
    }

    // Step 5: feesOK (Babbage version)
    if let Err(mut fee_errors) = fees_ok(&env.pp, tx, &state.utxo) {
        errors.append(&mut fee_errors);
    }

    // Step 6: allInputs ⊆ dom utxo
    if let Err(e) = validate_bad_inputs(
        &state.utxo,
        &tx_body.inputs,
        &tx_body.collateral_inputs,
        &tx_body.reference_inputs,
    ) {
        errors.push(e);
    }

    // Step 7: consumed == produced
    // Omitted for brevity

    // Step 8: validateOutputTooSmallUTxO (ALL outputs including collateral return)
    let all_sized_outputs = tx_body.all_sized_outputs();
    if let Err(e) = validate_output_too_small(&env.pp, &all_sized_outputs) {
        errors.push(e);
    }

    // Step 9: validateOutputTooBigUTxO (ALL outputs)
    // Omitted for brevity

    // Steps 10-15: Network checks, size limits, ex units
    // Omitted for brevity

    if !errors.is_empty() {
        return Err(errors);
    }

    // Step 16: UTXOS sub-rule
    // Would call Phase 2 validation

    Ok(BabbageUTxOState {
        utxo: state.utxo.clone(), // Would be updated
        deposited: state.deposited,
        fees: state.fees + tx_body.fee,
    })
}

/// Babbage UTXO Environment
#[derive(Debug, Clone)]
pub struct BabbageUtxoEnv {
    pub slot: SlotNo,
    pub pp: BabbagePParams,
    pub cert_state: CertState,
    pub network_id: Network,
}

/// Babbage UTXO State
#[derive(Debug, Clone)]
pub struct BabbageUTxOState {
    pub utxo: BabbageUTxO,
    pub deposited: Coin,
    pub fees: Coin,
}

// ============================================================================
// Failed Transaction Handling (Phase 2 Script Failure - UTXOS rule)
// Reference: eras/babbage/impl/src/Cardano/Ledger/Babbage/Rules/Utxos.hs:248-303
//
// Babbage improves on Alonzo by introducing a collateral return output.
// When scripts fail, the collateral inputs are seized, but the return output
// is produced — returning excess ADA and all native tokens to the user.
//
// Conway reuses Babbage's babbageEvalScriptsTxInvalid.
// ============================================================================

/// Compute the net collateral ADA balance (inputs minus return).
///
/// If no collateral return: net = sum of all collateral input ADA
/// If collateral return present: net = sum of inputs - return coin value
///
/// Reference: Babbage/Collateral.hs:30-41 (collAdaBalance)
///
/// ```haskell
/// collAdaBalance txBody utxoCollateral = toDeltaCoin $
///   case txBody ^. collateralReturnTxBodyL of
///     SNothing -> colbal
///     SJust txOut -> colbal <-> (txOut ^. coinTxOutL)
///   where
///     colbal = sumAllCoin utxoCollateral
/// ```
pub fn coll_ada_balance(
    collateral_utxo: &HashMap<TxIn, BabbageTxOut>,
    collateral_return: Option<&BabbageTxOut>,
) -> i64 {
    let col_bal: Coin = collateral_utxo.values().map(|out| out.value.coin).sum();
    match collateral_return {
        None => col_bal as i64,
        Some(ret) => (col_bal as i64) - (ret.value.coin as i64),
    }
}

/// Compute the collateral return output's TxIn (if it exists).
///
/// The return output gets index = len(regular outputs), i.e. one past the
/// last regular output index.
///
/// Reference: Babbage/Collateral.hs:52-60 (mkCollateralTxIn)
///
/// ```haskell
/// mkCollateralTxIn txBody = TxIn (txIdTxBody txBody) txIx
///   where txIx = txIxFromIntegral (length (txBody ^. outputsTxBodyL))
/// ```
pub fn mk_collateral_txin(tx_id: [u8; 32], num_regular_outputs: usize) -> TxIn {
    TxIn {
        tx_id,
        output_index: num_regular_outputs as u32,
    }
}

/// Compute the collateral return UTxO entries (if collateral return is present).
///
/// Reference: Babbage/Collateral.hs:43-50 (collOuts)
///
/// ```haskell
/// collOuts txBody =
///   case txBody ^. collateralReturnTxBodyL of
///     SNothing -> UTxO Map.empty
///     SJust txOut -> UTxO (Map.singleton (mkCollateralTxIn txBody) txOut)
/// ```
pub fn coll_outs(
    tx_id: [u8; 32],
    num_regular_outputs: usize,
    collateral_return: Option<&BabbageTxOut>,
) -> HashMap<TxIn, BabbageTxOut> {
    match collateral_return {
        None => HashMap::new(),
        Some(ret) => {
            let txin = mk_collateral_txin(tx_id, num_regular_outputs);
            let mut m = HashMap::new();
            m.insert(txin, ret.clone());
            m
        }
    }
}

/// Babbage/Conway: process a failed (phase-2 invalid) transaction.
///
/// When a transaction's Plutus scripts fail, the ledger:
/// 1. Removes all collateral inputs from the UTxO
/// 2. Adds the collateral return output to the UTxO (if present)
/// 3. Adds the net collateral (inputs - return) to the fee pot
///
/// ```text
/// utxoKeep = collateralInputs ⋪ utxo
/// utxoDel  = collateralInputs ◁ utxo
/// collouts = collOuts txBody              -- collateral return output (if any)
/// new_utxo = utxoKeep ∪ collouts
/// new_fees = fees + collAdaBalance(txBody, utxoDel)
/// ```
///
/// Reference: Babbage/Rules/Utxos.hs:265-303 (babbageEvalScriptsTxInvalid)
///
/// ```haskell
/// babbageEvalScriptsTxInvalid = do
///   TRC (UtxoEnv _ pp _, utxos@(UTxOState utxo _ fees _ _ _), tx) <- judgmentContext
///   let txBody = tx ^. bodyTxL
///   let !(utxoKeep, utxoDel) = extractKeys (unUTxO utxo) (txBody ^. collateralInputsTxBodyL)
///       UTxO collouts = collOuts txBody
///       DeltaCoin collateralFees = collAdaBalance txBody utxoDel
///   pure $!
///     utxos
///       { utxosUtxo = UTxO (Map.union utxoKeep collouts)
///       , utxosFees = fees <> Coin collateralFees
///       }
/// ```
///
/// | What       | How                                                 |
/// |------------|-----------------------------------------------------|
/// | Consumed   | All collateral inputs removed from UTxO              |
/// | Produced   | Collateral return output added to UTxO (if present)  |
/// | Fees       | fees + collAdaBalance (net collateral only)           |
pub fn babbage_apply_failed_tx(
    state: &BabbageUTxOState,
    tx_body: &BabbageTxBody,
    tx_id: [u8; 32],
) -> BabbageUTxOState {
    // Remove collateral inputs from UTxO
    let mut utxo_keep = state.utxo.utxo.clone();
    let mut utxo_del: HashMap<TxIn, BabbageTxOut> = HashMap::new();
    for txin in &tx_body.collateral_inputs {
        if let Some(txout) = utxo_keep.remove(txin) {
            utxo_del.insert(txin.clone(), txout);
        }
    }

    // Compute net collateral fees (inputs minus return)
    let collateral_fees = coll_ada_balance(&utxo_del, tx_body.collateral_return.as_ref());

    // Compute collateral return outputs
    let collouts = coll_outs(
        tx_id,
        tx_body.outputs.len(),
        tx_body.collateral_return.as_ref(),
    );

    // Add collateral return to UTxO
    utxo_keep.extend(collouts);

    BabbageUTxOState {
        utxo: BabbageUTxO { utxo: utxo_keep },
        deposited: state.deposited,
        fees: state.fees + (collateral_fees.max(0) as Coin),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn default_pp() -> BabbagePParams {
        BabbagePParams {
            min_fee_a: 44,
            min_fee_b: 155381,
            max_tx_size: 16384,
            key_deposit: 2_000_000,
            pool_deposit: 500_000_000,
            collateral_percentage: 150,
            max_collateral_inputs: 3,
            max_tx_ex_units: ExUnits {
                mem: 14_000_000,
                steps: 10_000_000_000,
            },
            max_val_size: 5000,
            coins_per_utxo_byte: 4310, // Babbage value
            protocol_version: (7, 0),
        }
    }

    #[test]
    fn test_disjoint_ref_inputs() {
        let inputs: HashSet<TxIn> = [
            TxIn { tx_id: [1u8; 32], output_index: 0 },
            TxIn { tx_id: [2u8; 32], output_index: 0 },
        ].into_iter().collect();

        let disjoint_refs: HashSet<TxIn> = [
            TxIn { tx_id: [3u8; 32], output_index: 0 },
        ].into_iter().collect();

        let overlapping_refs: HashSet<TxIn> = [
            TxIn { tx_id: [1u8; 32], output_index: 0 }, // Same as input!
        ].into_iter().collect();

        assert!(validate_disjoint_ref_inputs(&inputs, &disjoint_refs).is_ok());
        assert!(matches!(
            validate_disjoint_ref_inputs(&inputs, &overlapping_refs),
            Err(BabbageUtxoPredFailure::BabbageNonDisjointRefInputs(_))
        ));
    }

    #[test]
    fn test_collateral_eq_balance() {
        // Not specified - should pass
        assert!(validate_collateral_eq_balance(2_000_000, None).is_ok());

        // Correctly specified
        assert!(validate_collateral_eq_balance(2_000_000, Some(2_000_000)).is_ok());

        // Incorrectly specified
        assert!(matches!(
            validate_collateral_eq_balance(2_000_000, Some(3_000_000)),
            Err(BabbageUtxoPredFailure::IncorrectTotalCollateralField { .. })
        ));
    }

    #[test]
    fn test_min_utxo_byte_calculation() {
        let pp = default_pp();

        // 100 byte output with coinsPerUTxOByte = 4310
        // minCoin = 100 * 4310 = 431,000 lovelace
        let sized_out = SizedTxOut {
            output: BabbageTxOut {
                address: Addr::Enterprise {
                    network: Network::Mainnet,
                    payment_credential: Credential::KeyHash([0u8; 28]),
                },
                value: Value::default(),
                datum: BabbageDatum::None,
                reference_script: None,
            },
            size: 100,
        };

        let min_coin = get_min_coin_sized_tx_out(&pp, &sized_out);
        assert_eq!(min_coin, 431_000);
    }
}
