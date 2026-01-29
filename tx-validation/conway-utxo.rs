// Conway Era UTXO Validation
//
// This module implements the Conway UTXO rule, which introduces on-chain governance
// (CIP-1694). The key insight is that Conway REUSES Babbage UTXO validation entirely.
//
// Reference: eras/conway/impl/src/Cardano/Ledger/Conway/Rules/Utxo.hs
//
// ============================================================================
// CONWAY UTXO RULE SUMMARY
// ============================================================================
//
// Key points:
// - Conway UTXO validation is IDENTICAL to Babbage
// - No new Phase 1 validations added
// - Governance features affect UTXOS (Phase 2), UTXOW, and LEDGER rules
// - Error type is FLATTENED (all errors in one enum, not nested)
//
// ============================================================================

use std::collections::{HashMap, HashSet};

// Import types from previous eras
pub use super::babbage_utxo::{
    BabbageDatum, BabbagePParams, BabbageScript, BabbageTx, BabbageTxBody, BabbageTxOut,
    BabbageUTxO, BabbageUtxoEnv, BabbageUtxoPredFailure, NativeScriptBytes, SizedTxOut,
};
pub use super::alonzo_utxo::{
    AlonzoUtxoPredFailure, ExUnits, ValidityInterval, Value,
};
pub use super::shelley_utxo::{
    Addr, Certificate, CertState, Coin, Credential, Network, NativeScript,
    RewardAccount, SlotNo, TxIn, TxSize,
};

// ============================================================================
// Conway-Specific Type Definitions
// ============================================================================

/// Conway adds governance-related fields to the transaction body
#[derive(Debug, Clone)]
pub struct ConwayTxBody {
    /// All Babbage fields (via composition, not inheritance)
    pub babbage: BabbageTxBody,

    /// Voting procedures (NEW in Conway)
    pub voting_procedures: VotingProcedures,

    /// Proposal procedures (NEW in Conway)
    pub proposal_procedures: Vec<ProposalProcedure>,

    /// Current treasury value for validation (optional)
    pub current_treasury_value: Option<Coin>,

    /// Donation to treasury (NEW in Conway)
    pub treasury_donation: Coin,
}

/// Voting procedures map
#[derive(Debug, Clone, Default)]
pub struct VotingProcedures {
    pub votes: HashMap<Voter, HashMap<GovActionId, VotingProcedure>>,
}

/// Who is voting
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Voter {
    /// Constitutional Committee member
    CommitteeVoter(Credential),
    /// Delegated Representative
    DRepVoter(Credential),
    /// Stake Pool Operator
    StakePoolVoter([u8; 28]),
}

/// Governance action identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GovActionId {
    pub tx_id: [u8; 32],
    pub gov_action_ix: u32,
}

/// Individual voting procedure
#[derive(Debug, Clone)]
pub struct VotingProcedure {
    pub vote: Vote,
    pub anchor: Option<Anchor>,
}

/// Vote choice
#[derive(Debug, Clone)]
pub enum Vote {
    No,
    Yes,
    Abstain,
}

/// Proposal procedure
#[derive(Debug, Clone)]
pub struct ProposalProcedure {
    pub deposit: Coin,
    pub return_addr: RewardAccount,
    pub gov_action: GovAction,
    pub anchor: Anchor,
}

/// Governance action types
#[derive(Debug, Clone)]
pub enum GovAction {
    ParameterChange {
        prev_action_id: Option<GovActionId>,
        update: ProtocolParamUpdate,
        policy_hash: Option<[u8; 28]>,
    },
    HardForkInitiation {
        prev_action_id: Option<GovActionId>,
        protocol_version: (u32, u32),
    },
    TreasuryWithdrawals {
        withdrawals: HashMap<RewardAccount, Coin>,
        policy_hash: Option<[u8; 28]>,
    },
    NoConfidence {
        prev_action_id: Option<GovActionId>,
    },
    UpdateCommittee {
        prev_action_id: Option<GovActionId>,
        members_to_remove: HashSet<Credential>,
        members_to_add: HashMap<Credential, u64>, // credential -> expiration epoch
        quorum: Rational,
    },
    NewConstitution {
        prev_action_id: Option<GovActionId>,
        constitution: Constitution,
    },
    InfoAction,
}

/// Protocol parameter update (simplified)
#[derive(Debug, Clone, Default)]
pub struct ProtocolParamUpdate {
    // Would contain all updatable protocol parameters
}

/// Constitution
#[derive(Debug, Clone)]
pub struct Constitution {
    pub anchor: Anchor,
    pub script: Option<[u8; 28]>,
}

/// Anchor (URL + hash)
#[derive(Debug, Clone)]
pub struct Anchor {
    pub url: String,
    pub data_hash: [u8; 32],
}

/// Rational number for quorum
#[derive(Debug, Clone, Copy)]
pub struct Rational {
    pub numerator: u64,
    pub denominator: u64,
}

/// Conway Transaction
#[derive(Debug, Clone)]
pub struct ConwayTx {
    pub body: ConwayTxBody,
    pub wits: ConwayTxWits,
    pub is_valid: bool,
    pub auxiliary_data: Option<Vec<u8>>,
}

/// Conway witnesses (same as Alonzo but used for Conway)
pub type ConwayTxWits = super::alonzo_utxo::AlonzoTxWits;

/// Conway Protocol Parameters
#[derive(Debug, Clone)]
pub struct ConwayPParams {
    /// All Babbage parameters
    pub babbage: BabbagePParams,

    // Conway governance parameters
    /// DRep deposit
    pub drep_deposit: Coin,
    /// DRep inactivity period
    pub drep_activity: u64,
    /// Governance action deposit
    pub gov_action_deposit: Coin,
    /// Governance action lifetime
    pub gov_action_lifetime: u64,
    /// Committee minimum size
    pub committee_min_size: u32,
    /// Committee maximum term
    pub committee_max_term: u64,
}

// ============================================================================
// Error Types - FLATTENED Structure
// ============================================================================

/// Conway UTXO predicate failures
///
/// Reference: eras/conway/impl/src/Cardano/Ledger/Conway/Rules/Utxo.hs:86-158
///
/// Conway uses a FLATTENED error structure - all errors are defined directly
/// in this enum rather than being nested/wrapped from previous eras.
///
/// ```haskell
/// data ConwayUtxoPredFailure era
///   = UtxosFailure (PredicateFailure (EraRule "UTXOS" era))
///   | BadInputsUTxO (Set TxIn)
///   | OutsideValidityIntervalUTxO ValidityInterval SlotNo
///   | MaxTxSizeUTxO (Mismatch RelLTEQ Word32)
///   | InputSetEmptyUTxO
///   | FeeTooSmallUTxO (Mismatch RelGTEQ Coin)
///   | ValueNotConservedUTxO (Mismatch RelEQ (Value era))
///   | WrongNetwork Network (Set Addr)
///   | WrongNetworkWithdrawal Network (Set RewardAccount)
///   | OutputTooSmallUTxO [TxOut era]
///   | OutputBootAddrAttrsTooBig [TxOut era]
///   | OutputTooBigUTxO [(Int, Int, TxOut era)]
///   | InsufficientCollateral DeltaCoin Coin
///   | ScriptsNotPaidUTxO (UTxO era)
///   | ExUnitsTooBigUTxO (Mismatch RelLTEQ ExUnits)
///   | CollateralContainsNonADA (Value era)
///   | WrongNetworkInTxBody (Mismatch RelEQ Network)
///   | OutsideForecast SlotNo
///   | TooManyCollateralInputs (Mismatch RelLTEQ Natural)
///   | NoCollateralInputs
///   | IncorrectTotalCollateralField DeltaCoin Coin
///   | BabbageOutputTooSmallUTxO [(TxOut era, Coin)]
///   | BabbageNonDisjointRefInputs (NonEmpty TxIn)
/// ```
#[derive(Debug, Clone)]
pub enum ConwayUtxoPredFailure {
    // ========== Phase 2 (UTXOS) failure wrapper ==========

    /// Subtransition failure (Phase 2)
    UtxosFailure(Box<ConwayUtxosPredFailure>),

    // ========== From Shelley ==========

    /// Some inputs don't exist in UTxO
    BadInputsUTxO(HashSet<TxIn>),

    /// Transaction outside validity interval
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

    /// Outputs below minimum value (legacy Shelley style)
    OutputTooSmallUTxO(Vec<BabbageTxOut>),

    /// Bootstrap address attributes too big
    OutputBootAddrAttrsTooBig(Vec<BabbageTxOut>),

    // ========== From Alonzo ==========

    /// Output value serialization too big
    OutputTooBigUTxO(Vec<(usize, u32, BabbageTxOut)>),

    /// Collateral insufficient
    InsufficientCollateral {
        provided: i64,
        required: Coin,
    },

    /// Script-locked UTxOs used as collateral
    ScriptsNotPaidUTxO(HashMap<TxIn, BabbageTxOut>),

    /// Execution units exceed maximum
    ExUnitsTooBigUTxO {
        supplied: ExUnits,
        maximum: ExUnits,
    },

    /// Collateral contains native tokens
    CollateralContainsNonADA(Value),

    /// Wrong network ID in transaction body
    WrongNetworkInTxBody {
        supplied: Network,
        expected: Network,
    },

    /// Validity interval end outside forecast
    OutsideForecast(SlotNo),

    /// Too many collateral inputs
    TooManyCollateralInputs {
        supplied: u32,
        maximum: u32,
    },

    /// No collateral when needed
    NoCollateralInputs,

    // ========== From Babbage ==========

    /// totalCollateral field mismatch
    IncorrectTotalCollateralField {
        computed: i64,
        declared: Coin,
    },

    /// Outputs below minimum with minimum included
    BabbageOutputTooSmallUTxO(Vec<(BabbageTxOut, Coin)>),

    /// Reference inputs overlap with spending inputs
    BabbageNonDisjointRefInputs(Vec<TxIn>),
}

/// UTXOS (Phase 2) predicate failure for Conway
#[derive(Debug, Clone)]
pub enum ConwayUtxosPredFailure {
    /// isValid flag doesn't match script execution result
    ValidationTagMismatch(bool),
    /// Errors during script data collection
    CollectErrors(Vec<CollectError>),
}

/// Script collection error
#[derive(Debug, Clone)]
pub enum CollectError {
    NoRedeemer(ScriptPurpose),
    NoWitness([u8; 28]),
    NoCostModel(u8),
    BadTranslation(String),
}

/// Script purpose (Conway adds Voting and Proposing)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ScriptPurpose {
    Spending(TxIn),
    Minting([u8; 28]),
    Rewarding(RewardAccount),
    Certifying(usize),
    /// NEW in Conway: authorize voting
    Voting(Voter),
    /// NEW in Conway: authorize proposals
    Proposing(usize),
}

// ============================================================================
// Error Conversion Functions
// ============================================================================

/// Convert Babbage error to Conway (flattened)
///
/// Reference: eras/conway/impl/src/Cardano/Ledger/Conway/Rules/Utxo.hs:345-354
///
/// ```haskell
/// babbageToConwayUtxoPredFailure ::
///   BabbageUtxoPredFailure era ->
///   ConwayUtxoPredFailure era
/// babbageToConwayUtxoPredFailure = \case
///   Babbage.AlonzoInBabbageUtxoPredFailure a -> alonzoToConwayUtxoPredFailure a
///   Babbage.IncorrectTotalCollateralField c1 c2 -> IncorrectTotalCollateralField c1 c2
///   Babbage.BabbageOutputTooSmallUTxO ts -> BabbageOutputTooSmallUTxO ts
///   Babbage.BabbageNonDisjointRefInputs ts -> BabbageNonDisjointRefInputs ts
/// ```
pub fn babbage_to_conway_failure(
    failure: BabbageUtxoPredFailure,
) -> ConwayUtxoPredFailure {
    match failure {
        BabbageUtxoPredFailure::AlonzoInBabbage(alonzo) => {
            alonzo_to_conway_failure(alonzo)
        }
        BabbageUtxoPredFailure::IncorrectTotalCollateralField { computed, declared } => {
            ConwayUtxoPredFailure::IncorrectTotalCollateralField { computed, declared }
        }
        BabbageUtxoPredFailure::BabbageOutputTooSmallUTxO(outputs) => {
            ConwayUtxoPredFailure::BabbageOutputTooSmallUTxO(outputs)
        }
        BabbageUtxoPredFailure::BabbageNonDisjointRefInputs(inputs) => {
            ConwayUtxoPredFailure::BabbageNonDisjointRefInputs(inputs)
        }
    }
}

/// Convert Alonzo error to Conway (flattened)
///
/// Reference: eras/conway/impl/src/Cardano/Ledger/Conway/Rules/Utxo.hs:355-380
pub fn alonzo_to_conway_failure(
    failure: AlonzoUtxoPredFailure,
) -> ConwayUtxoPredFailure {
    match failure {
        AlonzoUtxoPredFailure::BadInputsUTxO(inputs) => {
            ConwayUtxoPredFailure::BadInputsUTxO(inputs)
        }
        AlonzoUtxoPredFailure::OutsideValidityIntervalUTxO { validity_interval, current_slot } => {
            ConwayUtxoPredFailure::OutsideValidityIntervalUTxO { validity_interval, current_slot }
        }
        AlonzoUtxoPredFailure::MaxTxSizeUTxO { actual_size, max_size } => {
            ConwayUtxoPredFailure::MaxTxSizeUTxO { actual_size, max_size }
        }
        AlonzoUtxoPredFailure::InputSetEmptyUTxO => {
            ConwayUtxoPredFailure::InputSetEmptyUTxO
        }
        AlonzoUtxoPredFailure::FeeTooSmallUTxO { supplied_fee, minimum_fee } => {
            ConwayUtxoPredFailure::FeeTooSmallUTxO { supplied_fee, minimum_fee }
        }
        AlonzoUtxoPredFailure::ValueNotConservedUTxO { consumed, produced } => {
            ConwayUtxoPredFailure::ValueNotConservedUTxO { consumed, produced }
        }
        AlonzoUtxoPredFailure::WrongNetwork { expected, wrong_addresses } => {
            ConwayUtxoPredFailure::WrongNetwork { expected, wrong_addresses }
        }
        AlonzoUtxoPredFailure::WrongNetworkWithdrawal { expected, wrong_accounts } => {
            ConwayUtxoPredFailure::WrongNetworkWithdrawal { expected, wrong_accounts }
        }
        AlonzoUtxoPredFailure::OutputTooSmallUTxO(outputs) => {
            ConwayUtxoPredFailure::OutputTooSmallUTxO(outputs)
        }
        AlonzoUtxoPredFailure::OutputBootAddrAttrsTooBig(outputs) => {
            ConwayUtxoPredFailure::OutputBootAddrAttrsTooBig(outputs)
        }
        AlonzoUtxoPredFailure::UtxosFailure(_) => {
            // Phase 2 failures handled separately
            ConwayUtxoPredFailure::UtxosFailure(Box::new(
                ConwayUtxosPredFailure::ValidationTagMismatch(false),
            ))
        }
        AlonzoUtxoPredFailure::OutputTooBigUTxO(outputs) => {
            ConwayUtxoPredFailure::OutputTooBigUTxO(outputs)
        }
        AlonzoUtxoPredFailure::InsufficientCollateral { provided, required } => {
            ConwayUtxoPredFailure::InsufficientCollateral { provided, required }
        }
        AlonzoUtxoPredFailure::ScriptsNotPaidUTxO(utxo) => {
            ConwayUtxoPredFailure::ScriptsNotPaidUTxO(convert_alonzo_utxo(utxo))
        }
        AlonzoUtxoPredFailure::ExUnitsTooBigUTxO { supplied, maximum } => {
            ConwayUtxoPredFailure::ExUnitsTooBigUTxO { supplied, maximum }
        }
        AlonzoUtxoPredFailure::CollateralContainsNonADA(value) => {
            ConwayUtxoPredFailure::CollateralContainsNonADA(value)
        }
        AlonzoUtxoPredFailure::WrongNetworkInTxBody { supplied, expected } => {
            ConwayUtxoPredFailure::WrongNetworkInTxBody { supplied, expected }
        }
        AlonzoUtxoPredFailure::OutsideForecast(slot) => {
            ConwayUtxoPredFailure::OutsideForecast(slot)
        }
        AlonzoUtxoPredFailure::TooManyCollateralInputs { supplied, maximum } => {
            ConwayUtxoPredFailure::TooManyCollateralInputs { supplied, maximum }
        }
        AlonzoUtxoPredFailure::NoCollateralInputs => {
            ConwayUtxoPredFailure::NoCollateralInputs
        }
    }
}

/// Helper to convert Alonzo UTxO format to Babbage
fn convert_alonzo_utxo(
    alonzo_utxo: HashMap<TxIn, super::alonzo_utxo::AlonzoTxOut>,
) -> HashMap<TxIn, BabbageTxOut> {
    alonzo_utxo
        .into_iter()
        .map(|(k, v)| {
            (
                k,
                BabbageTxOut {
                    address: v.address,
                    value: v.value,
                    datum: match v.datum_hash {
                        Some(h) => BabbageDatum::Hash(h),
                        None => BabbageDatum::None,
                    },
                    reference_script: None,
                },
            )
        })
        .collect()
}

// ============================================================================
// Main Transition Rule
// ============================================================================

/// Conway UTXO Transition Rule
///
/// Reference: eras/conway/impl/src/Cardano/Ledger/Conway/Rules/Utxo.hs:252-256
///
/// ```haskell
/// instance ... STS (ConwayUTXO era) where
///   ...
///   transitionRules = [Babbage.utxoTransition @era]
/// ```
///
/// KEY INSIGHT: Conway REUSES Babbage validation entirely!
/// The only difference is error type conversion (flattening).
pub fn conway_utxo_transition(
    env: &ConwayUtxoEnv,
    state: &ConwayUTxOState,
    tx: &ConwayTx,
) -> Result<ConwayUTxOState, Vec<ConwayUtxoPredFailure>> {
    // Conway uses Babbage validation
    // Convert Conway tx to Babbage format
    let babbage_tx = BabbageTx {
        body: tx.body.babbage.clone(),
        wits: tx.wits.clone(),
        is_valid: tx.is_valid,
        auxiliary_data: tx.auxiliary_data.clone(),
    };

    let babbage_env = BabbageUtxoEnv {
        slot: env.slot,
        pp: env.pp.babbage.clone(),
        cert_state: env.cert_state.clone(),
        network_id: env.network_id,
    };

    let babbage_state = super::babbage_utxo::BabbageUTxOState {
        utxo: state.utxo.clone(),
        deposited: state.deposited,
        fees: state.fees,
    };

    // Run Babbage validation
    match super::babbage_utxo::babbage_utxo_transition(&babbage_env, &babbage_state, &babbage_tx) {
        Ok(new_state) => Ok(ConwayUTxOState {
            utxo: new_state.utxo,
            deposited: new_state.deposited,
            fees: new_state.fees,
        }),
        Err(babbage_errors) => {
            // Convert Babbage errors to Conway (flattened)
            let conway_errors: Vec<ConwayUtxoPredFailure> = babbage_errors
                .into_iter()
                .map(babbage_to_conway_failure)
                .collect();
            Err(conway_errors)
        }
    }
}

/// Conway UTXO Environment
#[derive(Debug, Clone)]
pub struct ConwayUtxoEnv {
    pub slot: SlotNo,
    pub pp: ConwayPParams,
    pub cert_state: CertState,
    pub network_id: Network,
}

/// Conway UTXO State
#[derive(Debug, Clone)]
pub struct ConwayUTxOState {
    pub utxo: BabbageUTxO,
    pub deposited: Coin,
    pub fees: Coin,
}

// ============================================================================
// Governance-Related Functions (affect other rules, not UTXO)
// ============================================================================

/// Get scripts needed for voting (NEW in Conway)
///
/// Reference: eras/conway/impl/src/Cardano/Ledger/Conway/UTxO.hs:78-86
///
/// This affects UTXOS (Phase 2) not UTXO (Phase 1)
pub fn get_voting_scripts_needed(
    voting_procedures: &VotingProcedures,
) -> Vec<(ScriptPurpose, [u8; 28])> {
    voting_procedures
        .votes
        .keys()
        .filter_map(|voter| {
            let script_hash = match voter {
                Voter::CommitteeVoter(Credential::ScriptHash(h)) => Some(*h),
                Voter::DRepVoter(Credential::ScriptHash(h)) => Some(*h),
                Voter::StakePoolVoter(_) => None, // SPOs can't be scripts
                _ => None,
            };
            script_hash.map(|h| (ScriptPurpose::Voting(voter.clone()), h))
        })
        .collect()
}

/// Get scripts needed for proposing (NEW in Conway)
///
/// Reference: eras/conway/impl/src/Cardano/Ledger/Conway/UTxO.hs:88-102
pub fn get_proposing_scripts_needed(
    proposals: &[ProposalProcedure],
) -> Vec<(ScriptPurpose, [u8; 28])> {
    proposals
        .iter()
        .enumerate()
        .filter_map(|(ix, proposal)| {
            let script_hash = match &proposal.gov_action {
                GovAction::ParameterChange { policy_hash: Some(h), .. } => Some(*h),
                GovAction::TreasuryWithdrawals { policy_hash: Some(h), .. } => Some(*h),
                _ => None,
            };
            script_hash.map(|h| (ScriptPurpose::Proposing(ix), h))
        })
        .collect()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_babbage_to_conway_error_conversion() {
        // Test that Babbage errors are correctly flattened
        let babbage_error = BabbageUtxoPredFailure::BabbageNonDisjointRefInputs(vec![
            TxIn { tx_id: [1u8; 32], output_index: 0 },
        ]);

        let conway_error = babbage_to_conway_failure(babbage_error);

        assert!(matches!(
            conway_error,
            ConwayUtxoPredFailure::BabbageNonDisjointRefInputs(_)
        ));
    }

    #[test]
    fn test_alonzo_to_conway_error_conversion() {
        let alonzo_error = AlonzoUtxoPredFailure::NoCollateralInputs;
        let conway_error = alonzo_to_conway_failure(alonzo_error);

        assert!(matches!(
            conway_error,
            ConwayUtxoPredFailure::NoCollateralInputs
        ));
    }

    #[test]
    fn test_nested_error_flattening() {
        // Test that nested Alonzo-in-Babbage errors are properly flattened
        let nested_error = BabbageUtxoPredFailure::AlonzoInBabbage(
            AlonzoUtxoPredFailure::InputSetEmptyUTxO,
        );

        let conway_error = babbage_to_conway_failure(nested_error);

        assert!(matches!(
            conway_error,
            ConwayUtxoPredFailure::InputSetEmptyUTxO
        ));
    }

    #[test]
    fn test_voting_scripts_needed() {
        let mut votes = HashMap::new();
        votes.insert(
            Voter::DRepVoter(Credential::ScriptHash([42u8; 28])),
            HashMap::new(),
        );
        votes.insert(
            Voter::DRepVoter(Credential::KeyHash([0u8; 28])),
            HashMap::new(),
        );

        let voting_procedures = VotingProcedures { votes };

        let scripts_needed = get_voting_scripts_needed(&voting_procedures);

        // Should only find the script-based voter
        assert_eq!(scripts_needed.len(), 1);
        assert_eq!(scripts_needed[0].1, [42u8; 28]);
    }
}
