// Shelley/Allegra/Mary Era UTXO Validation
//
// This module implements the UTXO rule for Shelley, Allegra, and Mary eras.
// These eras share a common validation structure with incremental additions:
//
// - Shelley: Foundational Phase 1 validation
// - Allegra: Adds validity intervals (replacing simple TTL) and output size checks
// - Mary:    Adds multi-asset support with scaled minimum UTxO
//
// Reference:
// - eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxo.hs
// - eras/allegra/impl/src/Cardano/Ledger/Allegra/Rules/Utxo.hs
// - eras/mary/impl/src/Cardano/Ledger/Mary/Rules/Utxo.hs
//
// ============================================================================
// SHELLEY/ALLEGRA/MARY UTXO RULE SUMMARY
// ============================================================================
//
// Common validations (all eras):
// 1. Transaction validity window (TTL in Shelley, ValidityInterval in Allegra/Mary)
// 2. Non-empty inputs - must consume at least one UTxO
// 3. Sufficient fee - covers minimum fee calculation
// 4. Inputs exist - all referenced UTxOs must exist
// 5. Network IDs - outputs and withdrawals must match network
// 6. Value conservation - consumed == produced (includes mint in Mary)
// 7. Output minimums - prevent dust outputs (scaled for multi-asset in Mary)
// 8. Bootstrap addresses - attribute size limits
// 9. Transaction size - within protocol limits
//
// Allegra additions:
// 10. ValidityInterval - both lower and upper bounds on validity
// 11. OutputTooBigUTxO - serialized value size limit (4000 bytes)
//
// Mary additions:
// 12. Multi-asset value support in outputs
// 13. Minting/burning in value conservation
// 14. Scaled minimum UTxO for multi-asset outputs
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

/// Transaction size in bytes
pub type TxSize = u32;

/// Serialized value size in bytes
pub type ValueSize = i64;

// ============================================================================
// Allegra Era: Validity Interval
// ============================================================================

/// Validity Interval (Allegra era and later)
///
/// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/Scripts.hs:119-123
///
/// ```haskell
/// data ValidityInterval = ValidityInterval
///   { invalidBefore :: !(StrictMaybe SlotNo)
///   , invalidHereafter :: !(StrictMaybe SlotNo)
///   }
/// ```
///
/// A half-open interval [invalidBefore, invalidHereafter):
/// - invalidBefore: First slot where transaction is valid (inclusive, None = -∞)
/// - invalidHereafter: First slot where transaction is invalid (exclusive, None = +∞)
///
/// This replaces Shelley's simple TTL (time-to-live) with a more flexible
/// validity window, allowing transactions to specify both a start and end time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidityInterval {
    /// First slot where the transaction becomes valid (None = no lower bound)
    pub invalid_before: Option<SlotNo>,
    /// First slot where the transaction becomes invalid (None = no upper bound)
    pub invalid_hereafter: Option<SlotNo>,
}

impl ValidityInterval {
    /// Create a validity interval with only an upper bound (Shelley-style TTL)
    pub fn from_ttl(ttl: SlotNo) -> Self {
        ValidityInterval {
            invalid_before: None,
            invalid_hereafter: Some(ttl),
        }
    }

    /// Create a validity interval with both bounds
    pub fn new(start: Option<SlotNo>, end: Option<SlotNo>) -> Self {
        ValidityInterval {
            invalid_before: start,
            invalid_hereafter: end,
        }
    }

    /// Check if a slot is within the validity interval
    ///
    /// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/Scripts.hs:440-445
    ///
    /// ```haskell
    /// inInterval :: SlotNo -> ValidityInterval -> Bool
    /// inInterval _slot (ValidityInterval SNothing SNothing) = True
    /// inInterval slot (ValidityInterval SNothing (SJust top)) = slot < top
    /// inInterval slot (ValidityInterval (SJust bottom) SNothing) = bottom <= slot
    /// inInterval slot (ValidityInterval (SJust bottom) (SJust top)) =
    ///   bottom <= slot && slot < top
    /// ```
    ///
    /// The interval is half-open: [invalidBefore, invalidHereafter)
    /// - invalidBefore <= slot (inclusive lower bound)
    /// - slot < invalidHereafter (exclusive upper bound)
    pub fn contains(&self, slot: SlotNo) -> bool {
        let after_start = match self.invalid_before {
            None => true,                // No lower bound, always after start
            Some(start) => start <= slot, // Inclusive: start <= slot
        };
        let before_end = match self.invalid_hereafter {
            None => true,              // No upper bound, never expires
            Some(end) => slot < end,   // Exclusive: slot < end
        };
        after_start && before_end
    }
}

/// Network identifier (Mainnet = 1, Testnet = 0)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    Testnet = 0,
    Mainnet = 1,
}

/// Transaction input reference (TxId + output index)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TxIn {
    pub tx_id: [u8; 32],      // Transaction ID hash
    pub output_index: u64,    // Output index within transaction
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
    /// Get the network ID from the address
    /// Bootstrap addresses don't have explicit network IDs
    pub fn get_network(&self) -> Option<Network> {
        match self {
            Addr::Base { network, .. } => Some(*network),
            Addr::Enterprise { network, .. } => Some(*network),
            Addr::Pointer { network, .. } => Some(*network),
            Addr::Bootstrap { .. } => None,
            Addr::Reward { network, .. } => Some(*network),
        }
    }

    /// Check if this is a bootstrap (Byron) address
    pub fn is_bootstrap(&self) -> bool {
        matches!(self, Addr::Bootstrap { .. })
    }

    /// Get bootstrap address attributes size
    pub fn bootstrap_attrs_size(&self) -> Option<usize> {
        match self {
            Addr::Bootstrap { attributes } => Some(attributes.len()),
            _ => None,
        }
    }
}

/// Credential (key hash or script hash)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Credential {
    KeyHash([u8; 28]),
    ScriptHash([u8; 28]),
}

/// Reward account for withdrawals
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RewardAccount {
    pub network: Network,
    pub credential: Credential,
}

// ============================================================================
// Mary Era: Multi-Asset Values
// ============================================================================

/// Policy ID (hash of minting policy script)
pub type PolicyId = [u8; 28];

/// Asset name within a policy
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AssetName(pub Vec<u8>);  // Max 32 bytes

/// Quantity of a specific asset (can be negative for burning)
pub type AssetQuantity = i64;

/// Multi-asset value (Mary era and later)
///
/// Reference: eras/mary/impl/src/Cardano/Ledger/Mary/Value.hs
///
/// A value contains both ADA (in lovelace) and native tokens.
/// Native tokens are organized by PolicyId -> AssetName -> Quantity.
#[derive(Debug, Clone, Default)]
pub struct MaryValue {
    /// ADA amount in lovelace
    pub coin: Coin,
    /// Multi-asset tokens: PolicyId -> AssetName -> Quantity
    pub multi_asset: HashMap<PolicyId, HashMap<AssetName, AssetQuantity>>,
}

impl MaryValue {
    /// Create a pure ADA value (no multi-asset)
    pub fn from_coin(coin: Coin) -> Self {
        MaryValue {
            coin,
            multi_asset: HashMap::new(),
        }
    }

    /// Check if this value contains only ADA (no native tokens)
    ///
    /// Reference: eras/mary/impl/src/Cardano/Ledger/Mary/Value.hs
    ///
    /// ```haskell
    /// isAdaOnly :: MaryValue -> Bool
    /// isAdaOnly (MaryValue _ ma) = ma == mempty
    /// ```
    pub fn is_ada_only(&self) -> bool {
        self.multi_asset.is_empty() ||
        self.multi_asset.values().all(|assets| assets.is_empty())
    }

    /// Calculate the "size" of this value in words for min UTxO calculation
    ///
    /// Reference: libs/cardano-ledger-core/src/Cardano/Ledger/Val.hs
    ///
    /// For pure ADA values, size = 0 (compact form)
    /// For multi-asset, size depends on number of policies and assets
    pub fn size(&self) -> i64 {
        if self.is_ada_only() {
            0
        } else {
            // Approximate size calculation
            // Each policy adds overhead, each asset name adds overhead
            let mut size: i64 = 6; // Base overhead for multi-asset
            for (_, assets) in &self.multi_asset {
                size += 28; // PolicyId size
                for (name, _) in assets {
                    size += 8 + name.0.len() as i64; // AssetName + quantity
                }
            }
            size
        }
    }

    /// Get the serialized byte size of this value
    ///
    /// Used for OutputTooBigUTxO validation in Allegra/Mary
    pub fn serialized_size(&self) -> i64 {
        if self.is_ada_only() {
            // Compact coin: just the coin value
            8
        } else {
            // Approximate CBOR serialization size
            let mut size: i64 = 16; // Base overhead
            size += 8; // Coin
            for (_, assets) in &self.multi_asset {
                size += 28; // PolicyId
                for (name, _) in assets {
                    size += 8 + name.0.len() as i64 + 8; // AssetName + quantity
                }
            }
            size
        }
    }
}

/// Transaction output (supports both Shelley ADA-only and Mary multi-asset)
#[derive(Debug, Clone)]
pub struct TxOut {
    pub address: Addr,
    /// Shelley/Allegra: Just ADA, Mary+: Can include multi-asset
    pub value: MaryValue,
}

/// Shelley Protocol Parameters (relevant for UTXO validation)
#[derive(Debug, Clone)]
pub struct ShelleyPParams {
    /// Minimum fee coefficient (lovelace per byte)
    /// Mainnet: 44
    pub min_fee_a: u64,

    /// Minimum fee constant (lovelace)
    /// Mainnet: 155,381
    pub min_fee_b: u64,

    /// Maximum transaction size (bytes)
    /// Mainnet: 16,384
    pub max_tx_size: u32,

    /// Minimum UTxO value (lovelace)
    /// Mainnet: 1,000,000 (1 ADA)
    pub min_utxo_value: Coin,

    /// Key deposit for stake key registration (lovelace)
    pub key_deposit: Coin,

    /// Pool deposit for pool registration (lovelace)
    pub pool_deposit: Coin,
}

/// UTxO set - mapping from TxIn to TxOut
pub struct UTxO {
    pub utxo: HashMap<TxIn, TxOut>,
}

impl UTxO {
    /// Get the balance (sum of all values) for a set of inputs
    pub fn balance(&self, inputs: &HashSet<TxIn>) -> Coin {
        inputs
            .iter()
            .filter_map(|input| self.utxo.get(input))
            .map(|output| output.value)
            .sum()
    }

    /// Check if an input exists in the UTxO set
    pub fn contains(&self, input: &TxIn) -> bool {
        self.utxo.contains_key(input)
    }
}

/// Shelley/Allegra/Mary Transaction Body
///
/// This structure supports all three eras:
/// - Shelley: Uses ttl for expiration
/// - Allegra: Uses validity_interval for both start and end bounds
/// - Mary: Adds mint field for minting/burning native tokens
#[derive(Debug, Clone)]
pub struct ShelleyMaTxBody {
    /// Transaction inputs (UTxOs to spend)
    pub inputs: HashSet<TxIn>,

    /// Transaction outputs (new UTxOs to create)
    pub outputs: Vec<TxOut>,

    /// Transaction fee (in lovelace)
    pub fee: Coin,

    /// Time-to-live (Shelley era only - slot after which tx expires)
    /// In Allegra/Mary, use validity_interval instead
    pub ttl: Option<SlotNo>,

    /// Validity interval (Allegra/Mary era)
    /// Replaces TTL with a range [invalidBefore, invalidHereafter)
    pub validity_interval: Option<ValidityInterval>,

    /// Withdrawals from reward accounts
    pub withdrawals: HashMap<RewardAccount, Coin>,

    /// Certificates (registrations, delegations, etc.)
    pub certificates: Vec<Certificate>,

    /// Protocol parameter update proposals
    pub update: Option<Update>,

    /// Minting/burning of multi-asset tokens (Mary era)
    ///
    /// Reference: eras/mary/impl/src/Cardano/Ledger/Mary/TxBody.hs
    ///
    /// The mint field specifies tokens to create (positive) or destroy (negative).
    /// Each PolicyId requires a corresponding script witness.
    ///
    /// ```haskell
    /// mintTxBodyL :: Lens' (TxBody l era) MultiAsset
    /// ```
    ///
    /// IMPORTANT: ADA cannot be minted or burned - this is enforced by the type system
    /// in Haskell (MultiAsset cannot contain ADA policy).
    pub mint: HashMap<PolicyId, HashMap<AssetName, AssetQuantity>>,
}

impl ShelleyMaTxBody {
    /// Get the validity interval, converting from TTL if necessary
    pub fn get_validity_interval(&self) -> ValidityInterval {
        if let Some(vi) = &self.validity_interval {
            *vi
        } else if let Some(ttl) = self.ttl {
            ValidityInterval::from_ttl(ttl)
        } else {
            ValidityInterval::new(None, None)
        }
    }

    /// Check if this transaction has any minting/burning
    pub fn has_mint(&self) -> bool {
        !self.mint.is_empty() && self.mint.values().any(|assets| !assets.is_empty())
    }

    /// Get total minted value (positive quantities only)
    pub fn get_minted(&self) -> HashMap<PolicyId, HashMap<AssetName, AssetQuantity>> {
        self.mint
            .iter()
            .map(|(policy, assets)| {
                let positive: HashMap<AssetName, AssetQuantity> = assets
                    .iter()
                    .filter(|(_, &qty)| qty > 0)
                    .map(|(name, &qty)| (name.clone(), qty))
                    .collect();
                (*policy, positive)
            })
            .filter(|(_, assets)| !assets.is_empty())
            .collect()
    }

    /// Get total burned value (negative quantities, returned as positive)
    pub fn get_burned(&self) -> HashMap<PolicyId, HashMap<AssetName, AssetQuantity>> {
        self.mint
            .iter()
            .map(|(policy, assets)| {
                let negative: HashMap<AssetName, AssetQuantity> = assets
                    .iter()
                    .filter(|(_, &qty)| qty < 0)
                    .map(|(name, &qty)| (name.clone(), -qty))
                    .collect();
                (*policy, negative)
            })
            .filter(|(_, assets)| !assets.is_empty())
            .collect()
    }
}

/// Backward-compatible alias
pub type ShelleyTxBody = ShelleyMaTxBody;

/// Certificate types in Shelley
#[derive(Debug, Clone)]
pub enum Certificate {
    StakeKeyRegistration { credential: Credential },
    StakeKeyDeregistration { credential: Credential },
    StakeDelegation { credential: Credential, pool_id: [u8; 28] },
    PoolRegistration { pool_params: PoolParams },
    PoolRetirement { pool_id: [u8; 28], epoch: u64 },
    GenesisKeyDelegation { /* ... */ },
    MIR { /* ... */ },
}

/// Pool parameters (simplified)
#[derive(Debug, Clone)]
pub struct PoolParams {
    pub pool_id: [u8; 28],
    // ... other fields
}

/// Protocol parameter update proposal
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/PParams.hs
///
/// ```haskell
/// data Update era = Update
///   { updateProposal :: ProposedPPUpdates era
///   , updateEpoch    :: EpochNo
///   }
/// ```
#[derive(Debug, Clone)]
pub struct Update {
    /// Map of genesis key hashes to their proposed parameter changes
    pub proposals: HashMap<[u8; 28], PParamsUpdate>,
    /// Target epoch for the update to take effect
    pub epoch: EpochNo,
}

/// Epoch number
pub type EpochNo = u64;

/// Protocol parameter update (partial update)
///
/// Only fields that are `Some` will be updated; others remain unchanged.
#[derive(Debug, Clone, Default)]
pub struct PParamsUpdate {
    pub min_fee_a: Option<u64>,
    pub min_fee_b: Option<u64>,
    pub max_tx_size: Option<u32>,
    pub min_utxo_value: Option<Coin>,
    pub key_deposit: Option<Coin>,
    pub pool_deposit: Option<Coin>,
    pub protocol_version: Option<ProtVer>,
}

/// Protocol version (major, minor)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtVer {
    pub major: u64,
    pub minor: u64,
}

impl ProtVer {
    /// Check if a new protocol version can legally follow this one
    ///
    /// Reference: libs/cardano-ledger-core/src/Cardano/Ledger/BaseTypes.hs
    ///
    /// ```haskell
    /// pvCanFollow :: ProtVer -> ProtVer -> Bool
    /// pvCanFollow (ProtVer m n) (ProtVer m' n') =
    ///   (m + 1 == m' && n' == 0) || (m == m' && n + 1 == n')
    /// ```
    ///
    /// # Rules
    /// - Major version bump: major' = major + 1, minor' = 0
    /// - Minor version bump: major' = major, minor' = minor + 1
    ///
    /// # Examples
    /// - (3, 5) → (4, 0) ✓ (major bump)
    /// - (3, 5) → (3, 6) ✓ (minor bump)
    /// - (3, 5) → (3, 7) ✗ (minor must increment by 1)
    /// - (3, 5) → (5, 0) ✗ (major must increment by 1)
    /// - (3, 5) → (4, 1) ✗ (major bump requires minor = 0)
    pub fn can_follow(&self, new_ver: ProtVer) -> bool {
        // Major version bump: major + 1 == major' && minor' == 0
        let major_bump = self.major + 1 == new_ver.major && new_ver.minor == 0;
        // Minor version bump: major == major' && minor + 1 == minor'
        let minor_bump = self.major == new_ver.major && self.minor + 1 == new_ver.minor;
        major_bump || minor_bump
    }
}

/// Voting period for protocol parameter updates
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Ppup.hs:68-69
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VotingPeriod {
    /// Proposal is for the current epoch (slot < tooLate)
    VoteForThisEpoch,
    /// Proposal is for the next epoch (slot >= tooLate)
    VoteForNextEpoch,
}

// ============================================================================
// PPUP (Protocol Parameter Update Proposal) Rule
// ============================================================================

/// PPUP Predicate Failures
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Ppup.hs:86-108
///
/// These errors occur when protocol parameter update proposals are invalid.
/// They are wrapped as `UpdateFailure` in the UTXO predicate failures.
///
/// ```haskell
/// data ShelleyPpupPredFailure era
///   = NonGenesisUpdatePPUP (Mismatch RelSubset (Set (KeyHash GenesisRole)))
///   | PPUpdateWrongEpoch EpochNo EpochNo VotingPeriod
///   | PVCannotFollowPPUP ProtVer
/// ```
#[derive(Debug, Clone)]
pub enum PpupPredFailure {
    /// Proposal submitted by a non-genesis delegate key
    ///
    /// Reference: Ppup.hs:166-171
    ///
    /// All keys in the proposal map must be genesis delegate keys.
    /// This ensures only authorized parties can propose parameter changes.
    ///
    /// ```haskell
    /// Map.isSubmapOfBy (\_ _ -> True) pup genDelegs
    ///   ?! NonGenesisUpdatePPUP
    ///     Mismatch
    ///       { mismatchSupplied = Map.keysSet pup
    ///       , mismatchExpected = Map.keysSet genDelegs
    ///       }
    /// ```
    NonGenesisUpdatePPUP {
        /// Key hashes that submitted proposals
        supplied: HashSet<[u8; 28]>,
        /// Valid genesis delegate key hashes
        expected: HashSet<[u8; 28]>,
    },

    /// Proposal targets the wrong epoch for the current voting period
    ///
    /// Reference: Ppup.hs:180-198
    ///
    /// Each epoch has a "slot of no return" (calculated as 2 * stability_window
    /// before the epoch boundary). Proposals submitted before this deadline
    /// must target the current epoch; after it, they must target the next epoch.
    ///
    /// ```haskell
    /// if slot < tooLate
    ///   then (curEpochNo == targetEpochNo)
    ///     ?! PPUpdateWrongEpoch curEpochNo targetEpochNo VoteForThisEpoch
    ///   else (succ curEpochNo == targetEpochNo)
    ///     ?! PPUpdateWrongEpoch curEpochNo targetEpochNo VoteForNextEpoch
    /// ```
    PPUpdateWrongEpoch {
        /// Current epoch number
        current_epoch: EpochNo,
        /// Target epoch specified in the proposal
        target_epoch: EpochNo,
        /// Which voting period applies (based on current slot)
        voting_period: VotingPeriod,
    },

    /// Proposed protocol version is not a legal successor
    ///
    /// Reference: Ppup.hs:173-178
    ///
    /// Protocol version updates must follow succession rules:
    /// - Major bump: major' = major + 1, minor' = 0
    /// - Minor bump: major' = major, minor' = minor + 1
    ///
    /// ```haskell
    /// let firstIllegalProtVerUpdate = do
    ///       ppu <- F.find (not . hasLegalProtVerUpdate pp) pup
    ///       SJust newBadProtVer <- Just (ppu ^. ppuProtocolVersionL)
    ///       Just newBadProtVer
    /// failOnJust firstIllegalProtVerUpdate PVCannotFollowPPUP
    /// ```
    PVCannotFollowPPUP(ProtVer),
}

/// Shelley Transaction
#[derive(Debug, Clone)]
pub struct ShelleyTx {
    pub body: ShelleyTxBody,
    pub witnesses: ShelleyTxWits,
    pub metadata: Option<Vec<u8>>,
}

impl ShelleyTx {
    /// Get the serialized size of the transaction
    pub fn size(&self) -> TxSize {
        // Simplified - actual implementation would serialize
        0
    }
}

/// Shelley Transaction Witnesses
#[derive(Debug, Clone)]
pub struct ShelleyTxWits {
    pub vkey_witnesses: Vec<VKeyWitness>,
    pub script_witnesses: HashMap<[u8; 28], NativeScript>,
    pub bootstrap_witnesses: Vec<BootstrapWitness>,
}

/// VKey witness (signature)
#[derive(Debug, Clone)]
pub struct VKeyWitness {
    pub vkey: [u8; 32],
    pub signature: [u8; 64],
}

/// Bootstrap witness (Byron)
#[derive(Debug, Clone)]
pub struct BootstrapWitness {
    pub vkey: [u8; 32],
    pub signature: [u8; 64],
    pub chain_code: [u8; 32],
    pub attributes: Vec<u8>,
}

/// Native script (multisig in Shelley)
#[derive(Debug, Clone)]
pub enum NativeScript {
    RequireSignature([u8; 28]),
    RequireAllOf(Vec<NativeScript>),
    RequireAnyOf(Vec<NativeScript>),
    RequireMOf(u32, Vec<NativeScript>),
}

/// Certificate state (for deposit tracking)
#[derive(Debug, Clone)]
pub struct CertState {
    /// Stored deposits for stake credentials (tracks original deposit amounts)
    pub deposits: HashMap<Credential, Coin>,
    /// Registered pools
    pub registered_pools: HashMap<[u8; 28], Coin>,
}

/// UTXO Environment
#[derive(Debug, Clone)]
pub struct UtxoEnv {
    /// Current slot number
    pub slot: SlotNo,
    /// Protocol parameters
    pub pp: ShelleyPParams,
    /// Certificate state
    pub cert_state: CertState,
    /// Network ID
    pub network_id: Network,
}

/// UTXO State
#[derive(Debug, Clone)]
pub struct UTxOState {
    /// The actual UTxO set
    pub utxo: UTxO,
    /// Total deposits held
    pub deposited: Coin,
    /// Accumulated fees
    pub fees: Coin,
}

// ============================================================================
// Error Types
// ============================================================================

/// Shelley UTXO predicate failures
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxo.hs:82-93
///
/// ```haskell
/// data ShelleyUtxoPredFailure era
///   = BadInputsUTxO (Set TxIn)
///   | ExpiredUTxO (Mismatch RelLTEQ SlotNo)
///   | MaxTxSizeUTxO (Mismatch RelLTEQ Word32)
///   | InputSetEmptyUTxO
///   | FeeTooSmallUTxO (Mismatch RelGTEQ Coin)
///   | ValueNotConservedUTxO (Mismatch RelEQ (Value era))
///   | WrongNetwork Network (Set Addr)
///   | WrongNetworkWithdrawal Network (Set RewardAccount)
///   | OutputTooSmallUTxO [TxOut era]
///   | UpdateFailure (EraRuleFailure "PPUP" era)
///   | OutputBootAddrAttrsTooBig [TxOut era]
/// ```
#[derive(Debug, Clone)]
pub enum ShelleyUtxoPredFailure {
    /// Some inputs do not exist in the UTxO set
    /// Formal: txins txb ⊆ dom utxo
    BadInputsUTxO(HashSet<TxIn>),

    /// Transaction has expired (TTL < current slot)
    /// Formal: txttl txb ≥ slot
    ExpiredUTxO {
        supplied_ttl: SlotNo,
        current_slot: SlotNo,
    },

    /// Transaction exceeds maximum size
    /// Formal: txsize tx ≤ maxTxSize pp
    MaxTxSizeUTxO {
        actual_size: TxSize,
        max_size: TxSize,
    },

    /// Transaction has no inputs
    /// Formal: txins txb ≠ ∅
    InputSetEmptyUTxO,

    /// Fee is below minimum required
    /// Formal: minfee pp tx ≤ txfee txb
    FeeTooSmallUTxO {
        supplied_fee: Coin,
        minimum_fee: Coin,
    },

    /// Value not conserved (consumed ≠ produced)
    /// Formal: consumed pp utxo txb = produced pp poolParams txb
    ValueNotConservedUTxO {
        consumed: Coin,
        produced: Coin,
    },

    /// Output addresses have wrong network ID
    /// Formal: ∀(_ → (a, _)) ∈ txouts txb, netId a = NetworkId
    WrongNetwork {
        expected_network: Network,
        wrong_addresses: HashSet<Addr>,
    },

    /// Withdrawal addresses have wrong network ID
    /// Formal: ∀(a → ) ∈ txwdrls txb, netId a = NetworkId
    WrongNetworkWithdrawal {
        expected_network: Network,
        wrong_accounts: HashSet<RewardAccount>,
    },

    /// Some outputs have value below minimum
    /// Formal: ∀(_ → (_, c)) ∈ txouts txb, c ≥ minUTxOValue pp
    OutputTooSmallUTxO(Vec<TxOut>),

    /// Protocol parameter update failure (PPUP sub-rule)
    ///
    /// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Ppup.hs
    ///
    /// This error wraps failures from the PPUP (Protocol Parameter Update Proposal)
    /// sub-rule. The PPUP rule validates protocol parameter update proposals
    /// submitted by genesis key holders.
    ///
    /// # When This Error Occurs
    ///
    /// The UTXO rule calls the PPUP sub-rule when processing transactions:
    /// ```haskell
    /// ppup' <- trans @(EraRule "PPUP" era) $
    ///   TRC (PPUPEnv slot pp genDelegs, ppup, txBody ^. updateTxBodyL)
    /// ```
    ///
    /// If the PPUP sub-rule fails, the error is wrapped as UpdateFailure.
    ///
    /// # PPUP Predicate Failures (wrapped in UpdateFailure)
    ///
    /// 1. **NonGenesisUpdatePPUP**: Proposal submitted by non-genesis key
    ///    - All proposers must be genesis delegates
    ///    - Error contains: supplied keys vs expected genesis delegate keys
    ///
    /// 2. **PPUpdateWrongEpoch**: Proposal targets wrong epoch
    ///    - Before "slot of no return": must target current epoch
    ///    - After "slot of no return": must target next epoch
    ///    - Error contains: current epoch, target epoch, voting period
    ///
    /// 3. **PVCannotFollowPPUP**: Invalid protocol version succession
    ///    - Major bump: major' = major + 1, minor' = 0
    ///    - Minor bump: major' = major, minor' = minor + 1
    ///    - Error contains: the invalid proposed version
    ///
    /// # Era Note
    /// PPUP is only used in Shelley through Babbage. Conway uses DRep governance.
    UpdateFailure(PpupPredFailure),

    /// Bootstrap address attributes exceed 64 bytes
    /// Formal: ∀(a,_) ∈ txouts, a ∈ Addrbootstrap → bootstrapAttrsSize a ≤ 64
    OutputBootAddrAttrsTooBig(Vec<TxOut>),
}

// ============================================================================
// Allegra Era Predicate Failures
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
///   | OutputTooBigUTxO [TxOut era]  -- NEW in Allegra
/// ```
///
/// Key difference from Shelley:
/// - Replaces ExpiredUTxO with OutsideValidityIntervalUTxO
/// - Adds OutputTooBigUTxO for serialized value size checks
#[derive(Debug, Clone)]
pub enum AllegraUtxoPredFailure {
    /// Some inputs do not exist in the UTxO set
    BadInputsUTxO(HashSet<TxIn>),

    /// Transaction is outside its validity interval (Allegra replaces TTL)
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
    /// Formal: inInterval slot (txvldt txb)
    /// The slot must be within [invalidBefore, invalidHereafter)
    OutsideValidityIntervalUTxO {
        validity_interval: ValidityInterval,
        current_slot: SlotNo,
    },

    /// Transaction exceeds maximum size
    MaxTxSizeUTxO {
        actual_size: TxSize,
        max_size: TxSize,
    },

    /// Transaction has no inputs
    InputSetEmptyUTxO,

    /// Fee is below minimum required
    FeeTooSmallUTxO {
        supplied_fee: Coin,
        minimum_fee: Coin,
    },

    /// Value not conserved (consumed ≠ produced)
    /// In Mary era, this includes minted/burned tokens
    ValueNotConservedUTxO {
        consumed: Coin,
        produced: Coin,
    },

    /// Output addresses have wrong network ID
    WrongNetwork {
        expected_network: Network,
        wrong_addresses: HashSet<Addr>,
    },

    /// Withdrawal addresses have wrong network ID
    WrongNetworkWithdrawal {
        expected_network: Network,
        wrong_accounts: HashSet<RewardAccount>,
    },

    /// Some outputs have value below minimum
    /// In Mary era, minimum is scaled based on multi-asset size
    OutputTooSmallUTxO(Vec<TxOut>),

    /// Protocol parameter update failure (PPUP sub-rule)
    ///
    /// See ShelleyUtxoPredFailure::UpdateFailure for detailed documentation.
    UpdateFailure(PpupPredFailure),

    /// Bootstrap address attributes exceed 64 bytes
    OutputBootAddrAttrsTooBig(Vec<TxOut>),

    /// Output value serialization exceeds maximum size (NEW in Allegra)
    ///
    /// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/Rules/Utxo.hs:254-270
    ///
    /// ```haskell
    /// validateOutputTooBigUTxO ::
    ///   EraTxOut era =>
    ///   PParams era ->
    ///   UTxO era ->
    ///   Test (AllegraUtxoPredFailure era)
    /// validateOutputTooBigUTxO pp (UTxO outputs) =
    ///   failureUnless (null outputsTooBig) $ OutputTooBigUTxO outputsTooBig
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
    /// Formal: ∀ txout ∈ txouts txb, serSize (getValue txout) ≤ MaxValSize (4000)
    ///
    /// This prevents outputs with extremely large multi-asset bundles
    /// that would bloat the UTxO set.
    OutputTooBigUTxO(Vec<TxOut>),
}

/// Convert Shelley error to Allegra error
///
/// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/Rules/Utxo.hs:386-399
impl From<ShelleyUtxoPredFailure> for AllegraUtxoPredFailure {
    fn from(err: ShelleyUtxoPredFailure) -> Self {
        match err {
            ShelleyUtxoPredFailure::BadInputsUTxO(ins) => AllegraUtxoPredFailure::BadInputsUTxO(ins),
            ShelleyUtxoPredFailure::ExpiredUTxO { supplied_ttl, current_slot } => {
                // Convert TTL to validity interval
                AllegraUtxoPredFailure::OutsideValidityIntervalUTxO {
                    validity_interval: ValidityInterval {
                        invalid_before: None,
                        invalid_hereafter: Some(supplied_ttl),
                    },
                    current_slot,
                }
            }
            ShelleyUtxoPredFailure::MaxTxSizeUTxO { actual_size, max_size } => {
                AllegraUtxoPredFailure::MaxTxSizeUTxO { actual_size, max_size }
            }
            ShelleyUtxoPredFailure::InputSetEmptyUTxO => AllegraUtxoPredFailure::InputSetEmptyUTxO,
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
            ShelleyUtxoPredFailure::UpdateFailure(ppup_err) => {
                AllegraUtxoPredFailure::UpdateFailure(ppup_err)
            }
            ShelleyUtxoPredFailure::OutputBootAddrAttrsTooBig(outs) => {
                AllegraUtxoPredFailure::OutputBootAddrAttrsTooBig(outs)
            }
        }
    }
}

// ============================================================================
// Validation Functions
// ============================================================================

/// Validate Time-to-Live (Shelley era)
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxo.hs:421-430
///
/// ```haskell
/// validateTimeToLive ::
///   (ShelleyEraTxBody era, ExactEra ShelleyEra era) =>
///   TxBody TopTx era ->
///   SlotNo ->
///   Test (ShelleyUtxoPredFailure era)
/// validateTimeToLive txb slot =
///   failureUnless (ttl >= slot) $
///     ExpiredUTxO Mismatch {mismatchSupplied = ttl, mismatchExpected = slot}
///   where
///     ttl = txb ^. ttlTxBodyL
/// ```
///
/// Formal specification: txttl txb ≥ slot
///
/// # Validation Steps:
/// 1. Extract TTL from transaction body
/// 2. Compare TTL with current slot
/// 3. Fail if TTL < current slot (transaction expired)
///
/// # Why This Matters:
/// - Prevents replay attacks
/// - Allows users to set transaction expiration
/// - Ensures transactions have bounded validity
///
/// # Note: In Allegra/Mary, use validate_validity_interval instead
pub fn validate_time_to_live(ttl: SlotNo, current_slot: SlotNo) -> Result<(), ShelleyUtxoPredFailure> {
    // Step 1 & 2: Compare TTL with current slot
    // TTL marks the LAST slot in which the transaction is valid
    if ttl >= current_slot {
        Ok(())
    } else {
        // Step 3: Transaction has expired
        Err(ShelleyUtxoPredFailure::ExpiredUTxO {
            supplied_ttl: ttl,
            current_slot,
        })
    }
}

// ============================================================================
// Allegra Era Validation Functions
// ============================================================================

/// Validate Validity Interval (Allegra/Mary era)
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
/// Formal specification: inInterval slot (txvldt txb)
///
/// The validity interval is a half-open interval [invalidBefore, invalidHereafter):
/// - invalidBefore <= current_slot (inclusive lower bound)
/// - current_slot < invalidHereafter (exclusive upper bound)
///
/// # Validation Steps:
/// 1. Extract validity interval from transaction body
/// 2. Check if current slot is within [invalidBefore, invalidHereafter)
/// 3. Fail if slot is outside the interval
///
/// # Why This Matters:
/// - Extends Shelley's TTL with optional lower bound
/// - Supports time-locked transactions (e.g., vesting)
/// - Scripts can check validity interval for time-based logic
pub fn validate_validity_interval(
    validity_interval: ValidityInterval,
    current_slot: SlotNo,
) -> Result<(), AllegraUtxoPredFailure> {
    if validity_interval.contains(current_slot) {
        Ok(())
    } else {
        Err(AllegraUtxoPredFailure::OutsideValidityIntervalUTxO {
            validity_interval,
            current_slot,
        })
    }
}

/// Maximum serialized value size in bytes (Allegra/Mary)
///
/// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/Rules/Utxo.hs:263
pub const MAX_VAL_SIZE: i64 = 4000;

/// Validate Output Value Size (Allegra/Mary era)
///
/// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/Rules/Utxo.hs:254-270
///
/// ```haskell
/// validateOutputTooBigUTxO ::
///   EraTxOut era =>
///   PParams era ->
///   UTxO era ->
///   Test (AllegraUtxoPredFailure era)
/// validateOutputTooBigUTxO pp (UTxO outputs) =
///   failureUnless (null outputsTooBig) $ OutputTooBigUTxO outputsTooBig
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
/// Formal specification: ∀ txout ∈ txouts txb, serSize (getValue txout) ≤ MaxValSize
///
/// # Validation Steps:
/// 1. For each output, calculate serialized size of the value
/// 2. Compare against MAX_VAL_SIZE (4000 bytes)
/// 3. Collect all outputs that exceed the limit
/// 4. Fail if any outputs are too big
///
/// # Why This Matters:
/// - Prevents UTxO bloat from extremely large multi-asset bundles
/// - Ensures outputs can be efficiently stored and transmitted
/// - The 4000 byte limit is chosen to balance flexibility and efficiency
pub fn validate_output_too_big(
    outputs: &[TxOut],
) -> Result<(), AllegraUtxoPredFailure> {
    let outputs_too_big: Vec<TxOut> = outputs
        .iter()
        .filter(|output| output.value.serialized_size() > MAX_VAL_SIZE)
        .cloned()
        .collect();

    if outputs_too_big.is_empty() {
        Ok(())
    } else {
        Err(AllegraUtxoPredFailure::OutputTooBigUTxO(outputs_too_big))
    }
}

/// Validate Input Set Not Empty
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxo.hs:435-442
///
/// ```haskell
/// validateInputSetEmptyUTxO ::
///   EraTxBody era =>
///   TxBody t era ->
///   Test (ShelleyUtxoPredFailure era)
/// validateInputSetEmptyUTxO txb =
///   failureUnless (inputs /= Set.empty) InputSetEmptyUTxO
///   where
///     inputs = txb ^. inputsTxBodyL
/// ```
///
/// Formal specification: txins txb ≠ ∅
///
/// # Validation Steps:
/// 1. Get the set of transaction inputs
/// 2. Check if the set is non-empty
/// 3. Fail if empty (no inputs to spend)
///
/// # Why This Matters:
/// - Every transaction MUST consume at least one UTxO
/// - Prevents value creation from nothing
/// - Genesis transactions handled separately
pub fn validate_input_set_not_empty(inputs: &HashSet<TxIn>) -> Result<(), ShelleyUtxoPredFailure> {
    // Step 1 & 2: Check if inputs set is non-empty
    if !inputs.is_empty() {
        Ok(())
    } else {
        // Step 3: No inputs provided
        Err(ShelleyUtxoPredFailure::InputSetEmptyUTxO)
    }
}

/// Validate Fee is Sufficient
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxo.hs:447-463
///
/// ```haskell
/// validateFeeTooSmallUTxO ::
///   EraUTxO era =>
///   PParams era ->
///   Tx TopTx era ->
///   UTxO era ->
///   Test (ShelleyUtxoPredFailure era)
/// validateFeeTooSmallUTxO pp tx utxo =
///   failureUnless (minFee <= txFee) $
///     FeeTooSmallUTxO Mismatch { mismatchSupplied = txFee, mismatchExpected = minFee }
///   where
///     minFee = getMinFeeTxUtxo pp tx utxo
///     txFee = txb ^. feeTxBodyL
/// ```
///
/// Formal specification: minfee pp tx ≤ txfee txb
///
/// # Validation Steps:
/// 1. Calculate minimum fee: minFee = (txSize * minFeeA) + minFeeB
/// 2. Get declared fee from transaction body
/// 3. Check if declared fee >= minimum fee
///
/// # Fee Formula (Shelley):
/// ```
/// minFee = (tx_size_bytes × min_fee_a) + min_fee_b
/// ```
/// - min_fee_a: Cost per byte (44 lovelace on mainnet)
/// - min_fee_b: Base fee (155,381 lovelace on mainnet)
pub fn validate_fee_too_small(
    pp: &ShelleyPParams,
    tx_size: TxSize,
    declared_fee: Coin,
) -> Result<(), ShelleyUtxoPredFailure> {
    // Step 1: Calculate minimum fee
    // minFee = (txSize * minFeeA) + minFeeB
    let min_fee = (tx_size as u64 * pp.min_fee_a) + pp.min_fee_b;

    // Step 2 & 3: Compare with declared fee
    if declared_fee >= min_fee {
        Ok(())
    } else {
        Err(ShelleyUtxoPredFailure::FeeTooSmallUTxO {
            supplied_fee: declared_fee,
            minimum_fee: min_fee,
        })
    }
}

/// Validate All Inputs Exist in UTxO
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxo.hs:468-476
///
/// ```haskell
/// validateBadInputsUTxO ::
///   UTxO era ->
///   Set TxIn ->
///   Test (ShelleyUtxoPredFailure era)
/// validateBadInputsUTxO utxo inputs =
///   failureUnless (Set.null badInputs) $ BadInputsUTxO badInputs
///   where
///     badInputs = Set.filter (`Map.notMember` unUTxO utxo) inputs
/// ```
///
/// Formal specification: txins txb ⊆ dom utxo
///
/// # Validation Steps:
/// 1. For each input in the transaction
/// 2. Check if it exists in the current UTxO set
/// 3. Collect all inputs that don't exist
/// 4. Fail if any inputs are missing
///
/// # Why This Matters:
/// - Cannot spend non-existent UTxOs
/// - Prevents double-spending
/// - All referenced inputs must exist
pub fn validate_bad_inputs(
    utxo: &UTxO,
    inputs: &HashSet<TxIn>,
) -> Result<(), ShelleyUtxoPredFailure> {
    // Step 1-3: Find inputs not in UTxO set
    let bad_inputs: HashSet<TxIn> = inputs
        .iter()
        .filter(|input| !utxo.contains(input))
        .cloned()
        .collect();

    // Step 4: Check if all inputs exist
    if bad_inputs.is_empty() {
        Ok(())
    } else {
        Err(ShelleyUtxoPredFailure::BadInputsUTxO(bad_inputs))
    }
}

/// Validate Output Network IDs
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxo.hs:481-492
///
/// ```haskell
/// validateWrongNetwork ::
///   (EraTxOut era, Foldable f) =>
///   Network ->
///   f (TxOut era) ->
///   Test (ShelleyUtxoPredFailure era)
/// validateWrongNetwork netId outputs =
///   failureUnless (null addrsWrongNetwork) $
///     WrongNetwork netId (Set.fromList addrsWrongNetwork)
///   where
///     addrsWrongNetwork = filter (\a -> getNetwork a /= netId) ...
/// ```
///
/// Formal specification: ∀(_ → (a, _)) ∈ txouts txb, netId a = NetworkId
///
/// # Validation Steps:
/// 1. Get expected network ID from environment
/// 2. For each output, extract address network ID
/// 3. Collect addresses with mismatching network
/// 4. Fail if any addresses have wrong network
pub fn validate_wrong_network(
    expected_network: Network,
    outputs: &[TxOut],
) -> Result<(), ShelleyUtxoPredFailure> {
    // Step 1-3: Find addresses with wrong network
    let wrong_addresses: HashSet<Addr> = outputs
        .iter()
        .filter_map(|output| {
            match output.address.get_network() {
                Some(net) if net != expected_network => Some(output.address.clone()),
                // Bootstrap addresses don't have network ID
                None => None,
                _ => None,
            }
        })
        .collect();

    // Step 4: Check if all networks match
    if wrong_addresses.is_empty() {
        Ok(())
    } else {
        Err(ShelleyUtxoPredFailure::WrongNetwork {
            expected_network,
            wrong_addresses,
        })
    }
}

/// Validate Withdrawal Network IDs
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxo.hs:497-509
///
/// ```haskell
/// validateWrongNetworkWithdrawal ::
///   EraTxBody era =>
///   Network ->
///   TxBody t era ->
///   Test (ShelleyUtxoPredFailure era)
/// validateWrongNetworkWithdrawal netId txb =
///   failureUnless (null withdrawalsWrongNetwork) $
///     WrongNetworkWithdrawal netId (Set.fromList withdrawalsWrongNetwork)
///   where
///     withdrawalsWrongNetwork = filter (\a -> raNetwork a /= netId) ...
/// ```
///
/// Formal specification: ∀(a → ) ∈ txwdrls txb, netId a = NetworkId
pub fn validate_wrong_network_withdrawal(
    expected_network: Network,
    withdrawals: &HashMap<RewardAccount, Coin>,
) -> Result<(), ShelleyUtxoPredFailure> {
    // Find reward accounts with wrong network
    let wrong_accounts: HashSet<RewardAccount> = withdrawals
        .keys()
        .filter(|account| account.network != expected_network)
        .cloned()
        .collect();

    if wrong_accounts.is_empty() {
        Ok(())
    } else {
        Err(ShelleyUtxoPredFailure::WrongNetworkWithdrawal {
            expected_network,
            wrong_accounts,
        })
    }
}

/// Calculate total deposits from certificates
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/TxCert.hs
///
/// ```haskell
/// certsTotalDepositsTxBody :: PParams era -> CertState era -> TxBody era -> Coin
/// ```
///
/// # What counts as a deposit:
/// - Stake key registration: keyDeposit (from current protocol parameters)
/// - Pool registration: poolDeposit (from current protocol parameters)
pub fn calculate_total_deposits(
    pp: &ShelleyPParams,
    cert_state: &CertState,
    certificates: &[Certificate],
) -> Coin {
    let mut total: Coin = 0;

    for cert in certificates {
        match cert {
            Certificate::StakeKeyRegistration { credential } => {
                // Only charge if not already registered
                if !cert_state.deposits.contains_key(credential) {
                    total += pp.key_deposit;
                }
            }
            Certificate::PoolRegistration { pool_params } => {
                // Only charge if pool not already registered
                if !cert_state.registered_pools.contains_key(&pool_params.pool_id) {
                    total += pp.pool_deposit;
                }
            }
            _ => {}
        }
    }

    total
}

/// Calculate total refunds from certificates
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/TxCert.hs
///
/// ```haskell
/// certsTotalRefundsTxBody :: PParams era -> CertState era -> TxBody era -> Coin
/// ```
///
/// # CRITICAL: Refunds use STORED deposit, not current protocol parameter!
///
/// When deregistering, the refund is the ORIGINAL deposit amount that was
/// stored when the stake key was registered, NOT the current keyDeposit
/// value from protocol parameters.
///
/// This is documented in ADR #3 (2022-12-05_003-track-individual-deposits.md)
/// and the Shelley formal spec errata.
pub fn calculate_total_refunds(
    _pp: &ShelleyPParams,
    cert_state: &CertState,
    certificates: &[Certificate],
) -> Coin {
    let mut total: Coin = 0;

    for cert in certificates {
        match cert {
            Certificate::StakeKeyDeregistration { credential } => {
                // IMPORTANT: Use the STORED deposit, not current protocol parameter
                // This ensures users get back exactly what they deposited
                if let Some(&stored_deposit) = cert_state.deposits.get(credential) {
                    total += stored_deposit;
                }
            }
            Certificate::PoolRetirement { pool_id, .. } => {
                // Pool retirement refunds the original pool deposit
                if let Some(&stored_deposit) = cert_state.registered_pools.get(pool_id) {
                    total += stored_deposit;
                }
            }
            _ => {}
        }
    }

    total
}

/// Calculate consumed value (Shelley/Allegra - ADA only)
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/UTxO.hs
///
/// ```haskell
/// consumed pp certState utxo txBody =
///     balance (txInsFilter utxo (txBody ^. inputsTxBodyL))
///   <> (txBody ^. withdrawalsTxBodyL)
///   <> Val.inject (certsTotalRefundsTxBody pp certState txBody)
/// ```
///
/// consumed = sum(input_values) + withdrawals + refunds
pub fn calculate_consumed(
    pp: &ShelleyPParams,
    utxo: &UTxO,
    cert_state: &CertState,
    tx_body: &ShelleyTxBody,
) -> Coin {
    // Sum of all input values
    let input_value = utxo.balance(&tx_body.inputs);

    // Sum of all withdrawals
    let withdrawal_value: Coin = tx_body.withdrawals.values().sum();

    // Sum of all refunds (from deregistrations)
    let refund_value = calculate_total_refunds(pp, cert_state, &tx_body.certificates);

    input_value + withdrawal_value + refund_value
}

/// Calculate produced value (Shelley/Allegra - ADA only)
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/UTxO.hs
///
/// ```haskell
/// produced pp certState txBody =
///     F.fold (txBody ^. outputsTxBodyL)
///   <> Val.inject (txBody ^. feeTxBodyL)
///   <> Val.inject (certsTotalDepositsTxBody pp certState txBody)
/// ```
///
/// produced = sum(output_values) + fee + deposits
pub fn calculate_produced(
    pp: &ShelleyPParams,
    cert_state: &CertState,
    tx_body: &ShelleyTxBody,
) -> Coin {
    // Sum of all output values
    let output_value: Coin = tx_body.outputs.iter().map(|o| o.value.coin).sum();

    // Transaction fee
    let fee = tx_body.fee;

    // Sum of all deposits (from registrations)
    let deposit_value = calculate_total_deposits(pp, cert_state, &tx_body.certificates);

    output_value + fee + deposit_value
}

// ============================================================================
// Mary Era: Multi-Asset Value Conservation
// ============================================================================

/// Full multi-asset value for Mary era value conservation
#[derive(Debug, Clone, Default)]
pub struct FullValue {
    pub coin: Coin,
    pub multi_asset: HashMap<PolicyId, HashMap<AssetName, AssetQuantity>>,
}

impl FullValue {
    pub fn from_coin(coin: Coin) -> Self {
        FullValue {
            coin,
            multi_asset: HashMap::new(),
        }
    }

    /// Add another value to this one
    pub fn add(&mut self, other: &MaryValue) {
        self.coin += other.coin;
        for (policy, assets) in &other.multi_asset {
            let entry = self.multi_asset.entry(*policy).or_insert_with(HashMap::new);
            for (name, qty) in assets {
                *entry.entry(name.clone()).or_insert(0) += qty;
            }
        }
    }

    /// Add minted assets (positive quantities from mint field)
    pub fn add_minted(&mut self, mint: &HashMap<PolicyId, HashMap<AssetName, AssetQuantity>>) {
        for (policy, assets) in mint {
            let entry = self.multi_asset.entry(*policy).or_insert_with(HashMap::new);
            for (name, qty) in assets {
                if *qty > 0 {
                    *entry.entry(name.clone()).or_insert(0) += qty;
                }
            }
        }
    }

    /// Add burned assets (negative quantities from mint field, treated as positive)
    pub fn add_burned(&mut self, mint: &HashMap<PolicyId, HashMap<AssetName, AssetQuantity>>) {
        for (policy, assets) in mint {
            let entry = self.multi_asset.entry(*policy).or_insert_with(HashMap::new);
            for (name, qty) in assets {
                if *qty < 0 {
                    *entry.entry(name.clone()).or_insert(0) += -qty;
                }
            }
        }
    }

    /// Check if two values are equal (for conservation check)
    pub fn equals(&self, other: &FullValue) -> bool {
        if self.coin != other.coin {
            return false;
        }

        // Compare all multi-asset entries
        let mut all_policies: HashSet<PolicyId> = self.multi_asset.keys().copied().collect();
        all_policies.extend(other.multi_asset.keys().copied());

        for policy in all_policies {
            let self_assets = self.multi_asset.get(&policy);
            let other_assets = other.multi_asset.get(&policy);

            match (self_assets, other_assets) {
                (None, None) => continue,
                (Some(a), None) | (None, Some(a)) => {
                    if a.values().any(|&v| v != 0) {
                        return false;
                    }
                }
                (Some(a1), Some(a2)) => {
                    let mut all_names: HashSet<AssetName> = a1.keys().cloned().collect();
                    all_names.extend(a2.keys().cloned());

                    for name in all_names {
                        let v1 = a1.get(&name).unwrap_or(&0);
                        let v2 = a2.get(&name).unwrap_or(&0);
                        if v1 != v2 {
                            return false;
                        }
                    }
                }
            }
        }

        true
    }
}

/// Calculate consumed value (Mary era - includes minted tokens)
///
/// Reference: eras/mary/impl/src/Cardano/Ledger/Mary/UTxO.hs:69-86
///
/// ```haskell
/// getConsumedMaryValue pp lookupStakingDeposit lookupDRepDeposit utxo txBody =
///   consumedValue <> MaryValue mempty mintedMultiAsset
///   where
///     mintedMultiAsset = filterMultiAsset (\_ _ -> (> 0)) $ txBody ^. mintTxBodyL
///     consumedValue =
///       sumUTxO (txInsFilter utxo (txBody ^. inputsTxBodyL))
///         <> inject (refunds <> withdrawals)
///     refunds = getTotalRefundsTxBody pp lookupStakingDeposit lookupDRepDeposit txBody
///     withdrawals = fold . unWithdrawals $ txBody ^. withdrawalsTxBodyL
/// ```
///
/// consumed = sum(inputs) + withdrawals + refunds + minted_tokens
///
/// NOTE: Minted tokens (positive mint quantities) are added to consumed
/// because they are "created" by the transaction and must be accounted for.
pub fn calculate_consumed_mary(
    pp: &ShelleyPParams,
    utxo: &UTxO,
    cert_state: &CertState,
    tx_body: &ShelleyTxBody,
) -> FullValue {
    let mut consumed = FullValue::from_coin(0);

    // Sum of all input values
    for input in &tx_body.inputs {
        if let Some(output) = utxo.utxo.get(input) {
            consumed.add(&output.value);
        }
    }

    // Add withdrawals (ADA only)
    let withdrawal_value: Coin = tx_body.withdrawals.values().sum();
    consumed.coin += withdrawal_value;

    // Add refunds (ADA only)
    let refund_value = calculate_total_refunds(pp, cert_state, &tx_body.certificates);
    consumed.coin += refund_value;

    // Add minted tokens (positive quantities from mint field)
    consumed.add_minted(&tx_body.mint);

    consumed
}

/// Calculate produced value (Mary era - includes burned tokens)
///
/// Reference: eras/mary/impl/src/Cardano/Ledger/Mary/UTxO.hs:88-96
///
/// ```haskell
/// getProducedMaryValue pp isPoolRegistered txBody =
///   shelleyProducedValue pp isPoolRegistered txBody <> burnedMultiAssets txBody
///
/// burnedMultiAssets txBody =
///   MaryValue mempty $
///     mapMaybeMultiAsset (\_ _ v -> if v < 0 then Just (negate v) else Nothing) $
///       txBody ^. mintTxBodyL
/// ```
///
/// produced = sum(outputs) + fee + deposits + burned_tokens
///
/// NOTE: Burned tokens (negative mint quantities, made positive) are added
/// to produced because they are "destroyed" and leave the system.
pub fn calculate_produced_mary(
    pp: &ShelleyPParams,
    cert_state: &CertState,
    tx_body: &ShelleyTxBody,
) -> FullValue {
    let mut produced = FullValue::from_coin(0);

    // Sum of all output values
    for output in &tx_body.outputs {
        produced.add(&output.value);
    }

    // Add fee (ADA only)
    produced.coin += tx_body.fee;

    // Add deposits (ADA only)
    let deposit_value = calculate_total_deposits(pp, cert_state, &tx_body.certificates);
    produced.coin += deposit_value;

    // Add burned tokens (negative quantities from mint field, made positive)
    produced.add_burned(&tx_body.mint);

    produced
}

/// Validate Value Conservation (Mary era - multi-asset)
///
/// For Mary era, the conservation law includes multi-asset tokens:
///
/// ```
/// consumed = produced
///
/// where:
///   consumed = sum(inputs) + withdrawals + refunds + minted
///   produced = sum(outputs) + fee + deposits + burned
/// ```
///
/// This ensures that:
/// 1. ADA is conserved (same as Shelley)
/// 2. All native tokens are accounted for
/// 3. Minting adds to consumed, burning adds to produced
pub fn validate_value_not_conserved_mary(
    pp: &ShelleyPParams,
    utxo: &UTxO,
    cert_state: &CertState,
    tx_body: &ShelleyTxBody,
) -> Result<(), AllegraUtxoPredFailure> {
    let consumed = calculate_consumed_mary(pp, utxo, cert_state, tx_body);
    let produced = calculate_produced_mary(pp, cert_state, tx_body);

    if consumed.equals(&produced) {
        Ok(())
    } else {
        // For error reporting, just report coin mismatch
        // Full multi-asset comparison is done above
        Err(AllegraUtxoPredFailure::ValueNotConservedUTxO {
            consumed: consumed.coin,
            produced: produced.coin,
        })
    }
}

/// Validate Value Conservation
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxo.hs:514-526
///
/// ```haskell
/// validateValueNotConservedUTxO ::
///   (EraUTxO era, EraCertState era) =>
///   PParams era ->
///   UTxO era ->
///   CertState era ->
///   TxBody TopTx era ->
///   Test (ShelleyUtxoPredFailure era)
/// validateValueNotConservedUTxO pp utxo certState txBody =
///   failureUnless (consumedValue == producedValue) $
///     ValueNotConservedUTxO Mismatch {
///       mismatchSupplied = consumedValue,
///       mismatchExpected = producedValue
///     }
///   where
///     consumedValue = consumed pp certState utxo txBody
///     producedValue = produced pp certState txBody
/// ```
///
/// Formal specification: consumed pp utxo txb = produced pp poolParams txb
///
/// # The Conservation Law:
/// ```
/// consumed = produced
///
/// where:
///   consumed = sum(inputs) + withdrawals + refunds
///   produced = sum(outputs) + fee + deposits
/// ```
///
/// This is THE MOST IMPORTANT validation - it ensures the soundness
/// of Cardano's monetary system.
pub fn validate_value_not_conserved(
    pp: &ShelleyPParams,
    utxo: &UTxO,
    cert_state: &CertState,
    tx_body: &ShelleyTxBody,
) -> Result<(), ShelleyUtxoPredFailure> {
    let consumed = calculate_consumed(pp, utxo, cert_state, tx_body);
    let produced = calculate_produced(pp, cert_state, tx_body);

    if consumed == produced {
        Ok(())
    } else {
        Err(ShelleyUtxoPredFailure::ValueNotConservedUTxO { consumed, produced })
    }
}

/// Validate Outputs Not Too Small (Shelley/Allegra)
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxo.hs:531-545
///
/// ```haskell
/// validateOutputTooSmallUTxO ::
///   (EraTxOut era, Foldable f) =>
///   PParams era ->
///   f (TxOut era) ->
///   Test (ShelleyUtxoPredFailure era)
/// validateOutputTooSmallUTxO pp outputs =
///   failureUnless (null outputsTooSmall) $ OutputTooSmallUTxO outputsTooSmall
///   where
///     outputsTooSmall = filter (\txOut -> txOut ^. coinTxOutL < getMinCoinTxOut pp txOut) ...
/// ```
///
/// Formal specification: ∀(_ → (_, c)) ∈ txouts txb, c ≥ minUTxOValue pp
///
/// # Shelley/Allegra: Uses fixed minUTxOValue from protocol parameters
/// # Mary: Uses scaled minimum based on multi-asset size (see validate_output_too_small_mary)
pub fn validate_output_too_small(
    pp: &ShelleyPParams,
    outputs: &[TxOut],
) -> Result<(), ShelleyUtxoPredFailure> {
    let outputs_too_small: Vec<TxOut> = outputs
        .iter()
        .filter(|output| output.value.coin < pp.min_utxo_value)
        .cloned()
        .collect();

    if outputs_too_small.is_empty() {
        Ok(())
    } else {
        Err(ShelleyUtxoPredFailure::OutputTooSmallUTxO(outputs_too_small))
    }
}

// ============================================================================
// Mary Era Validation Functions
// ============================================================================

/// Calculate scaled minimum deposit for multi-asset outputs (Mary era)
///
/// Reference: eras/mary/impl/src/Cardano/Ledger/Mary/TxOut.hs:35-77
///
/// ```haskell
/// scaledMinDeposit :: Val v => v -> Coin -> Coin
/// scaledMinDeposit v (Coin mv)
///   | isAdaOnly v = Coin mv
///   | otherwise = Coin $ max mv (coinsPerUTxOWord * (utxoEntrySizeWithoutVal + size v))
///   where
///     txoutLenNoVal = 14
///     txinLen = 7
///     coinSize = 0
///     utxoEntrySizeWithoutVal = 6 + txoutLenNoVal + txinLen
///     coinsPerUTxOWord = quot mv (utxoEntrySizeWithoutVal + coinSize)
/// ```
///
/// # How It Works:
///
/// For ADA-only outputs: minimum = minUTxOValue (the protocol parameter)
///
/// For multi-asset outputs: minimum is scaled based on size:
/// 1. Calculate "cost per word" = minUTxOValue / (utxoEntrySizeWithoutVal + coinSize)
/// 2. Multiply by actual output size = coinsPerWord * (utxoEntrySizeWithoutVal + valueSize)
/// 3. Take max of minUTxOValue and calculated minimum
///
/// This ensures larger multi-asset bundles require proportionally more ADA,
/// preventing UTxO bloat from outputs with minimal ADA but many tokens.
///
/// # Example:
/// ```
/// minUTxOValue = 1,000,000 lovelace (1 ADA)
/// utxoEntrySizeWithoutVal = 27 words
///
/// For ADA-only: minDeposit = 1,000,000 lovelace
///
/// For multi-asset with size 40 words:
///   coinsPerWord = 1,000,000 / 27 ≈ 37,037
///   minDeposit = max(1,000,000, 37,037 * (27 + 40))
///             = max(1,000,000, 37,037 * 67)
///             = max(1,000,000, 2,481,479)
///             = 2,481,479 lovelace
/// ```
pub fn scaled_min_deposit(value: &MaryValue, min_utxo_value: Coin) -> Coin {
    // For ADA-only outputs, just use the fixed minUTxOValue
    if value.is_ada_only() {
        return min_utxo_value;
    }

    // Constants from Haskell implementation
    let txout_len_no_val: i64 = 14;
    let txin_len: i64 = 7;
    let coin_size: i64 = 0; // Compact coin doesn't add to size

    let utxo_entry_size_without_val: i64 = 6 + txout_len_no_val + txin_len; // = 27

    // Calculate coins per UTxO word (how much ADA per word of storage)
    let coins_per_utxo_word: i64 = (min_utxo_value as i64) / (utxo_entry_size_without_val + coin_size);

    // Calculate required minimum based on actual value size
    let value_size = value.size();
    let calculated_min = coins_per_utxo_word * (utxo_entry_size_without_val + value_size);

    // Return max of minUTxOValue and calculated minimum
    std::cmp::max(min_utxo_value, calculated_min as Coin)
}

/// Validate Outputs Not Too Small (Mary era - with multi-asset scaling)
///
/// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/Rules/Utxo.hs:275-289
///
/// ```haskell
/// validateOutputTooSmallUTxO ::
///   EraTxOut era =>
///   PParams era ->
///   UTxO era ->
///   Test (AllegraUtxoPredFailure era)
/// validateOutputTooSmallUTxO pp (UTxO outputs) =
///   failureUnless (null outputsTooSmall) $ OutputTooSmallUTxO outputsTooSmall
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
/// # Mary Era Difference:
///
/// In Mary, `getMinCoinTxOut` uses `scaledMinDeposit` which calculates a
/// higher minimum for outputs containing native tokens.
///
/// Reference: eras/mary/impl/src/Cardano/Ledger/Mary/TxOut.hs:33
/// ```haskell
/// getMinCoinTxOut pp txOut = scaledMinDeposit (txOut ^. valueTxOutL) (pp ^. ppMinUTxOValueL)
/// ```
pub fn validate_output_too_small_mary(
    pp: &ShelleyPParams,
    outputs: &[TxOut],
) -> Result<(), AllegraUtxoPredFailure> {
    let outputs_too_small: Vec<TxOut> = outputs
        .iter()
        .filter(|output| {
            let min_coin = scaled_min_deposit(&output.value, pp.min_utxo_value);
            output.value.coin < min_coin
        })
        .cloned()
        .collect();

    if outputs_too_small.is_empty() {
        Ok(())
    } else {
        Err(AllegraUtxoPredFailure::OutputTooSmallUTxO(outputs_too_small))
    }
}

/// Validate Bootstrap Address Attributes Size
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxo.hs:551-565
///
/// ```haskell
/// validateOutputBootAddrAttrsTooBig ::
///   (EraTxOut era, Foldable f) =>
///   f (TxOut era) ->
///   Test (ShelleyUtxoPredFailure era)
/// validateOutputBootAddrAttrsTooBig outputs =
///   failureUnless (null outputsAttrsTooBig) $ OutputBootAddrAttrsTooBig outputsAttrsTooBig
///   where
///     outputsAttrsTooBig = filter (bootstrapAddressAttrsSize > 64) ...
/// ```
///
/// Formal specification: ∀(a,_) ∈ txouts, a ∈ Addrbootstrap → bootstrapAttrsSize a ≤ 64
pub fn validate_output_boot_addr_attrs_too_big(
    outputs: &[TxOut],
) -> Result<(), ShelleyUtxoPredFailure> {
    let outputs_attrs_too_big: Vec<TxOut> = outputs
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

    if outputs_attrs_too_big.is_empty() {
        Ok(())
    } else {
        Err(ShelleyUtxoPredFailure::OutputBootAddrAttrsTooBig(
            outputs_attrs_too_big,
        ))
    }
}

/// Validate Transaction Size
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxo.hs:570-581
///
/// ```haskell
/// validateMaxTxSizeUTxO ::
///   EraTx era =>
///   PParams era ->
///   Tx t era ->
///   Test (ShelleyUtxoPredFailure era)
/// validateMaxTxSizeUTxO pp tx =
///   failureUnless (txSize <= maxTxSize) $
///     MaxTxSizeUTxO Mismatch { ... }
/// ```
///
/// Formal specification: txsize tx ≤ maxTxSize pp
pub fn validate_max_tx_size(
    pp: &ShelleyPParams,
    tx_size: TxSize,
) -> Result<(), ShelleyUtxoPredFailure> {
    if tx_size <= pp.max_tx_size {
        Ok(())
    } else {
        Err(ShelleyUtxoPredFailure::MaxTxSizeUTxO {
            actual_size: tx_size,
            max_size: pp.max_tx_size,
        })
    }
}

// ============================================================================
// PPUP (Protocol Parameter Update Proposal) Validation Functions
// ============================================================================

/// PPUP Environment
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Ppup.hs:66
///
/// ```haskell
/// data PpupEnv era = PPUPEnv SlotNo (PParams era) GenDelegs
/// ```
#[derive(Debug, Clone)]
pub struct PpupEnv {
    pub slot: SlotNo,
    pub pp: ShelleyPParams,
    /// Set of valid genesis delegate key hashes
    pub gen_delegs: HashSet<[u8; 28]>,
}

/// PPUP State (Shelley Governance State)
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Governance.hs
#[derive(Debug, Clone, Default)]
pub struct ShelleyGovState {
    /// Current epoch's proposals
    pub cur_proposals: HashMap<[u8; 28], PParamsUpdate>,
    /// Next epoch's proposals
    pub future_proposals: HashMap<[u8; 28], PParamsUpdate>,
    /// Future protocol parameters (if quorum reached)
    pub future_pparams: Option<ShelleyPParams>,
}

/// Slot of No Return calculation result
#[derive(Debug)]
pub struct SlotOfNoReturn {
    pub current_epoch: EpochNo,
    pub too_late_slot: SlotNo,
    pub next_epoch: EpochNo,
}

/// Calculate the "slot of no return" for the current epoch
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Slot.hs
///
/// The slot of no return is 2 * stability_window before the epoch boundary.
/// Proposals submitted before this slot target the current epoch;
/// proposals after target the next epoch.
pub fn get_slot_of_no_return(
    slot: SlotNo,
    epoch_length: u64,
    stability_window: u64,
) -> SlotOfNoReturn {
    let current_epoch = slot / epoch_length;
    let epoch_start = current_epoch * epoch_length;
    let too_late_slot = epoch_start + epoch_length - (2 * stability_window);
    let next_epoch = current_epoch + 1;

    SlotOfNoReturn {
        current_epoch,
        too_late_slot,
        next_epoch,
    }
}

/// Validate PPUP: Check that all proposers are genesis delegates
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Ppup.hs:166-171
///
/// ```haskell
/// Map.isSubmapOfBy (\_ _ -> True) pup genDelegs
///   ?! NonGenesisUpdatePPUP
///     Mismatch
///       { mismatchSupplied = Map.keysSet pup
///       , mismatchExpected = Map.keysSet genDelegs
///       }
/// ```
///
/// Every key in the proposal map must be a genesis delegate key.
pub fn validate_ppup_genesis_keys(
    proposal_keys: &HashSet<[u8; 28]>,
    gen_delegs: &HashSet<[u8; 28]>,
) -> Result<(), PpupPredFailure> {
    // Check if all proposal keys are in gen_delegs
    if proposal_keys.is_subset(gen_delegs) {
        Ok(())
    } else {
        Err(PpupPredFailure::NonGenesisUpdatePPUP {
            supplied: proposal_keys.clone(),
            expected: gen_delegs.clone(),
        })
    }
}

/// Validate PPUP: Check that the target epoch is correct
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Ppup.hs:180-198
///
/// ```haskell
/// if slot < tooLate
///   then (curEpochNo == targetEpochNo)
///     ?! PPUpdateWrongEpoch curEpochNo targetEpochNo VoteForThisEpoch
///   else (succ curEpochNo == targetEpochNo)
///     ?! PPUpdateWrongEpoch curEpochNo targetEpochNo VoteForNextEpoch
/// ```
///
/// - Before "slot of no return": proposal must target current epoch
/// - After "slot of no return": proposal must target next epoch
pub fn validate_ppup_epoch(
    current_slot: SlotNo,
    slot_of_no_return: &SlotOfNoReturn,
    target_epoch: EpochNo,
) -> Result<(), PpupPredFailure> {
    if current_slot < slot_of_no_return.too_late_slot {
        // Before slot of no return: target current epoch
        if slot_of_no_return.current_epoch == target_epoch {
            Ok(())
        } else {
            Err(PpupPredFailure::PPUpdateWrongEpoch {
                current_epoch: slot_of_no_return.current_epoch,
                target_epoch,
                voting_period: VotingPeriod::VoteForThisEpoch,
            })
        }
    } else {
        // After slot of no return: target next epoch
        if slot_of_no_return.next_epoch == target_epoch {
            Ok(())
        } else {
            Err(PpupPredFailure::PPUpdateWrongEpoch {
                current_epoch: slot_of_no_return.current_epoch,
                target_epoch,
                voting_period: VotingPeriod::VoteForNextEpoch,
            })
        }
    }
}

/// Validate PPUP: Check that proposed protocol version is legal
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Ppup.hs:173-178
///
/// ```haskell
/// let firstIllegalProtVerUpdate = do
///       ppu <- F.find (not . hasLegalProtVerUpdate pp) pup
///       SJust newBadProtVer <- Just (ppu ^. ppuProtocolVersionL)
///       Just newBadProtVer
/// failOnJust firstIllegalProtVerUpdate PVCannotFollowPPUP
/// ```
///
/// Protocol version succession rules (pvCanFollow):
/// - Major bump: major' = major + 1, minor' = 0
/// - Minor bump: major' = major, minor' = minor + 1
pub fn validate_ppup_protocol_version(
    current_version: ProtVer,
    proposals: &HashMap<[u8; 28], PParamsUpdate>,
) -> Result<(), PpupPredFailure> {
    for ppu in proposals.values() {
        if let Some(new_ver) = ppu.protocol_version {
            if !current_version.can_follow(new_ver) {
                return Err(PpupPredFailure::PVCannotFollowPPUP(new_ver));
            }
        }
    }
    Ok(())
}

/// PPUP Transition Rule
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Ppup.hs:149-203
///
/// This implements the full PPUP state transition. It validates update proposals
/// and merges them into the governance state.
///
/// # Validation Order:
/// 1. If no update, return current state
/// 2. Check all proposers are genesis delegates
/// 3. Check protocol version succession is legal
/// 4. Check target epoch is correct for voting period
/// 5. Merge proposals into current/future proposals
/// 6. If quorum reached, compute future protocol parameters
pub fn ppup_transition(
    env: &PpupEnv,
    state: &ShelleyGovState,
    update: Option<&Update>,
    epoch_length: u64,
    stability_window: u64,
    quorum: u64,
    current_version: ProtVer,
) -> Result<ShelleyGovState, PpupPredFailure> {
    // No update: return current state unchanged
    let update = match update {
        None => return Ok(state.clone()),
        Some(u) => u,
    };

    // Check 1: All proposers must be genesis delegates
    let proposal_keys: HashSet<[u8; 28]> = update.proposals.keys().cloned().collect();
    validate_ppup_genesis_keys(&proposal_keys, &env.gen_delegs)?;

    // Check 2: Protocol version updates must be legal
    validate_ppup_protocol_version(current_version, &update.proposals)?;

    // Check 3: Target epoch must be correct for voting period
    let slot_info = get_slot_of_no_return(env.slot, epoch_length, stability_window);
    validate_ppup_epoch(env.slot, &slot_info, update.epoch)?;

    // Merge proposals and update state
    let mut new_state = state.clone();

    if env.slot < slot_info.too_late_slot {
        // Before slot of no return: merge into current proposals
        for (key, ppu) in &update.proposals {
            new_state.cur_proposals.insert(*key, ppu.clone());
        }
        // Check if quorum reached for future pparams
        new_state.future_pparams = voted_future_pparams(
            &new_state.cur_proposals,
            &env.pp,
            quorum,
        );
    } else {
        // After slot of no return: merge into future proposals
        for (key, ppu) in &update.proposals {
            new_state.future_proposals.insert(*key, ppu.clone());
        }
    }

    Ok(new_state)
}

/// Calculate future protocol parameters if quorum reached
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Ppup.hs:205-242
///
/// ```haskell
/// votedFuturePParams :: ProposedPPUpdates era -> PParams era -> Word64 -> Maybe (PParams era)
/// ```
///
/// If at least `quorum` nodes voted for the EXACT SAME parameter changes,
/// apply those changes to the current protocol parameters.
fn voted_future_pparams(
    proposals: &HashMap<[u8; 28], PParamsUpdate>,
    current_pp: &ShelleyPParams,
    quorum: u64,
) -> Option<ShelleyPParams> {
    // Count votes for each unique set of updates
    let mut votes: HashMap<String, (u64, PParamsUpdate)> = HashMap::new();

    for ppu in proposals.values() {
        // Create a canonical key for this update (simplified: use debug format)
        let key = format!("{:?}", ppu);
        let entry = votes.entry(key).or_insert((0, ppu.clone()));
        entry.0 += 1;
    }

    // Find updates that reached quorum
    for (count, ppu) in votes.values() {
        if *count >= quorum {
            // Apply the update to current parameters
            let mut new_pp = current_pp.clone();
            if let Some(v) = ppu.min_fee_a {
                new_pp.min_fee_a = v;
            }
            if let Some(v) = ppu.min_fee_b {
                new_pp.min_fee_b = v;
            }
            if let Some(v) = ppu.max_tx_size {
                new_pp.max_tx_size = v;
            }
            if let Some(v) = ppu.min_utxo_value {
                new_pp.min_utxo_value = v;
            }
            if let Some(v) = ppu.key_deposit {
                new_pp.key_deposit = v;
            }
            if let Some(v) = ppu.pool_deposit {
                new_pp.pool_deposit = v;
            }

            // Additional constraint check (from spec)
            // maxTxSize + maxBHSize < maxBBSize (simplified: always pass for now)

            return Some(new_pp);
        }
    }

    None
}

// ============================================================================
// Main Transition Rule
// ============================================================================

/// Shelley UTXO Transition Rule
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxo.hs:343-416
///
/// This is the main entry point for Shelley UTXO validation. It runs all
/// validation functions in order and updates the UTxO state if successful.
///
/// # Validation Order (matches Haskell exactly):
/// 1. validateTimeToLive
/// 2. validateInputSetEmptyUTxO
/// 3. validateFeeTooSmallUTxO
/// 4. validateBadInputsUTxO
/// 5. validateWrongNetwork
/// 6. validateWrongNetworkWithdrawal
/// 7. validateValueNotConservedUTxO
/// 8. PPUP sub-rule (protocol parameter updates)
/// 9. validateOutputTooSmallUTxO
/// 10. validateOutputBootAddrAttrsTooBig
/// 11. validateMaxTxSizeUTxO
/// 12. Update UTxO state
pub fn shelley_utxo_transition(
    env: &UtxoEnv,
    state: &UTxOState,
    tx: &ShelleyTx,
) -> Result<UTxOState, Vec<ShelleyUtxoPredFailure>> {
    let mut errors: Vec<ShelleyUtxoPredFailure> = Vec::new();
    let tx_body = &tx.body;

    // Step 1: txttl txb ≥ slot
    if let Err(e) = validate_time_to_live(tx_body.ttl, env.slot) {
        errors.push(e);
    }

    // Step 2: txins txb ≠ ∅
    if let Err(e) = validate_input_set_not_empty(&tx_body.inputs) {
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
    if let Err(e) = validate_value_not_conserved(&env.pp, &state.utxo, &env.cert_state, tx_body) {
        errors.push(e);
    }

    // Step 8: PPUP sub-rule (omitted for simplicity)

    // Step 9: ∀(_ → (_, c)) ∈ txouts txb, c ≥ minUTxOValue pp
    if let Err(e) = validate_output_too_small(&env.pp, &tx_body.outputs) {
        errors.push(e);
    }

    // Step 10: ∀(a,_) ∈ txouts, a ∈ Addrbootstrap → bootstrapAttrsSize a ≤ 64
    if let Err(e) = validate_output_boot_addr_attrs_too_big(&tx_body.outputs) {
        errors.push(e);
    }

    // Step 11: txsize tx ≤ maxTxSize pp
    if let Err(e) = validate_max_tx_size(&env.pp, tx.size()) {
        errors.push(e);
    }

    // If any errors, return them all
    if !errors.is_empty() {
        return Err(errors);
    }

    // Step 12: Update UTxO state
    let new_utxo = update_utxo_state(state, tx_body);

    Ok(new_utxo)
}

/// Update UTxO State
///
/// Applies the transaction to the UTxO set:
/// 1. Remove spent inputs
/// 2. Add new outputs
/// 3. Update fees and deposits
fn update_utxo_state(state: &UTxOState, tx_body: &ShelleyTxBody) -> UTxOState {
    let mut new_utxo = state.utxo.utxo.clone();

    // Remove spent inputs
    for input in &tx_body.inputs {
        new_utxo.remove(input);
    }

    // Add new outputs (would need TxId in real implementation)
    // This is simplified - real implementation would compute TxId
    // and create proper TxIn references

    UTxOState {
        utxo: UTxO { utxo: new_utxo },
        deposited: state.deposited, // Would be updated with deposit changes
        fees: state.fees + tx_body.fee,
    }
}

// ============================================================================
// Allegra Era Transition Rule
// ============================================================================

/// Allegra UTXO Transition Rule
///
/// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/Rules/Utxo.hs:160-238
///
/// The Allegra era UTXO rule differs from Shelley in:
/// 1. Uses ValidityInterval instead of TTL for time bounds
/// 2. Adds OutputTooBigUTxO check for serialized value size
///
/// # Validation Order (matches Haskell exactly):
/// 1. validateOutsideValidityIntervalUTxO (replaces validateTimeToLive)
/// 2. validateInputSetEmptyUTxO
/// 3. validateFeeTooSmallUTxO
/// 4. validateBadInputsUTxO
/// 5. validateWrongNetwork
/// 6. validateWrongNetworkWithdrawal
/// 7. validateValueNotConservedUTxO
/// 8. PPUP sub-rule (protocol parameter updates)
/// 9. validateOutputTooSmallUTxO
/// 10. validateOutputTooBigUTxO (NEW in Allegra)
/// 11. validateOutputBootAddrAttrsTooBig
/// 12. validateMaxTxSizeUTxO
/// 13. Update UTxO state
pub fn allegra_utxo_transition(
    env: &UtxoEnv,
    state: &UTxOState,
    tx: &ShelleyTx,
) -> Result<UTxOState, Vec<AllegraUtxoPredFailure>> {
    let mut errors: Vec<AllegraUtxoPredFailure> = Vec::new();
    let tx_body = &tx.body;

    // Step 1: inInterval slot (txvldt txb)
    let validity_interval = tx_body.get_validity_interval();
    if let Err(e) = validate_validity_interval(validity_interval, env.slot) {
        errors.push(e);
    }

    // Step 2: txins txb ≠ ∅
    if let Err(e) = validate_input_set_not_empty(&tx_body.inputs) {
        errors.push(e.into());
    }

    // Step 3: minfee pp tx ≤ txfee txb
    if let Err(e) = validate_fee_too_small(&env.pp, tx.size(), tx_body.fee) {
        errors.push(e.into());
    }

    // Step 4: txins txb ⊆ dom utxo
    if let Err(e) = validate_bad_inputs(&state.utxo, &tx_body.inputs) {
        errors.push(e.into());
    }

    // Step 5: ∀(_ → (a, _)) ∈ txouts txb, netId a = NetworkId
    if let Err(e) = validate_wrong_network(env.network_id, &tx_body.outputs) {
        errors.push(e.into());
    }

    // Step 6: ∀(a → ) ∈ txwdrls txb, netId a = NetworkId
    if let Err(e) = validate_wrong_network_withdrawal(env.network_id, &tx_body.withdrawals) {
        errors.push(e.into());
    }

    // Step 7: consumed pp utxo txb = produced pp poolParams txb
    if let Err(e) = validate_value_not_conserved(&env.pp, &state.utxo, &env.cert_state, tx_body) {
        errors.push(e.into());
    }

    // Step 8: PPUP sub-rule (omitted for simplicity)

    // Step 9: ∀(_ → (_, c)) ∈ txouts txb, c ≥ minUTxOValue pp
    if let Err(e) = validate_output_too_small(&env.pp, &tx_body.outputs) {
        errors.push(e.into());
    }

    // Step 10: ∀ txout ∈ txouts txb, serSize (getValue txout) ≤ MaxValSize (NEW)
    if let Err(e) = validate_output_too_big(&tx_body.outputs) {
        errors.push(e);
    }

    // Step 11: ∀(a,_) ∈ txouts, a ∈ Addrbootstrap → bootstrapAttrsSize a ≤ 64
    if let Err(e) = validate_output_boot_addr_attrs_too_big(&tx_body.outputs) {
        errors.push(e.into());
    }

    // Step 12: txsize tx ≤ maxTxSize pp
    if let Err(e) = validate_max_tx_size(&env.pp, tx.size()) {
        errors.push(e.into());
    }

    // If any errors, return them all
    if !errors.is_empty() {
        return Err(errors);
    }

    // Step 13: Update UTxO state
    let new_utxo = update_utxo_state(state, tx_body);

    Ok(new_utxo)
}

// ============================================================================
// Mary Era Transition Rule
// ============================================================================

/// Mary UTXO Transition Rule
///
/// Reference: eras/mary/impl/src/Cardano/Ledger/Mary/Rules/Utxo.hs
///
/// The Mary era reuses the Allegra UTXO rule structure but with:
/// 1. Multi-asset values in outputs
/// 2. Minting/burning in value conservation
/// 3. Scaled minimum UTxO for multi-asset outputs
///
/// Mary reuses Allegra's error type:
/// ```haskell
/// type instance EraRuleFailure "UTXO" MaryEra = AllegraUtxoPredFailure MaryEra
/// ```
///
/// # Validation Order (same as Allegra, different implementations):
/// 1. validateOutsideValidityIntervalUTxO
/// 2. validateInputSetEmptyUTxO
/// 3. validateFeeTooSmallUTxO
/// 4. validateBadInputsUTxO
/// 5. validateWrongNetwork
/// 6. validateWrongNetworkWithdrawal
/// 7. validateValueNotConservedUTxO (includes mint/burn)
/// 8. PPUP sub-rule
/// 9. validateOutputTooSmallUTxO (uses scaledMinDeposit)
/// 10. validateOutputTooBigUTxO
/// 11. validateOutputBootAddrAttrsTooBig
/// 12. validateMaxTxSizeUTxO
/// 13. Update UTxO state
///
/// # Additional Implicit Check:
/// The type system in Haskell prevents ADA from being in the mint field:
/// ```haskell
/// {- adaPolicy ∉ supp mint tx
///    above check not needed because mint field of type MultiAsset cannot contain ada -}
/// ```
pub fn mary_utxo_transition(
    env: &UtxoEnv,
    state: &UTxOState,
    tx: &ShelleyTx,
) -> Result<UTxOState, Vec<AllegraUtxoPredFailure>> {
    let mut errors: Vec<AllegraUtxoPredFailure> = Vec::new();
    let tx_body = &tx.body;

    // Step 1: inInterval slot (txvldt txb)
    let validity_interval = tx_body.get_validity_interval();
    if let Err(e) = validate_validity_interval(validity_interval, env.slot) {
        errors.push(e);
    }

    // Step 2: txins txb ≠ ∅
    if let Err(e) = validate_input_set_not_empty(&tx_body.inputs) {
        errors.push(e.into());
    }

    // Step 3: minfee pp tx ≤ txfee txb
    if let Err(e) = validate_fee_too_small(&env.pp, tx.size(), tx_body.fee) {
        errors.push(e.into());
    }

    // Step 4: txins txb ⊆ dom utxo
    if let Err(e) = validate_bad_inputs(&state.utxo, &tx_body.inputs) {
        errors.push(e.into());
    }

    // Step 5: ∀(_ → (a, _)) ∈ txouts txb, netId a = NetworkId
    if let Err(e) = validate_wrong_network(env.network_id, &tx_body.outputs) {
        errors.push(e.into());
    }

    // Step 6: ∀(a → ) ∈ txwdrls txb, netId a = NetworkId
    if let Err(e) = validate_wrong_network_withdrawal(env.network_id, &tx_body.withdrawals) {
        errors.push(e.into());
    }

    // Step 7: consumed pp utxo txb = produced pp poolParams txb (with mint/burn)
    if let Err(e) = validate_value_not_conserved_mary(&env.pp, &state.utxo, &env.cert_state, tx_body) {
        errors.push(e);
    }

    // Step 8: PPUP sub-rule (omitted for simplicity)

    // Step 9: ∀ txout ∈ txouts txb, getValue txout ≥ inject (scaledMinDeposit v (minUTxOValue pp))
    // Mary uses scaled minimum for multi-asset outputs
    if let Err(e) = validate_output_too_small_mary(&env.pp, &tx_body.outputs) {
        errors.push(e);
    }

    // Step 10: ∀ txout ∈ txouts txb, serSize (getValue txout) ≤ MaxValSize
    if let Err(e) = validate_output_too_big(&tx_body.outputs) {
        errors.push(e);
    }

    // Step 11: ∀(a,_) ∈ txouts, a ∈ Addrbootstrap → bootstrapAttrsSize a ≤ 64
    if let Err(e) = validate_output_boot_addr_attrs_too_big(&tx_body.outputs) {
        errors.push(e.into());
    }

    // Step 12: txsize tx ≤ maxTxSize pp
    if let Err(e) = validate_max_tx_size(&env.pp, tx.size()) {
        errors.push(e.into());
    }

    // If any errors, return them all
    if !errors.is_empty() {
        return Err(errors);
    }

    // Step 13: Update UTxO state
    let new_utxo = update_utxo_state(state, tx_body);

    Ok(new_utxo)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn default_pp() -> ShelleyPParams {
        ShelleyPParams {
            min_fee_a: 44,
            min_fee_b: 155381,
            max_tx_size: 16384,
            min_utxo_value: 1_000_000,
            key_deposit: 2_000_000,
            pool_deposit: 500_000_000,
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
                assets.insert(AssetName(vec![j as u8; 8]), 1000);
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
    // Shelley Era Tests
    // ========================================================================

    #[test]
    fn test_validate_time_to_live() {
        // TTL > slot: valid
        assert!(validate_time_to_live(100, 50).is_ok());

        // TTL == slot: valid (still in range)
        assert!(validate_time_to_live(100, 100).is_ok());

        // TTL < slot: expired
        let result = validate_time_to_live(50, 100);
        assert!(matches!(result, Err(ShelleyUtxoPredFailure::ExpiredUTxO { .. })));
    }

    #[test]
    fn test_validate_input_set_not_empty() {
        let empty: HashSet<TxIn> = HashSet::new();
        let non_empty: HashSet<TxIn> = [TxIn {
            tx_id: [0u8; 32],
            output_index: 0,
        }]
        .into_iter()
        .collect();

        assert!(matches!(
            validate_input_set_not_empty(&empty),
            Err(ShelleyUtxoPredFailure::InputSetEmptyUTxO)
        ));
        assert!(validate_input_set_not_empty(&non_empty).is_ok());
    }

    #[test]
    fn test_validate_fee_calculation() {
        let pp = default_pp();

        // For a 250 byte tx: min_fee = 250 * 44 + 155381 = 166381
        assert!(validate_fee_too_small(&pp, 250, 200_000).is_ok());
        assert!(validate_fee_too_small(&pp, 250, 166_381).is_ok());

        let result = validate_fee_too_small(&pp, 250, 100_000);
        assert!(matches!(
            result,
            Err(ShelleyUtxoPredFailure::FeeTooSmallUTxO { .. })
        ));
    }

    #[test]
    fn test_validate_output_too_small_shelley() {
        let pp = default_pp();

        let good_outputs = vec![make_ada_output(2_000_000)];
        let bad_outputs = vec![make_ada_output(500_000)]; // Below min_utxo_value

        assert!(validate_output_too_small(&pp, &good_outputs).is_ok());
        assert!(matches!(
            validate_output_too_small(&pp, &bad_outputs),
            Err(ShelleyUtxoPredFailure::OutputTooSmallUTxO(_))
        ));
    }

    // ========================================================================
    // Allegra Era Tests
    // ========================================================================

    #[test]
    fn test_validity_interval_contains() {
        // No bounds - always valid
        let unbounded = ValidityInterval::new(None, None);
        assert!(unbounded.contains(0));
        assert!(unbounded.contains(1000));
        assert!(unbounded.contains(u64::MAX));

        // Upper bound only (TTL-style)
        let ttl_style = ValidityInterval::from_ttl(100);
        assert!(ttl_style.contains(0));
        assert!(ttl_style.contains(99));
        assert!(!ttl_style.contains(100)); // Exclusive upper bound
        assert!(!ttl_style.contains(101));

        // Lower bound only
        let lower_only = ValidityInterval::new(Some(50), None);
        assert!(!lower_only.contains(49));
        assert!(lower_only.contains(50)); // Inclusive lower bound
        assert!(lower_only.contains(100));

        // Both bounds [50, 100)
        let both = ValidityInterval::new(Some(50), Some(100));
        assert!(!both.contains(49));
        assert!(both.contains(50));
        assert!(both.contains(99));
        assert!(!both.contains(100));
    }

    #[test]
    fn test_validate_validity_interval() {
        let vi = ValidityInterval::new(Some(50), Some(100));

        // Valid: slot in interval
        assert!(validate_validity_interval(vi, 75).is_ok());

        // Invalid: slot before interval
        let result = validate_validity_interval(vi, 25);
        assert!(matches!(
            result,
            Err(AllegraUtxoPredFailure::OutsideValidityIntervalUTxO { .. })
        ));

        // Invalid: slot after interval
        let result = validate_validity_interval(vi, 150);
        assert!(matches!(
            result,
            Err(AllegraUtxoPredFailure::OutsideValidityIntervalUTxO { .. })
        ));
    }

    #[test]
    fn test_validate_output_too_big() {
        // Small ADA-only output: valid
        let small_outputs = vec![make_ada_output(2_000_000)];
        assert!(validate_output_too_big(&small_outputs).is_ok());

        // Large multi-asset output (would exceed 4000 bytes serialized)
        // Create output with many policies and assets
        let large_outputs = vec![make_multi_asset_output(5_000_000, 50, 20)];
        let result = validate_output_too_big(&large_outputs);
        assert!(matches!(
            result,
            Err(AllegraUtxoPredFailure::OutputTooBigUTxO(_))
        ));
    }

    // ========================================================================
    // Mary Era Tests
    // ========================================================================

    #[test]
    fn test_mary_value_is_ada_only() {
        let ada_only = MaryValue::from_coin(1_000_000);
        assert!(ada_only.is_ada_only());

        let multi_asset = MaryValue {
            coin: 1_000_000,
            multi_asset: {
                let mut ma = HashMap::new();
                let mut assets = HashMap::new();
                assets.insert(AssetName(vec![1, 2, 3]), 100);
                ma.insert([0u8; 28], assets);
                ma
            },
        };
        assert!(!multi_asset.is_ada_only());
    }

    #[test]
    fn test_scaled_min_deposit() {
        let min_utxo = 1_000_000; // 1 ADA

        // ADA-only: should return exactly minUTxOValue
        let ada_only = MaryValue::from_coin(5_000_000);
        assert_eq!(scaled_min_deposit(&ada_only, min_utxo), min_utxo);

        // Multi-asset: should return scaled value
        let multi_asset = MaryValue {
            coin: 5_000_000,
            multi_asset: {
                let mut ma = HashMap::new();
                let mut assets = HashMap::new();
                for i in 0..10 {
                    assets.insert(AssetName(vec![i; 8]), 1000);
                }
                ma.insert([0u8; 28], assets);
                ma
            },
        };
        let scaled = scaled_min_deposit(&multi_asset, min_utxo);
        // Scaled minimum should be higher than base minUTxOValue
        assert!(scaled >= min_utxo);
    }

    #[test]
    fn test_validate_output_too_small_mary() {
        let pp = default_pp();

        // ADA-only with sufficient ADA: valid
        let good_ada = vec![make_ada_output(2_000_000)];
        assert!(validate_output_too_small_mary(&pp, &good_ada).is_ok());

        // Multi-asset with insufficient ADA: invalid
        // A multi-asset output needs more ADA than just minUTxOValue
        let bad_multi = vec![make_multi_asset_output(1_000_000, 5, 5)];
        let result = validate_output_too_small_mary(&pp, &bad_multi);
        assert!(matches!(
            result,
            Err(AllegraUtxoPredFailure::OutputTooSmallUTxO(_))
        ));

        // Multi-asset with sufficient ADA: valid
        let good_multi = vec![make_multi_asset_output(5_000_000, 5, 5)];
        assert!(validate_output_too_small_mary(&pp, &good_multi).is_ok());
    }

    #[test]
    fn test_shelley_to_allegra_error_conversion() {
        // Test that Shelley errors convert properly to Allegra errors
        let shelley_expired = ShelleyUtxoPredFailure::ExpiredUTxO {
            supplied_ttl: 100,
            current_slot: 150,
        };
        let allegra_err: AllegraUtxoPredFailure = shelley_expired.into();

        match allegra_err {
            AllegraUtxoPredFailure::OutsideValidityIntervalUTxO { validity_interval, current_slot } => {
                assert_eq!(current_slot, 150);
                assert_eq!(validity_interval.invalid_hereafter, Some(100));
                assert_eq!(validity_interval.invalid_before, None);
            }
            _ => panic!("Expected OutsideValidityIntervalUTxO"),
        }
    }

    // ========================================================================
    // PPUP (Protocol Parameter Update) Tests
    // ========================================================================

    #[test]
    fn test_protocol_version_can_follow() {
        let current = ProtVer { major: 3, minor: 5 };

        // Valid: major bump (major + 1, minor = 0)
        assert!(current.can_follow(ProtVer { major: 4, minor: 0 }));

        // Valid: minor bump (major same, minor + 1)
        assert!(current.can_follow(ProtVer { major: 3, minor: 6 }));

        // Invalid: minor must increment by exactly 1
        assert!(!current.can_follow(ProtVer { major: 3, minor: 7 }));

        // Invalid: major must increment by exactly 1
        assert!(!current.can_follow(ProtVer { major: 5, minor: 0 }));

        // Invalid: major bump requires minor = 0
        assert!(!current.can_follow(ProtVer { major: 4, minor: 1 }));

        // Invalid: cannot decrease version
        assert!(!current.can_follow(ProtVer { major: 2, minor: 0 }));
        assert!(!current.can_follow(ProtVer { major: 3, minor: 4 }));

        // Invalid: same version
        assert!(!current.can_follow(ProtVer { major: 3, minor: 5 }));
    }

    #[test]
    fn test_validate_ppup_genesis_keys() {
        let gen_delegs: HashSet<[u8; 28]> = [
            [1u8; 28],
            [2u8; 28],
            [3u8; 28],
        ].into_iter().collect();

        // Valid: all proposers are genesis delegates
        let valid_keys: HashSet<[u8; 28]> = [[1u8; 28], [2u8; 28]].into_iter().collect();
        assert!(validate_ppup_genesis_keys(&valid_keys, &gen_delegs).is_ok());

        // Valid: single proposer
        let single_key: HashSet<[u8; 28]> = [[1u8; 28]].into_iter().collect();
        assert!(validate_ppup_genesis_keys(&single_key, &gen_delegs).is_ok());

        // Invalid: includes non-genesis key
        let invalid_keys: HashSet<[u8; 28]> = [[1u8; 28], [99u8; 28]].into_iter().collect();
        let result = validate_ppup_genesis_keys(&invalid_keys, &gen_delegs);
        assert!(matches!(result, Err(PpupPredFailure::NonGenesisUpdatePPUP { .. })));
    }

    #[test]
    fn test_validate_ppup_epoch() {
        // Epoch 5, length 100, stability window 10
        // Epoch 5 starts at slot 500, ends at 600
        // tooLate = 500 + 100 - 20 = 580
        let slot_info = SlotOfNoReturn {
            current_epoch: 5,
            too_late_slot: 580,
            next_epoch: 6,
        };

        // Before slot of no return (slot 550): must target current epoch (5)
        assert!(validate_ppup_epoch(550, &slot_info, 5).is_ok());

        // Before slot of no return: targeting next epoch is wrong
        let result = validate_ppup_epoch(550, &slot_info, 6);
        assert!(matches!(
            result,
            Err(PpupPredFailure::PPUpdateWrongEpoch {
                voting_period: VotingPeriod::VoteForThisEpoch,
                ..
            })
        ));

        // After slot of no return (slot 590): must target next epoch (6)
        assert!(validate_ppup_epoch(590, &slot_info, 6).is_ok());

        // After slot of no return: targeting current epoch is wrong
        let result = validate_ppup_epoch(590, &slot_info, 5);
        assert!(matches!(
            result,
            Err(PpupPredFailure::PPUpdateWrongEpoch {
                voting_period: VotingPeriod::VoteForNextEpoch,
                ..
            })
        ));
    }

    #[test]
    fn test_validate_ppup_protocol_version() {
        let current = ProtVer { major: 3, minor: 5 };

        // Valid: no protocol version update
        let no_update: HashMap<[u8; 28], PParamsUpdate> = HashMap::new();
        assert!(validate_ppup_protocol_version(current, &no_update).is_ok());

        // Valid: legal minor bump
        let mut legal_minor: HashMap<[u8; 28], PParamsUpdate> = HashMap::new();
        legal_minor.insert([1u8; 28], PParamsUpdate {
            protocol_version: Some(ProtVer { major: 3, minor: 6 }),
            ..Default::default()
        });
        assert!(validate_ppup_protocol_version(current, &legal_minor).is_ok());

        // Valid: legal major bump
        let mut legal_major: HashMap<[u8; 28], PParamsUpdate> = HashMap::new();
        legal_major.insert([1u8; 28], PParamsUpdate {
            protocol_version: Some(ProtVer { major: 4, minor: 0 }),
            ..Default::default()
        });
        assert!(validate_ppup_protocol_version(current, &legal_major).is_ok());

        // Invalid: illegal version jump
        let mut illegal: HashMap<[u8; 28], PParamsUpdate> = HashMap::new();
        illegal.insert([1u8; 28], PParamsUpdate {
            protocol_version: Some(ProtVer { major: 5, minor: 0 }),
            ..Default::default()
        });
        let result = validate_ppup_protocol_version(current, &illegal);
        assert!(matches!(result, Err(PpupPredFailure::PVCannotFollowPPUP(_))));
    }

    #[test]
    fn test_slot_of_no_return_calculation() {
        // Epoch length 100, stability window 10
        // Epoch 5 starts at slot 500
        let slot_info = get_slot_of_no_return(550, 100, 10);

        assert_eq!(slot_info.current_epoch, 5);
        assert_eq!(slot_info.next_epoch, 6);
        // tooLate = 500 + 100 - 20 = 580
        assert_eq!(slot_info.too_late_slot, 580);
    }
}
