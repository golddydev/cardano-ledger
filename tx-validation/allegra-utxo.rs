// Allegra Era UTXO Validation
//
// This module implements the Allegra UTXO rule. Allegra introduces:
// - ValidityInterval: replaces Shelley's simple TTL (time-to-live) with a
//   half-open interval [invalidBefore, invalidHereafter)
// - OutputTooBigUTxO: checks that the CBOR-serialized byte length of each
//   output's Value ≤ 4000 bytes (hardcoded, not a protocol parameter)
//
// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/Rules/Utxo.hs
//
// Mary era reuses this UTXO rule unchanged (same transition, same error types):
//   type instance EraRuleFailure "UTXO" MaryEra = AllegraUtxoPredFailure MaryEra
//
// ============================================================================
// ALLEGRA UTXO RULE SUMMARY
// ============================================================================
//
// Validation order (matches Haskell exactly):
//  1. validateOutsideValidityIntervalUTxO  (NEW: replaces validateTimeToLive)
//  2. validateInputSetEmptyUTxO            (from Shelley)
//  3. validateFeeTooSmallUTxO              (from Shelley)
//  4. validateBadInputsUTxO                (from Shelley)
//  5. validateWrongNetwork                 (from Shelley)
//  6. validateWrongNetworkWithdrawal       (from Shelley)
//  7. validateValueNotConservedUTxO        (from Shelley)
//  8. PPUP sub-rule                        (from Shelley)
//  9. validateOutputTooSmallUTxO           (Allegra version)
// 10. validateOutputTooBigUTxO             (NEW: hardcoded 4000 bytes)
// 11. validateOutputBootAddrAttrsTooBig    (from Shelley)
// 12. validateMaxTxSizeUTxO                (from Shelley)
// 13. Update UTxO state
//
// ============================================================================

use std::collections::{HashMap, HashSet};

// ============================================================================
// Type Definitions (shared with Shelley)
// ============================================================================

pub type SlotNo = u64;
pub type Coin = u64;
pub type TxSize = u32;

/// Validity Interval (introduced in Allegra, replaces Shelley TTL)
///
/// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/Scripts.hs:119-123
///
/// ```haskell
/// data ValidityInterval = ValidityInterval
///   { invalidBefore    :: !(StrictMaybe SlotNo)
///   , invalidHereafter :: !(StrictMaybe SlotNo)
///   }
/// ```
///
/// A half-open interval [invalidBefore, invalidHereafter):
/// - invalidBefore: first slot where the transaction is valid (inclusive)
/// - invalidHereafter: first slot where the transaction is invalid (exclusive)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidityInterval {
    pub invalid_before: Option<SlotNo>,
    pub invalid_hereafter: Option<SlotNo>,
}

impl ValidityInterval {
    pub fn from_ttl(ttl: SlotNo) -> Self {
        ValidityInterval {
            invalid_before: None,
            invalid_hereafter: Some(ttl),
        }
    }

    /// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/Scripts.hs:440-445
    ///
    /// ```haskell
    /// inInterval :: SlotNo -> ValidityInterval -> Bool
    /// inInterval _slot (ValidityInterval SNothing SNothing) = True
    /// inInterval slot  (ValidityInterval SNothing (SJust top)) = slot < top
    /// inInterval slot  (ValidityInterval (SJust bottom) SNothing) = bottom <= slot
    /// inInterval slot  (ValidityInterval (SJust bottom) (SJust top)) =
    ///   bottom <= slot && slot < top
    /// ```
    pub fn contains(&self, slot: SlotNo) -> bool {
        let after_start = match self.invalid_before {
            None => true,
            Some(start) => start <= slot,
        };
        let before_end = match self.invalid_hereafter {
            None => true,
            Some(end) => slot < end,
        };
        after_start && before_end
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Network {
    Testnet = 0,
    Mainnet = 1,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TxIn {
    pub tx_id: [u8; 32],
    pub output_index: u64,
}

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
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Credential {
    KeyHash([u8; 28]),
    ScriptHash([u8; 28]),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RewardAccount {
    pub network: Network,
    pub credential: Credential,
}

pub type PolicyId = [u8; 28];

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AssetName(pub Vec<u8>);

pub type AssetQuantity = i64;

/// Multi-asset value (Mary era and later, but the type is shared)
///
/// In Allegra, only ADA is used. Mary introduces native tokens.
/// The type is defined here for forward compatibility.
#[derive(Debug, Clone, Default)]
pub struct MaryValue {
    pub coin: Coin,
    pub multi_asset: HashMap<PolicyId, HashMap<AssetName, AssetQuantity>>,
}

impl MaryValue {
    pub fn from_coin(coin: Coin) -> Self {
        MaryValue {
            coin,
            multi_asset: HashMap::new(),
        }
    }

    pub fn is_ada_only(&self) -> bool {
        self.multi_asset.is_empty()
            || self.multi_asset.values().all(|assets| assets.is_empty())
    }

    /// Approximate CBOR-serialized byte length of this value.
    ///
    /// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/Rules/Utxo.hs:262-270
    ///
    /// The Haskell code uses `BSL.length (serialize version v)` which gives the actual
    /// CBOR-encoded byte length. This is an approximation.
    ///
    /// The unit is **bytes**. In Allegra/Mary this is compared against the hardcoded
    /// limit of 4000 bytes. In Alonzo+ it is compared against `ppMaxValSizeL`.
    pub fn cbor_serialized_size(&self) -> i64 {
        if self.is_ada_only() {
            cbor_uint_size(self.coin) as i64
        } else {
            let mut size: i64 = 2; // CBOR array(2) header
            size += cbor_uint_size(self.coin) as i64;
            size += 1; // CBOR map header
            for (_, assets) in &self.multi_asset {
                size += 30; // Policy ID: 2-byte CBOR header + 28 bytes
                size += 1;  // inner map header
                for (name, _) in assets {
                    size += 2 + name.0.len() as i64; // CBOR bytes header + asset name
                    size += 9; // quantity (worst case CBOR integer)
                }
            }
            size
        }
    }
}

/// Approximate CBOR encoding size of an unsigned integer.
fn cbor_uint_size(val: u64) -> u32 {
    if val < 24 {
        1
    } else if val <= 0xFF {
        2
    } else if val <= 0xFFFF {
        3
    } else if val <= 0xFFFF_FFFF {
        5
    } else {
        9
    }
}

/// Transaction output (Allegra uses Shelley TxOut structure with MaryValue)
#[derive(Debug, Clone)]
pub struct TxOut {
    pub address: Addr,
    pub value: MaryValue,
}

/// Allegra Protocol Parameters
///
/// Same as Shelley — Allegra does not add any protocol parameters.
/// maxValSize is hardcoded to 4000 in the Allegra UTXO rule, not a protocol param.
#[derive(Debug, Clone)]
pub struct AllegraPParams {
    pub min_fee_a: u64,
    pub min_fee_b: u64,
    pub max_tx_size: u32,
    pub min_utxo_value: Coin,
    pub key_deposit: Coin,
    pub pool_deposit: Coin,
    pub protocol_version: (u32, u32),
}

/// Certificate (simplified)
#[derive(Debug, Clone)]
pub enum Certificate {
    StakeRegistration(Credential),
    StakeDeregistration(Credential),
    StakeDelegation(Credential, [u8; 28]),
    PoolRegistration(Box<PoolParams>),
    PoolRetirement([u8; 28], u64),
}

/// Pool parameters (simplified)
#[derive(Debug, Clone)]
pub struct PoolParams {
    pub operator: [u8; 28],
    pub pledge: Coin,
    pub cost: Coin,
}

/// Update proposal (simplified)
#[derive(Debug, Clone)]
pub struct Update;

/// UTxO set
#[derive(Debug, Clone)]
pub struct UTxO {
    pub utxo: HashMap<TxIn, TxOut>,
}

/// Cert state (simplified)
#[derive(Debug, Clone)]
pub struct CertState;

/// UTxO state
#[derive(Debug, Clone)]
pub struct UTxOState {
    pub utxo: UTxO,
    pub deposited: Coin,
    pub fees: Coin,
}

/// Environment for UTXO rule
#[derive(Debug, Clone)]
pub struct UtxoEnv {
    pub slot: SlotNo,
    pub pp: AllegraPParams,
    pub cert_state: CertState,
    pub network_id: Network,
}

/// Transaction body (Allegra adds ValidityInterval)
#[derive(Debug, Clone)]
pub struct AllegraTxBody {
    pub inputs: HashSet<TxIn>,
    pub outputs: Vec<TxOut>,
    pub fee: Coin,
    pub validity_interval: ValidityInterval,
    pub withdrawals: HashMap<RewardAccount, Coin>,
    pub certificates: Vec<Certificate>,
    pub update: Option<Update>,
    pub auxiliary_data_hash: Option<[u8; 32]>,
    pub mint: HashMap<PolicyId, HashMap<AssetName, AssetQuantity>>,
}

/// Transaction (Allegra)
#[derive(Debug, Clone)]
pub struct AllegraTx {
    pub body: AllegraTxBody,
    pub auxiliary_data: Option<Vec<u8>>,
}

impl AllegraTx {
    pub fn size(&self) -> TxSize {
        0 // Simplified
    }
}

// ============================================================================
// PPUP Predicate Failure (simplified)
// ============================================================================

#[derive(Debug, Clone)]
pub enum PpupPredFailure {
    NonGenesisUpdate,
    PPUpdateWrongEpoch,
    PVCannotFollow,
}

// ============================================================================
// Allegra Error Types
// ============================================================================

/// Allegra UTXO predicate failures
///
/// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/Rules/Utxo.hs:71-93
///
/// ```haskell
/// data AllegraUtxoPredFailure era
///   = BadInputsUTxO (Set TxIn)
///   | OutsideValidityIntervalUTxO ValidityInterval SlotNo
///   | MaxTxSizeUTxO (Mismatch RelLTEQ Word32)
///   | InputSetEmptyUTxO
///   | FeeTooSmallUTxO (Mismatch RelGTEQ Coin)
///   | ValueNotConservedUTxO (Mismatch RelEQ (Value era))
///   | WrongNetwork Network (Set Addr)
///   | WrongNetworkWithdrawal Network (Set RewardAccount)
///   | OutputTooSmallUTxO [TxOut era]
///   | UpdateFailure (EraRuleFailure "PPUP" era)
///   | OutputBootAddrAttrsTooBig [TxOut era]
///   | OutputTooBigUTxO [TxOut era]
/// ```
///
/// Key changes from Shelley:
/// - `ExpiredUTxO` replaced by `OutsideValidityIntervalUTxO`
/// - `OutputTooBigUTxO` added (CBOR-serialized value size ≤ 4000 bytes)
#[derive(Debug, Clone)]
pub enum AllegraUtxoPredFailure {
    BadInputsUTxO(HashSet<TxIn>),

    /// Transaction outside its validity interval.
    ///
    /// Replaces Shelley's `ExpiredUTxO` (which only had a TTL upper bound).
    /// The interval is half-open: [invalidBefore, invalidHereafter)
    OutsideValidityIntervalUTxO {
        validity_interval: ValidityInterval,
        current_slot: SlotNo,
    },

    MaxTxSizeUTxO {
        actual_size: TxSize,
        max_size: TxSize,
    },

    InputSetEmptyUTxO,

    FeeTooSmallUTxO {
        supplied_fee: Coin,
        minimum_fee: Coin,
    },

    ValueNotConservedUTxO {
        consumed: Coin,
        produced: Coin,
    },

    WrongNetwork {
        expected_network: Network,
        wrong_addresses: HashSet<Addr>,
    },

    WrongNetworkWithdrawal {
        expected_network: Network,
        wrong_accounts: HashSet<RewardAccount>,
    },

    OutputTooSmallUTxO(Vec<TxOut>),

    UpdateFailure(PpupPredFailure),

    OutputBootAddrAttrsTooBig(Vec<TxOut>),

    /// CBOR-serialized value size exceeds 4000 bytes.
    ///
    /// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/Rules/Utxo.hs:254-270
    ///
    /// ```haskell
    /// validateOutputTooBigUTxO pp (UTxO outputs) =
    ///   failureOnNonEmpty outputsTooBig OutputTooBigUTxO
    ///   where
    ///     version = pvMajor (pp ^. ppProtocolVersionL)
    ///     maxValSize = 4000 :: Int64
    ///     outputsTooBig =
    ///       filter
    ///         ( \out ->
    ///             let v = out ^. valueTxOutL
    ///              in BSL.length (serialize version v) > maxValSize
    ///         )
    ///         (Map.elems outputs)
    /// ```
    ///
    /// The size is the CBOR-serialized byte length of the Value (not the
    /// entire TxOut). The 4000-byte limit is hardcoded — it does not come
    /// from a protocol parameter. Alonzo later makes this configurable via
    /// `ppMaxValSizeL` (mainnet value: 5000 bytes).
    OutputTooBigUTxO(Vec<TxOut>),
}

/// Shelley UTXO predicate failures (for conversion)
#[derive(Debug, Clone)]
pub enum ShelleyUtxoPredFailure {
    BadInputsUTxO(HashSet<TxIn>),
    ExpiredUTxO { supplied_ttl: SlotNo, current_slot: SlotNo },
    MaxTxSizeUTxO { actual_size: TxSize, max_size: TxSize },
    InputSetEmptyUTxO,
    FeeTooSmallUTxO { supplied_fee: Coin, minimum_fee: Coin },
    ValueNotConservedUTxO { consumed: Coin, produced: Coin },
    WrongNetwork { expected_network: Network, wrong_addresses: HashSet<Addr> },
    WrongNetworkWithdrawal { expected_network: Network, wrong_accounts: HashSet<RewardAccount> },
    OutputTooSmallUTxO(Vec<TxOut>),
    UpdateFailure(PpupPredFailure),
    OutputBootAddrAttrsTooBig(Vec<TxOut>),
}

/// Convert Shelley error to Allegra error
///
/// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/Rules/Utxo.hs:386-399
///
/// ```haskell
/// shelleyToAllegraUtxoPredFailure :: ShelleyUtxoPredFailure era -> AllegraUtxoPredFailure era
/// shelleyToAllegraUtxoPredFailure = \case
///   Shelley.BadInputsUTxO ins -> BadInputsUTxO ins
///   Shelley.ExpiredUTxO Mismatch {mismatchSupplied = ttl, mismatchExpected = current} ->
///     OutsideValidityIntervalUTxO (ValidityInterval SNothing (SJust ttl)) current
///   ...
///   Shelley.OutputBootAddrAttrsTooBig outs -> OutputTooBigUTxO outs
/// ```
///
/// Note the last case: Shelley's `OutputBootAddrAttrsTooBig` maps to
/// Allegra's `OutputTooBigUTxO`. This is because Allegra subsumes the
/// bootstrap address size check into the general output-too-big check.
impl From<ShelleyUtxoPredFailure> for AllegraUtxoPredFailure {
    fn from(err: ShelleyUtxoPredFailure) -> Self {
        match err {
            ShelleyUtxoPredFailure::BadInputsUTxO(ins) => {
                AllegraUtxoPredFailure::BadInputsUTxO(ins)
            }
            ShelleyUtxoPredFailure::ExpiredUTxO { supplied_ttl, current_slot } => {
                AllegraUtxoPredFailure::OutsideValidityIntervalUTxO {
                    validity_interval: ValidityInterval::from_ttl(supplied_ttl),
                    current_slot,
                }
            }
            ShelleyUtxoPredFailure::MaxTxSizeUTxO { actual_size, max_size } => {
                AllegraUtxoPredFailure::MaxTxSizeUTxO { actual_size, max_size }
            }
            ShelleyUtxoPredFailure::InputSetEmptyUTxO => {
                AllegraUtxoPredFailure::InputSetEmptyUTxO
            }
            ShelleyUtxoPredFailure::FeeTooSmallUTxO { supplied_fee, minimum_fee } => {
                AllegraUtxoPredFailure::FeeTooSmallUTxO { supplied_fee, minimum_fee }
            }
            ShelleyUtxoPredFailure::ValueNotConservedUTxO { consumed, produced } => {
                AllegraUtxoPredFailure::ValueNotConservedUTxO { consumed, produced }
            }
            ShelleyUtxoPredFailure::WrongNetwork { expected_network, wrong_addresses } => {
                AllegraUtxoPredFailure::WrongNetwork { expected_network, wrong_addresses }
            }
            ShelleyUtxoPredFailure::WrongNetworkWithdrawal { expected_network, wrong_accounts } => {
                AllegraUtxoPredFailure::WrongNetworkWithdrawal { expected_network, wrong_accounts }
            }
            ShelleyUtxoPredFailure::OutputTooSmallUTxO(outs) => {
                AllegraUtxoPredFailure::OutputTooSmallUTxO(outs)
            }
            ShelleyUtxoPredFailure::UpdateFailure(f) => {
                AllegraUtxoPredFailure::UpdateFailure(f)
            }
            // Shelley's OutputBootAddrAttrsTooBig maps to Allegra's OutputTooBigUTxO
            ShelleyUtxoPredFailure::OutputBootAddrAttrsTooBig(outs) => {
                AllegraUtxoPredFailure::OutputTooBigUTxO(outs)
            }
        }
    }
}

// ============================================================================
// Validation Functions
// ============================================================================

/// Maximum CBOR-serialized value size in bytes (Allegra/Mary eras)
///
/// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/Rules/Utxo.hs:263
///
/// ```haskell
/// maxValSize = 4000 :: Int64
/// ```
///
/// This is hardcoded in Allegra/Mary. Alonzo introduced `ppMaxValSizeL` as a
/// protocol parameter (mainnet value: 5000).
pub const MAX_VAL_SIZE: i64 = 4000;

/// Validate the transaction is within its validity interval.
///
/// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/Rules/Utxo.hs:242-249
///
/// ```haskell
/// validateOutsideValidityIntervalUTxO slot txb =
///   failureUnless (inInterval slot (txb ^. vldtTxBodyL)) $
///     OutsideValidityIntervalUTxO (txb ^. vldtTxBodyL) slot
/// ```
///
/// Formal: inInterval slot (txvldt txb)
///
/// Replaces Shelley's `validateTimeToLive` which only checked an upper bound (TTL).
/// Allegra adds an optional lower bound (`invalidBefore`), enabling time-locked
/// transactions and script-based time logic.
pub fn validate_outside_validity_interval(
    slot: SlotNo,
    validity_interval: &ValidityInterval,
) -> Result<(), AllegraUtxoPredFailure> {
    if validity_interval.contains(slot) {
        Ok(())
    } else {
        Err(AllegraUtxoPredFailure::OutsideValidityIntervalUTxO {
            validity_interval: *validity_interval,
            current_slot: slot,
        })
    }
}

/// Validate no output's value exceeds the maximum serialized size (4000 bytes).
///
/// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/Rules/Utxo.hs:254-270
///
/// ```haskell
/// validateOutputTooBigUTxO pp (UTxO outputs) =
///   failureOnNonEmpty outputsTooBig OutputTooBigUTxO
///   where
///     version = pvMajor (pp ^. ppProtocolVersionL)
///     maxValSize = 4000 :: Int64
///     outputsTooBig =
///       filter
///         ( \out ->
///             let v = out ^. valueTxOutL
///              in BSL.length (serialize version v) > maxValSize
///         )
///         (Map.elems outputs)
/// ```
///
/// Formal: ∀ txout ∈ txouts txb, serSize (getValue txout) ≤ 4000
///
/// The size measured is the CBOR-serialized **byte** length of the Value
/// (not the entire TxOut, not in words). The limit is hardcoded to 4000 bytes.
///
/// In the Allegra error type, the failing outputs are reported without size info.
/// Alonzo later enhanced the error to include `(actualSize, maxSize, output)`.
pub fn validate_output_too_big(
    _pp: &AllegraPParams,
    outputs: &[TxOut],
) -> Result<(), AllegraUtxoPredFailure> {
    let outputs_too_big: Vec<TxOut> = outputs
        .iter()
        .filter(|output| output.value.cbor_serialized_size() > MAX_VAL_SIZE)
        .cloned()
        .collect();

    if outputs_too_big.is_empty() {
        Ok(())
    } else {
        Err(AllegraUtxoPredFailure::OutputTooBigUTxO(outputs_too_big))
    }
}

/// Validate inputs set is not empty.
///
/// Reused from Shelley: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxo.hs:435-442
///
/// Formal: txins txb ≠ ∅
pub fn validate_input_set_empty(
    inputs: &HashSet<TxIn>,
) -> Result<(), AllegraUtxoPredFailure> {
    if !inputs.is_empty() {
        Ok(())
    } else {
        Err(AllegraUtxoPredFailure::InputSetEmptyUTxO)
    }
}

/// Validate fee is sufficient.
///
/// Reused from Shelley.
///
/// Formal: minfee pp tx ≤ txfee txb
pub fn validate_fee_too_small(
    pp: &AllegraPParams,
    tx_size: TxSize,
    fee: Coin,
) -> Result<(), AllegraUtxoPredFailure> {
    let min_fee = (tx_size as u64 * pp.min_fee_a) + pp.min_fee_b;
    if fee >= min_fee {
        Ok(())
    } else {
        Err(AllegraUtxoPredFailure::FeeTooSmallUTxO {
            supplied_fee: fee,
            minimum_fee: min_fee,
        })
    }
}

/// Validate all inputs exist in the UTxO set.
///
/// Reused from Shelley.
///
/// Formal: txins txb ⊆ dom utxo
pub fn validate_bad_inputs(
    utxo: &UTxO,
    inputs: &HashSet<TxIn>,
) -> Result<(), AllegraUtxoPredFailure> {
    let bad_inputs: HashSet<TxIn> = inputs
        .iter()
        .filter(|input| !utxo.utxo.contains_key(input))
        .cloned()
        .collect();

    if bad_inputs.is_empty() {
        Ok(())
    } else {
        Err(AllegraUtxoPredFailure::BadInputsUTxO(bad_inputs))
    }
}

/// Validate output addresses have correct network ID.
///
/// Reused from Shelley.
///
/// Formal: ∀(_ → (a, _)) ∈ txouts txb, netId a = NetworkId
pub fn validate_wrong_network(
    expected: Network,
    outputs: &[TxOut],
) -> Result<(), AllegraUtxoPredFailure> {
    let wrong: HashSet<Addr> = outputs
        .iter()
        .filter(|out| {
            out.address.get_network().map_or(false, |n| n != expected)
        })
        .map(|out| out.address.clone())
        .collect();

    if wrong.is_empty() {
        Ok(())
    } else {
        Err(AllegraUtxoPredFailure::WrongNetwork {
            expected_network: expected,
            wrong_addresses: wrong,
        })
    }
}

/// Validate withdrawal addresses have correct network ID.
///
/// Reused from Shelley.
///
/// Formal: ∀(a → ) ∈ txwdrls txb, netId a = NetworkId
pub fn validate_wrong_network_withdrawal(
    expected: Network,
    withdrawals: &HashMap<RewardAccount, Coin>,
) -> Result<(), AllegraUtxoPredFailure> {
    let wrong: HashSet<RewardAccount> = withdrawals
        .keys()
        .filter(|acct| acct.network != expected)
        .cloned()
        .collect();

    if wrong.is_empty() {
        Ok(())
    } else {
        Err(AllegraUtxoPredFailure::WrongNetworkWithdrawal {
            expected_network: expected,
            wrong_accounts: wrong,
        })
    }
}

/// Validate outputs meet minimum UTxO value.
///
/// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/Rules/Utxo.hs:275-289
///
/// ```haskell
/// validateOutputTooSmallUTxO pp (UTxO outputs) =
///   failureOnNonEmpty outputsTooSmall OutputTooSmallUTxO
///   where
///     outputsTooSmall =
///       filter
///         ( \txOut ->
///             let v = txOut ^. valueTxOutL
///              in Val.pointwise (<) v (Val.inject $ getMinCoinTxOut pp txOut)
///         )
///         (Map.elems outputs)
/// ```
///
/// Formal: ∀ txout ∈ txouts txb, getValue txout ≥ inject (getMinCoinTxOut pp txout)
///
/// In Allegra/Mary, `getMinCoinTxOut` uses `scaledMinDeposit` which scales the
/// minimum based on the value's size (supporting multi-asset outputs in Mary).
pub fn validate_output_too_small(
    pp: &AllegraPParams,
    outputs: &[TxOut],
) -> Result<(), AllegraUtxoPredFailure> {
    let too_small: Vec<TxOut> = outputs
        .iter()
        .filter(|out| out.value.coin < pp.min_utxo_value)
        .cloned()
        .collect();

    if too_small.is_empty() {
        Ok(())
    } else {
        Err(AllegraUtxoPredFailure::OutputTooSmallUTxO(too_small))
    }
}

/// Validate bootstrap address attributes are not too big.
///
/// Reused from Shelley.
///
/// Formal: ∀(a, _) ∈ txouts, a ∈ Addrbootstrap → bootstrapAttrsSize a ≤ 64
pub fn validate_output_boot_addr_attrs_too_big(
    outputs: &[TxOut],
) -> Result<(), AllegraUtxoPredFailure> {
    let too_big: Vec<TxOut> = outputs
        .iter()
        .filter(|out| {
            out.address
                .bootstrap_attrs_size()
                .map_or(false, |size| size > 64)
        })
        .cloned()
        .collect();

    if too_big.is_empty() {
        Ok(())
    } else {
        Err(AllegraUtxoPredFailure::OutputBootAddrAttrsTooBig(too_big))
    }
}

/// Validate transaction size is within limits.
///
/// Reused from Shelley.
///
/// Formal: txsize tx ≤ maxTxSize pp
pub fn validate_max_tx_size(
    pp: &AllegraPParams,
    tx_size: TxSize,
) -> Result<(), AllegraUtxoPredFailure> {
    if tx_size <= pp.max_tx_size {
        Ok(())
    } else {
        Err(AllegraUtxoPredFailure::MaxTxSizeUTxO {
            actual_size: tx_size,
            max_size: pp.max_tx_size,
        })
    }
}

// ============================================================================
// Allegra UTXO Transition Rule
// ============================================================================

/// Allegra UTXO Transition Rule
///
/// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/Rules/Utxo.hs:160-238
///
/// Mary reuses this exact transition (via the AllegraUTXO STS instance):
///   `type instance EraRule "UTXO" MaryEra = AllegraUTXO MaryEra`
///
/// Validation order matches Haskell exactly:
///  1. validateOutsideValidityIntervalUTxO  (NEW: replaces validateTimeToLive)
///  2. validateInputSetEmptyUTxO            (from Shelley)
///  3. validateFeeTooSmallUTxO              (from Shelley)
///  4. validateBadInputsUTxO                (from Shelley)
///  5. validateWrongNetwork                 (from Shelley, applied to outputsTxBodyL)
///  6. validateWrongNetworkWithdrawal       (from Shelley)
///  7. validateValueNotConservedUTxO        (from Shelley)
///  8. PPUP sub-rule                        (from Shelley)
///  9. validateOutputTooSmallUTxO           (Allegra version)
/// 10. validateOutputTooBigUTxO             (NEW: hardcoded 4000 bytes)
/// 11. validateOutputBootAddrAttrsTooBig    (from Shelley)
/// 12. validateMaxTxSizeUTxO                (from Shelley)
/// 13. Update UTxO state
pub fn allegra_utxo_transition(
    env: &UtxoEnv,
    state: &UTxOState,
    tx: &AllegraTx,
) -> Result<UTxOState, Vec<AllegraUtxoPredFailure>> {
    let mut errors: Vec<AllegraUtxoPredFailure> = Vec::new();
    let tx_body = &tx.body;

    // Step 1: inInterval slot (txvldt txb)
    if let Err(e) = validate_outside_validity_interval(env.slot, &tx_body.validity_interval) {
        errors.push(e);
    }

    // Step 2: txins txb ≠ ∅
    if let Err(e) = validate_input_set_empty(&tx_body.inputs) {
        errors.push(e);
    }

    // Step 3: minfee pp tx ≤ txfee txb
    if let Err(e) = validate_fee_too_small(&env.pp, tx.size(), tx_body.fee) {
        errors.push(e);
    }

    // Step 4: txins txb ⊆ dom utxo
    if let Err(e) = validate_bad_inputs(&state.utxo, &tx_body.inputs) {
        errors.push(e);
    }

    // Step 5: ∀(_ → (a, _)) ∈ txouts txb, netId a = NetworkId
    if let Err(e) = validate_wrong_network(env.network_id, &tx_body.outputs) {
        errors.push(e);
    }

    // Step 6: ∀(a → ) ∈ txwdrls txb, netId a = NetworkId
    if let Err(e) = validate_wrong_network_withdrawal(env.network_id, &tx_body.withdrawals) {
        errors.push(e);
    }

    // Step 7: consumed pp utxo txb = produced pp poolParams txb
    // Simplified — full implementation would check value conservation
    // runTest $ Shelley.validateValueNotConservedUTxO pp utxo certState txBody

    // Step 8: PPUP sub-rule
    // ppup' <- trans @(EraRule "PPUP" era) $ TRC (...)

    // Step 9: ∀ txout ∈ txouts txb, getValue txout ≥ inject (getMinCoinTxOut pp txout)
    if let Err(e) = validate_output_too_small(&env.pp, &tx_body.outputs) {
        errors.push(e);
    }

    // Step 10: ∀ txout ∈ txouts txb, serSize (getValue txout) ≤ 4000
    if let Err(e) = validate_output_too_big(&env.pp, &tx_body.outputs) {
        errors.push(e);
    }

    // Step 11: ∀(a, _) ∈ txouts, a ∈ Addrbootstrap → bootstrapAttrsSize a ≤ 64
    if let Err(e) = validate_output_boot_addr_attrs_too_big(&tx_body.outputs) {
        errors.push(e);
    }

    // Step 12: txsize tx ≤ maxTxSize pp
    if let Err(e) = validate_max_tx_size(&env.pp, tx.size()) {
        errors.push(e);
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    // Step 13: Update UTxO state
    let mut new_utxo = state.utxo.utxo.clone();

    // Remove spent inputs
    for input in &tx_body.inputs {
        new_utxo.remove(input);
    }

    // Add new outputs
    let tx_id = [0u8; 32]; // Simplified — real impl uses hash of tx body
    for (idx, output) in tx_body.outputs.iter().enumerate() {
        new_utxo.insert(
            TxIn {
                tx_id,
                output_index: idx as u64,
            },
            output.clone(),
        );
    }

    Ok(UTxOState {
        utxo: UTxO { utxo: new_utxo },
        deposited: state.deposited,
        fees: state.fees + tx_body.fee,
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn default_pp() -> AllegraPParams {
        AllegraPParams {
            min_fee_a: 44,
            min_fee_b: 155381,
            max_tx_size: 16384,
            min_utxo_value: 1_000_000,
            key_deposit: 2_000_000,
            pool_deposit: 500_000_000,
            protocol_version: (3, 0), // Allegra
        }
    }

    fn make_ada_output(value: Coin) -> TxOut {
        TxOut {
            address: Addr::Enterprise {
                network: Network::Mainnet,
                payment_credential: Credential::KeyHash([0u8; 28]),
            },
            value: MaryValue::from_coin(value),
        }
    }

    fn make_multi_asset_output(coin: Coin, num_policies: usize, assets_per_policy: usize) -> TxOut {
        let mut multi_asset = HashMap::new();
        for i in 0..num_policies {
            let mut policy_id = [0u8; 28];
            policy_id[0] = i as u8;
            let mut assets = HashMap::new();
            for j in 0..assets_per_policy {
                let name = AssetName(format!("asset_{}_{}__padding_to_make_it_larger", i, j).into_bytes());
                assets.insert(name, 1_000_000);
            }
            multi_asset.insert(policy_id, assets);
        }
        TxOut {
            address: Addr::Enterprise {
                network: Network::Mainnet,
                payment_credential: Credential::KeyHash([0u8; 28]),
            },
            value: MaryValue { coin, multi_asset },
        }
    }

    // ========================================================================
    // Validity Interval Tests
    // ========================================================================

    #[test]
    fn test_validity_interval_contains() {
        // No bounds — always valid
        let always = ValidityInterval { invalid_before: None, invalid_hereafter: None };
        assert!(always.contains(0));
        assert!(always.contains(u64::MAX));

        // Only lower bound: [100, +∞)
        let from_100 = ValidityInterval { invalid_before: Some(100), invalid_hereafter: None };
        assert!(!from_100.contains(99));
        assert!(from_100.contains(100));
        assert!(from_100.contains(200));

        // Only upper bound: (-∞, 200)
        let to_200 = ValidityInterval { invalid_before: None, invalid_hereafter: Some(200) };
        assert!(to_200.contains(0));
        assert!(to_200.contains(199));
        assert!(!to_200.contains(200)); // exclusive

        // Both bounds: [100, 200)
        let bounded = ValidityInterval { invalid_before: Some(100), invalid_hereafter: Some(200) };
        assert!(!bounded.contains(99));
        assert!(bounded.contains(100));  // inclusive
        assert!(bounded.contains(150));
        assert!(!bounded.contains(200)); // exclusive
    }

    #[test]
    fn test_validate_outside_validity_interval() {
        let vi = ValidityInterval { invalid_before: Some(100), invalid_hereafter: Some(200) };
        assert!(validate_outside_validity_interval(150, &vi).is_ok());
        assert!(validate_outside_validity_interval(50, &vi).is_err());
        assert!(validate_outside_validity_interval(200, &vi).is_err());
    }

    // ========================================================================
    // OutputTooBigUTxO Tests
    // ========================================================================

    #[test]
    fn test_validate_output_too_big_ada_only() {
        let pp = default_pp();
        let outputs = vec![make_ada_output(2_000_000)];
        assert!(validate_output_too_big(&pp, &outputs).is_ok());
    }

    #[test]
    fn test_validate_output_too_big_large_multi_asset() {
        let pp = default_pp();
        // Many policies + assets should exceed 4000 bytes
        let outputs = vec![make_multi_asset_output(5_000_000, 50, 20)];
        let result = validate_output_too_big(&pp, &outputs);
        assert!(matches!(result, Err(AllegraUtxoPredFailure::OutputTooBigUTxO(_))));
    }

    #[test]
    fn test_validate_output_too_big_size_is_bytes() {
        // MAX_VAL_SIZE is 4000 BYTES, not words
        assert_eq!(MAX_VAL_SIZE, 4000);

        // An ada-only value is a few bytes (CBOR uint), always fits
        let ada_value = MaryValue::from_coin(1_000_000);
        assert!(ada_value.cbor_serialized_size() < MAX_VAL_SIZE);
    }

    // ========================================================================
    // Input Validation Tests
    // ========================================================================

    #[test]
    fn test_validate_input_set_empty() {
        let empty: HashSet<TxIn> = HashSet::new();
        assert!(matches!(
            validate_input_set_empty(&empty),
            Err(AllegraUtxoPredFailure::InputSetEmptyUTxO)
        ));

        let mut non_empty = HashSet::new();
        non_empty.insert(TxIn { tx_id: [1u8; 32], output_index: 0 });
        assert!(validate_input_set_empty(&non_empty).is_ok());
    }

    // ========================================================================
    // Shelley → Allegra Error Conversion Tests
    // ========================================================================

    #[test]
    fn test_shelley_to_allegra_expired_conversion() {
        let shelley_err = ShelleyUtxoPredFailure::ExpiredUTxO {
            supplied_ttl: 500,
            current_slot: 600,
        };
        let allegra_err: AllegraUtxoPredFailure = shelley_err.into();

        match allegra_err {
            AllegraUtxoPredFailure::OutsideValidityIntervalUTxO { validity_interval, current_slot } => {
                assert_eq!(validity_interval.invalid_before, None);
                assert_eq!(validity_interval.invalid_hereafter, Some(500));
                assert_eq!(current_slot, 600);
            }
            _ => panic!("Expected OutsideValidityIntervalUTxO"),
        }
    }

    #[test]
    fn test_shelley_boot_addr_maps_to_output_too_big() {
        // Shelley's OutputBootAddrAttrsTooBig maps to Allegra's OutputTooBigUTxO
        let shelley_err = ShelleyUtxoPredFailure::OutputBootAddrAttrsTooBig(vec![]);
        let allegra_err: AllegraUtxoPredFailure = shelley_err.into();
        assert!(matches!(allegra_err, AllegraUtxoPredFailure::OutputTooBigUTxO(_)));
    }
}
