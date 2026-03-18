// Alonzo Era UTXO Validation
//
// This module implements the Alonzo UTXO rule, which introduces Plutus smart contracts
// and adds significant new validations for collateral and execution units.
//
// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Rules/Utxo.hs
//
// ============================================================================
// ALONZO UTXO RULE SUMMARY
// ============================================================================
//
// Key additions from Shelley/Allegra:
// - Collateral validation (for Plutus script transactions)
// - Execution units limits
// - Outside forecast check (slot to time translation)
// - Network ID in tx body
// - Size-based minUTxO calculation
// - UTXOS sub-rule for Phase 2 script execution
//
// ============================================================================

use std::collections::{HashMap, HashSet};

// Import base types from shelley-utxo
// In a real implementation, these would be shared modules
pub use super::shelley_utxo::{
    Addr, Certificate, CertState, Coin, Credential, Network, NativeScript,
    PoolParams, RewardAccount, SlotNo, TxIn, TxSize, UTxO, Update,
};

// ============================================================================
// Alonzo-Specific Type Definitions
// ============================================================================

/// Validity interval for transactions (introduced in Allegra, used in Alonzo)
#[derive(Debug, Clone, Copy)]
pub struct ValidityInterval {
    /// Transaction invalid before this slot (optional)
    pub invalid_before: Option<SlotNo>,
    /// Transaction invalid at or after this slot (optional)
    pub invalid_hereafter: Option<SlotNo>,
}

impl ValidityInterval {
    /// Check if a slot is within the validity interval
    pub fn contains(&self, slot: SlotNo) -> bool {
        let after_start = match self.invalid_before {
            Some(start) => slot >= start,
            None => true,
        };
        let before_end = match self.invalid_hereafter {
            Some(end) => slot < end,
            None => true,
        };
        after_start && before_end
    }
}

/// Execution units for Plutus script execution
#[derive(Debug, Clone, Copy, Default)]
pub struct ExUnits {
    /// Memory units consumed
    pub mem: u64,
    /// CPU steps consumed
    pub steps: u64,
}

impl ExUnits {
    /// Check if self is pointwise <= other
    pub fn fits_within(&self, other: &ExUnits) -> bool {
        self.mem <= other.mem && self.steps <= other.steps
    }

    /// Add two ExUnits
    pub fn add(&self, other: &ExUnits) -> ExUnits {
        ExUnits {
            mem: self.mem + other.mem,
            steps: self.steps + other.steps,
        }
    }
}

/// Multi-asset value (introduced in Mary, essential for Alonzo)
#[derive(Debug, Clone, Default)]
pub struct Value {
    /// ADA amount in lovelace
    pub coin: Coin,
    /// Multi-asset map: PolicyId -> AssetName -> Amount
    pub multi_asset: HashMap<[u8; 28], HashMap<Vec<u8>, u64>>,
}

impl Value {
    /// Check if value contains only ADA (no native tokens)
    pub fn is_ada_only(&self) -> bool {
        self.multi_asset.is_empty()
            || self.multi_asset.values().all(|assets| assets.is_empty())
    }

    /// Get the total ADA coin value
    pub fn coin(&self) -> Coin {
        self.coin
    }
}

/// Plutus script purpose (what action the script authorizes)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ScriptPurpose {
    Spending(TxIn),
    Minting([u8; 28]),  // PolicyId
    Rewarding(RewardAccount),
    Certifying(usize),  // Certificate index
}

/// Redeemer data with execution unit budget
#[derive(Debug, Clone)]
pub struct Redeemer {
    pub purpose: ScriptPurpose,
    pub data: Vec<u8>,
    pub ex_units: ExUnits,
}

/// Datum types for Alonzo outputs
#[derive(Debug, Clone)]
pub enum Datum {
    /// No datum
    None,
    /// Datum hash only (Alonzo style)
    Hash([u8; 32]),
}

/// Alonzo transaction output
#[derive(Debug, Clone)]
pub struct AlonzoTxOut {
    pub address: Addr,
    pub value: Value,
    pub datum_hash: Option<[u8; 32]>,
}

impl AlonzoTxOut {
    /// Check if output is VKey-locked (not script-locked)
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

    /// Calculate serialized size (simplified)
    pub fn serialized_size(&self) -> usize {
        // Simplified calculation
        // Real implementation would serialize and measure
        100 // Placeholder
    }

    /// Calculate value serialized size
    pub fn value_size(&self) -> usize {
        // Base size for coin
        let mut size = 8;
        // Add size for each asset
        for (_, assets) in &self.value.multi_asset {
            size += 28; // Policy ID
            for (name, _) in assets {
                size += name.len() + 8;
            }
        }
        size
    }
}

/// Alonzo Protocol Parameters
#[derive(Debug, Clone)]
pub struct AlonzoPParams {
    // Inherited from Shelley
    pub min_fee_a: u64,
    pub min_fee_b: u64,
    pub max_tx_size: u32,
    pub key_deposit: Coin,
    pub pool_deposit: Coin,

    // Alonzo additions
    /// Coins per UTxO word (for min UTxO calculation)
    pub coins_per_utxo_word: Coin,

    /// Maximum value size in bytes
    pub max_val_size: u32,

    /// Collateral percentage (e.g., 150 = 150%)
    pub collateral_percentage: u32,

    /// Maximum number of collateral inputs
    pub max_collateral_inputs: u32,

    /// Maximum execution units per transaction
    pub max_tx_ex_units: ExUnits,
}

/// Alonzo Transaction Body
#[derive(Debug, Clone)]
pub struct AlonzoTxBody {
    /// Transaction inputs
    pub inputs: HashSet<TxIn>,

    /// Collateral inputs (for Plutus script transactions)
    pub collateral_inputs: HashSet<TxIn>,

    /// Transaction outputs
    pub outputs: Vec<AlonzoTxOut>,

    /// Transaction fee
    pub fee: Coin,

    /// Validity interval (replaces TTL)
    pub validity_interval: ValidityInterval,

    /// Withdrawals from reward accounts
    pub withdrawals: HashMap<RewardAccount, Coin>,

    /// Certificates
    pub certificates: Vec<Certificate>,

    /// Minting/burning of native tokens
    pub mint: HashMap<[u8; 28], HashMap<Vec<u8>, i64>>,

    /// Script data hash (hash of redeemers + datums + language views)
    pub script_data_hash: Option<[u8; 32]>,

    /// Explicit network ID (optional, new in Alonzo)
    pub network_id: Option<Network>,

    /// Update proposal (legacy)
    pub update: Option<Update>,
}

/// Alonzo Transaction Witnesses
#[derive(Debug, Clone)]
pub struct AlonzoTxWits {
    /// VKey signatures
    pub vkey_witnesses: Vec<VKeyWitness>,
    /// Native scripts
    pub native_scripts: HashMap<[u8; 28], NativeScript>,
    /// Bootstrap witnesses
    pub bootstrap_witnesses: Vec<BootstrapWitness>,
    /// Plutus scripts
    pub plutus_scripts: HashMap<[u8; 28], PlutusScript>,
    /// Datums
    pub datums: HashMap<[u8; 32], Vec<u8>>,
    /// Redeemers
    pub redeemers: Vec<Redeemer>,
}

impl AlonzoTxWits {
    /// Check if transaction has redeemers (indicates Plutus scripts)
    pub fn has_redeemers(&self) -> bool {
        !self.redeemers.is_empty()
    }

    /// Get total execution units from all redeemers
    pub fn total_ex_units(&self) -> ExUnits {
        self.redeemers
            .iter()
            .fold(ExUnits::default(), |acc, r| acc.add(&r.ex_units))
    }
}

/// VKey witness
#[derive(Debug, Clone)]
pub struct VKeyWitness {
    pub vkey: [u8; 32],
    pub signature: [u8; 64],
}

/// Bootstrap witness
#[derive(Debug, Clone)]
pub struct BootstrapWitness {
    pub vkey: [u8; 32],
    pub signature: [u8; 64],
    pub chain_code: [u8; 32],
    pub attributes: Vec<u8>,
}

/// Plutus script
#[derive(Debug, Clone)]
pub struct PlutusScript {
    pub version: PlutusVersion,
    pub bytes: Vec<u8>,
}

/// Plutus version
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlutusVersion {
    V1,
    V2, // Actually introduced in Babbage
}

/// Alonzo Transaction
#[derive(Debug, Clone)]
pub struct AlonzoTx {
    pub body: AlonzoTxBody,
    pub wits: AlonzoTxWits,
    pub is_valid: bool,  // IsValid flag for Phase 2
    pub auxiliary_data: Option<Vec<u8>>,
}

impl AlonzoTx {
    pub fn size(&self) -> TxSize {
        // Simplified - real implementation would serialize
        0
    }
}

/// Alonzo UTxO (with Value support)
pub struct AlonzoUTxO {
    pub utxo: HashMap<TxIn, AlonzoTxOut>,
}

impl AlonzoUTxO {
    /// Get output for a given input
    pub fn get(&self, input: &TxIn) -> Option<&AlonzoTxOut> {
        self.utxo.get(input)
    }

    /// Check if input exists
    pub fn contains(&self, input: &TxIn) -> bool {
        self.utxo.contains_key(input)
    }

    /// Get multiple outputs for a set of inputs
    pub fn restrict(&self, inputs: &HashSet<TxIn>) -> HashMap<TxIn, AlonzoTxOut> {
        inputs
            .iter()
            .filter_map(|input| self.utxo.get(input).map(|out| (input.clone(), out.clone())))
            .collect()
    }

    /// Sum all coin values for a set of inputs
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
                // Would also merge multi_asset in real implementation
            }
        }
        total
    }
}

/// Alonzo UTXO Environment
#[derive(Debug, Clone)]
pub struct AlonzoUtxoEnv {
    pub slot: SlotNo,
    pub pp: AlonzoPParams,
    pub cert_state: CertState,
    pub network_id: Network,
    /// Epoch info for slot-to-time translation
    pub epoch_info: EpochInfo,
    pub system_start: SystemStart,
}

/// Epoch information (for slot-to-time translation)
#[derive(Debug, Clone)]
pub struct EpochInfo {
    /// Forecast range in slots
    pub forecast_range: SlotNo,
}

impl EpochInfo {
    /// Try to translate slot to POSIX time
    /// Returns None if slot is outside forecast range
    pub fn slot_to_posix_time(&self, current_slot: SlotNo, target_slot: SlotNo) -> Option<u64> {
        if target_slot <= current_slot + self.forecast_range {
            // Simplified - real implementation uses epoch boundaries
            Some(target_slot * 1000) // Placeholder
        } else {
            None
        }
    }
}

/// System start time
#[derive(Debug, Clone)]
pub struct SystemStart {
    pub posix_time: u64,
}

/// Alonzo UTXO State
#[derive(Debug, Clone)]
pub struct AlonzoUTxOState {
    pub utxo: AlonzoUTxO,
    pub deposited: Coin,
    pub fees: Coin,
}

// ============================================================================
// Error Types
// ============================================================================

/// Alonzo UTXO predicate failures
///
/// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Rules/Utxo.hs:91-130
///
/// ```haskell
/// data AlonzoUtxoPredFailure era
///   = BadInputsUTxO (Set TxIn)
///   | OutsideValidityIntervalUTxO ValidityInterval SlotNo
///   | MaxTxSizeUTxO (Mismatch RelLTEQ Word32)
///   | InputSetEmptyUTxO
///   | FeeTooSmallUTxO (Mismatch RelGTEQ Coin)
///   | ValueNotConservedUTxO (Mismatch RelEQ (Value era))
///   | WrongNetwork Network (Set Addr)
///   | WrongNetworkWithdrawal Network (Set RewardAccount)
///   | OutputTooSmallUTxO [TxOut era]
///   | OutputBootAddrAttrsTooBig [TxOut era]
///   | UtxosFailure (PredicateFailure (EraRule "UTXOS" era))
///   | OutputTooBigUTxO [(Int, Int, TxOut era)]
///   | InsufficientCollateral DeltaCoin Coin
///   | ScriptsNotPaidUTxO (UTxO era)
///   | ExUnitsTooBigUTxO (Mismatch RelLTEQ ExUnits)
///   | CollateralContainsNonADA (Value era)
///   | WrongNetworkInTxBody (Mismatch RelEQ Network)
///   | OutsideForecast SlotNo
///   | TooManyCollateralInputs (Mismatch RelLTEQ Natural)
///   | NoCollateralInputs
/// ```
#[derive(Debug, Clone)]
pub enum AlonzoUtxoPredFailure {
    // ========== Inherited from Shelley/Allegra ==========

    /// Inputs don't exist in UTxO
    BadInputsUTxO(HashSet<TxIn>),

    /// Transaction outside validity interval (from Allegra)
    OutsideValidityIntervalUTxO {
        validity_interval: ValidityInterval,
        current_slot: SlotNo,
    },

    /// Transaction too large
    MaxTxSizeUTxO {
        actual_size: TxSize,
        max_size: TxSize,
    },

    /// No inputs provided
    InputSetEmptyUTxO,

    /// Fee too small
    FeeTooSmallUTxO {
        supplied_fee: Coin,
        minimum_fee: Coin,
    },

    /// Value not conserved
    ValueNotConservedUTxO {
        consumed: Value,
        produced: Value,
    },

    /// Wrong network in output addresses
    WrongNetwork {
        expected: Network,
        wrong_addresses: HashSet<Addr>,
    },

    /// Wrong network in withdrawal accounts
    WrongNetworkWithdrawal {
        expected: Network,
        wrong_accounts: HashSet<RewardAccount>,
    },

    /// Outputs below minimum UTxO value
    OutputTooSmallUTxO(Vec<AlonzoTxOut>),

    /// Bootstrap address attributes too big
    OutputBootAddrAttrsTooBig(Vec<AlonzoTxOut>),

    // ========== New in Alonzo ==========

    /// Phase 2 (script execution) failure
    UtxosFailure(Box<AlonzoUtxosPredFailure>),

    /// Output value serialization too big
    /// (actual_size, max_size, output)
    OutputTooBigUTxO(Vec<(usize, u32, AlonzoTxOut)>),

    /// Collateral insufficient for fee
    InsufficientCollateral {
        provided: i64,    // DeltaCoin (can be negative)
        required: Coin,
    },

    /// Script-locked UTxOs used as collateral
    ScriptsNotPaidUTxO(HashMap<TxIn, AlonzoTxOut>),

    /// Execution units exceed maximum
    ExUnitsTooBigUTxO {
        supplied: ExUnits,
        maximum: ExUnits,
    },

    /// Collateral contains native tokens (not just ADA)
    CollateralContainsNonADA(Value),

    /// Wrong network ID in transaction body
    WrongNetworkInTxBody {
        supplied: Network,
        expected: Network,
    },

    /// Validity interval end (invalidHereafter) is outside consensus forecast range
    ///
    /// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Rules/Utxo.hs:168-169
    ///
    /// This error occurs when:
    /// 1. The transaction contains Plutus scripts (has redeemers)
    /// 2. The `invalidHereafter` slot cannot be translated to UTC/POSIX time
    ///
    /// The consensus layer can only forecast slot-to-time mappings for a limited
    /// window into the future (approximately 3k/f slots, ~36 hours on mainnet).
    ///
    /// Plutus scripts receive time as POSIX time (not slots) because slot length
    /// could change between eras. If we can't translate the slot, we can't provide
    /// correct time context to scripts.
    ///
    /// The SlotNo contained is the `invalidHereafter` value that couldn't be translated.
    OutsideForecast(SlotNo),

    /// Too many collateral inputs
    TooManyCollateralInputs {
        supplied: u32,
        maximum: u32,
    },

    /// No collateral inputs provided (but redeemers present)
    NoCollateralInputs,
}

/// UTXOS (Phase 2) predicate failure placeholder
#[derive(Debug, Clone)]
pub enum AlonzoUtxosPredFailure {
    // Phase 2 script failures would go here
    ValidationTagMismatch(bool),
    CollectErrors(Vec<String>),
}

// ============================================================================
// Validation Functions
// ============================================================================

/// Validate validity interval (Allegra-style, used in Alonzo)
///
/// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/Rules/Utxo.hs:242-249
///
/// ```haskell
/// validateOutsideValidityIntervalUTxO ::
///   AllegraEraTxBody era =>
///   SlotNo ->
///   TxBody l era ->
///   Test (AllegraUtxoPredFailure era)
/// validateOutsideValidityIntervalUTxO slot txb =
///   failureUnless (inInterval slot (txb ^. vldtTxBodyL)) $
///     OutsideValidityIntervalUTxO (txb ^. vldtTxBodyL) slot
/// ```
///
/// Formal specification: ininterval slot (txvld txb)
pub fn validate_outside_validity_interval(
    slot: SlotNo,
    validity_interval: &ValidityInterval,
) -> Result<(), AlonzoUtxoPredFailure> {
    if validity_interval.contains(slot) {
        Ok(())
    } else {
        Err(AlonzoUtxoPredFailure::OutsideValidityIntervalUTxO {
            validity_interval: *validity_interval,
            current_slot: slot,
        })
    }
}

/// Validate outside forecast (validity interval end must be translatable to time)
///
/// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Rules/Utxo.hs:352-385
///
/// ```haskell
/// validateOutsideForecast ::
///   ( MaryEraTxBody era
///   , AlonzoEraTxWits era
///   , EraTx era
///   ) =>
///   EpochInfo (Either a) ->
///   SlotNo ->
///   SystemStart ->
///   Tx l era ->
///   Test (AlonzoUtxoPredFailure era)
/// validateOutsideForecast ei slotNo sysSt tx =
///   case tx ^. bodyTxL . vldtTxBodyL of
///     ValidityInterval _ (SJust ifj)
///       | not . null $ tx ^. witsTxL . rdmrsTxWitsL . unRedeemersL ->
///           let ei' = unsafeLinearExtendEpochInfo slotNo ei
///            in failureUnless (isRight (epochInfoSlotToUTCTime ei' sysSt ifj)) $
///                 OutsideForecast ifj
///     _ -> pure ()
/// ```
///
/// Formal specification (from alonzo/formal-spec/utxo.tex):
/// ```
/// (_,i_f) := txvldt tx
/// ◇ ∉ { txrdmrs tx, i_f } ⇒ epochInfoSlotToUTCTime epochInfo systemTime i_f ≠ ◇
/// ```
///
/// # When This Error Occurs:
///
/// 1. The transaction contains Plutus scripts (has redeemers)
/// 2. The transaction has an `invalidHereafter` upper validity bound
/// 3. That slot CANNOT be translated to UTC/POSIX time
///
/// # What is the Consensus Forecast Range?
///
/// The **consensus forecast range** (or "forecast horizon") is the maximum number
/// of slots into the future for which the consensus layer can reliably predict
/// the exact UTC time. This limitation exists because:
///
/// 1. **Slot length may change**: Different Cardano eras can have different slot
///    lengths. While currently stable at 1 second, this could change in future
///    hard forks.
///
/// 2. **Hard forks affect time mapping**: Protocol upgrades can change timing
///    parameters, making it impossible to predict exact times beyond the known
///    epoch structure.
///
/// 3. **Epoch boundaries**: The consensus layer can only forecast times for
///    epochs it knows about. Beyond the forecast horizon, epoch parameters
///    (and thus slot-to-time mapping) become uncertain.
///
/// ## Mainnet Forecast Window:
///
/// The forecast window is approximately **3k/f slots** beyond the current slot:
/// - k = security parameter (2160 on mainnet)
/// - f = active slot coefficient (0.05 on mainnet)
/// - 3 × 2160 / 0.05 = **129,600 slots** (~36 hours)
///
/// This means for a transaction with Plutus scripts, the `invalidHereafter`
/// must be within ~36 hours of the current slot.
///
/// ## Why Plutus Scripts Need Time Translation:
///
/// From the formal spec (alonzo/formal-spec/utxo.tex):
///
/// > "Plutus scripts receive system time instead of slot numbers. This is
/// > because the length of a slot may change in a future era. If scripts
/// > received slot numbers, the script logic (assuming old slot length)
/// > would mismatch with transaction validity intervals (using current
/// > slot length)."
///
/// The conversion function `epochInfoSlotToUTCTime` can translate:
/// - All slots **prior** to the current slot
/// - Slots up to the **forecast window** after the current slot
///
/// ## Example:
///
/// ```
/// Current slot: 100,000,000
/// Forecast window: ~129,600 slots
/// Max forecastable slot: ~100,129,600
///
/// Transaction with invalidHereafter = 100,100,000  → OK (within window)
/// Transaction with invalidHereafter = 101,000,000  → OutsideForecast (too far)
/// ```
///
/// ## Extension with unsafeLinearExtendEpochInfo:
///
/// The validation uses `unsafeLinearExtendEpochInfo` which extends the epoch
/// info linearly from the current slot. This allows slightly longer forecasts
/// by assuming epoch parameters remain constant, but it's still bounded.
///
/// # Why This Matters:
///
/// - Plutus scripts receive time context as POSIXTime, not slots
/// - If we can't translate the slot to time, we can't provide valid context
/// - Scripts would receive incorrect or missing time information
/// - This ensures scripts always get accurate time data
pub fn validate_outside_forecast(
    epoch_info: &EpochInfo,
    current_slot: SlotNo,
    invalid_hereafter: Option<SlotNo>,
    has_redeemers: bool,
) -> Result<(), AlonzoUtxoPredFailure> {
    // Only applies if transaction has Plutus scripts (redeemers)
    // Native scripts don't need time translation - they work with slots directly
    if !has_redeemers {
        return Ok(());
    }

    // Only check if there's an upper validity bound (invalidHereafter)
    // If invalidHereafter is None (unbounded), we can't check
    // (but this is unusual for transactions with scripts)
    if let Some(end_slot) = invalid_hereafter {
        // Try to translate the end slot to POSIX time
        // Uses the extended epoch info from current slot
        if epoch_info.slot_to_posix_time(current_slot, end_slot).is_none() {
            // The slot is beyond the consensus forecast horizon
            return Err(AlonzoUtxoPredFailure::OutsideForecast(end_slot));
        }
    }

    Ok(())
}

/// Validate input set not empty (reused from Shelley)
pub fn validate_input_set_not_empty(inputs: &HashSet<TxIn>) -> Result<(), AlonzoUtxoPredFailure> {
    if inputs.is_empty() {
        Err(AlonzoUtxoPredFailure::InputSetEmptyUTxO)
    } else {
        Ok(())
    }
}

/// Comprehensive fee and collateral validation
///
/// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Rules/Utxo.hs:263-287
///
/// ```haskell
/// feesOK ::
///   forall era.
///   ( AlonzoEraTx era
///   , EraUTxO era
///   ) =>
///   PParams era ->
///   Tx TopTx era ->
///   UTxO era ->
///   Test (AlonzoUtxoPredFailure era)
/// feesOK pp tx u@(UTxO utxo) =
///   let txBody = tx ^. bodyTxL
///       collateral = txBody ^. collateralInputsTxBodyL
///       utxoCollateral = Map.restrictKeys utxo collateral
///       theFee = txBody ^. feeTxBodyL
///       minFee = getMinFeeTxUtxo pp tx u
///    in sequenceA_
///         [ failureUnless (minFee <= theFee) (FeeTooSmallUTxO ...)
///         , unless (null $ tx ^. witsTxL . rdmrsTxWitsL) $
///             validateCollateral pp txBody utxoCollateral
///         ]
/// ```
pub fn fees_ok(
    pp: &AlonzoPParams,
    tx: &AlonzoTx,
    utxo: &AlonzoUTxO,
) -> Result<(), Vec<AlonzoUtxoPredFailure>> {
    let mut errors = Vec::new();

    // Calculate minimum fee
    let tx_size = tx.size();
    let min_fee = calculate_min_fee(pp, tx_size);
    let declared_fee = tx.body.fee;

    // Part 1: Check fee is sufficient
    if declared_fee < min_fee {
        errors.push(AlonzoUtxoPredFailure::FeeTooSmallUTxO {
            supplied_fee: declared_fee,
            minimum_fee: min_fee,
        });
    }

    // Part 2: If redeemers present, validate collateral
    if tx.wits.has_redeemers() {
        let collateral_utxo = utxo.restrict(&tx.body.collateral_inputs);
        if let Err(mut coll_errors) = validate_collateral(pp, &tx.body, &collateral_utxo) {
            errors.append(&mut coll_errors);
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Calculate minimum fee for transaction
fn calculate_min_fee(pp: &AlonzoPParams, tx_size: TxSize) -> Coin {
    (tx_size as u64 * pp.min_fee_a) + pp.min_fee_b
}

/// Validate collateral
///
/// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Rules/Utxo.hs:289-309
///
/// ```haskell
/// validateCollateral ::
///   ( EraTxBody era
///   , AlonzoEraPParams era
///   ) =>
///   PParams era ->
///   TxBody TopTx era ->
///   Map.Map TxIn (TxOut era) ->
///   Test (AlonzoUtxoPredFailure era)
/// validateCollateral pp txb utxoCollateral =
///   sequenceA_
///     [ validateScriptsNotPaidUTxO utxoCollateral
///     , validateInsufficientCollateral pp txb bal
///     , validateCollateralContainsNonADA utxoCollateral
///     , failureIf (null utxoCollateral) NoCollateralInputs
///     ]
/// ```
pub fn validate_collateral(
    pp: &AlonzoPParams,
    tx_body: &AlonzoTxBody,
    collateral_utxo: &HashMap<TxIn, AlonzoTxOut>,
) -> Result<(), Vec<AlonzoUtxoPredFailure>> {
    let mut errors = Vec::new();

    // Part 3: All collateral must be VKey-locked
    if let Err(e) = validate_scripts_not_paid(collateral_utxo) {
        errors.push(e);
    }

    // Calculate collateral balance
    let collateral_balance: Coin = collateral_utxo.values().map(|out| out.value.coin).sum();

    // Part 4: Collateral must be sufficient
    if let Err(e) = validate_insufficient_collateral(pp, tx_body.fee, collateral_balance as i64) {
        errors.push(e);
    }

    // Part 5: Collateral must be ADA-only
    if let Err(e) = validate_collateral_contains_non_ada(collateral_utxo) {
        errors.push(e);
    }

    // Part 6: Must have at least one collateral input
    if collateral_utxo.is_empty() {
        errors.push(AlonzoUtxoPredFailure::NoCollateralInputs);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Validate collateral is VKey-locked (not script-locked)
///
/// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Rules/Utxo.hs:311-318
///
/// ```haskell
/// validateScriptsNotPaidUTxO ::
///   EraTxOut era =>
///   Map.Map TxIn (TxOut era) ->
///   Test (AlonzoUtxoPredFailure era)
/// validateScriptsNotPaidUTxO utxoCollateral =
///   failureUnless (all vKeyLocked utxoCollateral) $
///     ScriptsNotPaidUTxO (UTxO (Map.filter (not . vKeyLocked) utxoCollateral))
/// ```
///
/// # Why This Matters:
/// - If Phase 2 fails, we collect collateral
/// - Script-locked UTxOs require script execution
/// - Can't run scripts to collect failed script's collateral!
pub fn validate_scripts_not_paid(
    collateral_utxo: &HashMap<TxIn, AlonzoTxOut>,
) -> Result<(), AlonzoUtxoPredFailure> {
    let script_locked: HashMap<TxIn, AlonzoTxOut> = collateral_utxo
        .iter()
        .filter(|(_, out)| !out.is_vkey_locked())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    if script_locked.is_empty() {
        Ok(())
    } else {
        Err(AlonzoUtxoPredFailure::ScriptsNotPaidUTxO(script_locked))
    }
}

/// Validate collateral is sufficient for fee
///
/// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Rules/Utxo.hs:320-336
///
/// ```haskell
/// validateInsufficientCollateral ::
///   ( EraTxBody era
///   , AlonzoEraPParams era
///   ) =>
///   PParams era ->
///   TxBody TopTx era ->
///   DeltaCoin ->
///   Test (AlonzoUtxoPredFailure era)
/// validateInsufficientCollateral pp txBody bal =
///   failureUnless (Val.scale (100 :: Int) bal >= Val.scale collPerc (toDeltaCoin txfee)) $
///     InsufficientCollateral bal $ ...
/// ```
///
/// Formal specification: balance * 100 ≥ txfee * collateralPercent pp
///
/// # Example:
/// ```text
/// fee = 1,000,000 (1 ADA)
/// collateralPercent = 150
/// required = fee * 150 / 100 = 1,500,000 (1.5 ADA)
/// ```
pub fn validate_insufficient_collateral(
    pp: &AlonzoPParams,
    tx_fee: Coin,
    collateral_balance: i64,
) -> Result<(), AlonzoUtxoPredFailure> {
    // balance * 100 >= fee * collateralPercent
    let lhs = collateral_balance * 100;
    let rhs = (tx_fee as i64) * (pp.collateral_percentage as i64);

    if lhs >= rhs {
        Ok(())
    } else {
        // Calculate required collateral for error message
        let required = (tx_fee * pp.collateral_percentage as u64 + 99) / 100;
        Err(AlonzoUtxoPredFailure::InsufficientCollateral {
            provided: collateral_balance,
            required,
        })
    }
}

/// Validate collateral contains only ADA
///
/// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Rules/Utxo.hs:338-346
///
/// ```haskell
/// validateCollateralContainsNonADA ::
///   (Foldable f, EraTxOut era) =>
///   f (TxOut era) ->
///   Test (AlonzoUtxoPredFailure era)
/// validateCollateralContainsNonADA collateralTxOuts =
///   failureUnless (areAllAdaOnly collateralTxOuts) $
///     CollateralContainsNonADA $ sumAllValue collateralTxOuts
/// ```
pub fn validate_collateral_contains_non_ada(
    collateral_utxo: &HashMap<TxIn, AlonzoTxOut>,
) -> Result<(), AlonzoUtxoPredFailure> {
    let all_ada_only = collateral_utxo.values().all(|out| out.value.is_ada_only());

    if all_ada_only {
        Ok(())
    } else {
        // Sum all values for error message
        let total_value = collateral_utxo.values().fold(Value::default(), |mut acc, out| {
            acc.coin += out.value.coin;
            // Would merge multi_asset in real implementation
            acc
        });
        Err(AlonzoUtxoPredFailure::CollateralContainsNonADA(total_value))
    }
}

/// Validate all inputs exist in UTxO (includes collateral in Alonzo)
pub fn validate_bad_inputs(
    utxo: &AlonzoUTxO,
    inputs: &HashSet<TxIn>,
    collateral_inputs: &HashSet<TxIn>,
) -> Result<(), AlonzoUtxoPredFailure> {
    // Alonzo checks both regular inputs AND collateral inputs
    let all_inputs: HashSet<TxIn> = inputs.union(collateral_inputs).cloned().collect();

    let bad_inputs: HashSet<TxIn> = all_inputs
        .iter()
        .filter(|input| !utxo.contains(input))
        .cloned()
        .collect();

    if bad_inputs.is_empty() {
        Ok(())
    } else {
        Err(AlonzoUtxoPredFailure::BadInputsUTxO(bad_inputs))
    }
}

/// Validate wrong network in transaction body
///
/// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Rules/Utxo.hs:421-432
///
/// ```haskell
/// validateWrongNetworkInTxBody ::
///   AlonzoEraTxBody era =>
///   Network ->
///   TxBody t era ->
///   Test (AlonzoUtxoPredFailure era)
/// validateWrongNetworkInTxBody netId txBody =
///   case txBody ^. networkIdTxBodyL of
///     SNothing -> pure ()
///     SJust n -> failureUnless (n == netId) $ WrongNetworkInTxBody ...
/// ```
pub fn validate_wrong_network_in_tx_body(
    expected_network: Network,
    tx_network_id: Option<Network>,
) -> Result<(), AlonzoUtxoPredFailure> {
    match tx_network_id {
        None => Ok(()), // Not specified, no check needed
        Some(n) if n == expected_network => Ok(()),
        Some(n) => Err(AlonzoUtxoPredFailure::WrongNetworkInTxBody {
            supplied: n,
            expected: expected_network,
        }),
    }
}

/// Validate execution units within limits
///
/// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Rules/Utxo.hs:439-456
///
/// ```haskell
/// validateExUnitsTooBigUTxO ::
///   ( AlonzoEraTxWits era
///   , EraTx era
///   , AlonzoEraPParams era
///   ) =>
///   PParams era ->
///   Tx t era ->
///   Test (AlonzoUtxoPredFailure era)
/// validateExUnitsTooBigUTxO pp tx =
///   failureUnless (pointWiseExUnits (<=) totalExUnits maxTxExUnits) $
///     ExUnitsTooBigUTxO Mismatch { ... }
/// ```
pub fn validate_ex_units_too_big(
    pp: &AlonzoPParams,
    total_ex_units: &ExUnits,
) -> Result<(), AlonzoUtxoPredFailure> {
    if total_ex_units.fits_within(&pp.max_tx_ex_units) {
        Ok(())
    } else {
        Err(AlonzoUtxoPredFailure::ExUnitsTooBigUTxO {
            supplied: *total_ex_units,
            maximum: pp.max_tx_ex_units,
        })
    }
}

/// Validate collateral input count
///
/// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Rules/Utxo.hs:458-467
pub fn validate_too_many_collateral_inputs(
    pp: &AlonzoPParams,
    collateral_count: u32,
) -> Result<(), AlonzoUtxoPredFailure> {
    if collateral_count <= pp.max_collateral_inputs {
        Ok(())
    } else {
        Err(AlonzoUtxoPredFailure::TooManyCollateralInputs {
            supplied: collateral_count,
            maximum: pp.max_collateral_inputs,
        })
    }
}

/// Validate output value size
///
/// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Rules/Utxo.hs:394-418
pub fn validate_output_too_big(
    pp: &AlonzoPParams,
    outputs: &[AlonzoTxOut],
) -> Result<(), AlonzoUtxoPredFailure> {
    let too_big: Vec<(usize, u32, AlonzoTxOut)> = outputs
        .iter()
        .filter_map(|out| {
            let size = out.value_size();
            if size > pp.max_val_size as usize {
                Some((size, pp.max_val_size, out.clone()))
            } else {
                None
            }
        })
        .collect();

    if too_big.is_empty() {
        Ok(())
    } else {
        Err(AlonzoUtxoPredFailure::OutputTooBigUTxO(too_big))
    }
}

/// Calculate minimum UTxO value (Alonzo size-based)
///
/// Alonzo formula: (utxoEntrySizeWithoutVal + valueSize/8) * coinsPerUTxOWord
pub fn get_min_coin_tx_out(pp: &AlonzoPParams, output: &AlonzoTxOut) -> Coin {
    // Simplified calculation
    // Real implementation is more complex
    let base_size: u64 = 27; // UTxO entry size without value
    let value_size = output.value_size() as u64;
    let words = base_size + (value_size + 7) / 8;
    words * pp.coins_per_utxo_word
}

/// Validate outputs meet minimum value (Alonzo version)
pub fn validate_output_too_small(
    pp: &AlonzoPParams,
    outputs: &[AlonzoTxOut],
) -> Result<(), AlonzoUtxoPredFailure> {
    let too_small: Vec<AlonzoTxOut> = outputs
        .iter()
        .filter(|out| {
            let min_coin = get_min_coin_tx_out(pp, out);
            out.value.coin < min_coin
        })
        .cloned()
        .collect();

    if too_small.is_empty() {
        Ok(())
    } else {
        Err(AlonzoUtxoPredFailure::OutputTooSmallUTxO(too_small))
    }
}

// ============================================================================
// Main Transition Rule
// ============================================================================

/// Alonzo UTXO Transition Rule
///
/// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Rules/Utxo.hs:472-556
pub fn alonzo_utxo_transition(
    env: &AlonzoUtxoEnv,
    state: &AlonzoUTxOState,
    tx: &AlonzoTx,
) -> Result<AlonzoUTxOState, Vec<AlonzoUtxoPredFailure>> {
    let mut errors: Vec<AlonzoUtxoPredFailure> = Vec::new();
    let tx_body = &tx.body;

    // Step 1: ininterval slot (txvld txb)
    if let Err(e) = validate_outside_validity_interval(env.slot, &tx_body.validity_interval) {
        errors.push(e);
    }

    // Step 2: epochInfoSlotToUTCTime ≠ ◇
    if let Err(e) = validate_outside_forecast(
        &env.epoch_info,
        env.slot,
        tx_body.validity_interval.invalid_hereafter,
        tx.wits.has_redeemers(),
    ) {
        errors.push(e);
    }

    // Step 3: txins txb ≠ ∅
    if let Err(e) = validate_input_set_not_empty(&tx_body.inputs) {
        errors.push(e);
    }

    // Step 4: feesOK pp tx utxo
    if let Err(mut fee_errors) = fees_ok(&env.pp, tx, &state.utxo) {
        errors.append(&mut fee_errors);
    }

    // Step 5: (txins ∪ collateral) ⊆ dom utxo
    if let Err(e) = validate_bad_inputs(&state.utxo, &tx_body.inputs, &tx_body.collateral_inputs) {
        errors.push(e);
    }

    // Step 6: consumed == produced (value conservation)
    // Omitted for brevity - same as Shelley but with Value type

    // Step 7: output too small
    if let Err(e) = validate_output_too_small(&env.pp, &tx_body.outputs) {
        errors.push(e);
    }

    // Step 8: output too big
    if let Err(e) = validate_output_too_big(&env.pp, &tx_body.outputs) {
        errors.push(e);
    }

    // Step 9-11: network checks (from Shelley)
    // Omitted for brevity

    // Step 12: wrong network in tx body
    if let Err(e) = validate_wrong_network_in_tx_body(env.network_id, tx_body.network_id) {
        errors.push(e);
    }

    // Step 13: max tx size
    // Omitted for brevity

    // Step 14: totExunits tx ≤ maxTxExUnits pp
    let total_ex_units = tx.wits.total_ex_units();
    if let Err(e) = validate_ex_units_too_big(&env.pp, &total_ex_units) {
        errors.push(e);
    }

    // Step 15: collateral input count
    if tx.wits.has_redeemers() {
        if let Err(e) = validate_too_many_collateral_inputs(
            &env.pp,
            tx_body.collateral_inputs.len() as u32,
        ) {
            errors.push(e);
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    // Step 16: UTXOS sub-rule (Phase 2)
    // Would call Phase 2 validation here

    // Update state
    Ok(AlonzoUTxOState {
        utxo: state.utxo.clone(), // Would be updated
        deposited: state.deposited,
        fees: state.fees + tx_body.fee,
    })
}

// ============================================================================
// Failed Transaction Handling (Phase 2 Script Failure - UTXOS rule)
// Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Rules/Utxos.hs:281-315
//
// When IsValid = False (phase-2 scripts fail):
// - Normal tx inputs are NOT consumed
// - Normal tx outputs are NOT produced
// - Certificates, withdrawals are NOT processed
// - Value conservation (consumed == produced) is NOT checked
// - ONLY collateral is affected
// ============================================================================

/// Alonzo: process a failed (phase-2 invalid) transaction.
///
/// When a transaction's Plutus scripts fail (or tx declares `is_valid = false`
/// and scripts indeed fail), the ledger applies a "penalty" by seizing collateral.
///
/// ```text
/// utxoKeep = collateralInputs ⋪ utxo     -- remove collateral from UTxO
/// utxoDel  = collateralInputs ◁ utxo      -- the collateral entries being seized
/// new_utxo = utxoKeep                     -- nothing new is added
/// new_fees = fees + sumAllCoin(utxoDel)   -- ALL collateral ADA goes to fee pot
/// ```
///
/// Reference: Alonzo/Rules/Utxos.hs:291-315 (alonzoEvalScriptsTxInvalid)
///
/// ```haskell
/// alonzoEvalScriptsTxInvalid = do
///   TRC (UtxoEnv slot pp _, utxos@(UTxOState utxo _ fees _ _ _), tx) <- judgmentContext
///   let txBody = tx ^. bodyTxL
///   let !(utxoKeep, utxoDel) = extractKeys (unUTxO utxo) (txBody ^. collateralInputsTxBodyL)
///   pure $!
///     utxos
///       { utxosUtxo = UTxO utxoKeep
///       , utxosFees = fees <> sumAllCoin utxoDel
///       }
/// ```
///
/// | What       | How                                           |
/// |------------|-----------------------------------------------|
/// | Consumed   | All collateral inputs removed from UTxO       |
/// | Produced   | Nothing                                       |
/// | Fees       | fees + sumAllCoin(collateral inputs)           |
pub fn alonzo_apply_failed_tx(
    state: &AlonzoUTxOState,
    collateral_inputs: &HashSet<TxIn>,
) -> AlonzoUTxOState {
    // Partition UTxO: keep everything except collateral, seize collateral
    let mut utxo_keep = state.utxo.utxo.clone();
    let mut collateral_coin: Coin = 0;
    for txin in collateral_inputs {
        if let Some(txout) = utxo_keep.remove(txin) {
            collateral_coin += txout.value.coin;
        }
    }
    AlonzoUTxOState {
        utxo: AlonzoUTxO { utxo: utxo_keep },
        deposited: state.deposited,
        fees: state.fees + collateral_coin,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn default_pp() -> AlonzoPParams {
        AlonzoPParams {
            min_fee_a: 44,
            min_fee_b: 155381,
            max_tx_size: 16384,
            key_deposit: 2_000_000,
            pool_deposit: 500_000_000,
            coins_per_utxo_word: 34_482,
            max_val_size: 5000,
            collateral_percentage: 150,
            max_collateral_inputs: 3,
            max_tx_ex_units: ExUnits {
                mem: 14_000_000,
                steps: 10_000_000_000,
            },
        }
    }

    #[test]
    fn test_validity_interval() {
        let interval = ValidityInterval {
            invalid_before: Some(10),
            invalid_hereafter: Some(100),
        };

        assert!(!interval.contains(5));   // Before start
        assert!(interval.contains(10));   // At start
        assert!(interval.contains(50));   // In middle
        assert!(interval.contains(99));   // Just before end
        assert!(!interval.contains(100)); // At end (exclusive)
    }

    #[test]
    fn test_insufficient_collateral() {
        let pp = default_pp();

        // Fee: 1 ADA, need 1.5 ADA collateral (150%)
        // Provide 2 ADA - should pass
        assert!(validate_insufficient_collateral(&pp, 1_000_000, 2_000_000).is_ok());

        // Provide 1 ADA - should fail
        assert!(matches!(
            validate_insufficient_collateral(&pp, 1_000_000, 1_000_000),
            Err(AlonzoUtxoPredFailure::InsufficientCollateral { .. })
        ));
    }

    #[test]
    fn test_ex_units() {
        let pp = default_pp();

        let within = ExUnits {
            mem: 10_000_000,
            steps: 5_000_000_000,
        };
        assert!(validate_ex_units_too_big(&pp, &within).is_ok());

        let exceeds_mem = ExUnits {
            mem: 20_000_000,
            steps: 5_000_000_000,
        };
        assert!(matches!(
            validate_ex_units_too_big(&pp, &exceeds_mem),
            Err(AlonzoUtxoPredFailure::ExUnitsTooBigUTxO { .. })
        ));
    }
}
