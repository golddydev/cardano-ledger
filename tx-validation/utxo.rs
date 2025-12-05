// Cardano Transaction Phase 1 Validation (Complete)
// This module implements the complete UTXO rule validation across all eras
// Reference: See utxo.md for detailed explanations
//
// ============================================================================
// ERA EVOLUTION & ARCHITECTURE
// ============================================================================
//
// SHELLEY ERA (Foundation)
// - Defines: ShelleyUtxoPredFailure with all basic error types
// - Exports: All validation functions (validateX)
// - Key feature: Simple TTL (time-to-live) expiration
//
// ALLEGRA ERA (Validity Intervals)
// - Defines: AllegraUtxoPredFailure (completely NEW type, not nested)
// - Exports: validateOutsideValidityIntervalUTxO, validateOutputTooBigUTxO
// - Reuses: Most Shelley validation functions via qualified imports
// - Conversion: shelleyToAllegraUtxoPredFailure function
// - Key features:
//   * ValidityInterval with both lower and upper bounds
//   * Output value size limit (4000 bytes hardcoded)
//
// MARY ERA (Multi-Asset)
// - Defines: NO new error type! Uses AllegraUtxoPredFailure directly
// - Type alias: EraRuleFailure "UTXO" MaryEra = AllegraUtxoPredFailure MaryEra
// - Exports: NOTHING (empty module besides type instances)
// - Reuses: All Allegra validation entirely
// - Key feature: Multi-asset Value type (handled by existing validations)
// - Architecture note: First era to completely reuse previous era's UTXO rules
//
// ALONZO ERA (Plutus Scripts)
// - Defines: AlonzoUtxoPredFailure (builds on Allegra)
// - Key features: Collateral validation, execution units, Plutus script support
//
// BABBAGE ERA (Reference Inputs)
// - Defines: BabbageUtxoPredFailure
// - Key features: Reference inputs, collateral return output
//
// CONWAY ERA (Governance)
// - Defines: ConwayUtxoPredFailure (FLATTENED - all errors in one enum)
// - Reuses: Babbage validation for Phase 1
// - Key feature: Governance (mainly affects Phase 2/UTXOS)
//
// ============================================================================
// VALIDATION FUNCTION REUSE PATTERN
// ============================================================================
//
// Each era follows a pattern of reusing previous validation logic:
//
// Shelley → Allegra:
//   Allegra imports: Shelley.validateInputSetEmptyUTxO, Shelley.validateFeeTooSmallUTxO, etc.
//   Allegra defines new: validateOutsideValidityIntervalUTxO, validateOutputTooBigUTxO
//
// Allegra → Mary:
//   Mary reuses EVERYTHING from Allegra (no new validation functions)
//   Mary only changes the Value type to support multi-asset
//
// This establishes the architectural principle:
// "Define new error types only when adding new validations, otherwise reuse"
//
// ============================================================================

use std::collections::{HashMap, HashSet};

// ============================================================================
// Type Definitions
// ============================================================================

/// Slot number on the blockchain
pub type SlotNo = u64;

/// Coin value in lovelace (1 ADA = 1,000,000 lovelace)
pub type Coin = u64;

/// Delta coin (can be negative for calculations)
pub type DeltaCoin = i64;

/// Transaction size in bytes
pub type TxSize = u32;

/// Network identifier (Mainnet = 1, Testnet = 0)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    Testnet = 0,
    Mainnet = 1,
}

/// Transaction input reference (TxId + output index)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TxIn {
    pub tx_id: Vec<u8>,      // Transaction ID (32 bytes hash)
    pub output_index: u64,   // Output index within transaction
}

/// Validity interval for transactions (Allegra+)
#[derive(Debug, Clone, Copy)]
pub struct ValidityInterval {
    pub invalid_before: Option<SlotNo>,
    pub invalid_hereafter: Option<SlotNo>,
}

/// Address on the blockchain
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Addr {
    Base {
        network: Network,
        payment_credential: Credential,
        stake_credential: Credential,
    },
    Enterprise {
        network: Network,
        payment_credential: Credential,
    },
    Pointer {
        network: Network,
        payment_credential: Credential,
        slot: u64,
        tx_index: u64,
        cert_index: u64,
    },
    Bootstrap {
        attributes: Vec<u8>,
    },
    Reward {
        network: Network,
        stake_credential: Credential,
    },
}

impl Addr {
    pub fn get_network(&self) -> Option<Network> {
        match self {
            Addr::Base { network, .. } => Some(*network),
            Addr::Enterprise { network, .. } => Some(*network),
            Addr::Pointer { network, .. } => Some(*network),
            Addr::Bootstrap { .. } => None,
            Addr::Reward { network, .. } => Some(*network),
        }
    }

    pub fn is_bootstrap(&self) -> bool {
        matches!(self, Addr::Bootstrap { .. })
    }

    pub fn bootstrap_attrs_size(&self) -> Option<usize> {
        match self {
            Addr::Bootstrap { attributes } => Some(attributes.len()),
            _ => None,
        }
    }

    /// Check if address is VKey-locked (not script-locked)
    pub fn is_vkey_locked(&self) -> bool {
        match self {
            Addr::Base { payment_credential, .. } => matches!(payment_credential, Credential::KeyHash(_)),
            Addr::Enterprise { payment_credential, .. } => matches!(payment_credential, Credential::KeyHash(_)),
            Addr::Pointer { payment_credential, .. } => matches!(payment_credential, Credential::KeyHash(_)),
            Addr::Bootstrap { .. } => true,
            Addr::Reward { .. } => true,
        }
    }
}

/// Credential (key hash or script hash)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Credential {
    KeyHash(Vec<u8>),      // 28 bytes
    ScriptHash(Vec<u8>),   // 28 bytes
}

/// Reward account for withdrawals
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RewardAccount {
    pub network: Network,
    pub credential: Credential,
}

impl RewardAccount {
    pub fn network(&self) -> Network {
        self.network
    }
}

/// Transaction output
#[derive(Debug, Clone)]
pub struct TxOut {
    pub address: Addr,
    pub value: Value,
    pub datum_hash: Option<Vec<u8>>,
    pub datum: Option<Vec<u8>>,          // Babbage inline datum
    pub script_ref: Option<Vec<u8>>,     // Babbage reference script
}

impl TxOut {
    /// Check if output is VKey-locked
    pub fn is_vkey_locked(&self) -> bool {
        self.address.is_vkey_locked()
    }

    /// Calculate serialized size (simplified)
    pub fn serialized_size(&self) -> usize {
        // Simplified calculation
        let mut size = 64; // Base size for address + coin
        size += self.value.multi_asset.len() * 64; // Rough estimate for tokens
        if self.datum.is_some() {
            size += self.datum.as_ref().unwrap().len();
        }
        if self.script_ref.is_some() {
            size += self.script_ref.as_ref().unwrap().len();
        }
        size
    }
}

/// Value (ADA + native tokens)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Value {
    pub coin: Coin,
    pub multi_asset: HashMap<Vec<u8>, HashMap<Vec<u8>, u64>>, // PolicyID -> AssetName -> Quantity
}

impl Value {
    pub fn new_ada(coin: Coin) -> Self {
        Value {
            coin,
            multi_asset: HashMap::new(),
        }
    }

    pub fn add(&self, other: &Value) -> Value {
        let mut result = self.clone();
        result.coin += other.coin;

        for (policy_id, assets) in &other.multi_asset {
            let result_assets = result.multi_asset.entry(policy_id.clone()).or_insert_with(HashMap::new);
            for (asset_name, quantity) in assets {
                *result_assets.entry(asset_name.clone()).or_insert(0) += quantity;
            }
        }

        result
    }

    pub fn subtract(&self, other: &Value) -> Value {
        let mut result = self.clone();
        result.coin = result.coin.saturating_sub(other.coin);

        for (policy_id, assets) in &other.multi_asset {
            if let Some(result_assets) = result.multi_asset.get_mut(policy_id) {
                for (asset_name, quantity) in assets {
                    if let Some(result_qty) = result_assets.get_mut(asset_name) {
                        *result_qty = result_qty.saturating_sub(*quantity);
                    }
                }
            }
        }

        result
    }

    pub fn equals(&self, other: &Value) -> bool {
        self == other
    }

    pub fn sum(values: &[Value]) -> Value {
        values.iter().fold(Value::new_ada(0), |acc, v| acc.add(v))
    }

    /// Check if value contains only ADA (no native tokens)
    pub fn is_ada_only(&self) -> bool {
        self.multi_asset.is_empty() ||
        self.multi_asset.values().all(|assets| assets.values().all(|&qty| qty == 0))
    }

    /// Get serialized size of value
    pub fn serialized_size(&self) -> usize {
        let mut size = 9; // Coin encoding
        size += self.multi_asset.len() * 64; // Rough estimate for multi-asset
        size
    }
}

/// Certificate (stake registration, delegation, pool registration, etc.)
#[derive(Debug, Clone)]
pub enum Certificate {
    StakeRegistration { stake_credential: Credential },
    StakeDeregistration { stake_credential: Credential },
    StakeDelegation { stake_credential: Credential, pool_id: Vec<u8> },
    PoolRegistration { pool_id: Vec<u8>, deposit: Coin },
    PoolRetirement { pool_id: Vec<u8>, epoch: u64 },
}

/// Execution units for Plutus scripts (Alonzo+)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExUnits {
    pub mem: u64,   // Memory units
    pub steps: u64, // CPU steps
}

impl ExUnits {
    pub fn zero() -> Self {
        ExUnits { mem: 0, steps: 0 }
    }

    pub fn add(&self, other: &ExUnits) -> ExUnits {
        ExUnits {
            mem: self.mem + other.mem,
            steps: self.steps + other.steps,
        }
    }

    /// Point-wise comparison
    pub fn lte(&self, other: &ExUnits) -> bool {
        self.mem <= other.mem && self.steps <= other.steps
    }
}

/// Transaction body
#[derive(Debug, Clone)]
pub struct TxBody {
    // Basic fields (Shelley)
    pub inputs: HashSet<TxIn>,
    pub outputs: Vec<TxOut>,
    pub fee: Coin,
    pub ttl: Option<SlotNo>,  // Shelley only
    pub certificates: Vec<Certificate>,
    pub withdrawals: HashMap<RewardAccount, Coin>,
    pub update: Option<ProtocolParamUpdate>,

    // Allegra+ fields
    pub validity_interval: Option<ValidityInterval>,

    // Mary+ fields
    pub mint: HashMap<Vec<u8>, HashMap<Vec<u8>, i64>>, // Minting/burning tokens

    // Alonzo+ fields
    pub collateral_inputs: HashSet<TxIn>,
    pub required_signers: HashSet<Vec<u8>>,
    pub network_id: Option<Network>,

    // Babbage+ fields
    pub reference_inputs: HashSet<TxIn>,
    pub collateral_return: Option<TxOut>,
    pub total_collateral: Option<Coin>,
}

/// Protocol parameter update proposal
#[derive(Debug, Clone)]
pub struct ProtocolParamUpdate {
    pub proposals: HashMap<Vec<u8>, ProtocolParams>,
    pub epoch: u64,
}

/// Redeemer for Plutus scripts (Alonzo+)
#[derive(Debug, Clone)]
pub struct Redeemer {
    pub tag: RedeemerTag,
    pub index: u64,
    pub data: Vec<u8>,
    pub ex_units: ExUnits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedeemerTag {
    Spend,
    Mint,
    Cert,
    Reward,
}

/// Transaction witnesses
#[derive(Debug, Clone)]
pub struct TxWitnesses {
    pub vkey_witnesses: Vec<Vec<u8>>,
    pub scripts: Vec<Vec<u8>>,
    pub bootstrap_witnesses: Vec<Vec<u8>>,
    pub redeemers: Vec<Redeemer>,  // Alonzo+
}

/// Transaction (body + witnesses + metadata)
#[derive(Debug, Clone)]
pub struct Tx {
    pub body: TxBody,
    pub witnesses: TxWitnesses,
    pub auxiliary_data: Option<Vec<u8>>,
    pub serialized_size: TxSize,
}

/// Protocol parameters
#[derive(Debug, Clone)]
pub struct ProtocolParams {
    // Shelley era
    pub min_fee_a: Coin,
    pub min_fee_b: Coin,
    pub max_tx_size: TxSize,
    pub min_utxo_value: Coin,
    pub key_deposit: Coin,
    pub pool_deposit: Coin,

    // Alonzo era
    pub coins_per_utxo_word: Option<Coin>,       // Alonzo
    pub max_tx_ex_units: Option<ExUnits>,        // Alonzo
    pub max_collateral_inputs: Option<u64>,      // Alonzo
    pub collateral_percentage: Option<u64>,      // Alonzo (e.g., 150%)
    pub max_value_size: Option<usize>,           // Alonzo

    // Babbage era
    pub coins_per_utxo_byte: Option<Coin>,       // Babbage
    pub protocol_version: (u64, u64),            // Major, Minor
}

/// UTxO set
pub type UTxO = HashMap<TxIn, TxOut>;

/// Certificate state
#[derive(Debug, Clone)]
pub struct CertState {
    pub registered_stake_keys: HashSet<Credential>,
    pub delegations: HashMap<Credential, Vec<u8>>,
    pub registered_pools: HashMap<Vec<u8>, PoolParams>,
    pub deposits: HashMap<Credential, Coin>,
}

#[derive(Debug, Clone)]
pub struct PoolParams {
    pub pool_id: Vec<u8>,
    pub deposit: Coin,
}

/// Governance state
#[derive(Debug, Clone, Default)]
pub struct GovState {
    pub proposals: HashMap<Vec<u8>, ProtocolParams>,
}

/// UTxO state
#[derive(Debug, Clone)]
pub struct UTxOState {
    pub utxo: UTxO,
    pub deposits: Coin,
    pub fees: Coin,
    pub gov_state: GovState,
}

/// Environment for UTxO validation
#[derive(Debug, Clone)]
pub struct UtxoEnv {
    pub slot: SlotNo,
    pub pparams: ProtocolParams,
    pub cert_state: CertState,
    pub network_id: Network,
}

// ============================================================================
// Validation Errors (Conway-style Flattened Structure)
// ============================================================================
//
// ARCHITECTURE DECISION: Flat vs Era-Specific Error Structure
//
// This implementation uses a FLAT structure where all errors from all eras
// (Shelley, Allegra, Mary, Alonzo, Babbage, Conway) are combined into a single
// enum without nesting. This matches the Haskell implementation's Conway approach.
//
// Why Flat Structure?
// ✅ Matches Haskell's Conway era design (no nested enums)
// ✅ Simpler error handling - no pattern matching on nested types
// ✅ Easier to extend - just add new variants for new eras
// ✅ Better for users - one error type handles all eras
// ✅ Forward compatible - Conway sets precedent for unified errors
//
// Alternative (NOT used): Era-specific wrapping like:
//   enum UtxoPredFailure {
//     Shelley(ShelleyError),
//     Alonzo(AlonzoError),
//     ...
//   }
// This was used in Babbage (AlonzoInBabbageUtxoPredFailure) but Conway
// abandoned this pattern in favor of flattening.
//
// Reference: eras/conway/impl/src/Cardano/Ledger/Conway/Rules/Utxo.hs:85-157

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UtxoPredFailure {
    // ===== Shelley Era Errors =====

    /// The transaction inputs are not in the current UTxO set
    /// Reference: Shelley/Utxo.hs:169-170
    BadInputsUTxO {
        bad_inputs: HashSet<TxIn>,
    },

    /// Transaction has expired (Shelley only - replaced by OutsideValidityIntervalUTxO in Allegra+)
    /// Reference: Shelley/Utxo.hs:171-172
    ExpiredUTxO {
        ttl: SlotNo,
        current_slot: SlotNo,
    },

    /// Transaction size exceeds protocol parameter limit
    /// Reference: Shelley/Utxo.hs:173-174
    MaxTxSizeUTxO {
        supplied: TxSize,
        max: TxSize,
    },

    /// Transaction has no inputs
    /// Reference: Shelley/Utxo.hs:175
    InputSetEmptyUTxO,

    /// Transaction fee is too small
    /// Reference: Shelley/Utxo.hs:176-177
    FeeTooSmallUTxO {
        supplied: Coin,
        required: Coin,
    },

    /// Value is not conserved (inputs + withdrawals + refunds != outputs + fees + deposits)
    /// Reference: Shelley/Utxo.hs:178-179
    ValueNotConservedUTxO {
        consumed: Value,
        produced: Value,
    },

    /// Output addresses have wrong network ID
    /// Reference: Shelley/Utxo.hs:180-182
    WrongNetwork {
        expected: Network,
        wrong_addresses: HashSet<Addr>,
    },

    /// Withdrawal reward accounts have wrong network ID
    /// Reference: Shelley/Utxo.hs:183-185
    WrongNetworkWithdrawal {
        expected: Network,
        wrong_accounts: HashSet<RewardAccount>,
    },

    /// Output values are below minimum UTxO value
    /// Reference: Shelley/Utxo.hs:186-187
    OutputTooSmallUTxO {
        small_outputs: Vec<TxOut>,
    },

    /// Bootstrap address attributes exceed 64 bytes
    /// Reference: Shelley/Utxo.hs:189-190
    OutputBootAddrAttrsTooBig {
        large_outputs: Vec<TxOut>,
    },

    // ===== Allegra Era Errors =====

    /// Current slot is outside transaction's validity interval
    /// Reference: Allegra/Utxo.hs:73-75 (replaces ExpiredUTxO)
    ///
    /// Allegra introduced ValidityInterval with both lower and upper bounds:
    /// - invalid_before (Option<SlotNo>): Transaction invalid before this slot
    /// - invalid_hereafter (Option<SlotNo>): Transaction invalid at or after this slot
    ///
    /// This replaces Shelley's simple TTL (time-to-live) with a more flexible interval,
    /// enabling time-locked transactions that become valid at a future slot.
    OutsideValidityIntervalUTxO {
        interval: ValidityInterval,
        current_slot: SlotNo,
    },

    /// Output value serialized size exceeds limit (Allegra+)
    /// Reference: Allegra/Utxo.hs:91-92
    ///
    /// NEW in Allegra: Validates that the CBOR-serialized size of an output's Value
    /// doesn't exceed MaxValSize (hardcoded to 4000 bytes in Allegra, configurable
    /// in Alonzo via ppMaxValSizeL).
    ///
    /// This validation was introduced in preparation for Mary's multi-asset support,
    /// preventing outputs from containing too many different native tokens.
    ///
    /// NOTE: This is DIFFERENT from OutputTooBigUTxO in Alonzo, which returns
    /// different data (actual size, max size, output) vs just the outputs list.
    OutputTooBigUTxOAllegra {
        outputs_too_big: Vec<TxOut>,
    },

    // ===== Alonzo Era Errors =====

    /// Validity interval end is outside consensus forecast range
    /// Reference: Alonzo/Utxo.hs:353-372
    OutsideForecast {
        slot: SlotNo,
    },

    /// Total execution units exceed protocol parameter limit
    /// Reference: Alonzo/Utxo.hs:438-452
    ExUnitsTooBigUTxO {
        supplied: ExUnits,
        max: ExUnits,
    },

    /// Output value serialized size exceeds protocol parameter limit
    /// Reference: Alonzo/Utxo.hs:399-418
    OutputTooBigUTxO {
        outputs_too_big: Vec<(usize, usize, TxOut)>, // (actual_size, max_size, output)
    },

    /// Network ID in transaction body doesn't match expected network
    /// Reference: Alonzo/Utxo.hs:423-433
    WrongNetworkInTxBody {
        supplied: Network,
        expected: Network,
    },

    /// Too many collateral inputs
    /// Reference: Alonzo/Utxo.hs:457-468
    TooManyCollateralInputs {
        supplied: u64,
        max: u64,
    },

    /// Collateral inputs are script-locked (must be VKey-locked)
    /// Reference: Alonzo/Utxo.hs:313-319
    ScriptsNotPaidUTxO {
        script_locked_collateral: UTxO,
    },

    /// Collateral is insufficient (must be >= collateralPercentage * fee)
    /// Reference: Alonzo/Utxo.hs:322-337
    InsufficientCollateral {
        balance: DeltaCoin,
        required: Coin,
    },

    /// Collateral contains non-ADA tokens
    /// Reference: Alonzo/Utxo.hs:340-347
    CollateralContainsNonADA {
        value: Value,
    },

    /// Transaction has redeemers but no collateral inputs
    /// Reference: Alonzo/Utxo.hs:307
    NoCollateralInputs,

    // ===== Babbage Era Errors =====

    /// Reference inputs overlap with regular inputs
    /// Reference: Babbage/Utxo.hs:225-239
    BabbageNonDisjointRefInputs {
        common_inputs: HashSet<TxIn>,
    },

    /// Total collateral field doesn't match computed collateral
    /// Reference: Babbage/Utxo.hs:320-325
    IncorrectTotalCollateralField {
        computed: DeltaCoin,
        declared: Coin,
    },

    /// Output is too small (Babbage version with per-output minimums)
    /// Reference: Babbage/Utxo.hs:328-348
    BabbageOutputTooSmallUTxO {
        small_outputs: Vec<(TxOut, Coin)>, // (output, min_required)
    },

    // ===== Conway Era Errors =====
    // (Conway doesn't add new UTXO-level errors, but uses flattened structure)
}

impl UtxoPredFailure {
    /// Returns the era in which this error was first introduced
    pub fn introduced_in_era(&self) -> &'static str {
        match self {
            // Shelley era
            Self::BadInputsUTxO { .. } => "Shelley",
            Self::ExpiredUTxO { .. } => "Shelley",
            Self::MaxTxSizeUTxO { .. } => "Shelley",
            Self::InputSetEmptyUTxO => "Shelley",
            Self::FeeTooSmallUTxO { .. } => "Shelley",
            Self::ValueNotConservedUTxO { .. } => "Shelley",
            Self::WrongNetwork { .. } => "Shelley",
            Self::WrongNetworkWithdrawal { .. } => "Shelley",
            Self::OutputTooSmallUTxO { .. } => "Shelley",
            Self::OutputBootAddrAttrsTooBig { .. } => "Shelley",

            // Allegra era
            Self::OutsideValidityIntervalUTxO { .. } => "Allegra",
            Self::OutputTooBigUTxOAllegra { .. } => "Allegra",

            // Alonzo era
            Self::OutsideForecast { .. } => "Alonzo",
            Self::ExUnitsTooBigUTxO { .. } => "Alonzo",
            Self::OutputTooBigUTxO { .. } => "Alonzo",
            Self::WrongNetworkInTxBody { .. } => "Alonzo",
            Self::TooManyCollateralInputs { .. } => "Alonzo",
            Self::ScriptsNotPaidUTxO { .. } => "Alonzo",
            Self::InsufficientCollateral { .. } => "Alonzo",
            Self::CollateralContainsNonADA { .. } => "Alonzo",
            Self::NoCollateralInputs => "Alonzo",

            // Babbage era
            Self::BabbageNonDisjointRefInputs { .. } => "Babbage",
            Self::IncorrectTotalCollateralField { .. } => "Babbage",
            Self::BabbageOutputTooSmallUTxO { .. } => "Babbage",
        }
    }

    /// Returns true if this error can occur in the specified era
    pub fn applies_to_era(&self, era: &str) -> bool {
        let introduced = self.introduced_in_era();
        match (introduced, era) {
            ("Shelley", _) => true, // All eras support Shelley errors
            ("Allegra", "Allegra" | "Mary" | "Alonzo" | "Babbage" | "Conway") => true,
            ("Alonzo", "Alonzo" | "Babbage" | "Conway") => true,
            ("Babbage", "Babbage" | "Conway") => true,
            ("Conway", "Conway") => true,
            _ => false,
        }
    }
}

pub type ValidationResult<T> = Result<T, UtxoPredFailure>;

// ============================================================================
// Validation Functions (Phase 1)
// ============================================================================

// ===== SHELLEY ERA VALIDATIONS =====

/// Validate time to live (Shelley only)
/// Reference: Utxo.hs:421-430
pub fn validate_time_to_live(tx_body: &TxBody, current_slot: SlotNo) -> ValidationResult<()> {
    if let Some(ttl) = tx_body.ttl {
        if ttl < current_slot {
            return Err(UtxoPredFailure::ExpiredUTxO {
                ttl,
                current_slot,
            });
        }
    }
    Ok(())
}

/// Validate validity interval (Allegra+)
/// Reference: Allegra/Utxo.hs:242-249
///
/// Allegra introduced ValidityInterval to replace Shelley's TTL with a more flexible system.
/// The interval has two optional bounds:
/// - invalid_before: Transaction is invalid if current_slot < invalid_before
/// - invalid_hereafter: Transaction is invalid if current_slot >= invalid_hereafter
///
/// The check uses the `inInterval` function from Allegra.Scripts module:
/// ```haskell
/// inInterval :: SlotNo -> ValidityInterval -> Bool
/// inInterval slot (ValidityInterval mBefore mAfter) =
///   case mBefore of
///     SNothing -> True
///     SJust before -> before <= slot
///   && case mAfter of
///     SNothing -> True
///     SJust after -> slot < after
/// ```
pub fn validate_validity_interval(tx_body: &TxBody, current_slot: SlotNo) -> ValidationResult<()> {
    if let Some(interval) = &tx_body.validity_interval {
        // Check lower bound: slot must be >= invalid_before
        if let Some(invalid_before) = interval.invalid_before {
            if current_slot < invalid_before {
                return Err(UtxoPredFailure::OutsideValidityIntervalUTxO {
                    interval: *interval,
                    current_slot,
                });
            }
        }
        // Check upper bound: slot must be < invalid_hereafter
        if let Some(invalid_hereafter) = interval.invalid_hereafter {
            if current_slot >= invalid_hereafter {
                return Err(UtxoPredFailure::OutsideValidityIntervalUTxO {
                    interval: *interval,
                    current_slot,
                });
            }
        }
    }
    Ok(())
}

/// Validate output value size (Allegra+)
/// Reference: Allegra/Utxo.hs:254-270
///
/// Ensures that the serialized size of each output's Value doesn't exceed MaxValSize.
/// In Allegra, this was hardcoded to 4000 bytes. In Alonzo+, it became configurable
/// via the ppMaxValSizeL protocol parameter.
///
/// This validation was introduced to prevent outputs with too many native tokens
/// (introduced in Mary era) from bloating the UTxO set.
pub fn validate_output_value_size_allegra(tx_body: &TxBody) -> ValidationResult<()> {
    const MAX_VAL_SIZE: usize = 4000; // Hardcoded in Allegra

    let outputs_too_big: Vec<TxOut> = tx_body
        .outputs
        .iter()
        .filter(|output| output.value.serialized_size() > MAX_VAL_SIZE)
        .cloned()
        .collect();

    if !outputs_too_big.is_empty() {
        return Err(UtxoPredFailure::OutputTooBigUTxOAllegra { outputs_too_big });
    }
    Ok(())
}

/// Reference: Utxo.hs:435-442
pub fn validate_input_set_empty(tx_body: &TxBody) -> ValidationResult<()> {
    if tx_body.inputs.is_empty() {
        return Err(UtxoPredFailure::InputSetEmptyUTxO);
    }
    Ok(())
}

/// Calculate minimum fee
/// Reference: Shelley/Tx.hs:256-258
pub fn calculate_min_fee(tx: &Tx, pparams: &ProtocolParams) -> Coin {
    (tx.serialized_size as u64 * pparams.min_fee_a) + pparams.min_fee_b
}

/// Reference: Utxo.hs:447-463
pub fn validate_fee_too_small(tx: &Tx, pparams: &ProtocolParams) -> ValidationResult<()> {
    let min_fee = calculate_min_fee(tx, pparams);
    let tx_fee = tx.body.fee;

    if tx_fee < min_fee {
        return Err(UtxoPredFailure::FeeTooSmallUTxO {
            supplied: tx_fee,
            required: min_fee,
        });
    }
    Ok(())
}

/// Reference: Utxo.hs:468-476
/// In Alonzo+, this checks inputs + collateral inputs + reference inputs (Babbage)
pub fn validate_bad_inputs(
    utxo: &UTxO,
    tx_body: &TxBody,
    check_collateral: bool,
    check_reference: bool,
) -> ValidationResult<()> {
    let mut all_inputs = tx_body.inputs.clone();

    if check_collateral {
        all_inputs.extend(tx_body.collateral_inputs.clone());
    }

    if check_reference {
        all_inputs.extend(tx_body.reference_inputs.clone());
    }

    let bad_inputs: HashSet<TxIn> = all_inputs
        .iter()
        .filter(|input| !utxo.contains_key(input))
        .cloned()
        .collect();

    if !bad_inputs.is_empty() {
        return Err(UtxoPredFailure::BadInputsUTxO { bad_inputs });
    }
    Ok(())
}

/// Reference: Utxo.hs:481-492
pub fn validate_wrong_network(network_id: Network, tx_body: &TxBody) -> ValidationResult<()> {
    let wrong_addrs: HashSet<Addr> = tx_body
        .outputs
        .iter()
        .filter_map(|output| {
            if let Some(net) = output.address.get_network() {
                if net != network_id {
                    Some(output.address.clone())
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    if !wrong_addrs.is_empty() {
        return Err(UtxoPredFailure::WrongNetwork {
            expected: network_id,
            wrong_addresses: wrong_addrs,
        });
    }
    Ok(())
}

/// Reference: Utxo.hs:497-509
pub fn validate_wrong_network_withdrawal(
    network_id: Network,
    tx_body: &TxBody,
) -> ValidationResult<()> {
    let wrong_accounts: HashSet<RewardAccount> = tx_body
        .withdrawals
        .keys()
        .filter(|account| account.network() != network_id)
        .cloned()
        .collect();

    if !wrong_accounts.is_empty() {
        return Err(UtxoPredFailure::WrongNetworkWithdrawal {
            expected: network_id,
            wrong_accounts,
        });
    }
    Ok(())
}

fn calculate_total_deposits(
    tx_body: &TxBody,
    pparams: &ProtocolParams,
    cert_state: &CertState,
) -> Coin {
    tx_body
        .certificates
        .iter()
        .map(|cert| match cert {
            Certificate::StakeRegistration { stake_credential } => {
                if cert_state.registered_stake_keys.contains(stake_credential) {
                    0
                } else {
                    pparams.key_deposit
                }
            }
            Certificate::PoolRegistration { deposit, .. } => *deposit,
            _ => 0,
        })
        .sum()
}

fn calculate_total_refunds(tx_body: &TxBody, cert_state: &CertState) -> Coin {
    tx_body
        .certificates
        .iter()
        .map(|cert| match cert {
            Certificate::StakeDeregistration { stake_credential } => {
                cert_state
                    .deposits
                    .get(stake_credential)
                    .copied()
                    .unwrap_or(0)
            }
            Certificate::PoolRetirement { pool_id, .. } => cert_state
                .registered_pools
                .get(pool_id)
                .map(|params| params.deposit)
                .unwrap_or(0),
            _ => 0,
        })
        .sum()
}

/// Calculate consumed value
/// Reference: Shelley/UTxO.hs:123-176
pub fn calculate_consumed(
    tx_body: &TxBody,
    utxo: &UTxO,
    cert_state: &CertState,
) -> Value {
    let input_value = tx_body
        .inputs
        .iter()
        .filter_map(|input| utxo.get(input).map(|output| &output.value))
        .fold(Value::new_ada(0), |acc, val| acc.add(val));

    let withdrawal_value = Value::new_ada(tx_body.withdrawals.values().sum());
    let refunds = calculate_total_refunds(tx_body, cert_state);
    let refund_value = Value::new_ada(refunds);

    input_value.add(&withdrawal_value).add(&refund_value)
}

/// Calculate produced value
/// Reference: Shelley/UTxO.hs:138-158
pub fn calculate_produced(
    tx_body: &TxBody,
    pparams: &ProtocolParams,
    cert_state: &CertState,
) -> Value {
    let output_value = tx_body
        .outputs
        .iter()
        .fold(Value::new_ada(0), |acc, output| acc.add(&output.value));

    let fee_value = Value::new_ada(tx_body.fee);
    let deposits = calculate_total_deposits(tx_body, pparams, cert_state);
    let deposit_value = Value::new_ada(deposits);

    output_value.add(&fee_value).add(&deposit_value)
}

/// Reference: Utxo.hs:514-526
pub fn validate_value_not_conserved(
    tx_body: &TxBody,
    utxo: &UTxO,
    pparams: &ProtocolParams,
    cert_state: &CertState,
) -> ValidationResult<()> {
    let consumed = calculate_consumed(tx_body, utxo, cert_state);
    let produced = calculate_produced(tx_body, pparams, cert_state);

    if !consumed.equals(&produced) {
        return Err(UtxoPredFailure::ValueNotConservedUTxO { consumed, produced });
    }
    Ok(())
}

/// Calculate minimum UTxO value
fn calculate_min_utxo_value(output: &TxOut, pparams: &ProtocolParams) -> Coin {
    if let Some(coins_per_byte) = pparams.coins_per_utxo_byte {
        // Babbage: size-based minimum
        let size = output.serialized_size() as u64;
        size * coins_per_byte
    } else if let Some(coins_per_word) = pparams.coins_per_utxo_word {
        // Alonzo: word-based minimum
        let size = output.serialized_size() as u64;
        (size / 8) * coins_per_word
    } else {
        // Shelley: fixed minimum
        pparams.min_utxo_value
    }
}

/// Reference: Utxo.hs:531-545
pub fn validate_output_too_small(
    tx_body: &TxBody,
    pparams: &ProtocolParams,
) -> ValidationResult<()> {
    let small_outputs: Vec<TxOut> = tx_body
        .outputs
        .iter()
        .filter(|output| {
            let min_value = calculate_min_utxo_value(output, pparams);
            output.value.coin < min_value
        })
        .cloned()
        .collect();

    if !small_outputs.is_empty() {
        return Err(UtxoPredFailure::OutputTooSmallUTxO { small_outputs });
    }
    Ok(())
}

/// Reference: Utxo.hs:551-565
pub fn validate_output_boot_addr_attrs_too_big(tx_body: &TxBody) -> ValidationResult<()> {
    let large_outputs: Vec<TxOut> = tx_body
        .outputs
        .iter()
        .filter(|output| {
            if let Some(size) = output.address.bootstrap_attrs_size() {
                size > 64
            } else {
                false
            }
        })
        .cloned()
        .collect();

    if !large_outputs.is_empty() {
        return Err(UtxoPredFailure::OutputBootAddrAttrsTooBig { large_outputs });
    }
    Ok(())
}

/// Reference: Utxo.hs:570-584
pub fn validate_max_tx_size(tx: &Tx, pparams: &ProtocolParams) -> ValidationResult<()> {
    if tx.serialized_size > pparams.max_tx_size {
        return Err(UtxoPredFailure::MaxTxSizeUTxO {
            supplied: tx.serialized_size,
            max: pparams.max_tx_size,
        });
    }
    Ok(())
}

// ===== ALONZO ERA VALIDATIONS (Plutus & Collateral) =====

/// Reference: Alonzo/Utxo.hs:353-372
pub fn validate_outside_forecast(tx: &Tx, current_slot: SlotNo) -> ValidationResult<()> {
    // Simplified: In real implementation, check if slot can be converted to UTC time
    if !tx.witnesses.redeemers.is_empty() {
        if let Some(interval) = &tx.body.validity_interval {
            if let Some(invalid_hereafter) = interval.invalid_hereafter {
                // Check if slot is within forecast window (simplified)
                let forecast_window = 36 * 60 * 20; // ~3.6 hours
                if invalid_hereafter > current_slot + forecast_window {
                    return Err(UtxoPredFailure::OutsideForecast {
                        slot: invalid_hereafter,
                    });
                }
            }
        }
    }
    Ok(())
}

/// Reference: Alonzo/Utxo.hs:438-452
pub fn validate_ex_units_too_big(tx: &Tx, pparams: &ProtocolParams) -> ValidationResult<()> {
    if let Some(max_ex_units) = pparams.max_tx_ex_units {
        let total_ex_units = tx.witnesses.redeemers.iter()
            .fold(ExUnits::zero(), |acc, r| acc.add(&r.ex_units));

        if !total_ex_units.lte(&max_ex_units) {
            return Err(UtxoPredFailure::ExUnitsTooBigUTxO {
                supplied: total_ex_units,
                max: max_ex_units,
            });
        }
    }
    Ok(())
}

/// Reference: Alonzo/Utxo.hs:399-418
pub fn validate_output_too_big(tx_body: &TxBody, pparams: &ProtocolParams) -> ValidationResult<()> {
    if let Some(max_val_size) = pparams.max_value_size {
        let outputs_too_big: Vec<(usize, usize, TxOut)> = tx_body
            .outputs
            .iter()
            .filter_map(|output| {
                let val_size = output.value.serialized_size();
                if val_size > max_val_size {
                    Some((val_size, max_val_size, output.clone()))
                } else {
                    None
                }
            })
            .collect();

        if !outputs_too_big.is_empty() {
            return Err(UtxoPredFailure::OutputTooBigUTxO { outputs_too_big });
        }
    }
    Ok(())
}

/// Reference: Alonzo/Utxo.hs:423-433
pub fn validate_wrong_network_in_tx_body(
    network_id: Network,
    tx_body: &TxBody,
) -> ValidationResult<()> {
    if let Some(body_net_id) = tx_body.network_id {
        if body_net_id != network_id {
            return Err(UtxoPredFailure::WrongNetworkInTxBody {
                supplied: body_net_id,
                expected: network_id,
            });
        }
    }
    Ok(())
}

/// Reference: Alonzo/Utxo.hs:457-468
pub fn validate_too_many_collateral_inputs(
    tx_body: &TxBody,
    pparams: &ProtocolParams,
) -> ValidationResult<()> {
    if let Some(max_coll) = pparams.max_collateral_inputs {
        let num_coll = tx_body.collateral_inputs.len() as u64;
        if num_coll > max_coll {
            return Err(UtxoPredFailure::TooManyCollateralInputs {
                supplied: num_coll,
                max: max_coll,
            });
        }
    }
    Ok(())
}

/// Reference: Alonzo/Utxo.hs:313-319
pub fn validate_scripts_not_paid(utxo_collateral: &UTxO) -> ValidationResult<()> {
    let script_locked: UTxO = utxo_collateral
        .iter()
        .filter(|(_, output)| !output.is_vkey_locked())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    if !script_locked.is_empty() {
        return Err(UtxoPredFailure::ScriptsNotPaidUTxO {
            script_locked_collateral: script_locked,
        });
    }
    Ok(())
}

/// Reference: Alonzo/Utxo.hs:322-337
pub fn validate_insufficient_collateral(
    collateral_balance: DeltaCoin,
    fee: Coin,
    collateral_percentage: u64,
) -> ValidationResult<()> {
    let required = (fee as u128 * collateral_percentage as u128 / 100) as Coin;
    if (collateral_balance as u64) < required {
        return Err(UtxoPredFailure::InsufficientCollateral {
            balance: collateral_balance,
            required,
        });
    }
    Ok(())
}

/// Reference: Alonzo/Utxo.hs:340-347 & Babbage/Utxo.hs:278-317
pub fn validate_collateral_contains_non_ada(
    tx_body: &TxBody,
    utxo_collateral: &UTxO,
) -> ValidationResult<()> {
    let collateral_balance: Value = utxo_collateral
        .values()
        .fold(Value::new_ada(0), |acc, output| acc.add(&output.value));

    // Babbage: If there's a collateral return, subtract it
    let net_collateral = if let Some(ret) = &tx_body.collateral_return {
        collateral_balance.subtract(&ret.value)
    } else {
        collateral_balance.clone()
    };

    if !net_collateral.is_ada_only() {
        return Err(UtxoPredFailure::CollateralContainsNonADA {
            value: net_collateral,
        });
    }
    Ok(())
}

/// Reference: Alonzo/Utxo.hs:307
pub fn validate_no_collateral_inputs(tx_body: &TxBody) -> ValidationResult<()> {
    if tx_body.collateral_inputs.is_empty() {
        return Err(UtxoPredFailure::NoCollateralInputs);
    }
    Ok(())
}

/// Combined collateral validation (feesOK)
/// Reference: Alonzo/Utxo.hs:264-288 & Babbage/Utxo.hs:195-222
pub fn validate_collateral(
    tx: &Tx,
    utxo: &UTxO,
    pparams: &ProtocolParams,
) -> ValidationResult<()> {
    // Only validate collateral if transaction has redeemers (Plutus scripts)
    if tx.witnesses.redeemers.is_empty() {
        return Ok(());
    }

    // Get collateral UTxOs
    let utxo_collateral: UTxO = tx.body.collateral_inputs
        .iter()
        .filter_map(|input| utxo.get(input).map(|output| (input.clone(), output.clone())))
        .collect();

    // Part 1: At least one collateral input
    validate_no_collateral_inputs(&tx.body)?;

    // Part 2: Collateral must be VKey-locked
    validate_scripts_not_paid(&utxo_collateral)?;

    // Part 3: Collateral contains only ADA (or net-ADA in Babbage)
    validate_collateral_contains_non_ada(&tx.body, &utxo_collateral)?;

    // Part 4: Sufficient collateral
    let collateral_balance: i64 = utxo_collateral
        .values()
        .map(|output| output.value.coin as i64)
        .sum();

    if let Some(collateral_pct) = pparams.collateral_percentage {
        validate_insufficient_collateral(collateral_balance, tx.body.fee, collateral_pct)?;
    }

    // Part 5 (Babbage): Total collateral field matches
    if let Some(total_coll) = tx.body.total_collateral {
        let computed = if let Some(ret) = &tx.body.collateral_return {
            collateral_balance - ret.value.coin as i64
        } else {
            collateral_balance
        };

        if computed != total_coll as i64 {
            return Err(UtxoPredFailure::IncorrectTotalCollateralField {
                computed,
                declared: total_coll,
            });
        }
    }

    Ok(())
}

// ===== BABBAGE ERA VALIDATIONS =====

/// Reference: Babbage/Utxo.hs:225-239
pub fn validate_disjoint_ref_inputs(tx_body: &TxBody) -> ValidationResult<()> {
    let common: HashSet<TxIn> = tx_body
        .inputs
        .intersection(&tx_body.reference_inputs)
        .cloned()
        .collect();

    if !common.is_empty() {
        return Err(UtxoPredFailure::BabbageNonDisjointRefInputs {
            common_inputs: common,
        });
    }
    Ok(())
}

// ============================================================================
// State Update Function
// ============================================================================

/// Reference: Utxo.hs:591-624
pub fn update_utxo_state(
    utxo_state: &UTxOState,
    tx: &Tx,
    pparams: &ProtocolParams,
    cert_state: &CertState,
) -> UTxOState {
    let mut new_utxo = utxo_state.utxo.clone();

    // Remove spent inputs
    for input in &tx.body.inputs {
        new_utxo.remove(input);
    }

    // Add new outputs
    for (index, output) in tx.body.outputs.iter().enumerate() {
        let tx_in = TxIn {
            tx_id: vec![0; 32], // Would be actual tx hash
            output_index: index as u64,
        };
        new_utxo.insert(tx_in, output.clone());
    }

    let total_deposits = calculate_total_deposits(&tx.body, pparams, cert_state);
    let total_refunds = calculate_total_refunds(&tx.body, cert_state);
    let deposit_change = total_deposits as i64 - total_refunds as i64;

    UTxOState {
        utxo: new_utxo,
        deposits: (utxo_state.deposits as i64 + deposit_change) as u64,
        fees: utxo_state.fees + tx.body.fee,
        gov_state: utxo_state.gov_state.clone(),
    }
}

// ============================================================================
// Main Validation Functions
// ============================================================================

// ============================================================================
// Era-Specific Validation Functions
// ============================================================================

/// Phase 1 validation for Shelley era
/// Reference: Shelley/Utxo.hs:343-416
///
/// Shelley defines the foundational UTXO validation rules:
/// - Basic structural checks (inputs, outputs, fees)
/// - Value conservation
/// - Network ID validation
/// - Size limits
pub fn utxo_phase1_shelley(
    env: &UtxoEnv,
    utxo_state: &UTxOState,
    tx: &Tx,
) -> ValidationResult<UTxOState> {
    let tx_body = &tx.body;
    let utxo = &utxo_state.utxo;
    let pparams = &env.pparams;
    let cert_state = &env.cert_state;

    // 1. Time to live
    validate_time_to_live(tx_body, env.slot)?;

    // 2. Input set not empty
    validate_input_set_empty(tx_body)?;

    // 3. Fee sufficient
    validate_fee_too_small(tx, pparams)?;

    // 4. Inputs exist
    validate_bad_inputs(utxo, tx_body, false, false)?;

    // 5. Output addresses correct network
    validate_wrong_network(env.network_id, tx_body)?;

    // 6. Withdrawal addresses correct network
    validate_wrong_network_withdrawal(env.network_id, tx_body)?;

    // 7. Value conserved
    validate_value_not_conserved(tx_body, utxo, pparams, cert_state)?;

    // 8. (PPUP - protocol parameter updates would be processed here)

    // 9. Outputs meet minimum value
    validate_output_too_small(tx_body, pparams)?;

    // 10. Bootstrap addresses not too big
    validate_output_boot_addr_attrs_too_big(tx_body)?;

    // 11. Transaction size within limit
    validate_max_tx_size(tx, pparams)?;

    Ok(update_utxo_state(utxo_state, tx, pparams, cert_state))
}

/// Phase 1 validation for Allegra era
/// Reference: Allegra/Utxo.hs:160-238 (utxoTransition)
///
/// Allegra introduced two key changes to UTXO validation:
/// 1. **Validity Intervals**: Replaces TTL with flexible time bounds (invalidBefore, invalidHereafter)
/// 2. **Value Size Validation**: Limits serialized output value size to 4000 bytes
///
/// Most validations are reused from Shelley via qualified imports (Shelley.validateX).
/// This era established the pattern of validation function reuse that Mary continued.
///
/// ARCHITECTURAL NOTE:
/// - Defines AllegraUtxoPredFailure (not nested in Shelley)
/// - Provides shelleyToAllegraUtxoPredFailure conversion function
/// - Mary era reuses this implementation entirely (no Mary-specific UTXO rules)
pub fn utxo_phase1_allegra(
    env: &UtxoEnv,
    utxo_state: &UTxOState,
    tx: &Tx,
) -> ValidationResult<UTxOState> {
    let tx_body = &tx.body;
    let utxo = &utxo_state.utxo;
    let pparams = &env.pparams;
    let cert_state = &env.cert_state;

    // 1. Validity interval (replaces TTL check)
    validate_validity_interval(tx_body, env.slot)?;

    // 2. Input set not empty (reused from Shelley)
    validate_input_set_empty(tx_body)?;

    // 3. Fee sufficient (reused from Shelley)
    validate_fee_too_small(tx, pparams)?;

    // 4. Inputs exist (reused from Shelley)
    validate_bad_inputs(utxo, tx_body, false, false)?;

    // 5. Output addresses correct network (reused from Shelley)
    validate_wrong_network(env.network_id, tx_body)?;

    // 6. Withdrawal addresses correct network (reused from Shelley)
    validate_wrong_network_withdrawal(env.network_id, tx_body)?;

    // 7. Value conserved (reused from Shelley)
    validate_value_not_conserved(tx_body, utxo, pparams, cert_state)?;

    // 8. (PPUP - protocol parameter updates would be processed here)

    // 9. Outputs meet minimum value (Allegra version with pointwise comparison)
    validate_output_too_small(tx_body, pparams)?;

    // 10. Output value size limit (NEW in Allegra)
    validate_output_value_size_allegra(tx_body)?;

    // 11. Bootstrap addresses not too big (reused from Shelley)
    validate_output_boot_addr_attrs_too_big(tx_body)?;

    // 12. Transaction size within limit (reused from Shelley)
    validate_max_tx_size(tx, pparams)?;

    Ok(update_utxo_state(utxo_state, tx, pparams, cert_state))
}

/// Phase 1 validation for Mary era
/// Reference: Mary/Utxo.hs:1-26
///
/// Mary era introduces native tokens (multi-asset support) but DOES NOT add
/// new UTXO validation rules. It reuses Allegra's validation entirely.
///
/// ARCHITECTURAL NOTE:
/// ```haskell
/// type instance EraRuleFailure "UTXO" MaryEra = AllegraUtxoPredFailure MaryEra
/// ```
///
/// Mary's contribution is in the Value type (supporting multi-asset), not in
/// validation logic. The existing Allegra validations (especially
/// validateOutputTooBigUTxO) handle multi-asset outputs.
pub fn utxo_phase1_mary(
    env: &UtxoEnv,
    utxo_state: &UTxOState,
    tx: &Tx,
) -> ValidationResult<UTxOState> {
    // Mary uses identical validation logic to Allegra
    utxo_phase1_allegra(env, utxo_state, tx)
}

/// Phase 1 validation for Alonzo era (includes collateral)
/// Reference: Alonzo/Utxo.hs:473-557
pub fn utxo_phase1_alonzo(
    env: &UtxoEnv,
    utxo_state: &UTxOState,
    tx: &Tx,
) -> ValidationResult<UTxOState> {
    let tx_body = &tx.body;
    let utxo = &utxo_state.utxo;
    let pparams = &env.pparams;
    let cert_state = &env.cert_state;

    // 1. Validity interval
    validate_validity_interval(tx_body, env.slot)?;

    // 2. Outside forecast
    validate_outside_forecast(tx, env.slot)?;

    // 3. Input set not empty
    validate_input_set_empty(tx_body)?;

    // 4. Fees OK (includes collateral validation)
    validate_fee_too_small(tx, pparams)?;
    validate_collateral(tx, utxo, pparams)?;

    // 5. Inputs + collateral exist
    validate_bad_inputs(utxo, tx_body, true, false)?;

    // 6. Value conserved
    validate_value_not_conserved(tx_body, utxo, pparams, cert_state)?;

    // 7. Outputs meet minimum value
    validate_output_too_small(tx_body, pparams)?;

    // 8. Output value size limit
    validate_output_too_big(tx_body, pparams)?;

    // 9. Bootstrap addresses not too big
    validate_output_boot_addr_attrs_too_big(tx_body)?;

    // 10. Output addresses correct network
    validate_wrong_network(env.network_id, tx_body)?;

    // 11. Withdrawal addresses correct network
    validate_wrong_network_withdrawal(env.network_id, tx_body)?;

    // 12. Network ID in tx body
    validate_wrong_network_in_tx_body(env.network_id, tx_body)?;

    // 13. Transaction size within limit
    validate_max_tx_size(tx, pparams)?;

    // 14. Execution units limit
    validate_ex_units_too_big(tx, pparams)?;

    // 15. Collateral input count limit
    validate_too_many_collateral_inputs(tx_body, pparams)?;

    Ok(update_utxo_state(utxo_state, tx, pparams, cert_state))
}

/// Phase 1 validation for Babbage era (includes reference inputs)
/// Reference: Babbage/Utxo.hs:350-444
pub fn utxo_phase1_babbage(
    env: &UtxoEnv,
    utxo_state: &UTxOState,
    tx: &Tx,
) -> ValidationResult<UTxOState> {
    let tx_body = &tx.body;
    let utxo = &utxo_state.utxo;
    let pparams = &env.pparams;
    let cert_state = &env.cert_state;

    // 1. Reference inputs disjoint
    validate_disjoint_ref_inputs(tx_body)?;

    // 2-15. All Alonzo validations
    validate_validity_interval(tx_body, env.slot)?;
    validate_outside_forecast(tx, env.slot)?;
    validate_input_set_empty(tx_body)?;
    validate_fee_too_small(tx, pparams)?;
    validate_collateral(tx, utxo, pparams)?;

    // Include reference inputs in validation
    validate_bad_inputs(utxo, tx_body, true, true)?;

    validate_value_not_conserved(tx_body, utxo, pparams, cert_state)?;
    validate_output_too_small(tx_body, pparams)?;
    validate_output_too_big(tx_body, pparams)?;
    validate_output_boot_addr_attrs_too_big(tx_body)?;
    validate_wrong_network(env.network_id, tx_body)?;
    validate_wrong_network_withdrawal(env.network_id, tx_body)?;
    validate_wrong_network_in_tx_body(env.network_id, tx_body)?;
    validate_max_tx_size(tx, pparams)?;
    validate_ex_units_too_big(tx, pparams)?;
    validate_too_many_collateral_inputs(tx_body, pparams)?;

    Ok(update_utxo_state(utxo_state, tx, pparams, cert_state))
}

/// Phase 1 validation for Conway era
/// Reference: Conway/Utxo.hs (uses Babbage.utxoTransition)
///
/// Conway doesn't add new UTXO-level validations beyond Babbage, but it does:
/// 1. Use the flattened error structure (ConwayUtxoPredFailure)
/// 2. Support new governance features (handled in UTXOS/Phase 2)
/// 3. Maintain all Babbage validations
pub fn utxo_phase1_conway(
    env: &UtxoEnv,
    utxo_state: &UTxOState,
    tx: &Tx,
) -> ValidationResult<UTxOState> {
    // Conway uses the same validation logic as Babbage for Phase 1
    // The main difference is in Phase 2 (UTXOS) for governance
    utxo_phase1_babbage(env, utxo_state, tx)
}

/// Generic Phase 1 validation that dispatches to era-specific functions
/// This is a convenience function for validators that need to support multiple eras
///
/// Era Evolution Summary:
/// - **Shelley**: Foundation - basic UTXO rules with TTL
/// - **Allegra**: Validity intervals + value size limits
/// - **Mary**: Multi-asset support (reuses Allegra validation)
/// - **Alonzo**: Plutus scripts + collateral validation
/// - **Babbage**: Reference inputs + collateral return
/// - **Conway**: Governance (reuses Babbage validation for Phase 1)
pub fn utxo_phase1(
    era: &str,
    env: &UtxoEnv,
    utxo_state: &UTxOState,
    tx: &Tx,
) -> ValidationResult<UTxOState> {
    match era {
        "Shelley" => utxo_phase1_shelley(env, utxo_state, tx),
        "Allegra" => utxo_phase1_allegra(env, utxo_state, tx),
        "Mary" => utxo_phase1_mary(env, utxo_state, tx),
        "Alonzo" => utxo_phase1_alonzo(env, utxo_state, tx),
        "Babbage" => utxo_phase1_babbage(env, utxo_state, tx),
        "Conway" => utxo_phase1_conway(env, utxo_state, tx),
        _ => Err(UtxoPredFailure::BadInputsUTxO {
            bad_inputs: HashSet::new(),
        }),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_env() -> UtxoEnv {
        UtxoEnv {
            slot: 1000,
            pparams: ProtocolParams {
                min_fee_a: 44,
                min_fee_b: 155381,
                max_tx_size: 16384,
                min_utxo_value: 1_000_000,
                key_deposit: 2_000_000,
                pool_deposit: 500_000_000,
                coins_per_utxo_word: None,
                max_tx_ex_units: Some(ExUnits { mem: 14_000_000, steps: 10_000_000_000 }),
                max_collateral_inputs: Some(3),
                collateral_percentage: Some(150),
                max_value_size: Some(5000),
                coins_per_utxo_byte: None,
                protocol_version: (8, 0),
            },
            cert_state: CertState {
                registered_stake_keys: HashSet::new(),
                delegations: HashMap::new(),
                registered_pools: HashMap::new(),
                deposits: HashMap::new(),
            },
            network_id: Network::Mainnet,
        }
    }

    #[test]
    fn test_validate_input_set_empty() {
        let tx_body = TxBody {
            inputs: HashSet::new(),
            outputs: Vec::new(),
            fee: 0,
            ttl: None,
            certificates: Vec::new(),
            withdrawals: HashMap::new(),
            update: None,
            validity_interval: None,
            mint: HashMap::new(),
            collateral_inputs: HashSet::new(),
            required_signers: HashSet::new(),
            network_id: None,
            reference_inputs: HashSet::new(),
            collateral_return: None,
            total_collateral: None,
        };
        assert!(matches!(
            validate_input_set_empty(&tx_body),
            Err(UtxoPredFailure::InputSetEmptyUTxO)
        ));
    }

    #[test]
    fn test_value_addition() {
        let v1 = Value::new_ada(1_000_000);
        let v2 = Value::new_ada(2_000_000);
        let sum = v1.add(&v2);
        assert_eq!(sum.coin, 3_000_000);
    }

    #[test]
    fn test_ex_units_comparison() {
        let a = ExUnits { mem: 1000, steps: 5000 };
        let b = ExUnits { mem: 2000, steps: 10000 };
        assert!(a.lte(&b));
        assert!(!b.lte(&a));
    }
}
