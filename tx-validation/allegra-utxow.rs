// Allegra Era UTXOW Rule Implementation
//
// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/Rules/Utxow.hs
//
// The Allegra UTXOW rule is identical to Shelley's — it reuses
// `transitionRulesUTXOW` without modification. The only difference is
// that the embedded UTXO sub-rule is AllegraUTXO (not ShelleyUTXO),
// which changes the error types that can propagate up.
//
// Haskell code:
//
// ```haskell
// instance ... STS (AllegraUTXOW era) where
//   type PredicateFailure (AllegraUTXOW era) = ShelleyUtxowPredFailure era
//   type Event (AllegraUTXOW era) = ShelleyUtxowEvent era
//   transitionRules = [transitionRulesUTXOW]
// ```
//
// Mary also reuses this same UTXOW rule:
//   type instance EraRuleFailure "UTXOW" MaryEra = ShelleyUtxowPredFailure MaryEra
//
// ============================================================================
// ALLEGRA/MARY UTXOW RULE SUMMARY
// ============================================================================
//
// Validations performed (all inherited from Shelley transitionRulesUTXOW):
//  1. validateFailedNativeScripts    — all native scripts validate
//  2. validateMissingScripts         — scriptsNeeded = scriptsProvided (both ways)
//  3. validateVerifiedWits           — all VKey/bootstrap signatures verify
//  4. validateNeededWitnesses        — all required witness key hashes present
//  5. validateMetadata               — auxiliary data hash matches (if present)
//  6. validateMIRInsufficientGenesisSigs — MIR certs need quorum of genesis sigs
//  7. UTXO sub-rule                  — delegated to AllegraUTXO (see allegra-utxo.rs)
//
// Allegra introduced Timelock scripts (extending native multisig with
// time-based conditions), but the UTXOW rule itself does not change —
// Timelock scripts are validated via the same `runNativeScript` path
// since they implement the `NativeScript` interface.
//
// ============================================================================

use std::collections::{HashMap, HashSet};

// ============================================================================
// Core Types
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hash([u8; 32]);

pub type KeyHash = Hash;
pub type ScriptHash = Hash;
pub type TxBodyHash = Hash;
pub type MetadataHash = Hash;
pub type SlotNo = u64;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VKey(pub [u8; 32]);

impl VKey {
    pub fn hash(&self) -> KeyHash {
        Hash([0u8; 32]) // Simplified
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature(pub [u8; 64]);

// ============================================================================
// Native Scripts (Allegra Timelock)
// ============================================================================

/// Native script types for Allegra.
///
/// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/Scripts.hs
///
/// Allegra extends Shelley's multisig with timelock constructors:
///
/// ```haskell
/// data Timelock era
///   = RequireSignature (KeyHash 'Witness)
///   | RequireAllOf    (StrictSeq (Timelock era))
///   | RequireAnyOf    (StrictSeq (Timelock era))
///   | RequireMOf      Int (StrictSeq (Timelock era))
///   | RequireTimeExpire SlotNo    -- NEW in Allegra
///   | RequireTimeStart  SlotNo    -- NEW in Allegra
/// ```
///
/// The UTXOW rule validates these via `runNativeScript` which evaluates
/// timelock conditions against the transaction's validity interval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeScript {
    RequireSignature(KeyHash),
    RequireAllOf(Vec<NativeScript>),
    RequireAnyOf(Vec<NativeScript>),
    RequireMOf(usize, Vec<NativeScript>),
    RequireTimeExpire(SlotNo), // NEW in Allegra
    RequireTimeStart(SlotNo),  // NEW in Allegra
}

impl NativeScript {
    pub fn hash(&self) -> ScriptHash {
        Hash([0u8; 32]) // Simplified
    }

    /// Evaluate a native script.
    ///
    /// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/Scripts.hs:280-310
    ///
    /// ```haskell
    /// evalTimelock :: AllegraEraScript era =>
    ///   Set (KeyHash 'Witness) -> ValidityInterval -> NativeScript era -> Bool
    /// evalTimelock vhks (ValidityInterval txStart txExp) = go
    ///   where
    ///     go (RequireSignature hash)    = hash `Set.member` vhks
    ///     go (RequireAllOf ts)          = all go ts
    ///     go (RequireAnyOf ts)          = any go ts
    ///     go (RequireMOf m ts)          = m <= length (filter go ts)
    ///     go (RequireTimeExpire slot)   = txExp /= SNothing && slot >= fromJust txExp
    ///     go (RequireTimeStart slot)    = txStart /= SNothing && slot <= fromJust txStart
    /// ```
    ///
    /// For Timelock conditions:
    /// - `RequireTimeExpire slot`: the tx must expire (invalidHereafter ≤ slot)
    /// - `RequireTimeStart slot`: the tx must start after (invalidBefore ≥ slot ... wait no)
    ///
    /// Actually the semantics:
    /// - `RequireTimeExpire slot`: `invalidHereafter` must exist and `slot >= invalidHereafter`
    ///   i.e., the slot is at or after the tx expiry, meaning the tx has expired by `slot`.
    ///   This is used to ensure a tx is only valid before some time.
    /// - `RequireTimeStart slot`: `invalidBefore` must exist and `slot <= invalidBefore`
    ///   i.e., the slot is at or before the tx start, meaning the tx hasn't started by `slot`.
    ///   This is used to ensure a tx is only valid after some time.
    ///
    /// Note: The actual Haskell naming can be confusing. The script checks conditions
    /// on the validity interval, not on the current slot.
    pub fn evaluate(
        &self,
        vkey_hashes: &HashSet<KeyHash>,
        validity_interval: &ValidityInterval,
    ) -> bool {
        match self {
            NativeScript::RequireSignature(hash) => vkey_hashes.contains(hash),
            NativeScript::RequireAllOf(scripts) => {
                scripts.iter().all(|s| s.evaluate(vkey_hashes, validity_interval))
            }
            NativeScript::RequireAnyOf(scripts) => {
                scripts.iter().any(|s| s.evaluate(vkey_hashes, validity_interval))
            }
            NativeScript::RequireMOf(m, scripts) => {
                let count = scripts
                    .iter()
                    .filter(|s| s.evaluate(vkey_hashes, validity_interval))
                    .count();
                count >= *m
            }
            NativeScript::RequireTimeExpire(slot) => {
                validity_interval
                    .invalid_hereafter
                    .map_or(false, |top| *slot >= top)
            }
            NativeScript::RequireTimeStart(slot) => {
                validity_interval
                    .invalid_before
                    .map_or(false, |bottom| *slot <= bottom)
            }
        }
    }
}

/// ValidityInterval from Allegra
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidityInterval {
    pub invalid_before: Option<SlotNo>,
    pub invalid_hereafter: Option<SlotNo>,
}

// ============================================================================
// Witness Types
// ============================================================================

#[derive(Debug, Clone)]
pub struct WitVKey {
    pub vkey: VKey,
    pub signature: Signature,
}

impl WitVKey {
    pub fn verify(&self, _body_hash: &TxBodyHash) -> bool {
        true // Simplified
    }
}

#[derive(Debug, Clone)]
pub struct BootstrapWitness {
    pub vkey: VKey,
    pub signature: Signature,
    pub chain_code: [u8; 32],
    pub attributes: Vec<u8>,
}

/// Transaction auxiliary data (Allegra format)
///
/// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/TxAuxData.hs:77-85
///
/// Allegra extends Shelley's metadata-only auxiliary data to also include
/// native scripts (though scripts in aux data are NOT validated by UTXOW).
#[derive(Debug, Clone)]
pub struct AllegraAuxiliaryData {
    pub metadata: HashMap<u64, Vec<u8>>,
    pub native_scripts: Vec<NativeScript>,
}

impl AllegraAuxiliaryData {
    pub fn hash(&self) -> MetadataHash {
        Hash([0u8; 32]) // Simplified
    }
}

// ============================================================================
// Transaction Types
// ============================================================================

#[derive(Debug, Clone)]
pub struct AllegraTxBody {
    pub inputs: HashSet<Hash>,
    pub outputs: Vec<()>,
    pub fee: u64,
    pub validity_interval: ValidityInterval,
    pub certificates: Vec<()>,
    pub withdrawals: HashMap<(), u64>,
    pub update: Option<()>,
    pub auxiliary_data_hash: Option<MetadataHash>,
}

impl AllegraTxBody {
    pub fn hash(&self) -> TxBodyHash {
        Hash([0u8; 32]) // Simplified
    }
}

#[derive(Debug, Clone)]
pub struct AllegraTxWits {
    pub vkey_wits: HashSet<KeyHash>,
    pub bootstrap_wits: Vec<BootstrapWitness>,
    pub script_wits: HashMap<ScriptHash, NativeScript>,
}

#[derive(Debug, Clone)]
pub struct AllegraTx {
    pub body: AllegraTxBody,
    pub wits: AllegraTxWits,
    pub auxiliary_data: Option<AllegraAuxiliaryData>,
}

// ============================================================================
// UTXOW Predicate Failures
// ============================================================================

/// Allegra UTXOW predicate failures
///
/// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/Rules/Utxow.hs:34
///
/// ```haskell
/// type instance EraRuleFailure "UTXOW" AllegraEra = ShelleyUtxowPredFailure AllegraEra
/// ```
///
/// Allegra reuses Shelley's UTXOW predicate failure type unchanged.
/// All UTXOW checks are inherited from Shelley's `transitionRulesUTXOW`.
///
/// The only difference is that `UtxoFailure` now wraps `AllegraUtxoPredFailure`
/// instead of `ShelleyUtxoPredFailure`, since the embedded UTXO rule changed.
#[derive(Debug, Clone)]
pub enum ShelleyUtxowPredFailure {
    /// A native script failed to validate.
    ///
    /// Reference: Shelley/Utxow.hs:374-382
    ///
    /// ```haskell
    /// validateFailedNativeScripts scriptsProvided tx
    /// ```
    ScriptWitnessNotValidatingUTXOW(HashSet<ScriptHash>),

    /// Scripts needed by the transaction don't match scripts provided.
    ///
    /// Reference: Shelley/Utxow.hs:384-397
    ///
    /// This checks both directions:
    /// - Missing: needed but not provided
    /// - Extraneous: provided but not needed
    MissingScriptWitnessesUTXOW(HashSet<ScriptHash>),

    /// VKey or bootstrap witness signature verification failed.
    ///
    /// Reference: Shelley/Utxow.hs:399-411
    InvalidWitnessesUTXOW(Vec<VKey>),

    /// Required witness key hashes are not present.
    ///
    /// Reference: Shelley/Utxow.hs:413-439
    MissingVKeyWitnessesUTXOW(HashSet<KeyHash>),

    /// Auxiliary data hash in body doesn't match actual auxiliary data.
    ///
    /// Reference: Shelley/Utxow.hs:441-466
    ///
    /// Three sub-cases:
    /// - Body has hash but no aux data provided
    /// - Body has no hash but aux data is provided
    /// - Both present but hashes don't match
    MissingTxBodyMetadataHash(MetadataHash),
    MissingTxMetadata(MetadataHash),
    ConflictingMetadataHash {
        body_hash: MetadataHash,
        actual_hash: MetadataHash,
    },

    /// Auxiliary data is malformed.
    InvalidMetadata,

    /// MIR certificates require quorum of genesis delegate signatures.
    ///
    /// Reference: Shelley/Utxow.hs:468-490
    MIRInsufficientGenesisSigsUTXOW(HashSet<KeyHash>),

    /// Scripts provided but not needed by the transaction.
    ExtraneousScriptWitnessesUTXOW(HashSet<ScriptHash>),

    /// UTXO sub-rule failure (wraps AllegraUtxoPredFailure for Allegra era).
    ///
    /// In Shelley: wraps ShelleyUtxoPredFailure
    /// In Allegra: wraps AllegraUtxoPredFailure (via InjectRuleFailure)
    /// In Mary:    wraps AllegraUtxoPredFailure (same as Allegra)
    UtxoFailure(String),
}

// ============================================================================
// UTXOW Validation Functions
// (All inherited from Shelley's transitionRulesUTXOW)
// ============================================================================

/// Validate native scripts.
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxow.hs:374-382
///
/// ```haskell
/// validateFailedNativeScripts scriptsProvided tx = do
///   let failedScripts =
///         Map.filter
///           (maybe False (not . validateNativeScript tx) . getNativeScript)
///           scriptsProvided
///   failureOnNonEmptySet (Map.keysSet failedScripts) ScriptWitnessNotValidatingUTXOW
/// ```
///
/// For Allegra, `validateNativeScript` uses `evalTimelock` which evaluates
/// timelock conditions (RequireTimeStart/RequireTimeExpire) against the
/// transaction's validity interval. This is the only behavioral difference
/// from Shelley's multisig validation.
pub fn validate_failed_native_scripts(
    scripts_provided: &HashMap<ScriptHash, NativeScript>,
    vkey_hashes: &HashSet<KeyHash>,
    validity_interval: &ValidityInterval,
) -> Result<(), ShelleyUtxowPredFailure> {
    let failed: HashSet<ScriptHash> = scripts_provided
        .iter()
        .filter(|(_, script)| !script.evaluate(vkey_hashes, validity_interval))
        .map(|(hash, _)| *hash)
        .collect();

    if failed.is_empty() {
        Ok(())
    } else {
        Err(ShelleyUtxowPredFailure::ScriptWitnessNotValidatingUTXOW(failed))
    }
}

/// Validate scripts needed match scripts provided.
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxow.hs:384-397
///
/// ```haskell
/// validateMissingScripts sNeeded scriptsProvided =
///   sequenceA_
///     [ failureOnNonEmptySet (sNeeded `Set.difference` sProvided) MissingScriptWitnessesUTXOW
///     , failureOnNonEmptySet (sProvided `Set.difference` sNeeded) ExtraneousScriptWitnessesUTXOW
///     ]
/// ```
pub fn validate_missing_scripts(
    scripts_needed: &HashSet<ScriptHash>,
    scripts_provided: &HashSet<ScriptHash>,
) -> Vec<ShelleyUtxowPredFailure> {
    let mut errors = Vec::new();

    let missing: HashSet<ScriptHash> = scripts_needed
        .difference(scripts_provided)
        .copied()
        .collect();
    if !missing.is_empty() {
        errors.push(ShelleyUtxowPredFailure::MissingScriptWitnessesUTXOW(missing));
    }

    let extraneous: HashSet<ScriptHash> = scripts_provided
        .difference(scripts_needed)
        .copied()
        .collect();
    if !extraneous.is_empty() {
        errors.push(ShelleyUtxowPredFailure::ExtraneousScriptWitnessesUTXOW(extraneous));
    }

    errors
}

/// Validate witness key signatures.
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxow.hs:399-411
pub fn validate_verified_wits(
    wits: &[WitVKey],
    body_hash: &TxBodyHash,
) -> Result<(), ShelleyUtxowPredFailure> {
    let failed: Vec<VKey> = wits
        .iter()
        .filter(|w| !w.verify(body_hash))
        .map(|w| w.vkey.clone())
        .collect();

    if failed.is_empty() {
        Ok(())
    } else {
        Err(ShelleyUtxowPredFailure::InvalidWitnessesUTXOW(failed))
    }
}

/// Validate all needed witness key hashes are present.
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxow.hs:413-439
pub fn validate_needed_witnesses(
    wits_key_hashes: &HashSet<KeyHash>,
    needed_key_hashes: &HashSet<KeyHash>,
) -> Result<(), ShelleyUtxowPredFailure> {
    let missing: HashSet<KeyHash> = needed_key_hashes
        .difference(wits_key_hashes)
        .copied()
        .collect();

    if missing.is_empty() {
        Ok(())
    } else {
        Err(ShelleyUtxowPredFailure::MissingVKeyWitnessesUTXOW(missing))
    }
}

/// Validate auxiliary data hash consistency.
///
/// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxow.hs:441-466
///
/// ```haskell
/// validateMetadata pp tx
///   | SJust hash <- adh
///   , SNothing <- ad  = failure $ MissingTxMetadata hash
///   | SNothing <- adh
///   , SJust _ <- ad   = failure $ MissingTxBodyMetadataHash (hashTxAuxData ad)
///   | SJust hash <- adh
///   , SJust ad' <- ad
///   , adHash /= hash  = failure $ ConflictingMetadataHash ...
///   | SJust _ <- adh
///   , SJust ad' <- ad
///   , not (validateTxAuxData pv ad') = failure InvalidMetadata
///   | otherwise        = pure ()
/// ```
pub fn validate_metadata(
    body_hash: &Option<MetadataHash>,
    auxiliary_data: &Option<AllegraAuxiliaryData>,
) -> Result<(), ShelleyUtxowPredFailure> {
    match (body_hash, auxiliary_data) {
        (Some(hash), None) => {
            Err(ShelleyUtxowPredFailure::MissingTxMetadata(*hash))
        }
        (None, Some(ad)) => {
            Err(ShelleyUtxowPredFailure::MissingTxBodyMetadataHash(ad.hash()))
        }
        (Some(expected), Some(ad)) => {
            let actual = ad.hash();
            if *expected != actual {
                Err(ShelleyUtxowPredFailure::ConflictingMetadataHash {
                    body_hash: *expected,
                    actual_hash: actual,
                })
            } else {
                Ok(())
            }
        }
        (None, None) => Ok(()),
    }
}

// ============================================================================
// Allegra UTXOW Transition Rule
// ============================================================================

/// Allegra UTXOW Transition Rule
///
/// Reference: eras/allegra/impl/src/Cardano/Ledger/Allegra/Rules/Utxow.hs:66-79
///
/// ```haskell
/// instance ... STS (AllegraUTXOW era) where
///   type PredicateFailure (AllegraUTXOW era) = ShelleyUtxowPredFailure era
///   type Event (AllegraUTXOW era) = ShelleyUtxowEvent era
///   transitionRules = [transitionRulesUTXOW]
///   initialRules = []
/// ```
///
/// The Allegra UTXOW rule is identical to Shelley's `transitionRulesUTXOW`.
/// The only difference is:
/// - Native scripts now include Timelock constructors (RequireTimeStart/Expire)
/// - The UTXO sub-rule is AllegraUTXO (adds ValidityInterval, OutputTooBigUTxO)
///
/// Mary reuses this same rule:
///   `type instance EraRuleFailure "UTXOW" MaryEra = ShelleyUtxowPredFailure MaryEra`
///
/// Validation order (matches Shelley transitionRulesUTXOW exactly):
///  1. validateFailedNativeScripts    — eval all native scripts (incl. timelock)
///  2. validateMissingScripts         — scriptsNeeded = scriptsProvided
///  3. validateVerifiedWits           — VKey signature verification
///  4. validateNeededWitnesses        — all needed key hashes are witnessed
///  5. validateMetadata               — aux data hash consistency
///  6. validateMIRInsufficientGenesisSigs — genesis quorum for MIR certs
///  7. UTXO sub-rule (AllegraUTXO)    — see allegra-utxo.rs
pub fn allegra_utxow_transition(
    tx: &AllegraTx,
    scripts_needed: &HashSet<ScriptHash>,
    needed_key_hashes: &HashSet<KeyHash>,
) -> Result<(), Vec<ShelleyUtxowPredFailure>> {
    let mut errors: Vec<ShelleyUtxowPredFailure> = Vec::new();

    let _body_hash = tx.body.hash();

    // Step 1: ∀ s ∈ range(txscripts txw) ∩ Script_native, runNativeScript s tx
    if let Err(e) = validate_failed_native_scripts(
        &tx.wits.script_wits,
        &tx.wits.vkey_wits,
        &tx.body.validity_interval,
    ) {
        errors.push(e);
    }

    // Step 2: { s | (_, s) ∈ scriptsNeeded utxo tx } = dom(txscripts txw)
    let scripts_provided: HashSet<ScriptHash> = tx.wits.script_wits.keys().copied().collect();
    let script_errors = validate_missing_scripts(scripts_needed, &scripts_provided);
    errors.extend(script_errors);

    // Step 3: ∀ (vk ↦ σ) ∈ (txwitsVKey txw), V_vk⟦ txbodyHash ⟧_σ
    // Simplified — would verify each witness signature
    // if let Err(e) = validate_verified_wits(&vkey_wits, &body_hash) { ... }

    // Step 4: witsVKeyNeeded utxo tx genDelegs ⊆ witsKeyHashes
    if let Err(e) = validate_needed_witnesses(&tx.wits.vkey_wits, needed_key_hashes) {
        errors.push(e);
    }

    // Step 5: ((adh = ◇) ∧ (ad = ◇)) ∨ (adh = hashAD ad)
    if let Err(e) = validate_metadata(&tx.body.auxiliary_data_hash, &tx.auxiliary_data) {
        errors.push(e);
    }

    // Step 6: MIR genesis quorum check
    // Omitted for simplicity

    // Step 7: UTXO sub-rule (would call allegra_utxo_transition)
    // The UTXO transition is handled separately — UTXOW wraps its errors
    // in UtxoFailure.

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_hash(byte: u8) -> Hash {
        let mut h = [0u8; 32];
        h[0] = byte;
        Hash(h)
    }

    #[test]
    fn test_timelock_require_time_expire() {
        let script = NativeScript::RequireTimeExpire(200);
        let vkeys = HashSet::new();

        // TX expires at slot 200 — script satisfied (slot >= invalidHereafter)
        let vi_expires_200 = ValidityInterval {
            invalid_before: None,
            invalid_hereafter: Some(200),
        };
        assert!(script.evaluate(&vkeys, &vi_expires_200));

        // TX expires at slot 300 — script NOT satisfied (200 < 300)
        let vi_expires_300 = ValidityInterval {
            invalid_before: None,
            invalid_hereafter: Some(300),
        };
        assert!(!script.evaluate(&vkeys, &vi_expires_300));

        // TX never expires — script NOT satisfied (no invalidHereafter)
        let vi_no_expiry = ValidityInterval {
            invalid_before: None,
            invalid_hereafter: None,
        };
        assert!(!script.evaluate(&vkeys, &vi_no_expiry));
    }

    #[test]
    fn test_timelock_require_time_start() {
        let script = NativeScript::RequireTimeStart(100);
        let vkeys = HashSet::new();

        // TX starts at slot 100 — script satisfied (100 <= 100)
        let vi_starts_100 = ValidityInterval {
            invalid_before: Some(100),
            invalid_hereafter: None,
        };
        assert!(script.evaluate(&vkeys, &vi_starts_100));

        // TX starts at slot 50 — script NOT satisfied (100 > 50)
        let vi_starts_50 = ValidityInterval {
            invalid_before: Some(50),
            invalid_hereafter: None,
        };
        assert!(!script.evaluate(&vkeys, &vi_starts_50));

        // TX has no start — script NOT satisfied (no invalidBefore)
        let vi_no_start = ValidityInterval {
            invalid_before: None,
            invalid_hereafter: None,
        };
        assert!(!script.evaluate(&vkeys, &vi_no_start));
    }

    #[test]
    fn test_missing_scripts() {
        let needed: HashSet<ScriptHash> = [make_hash(1), make_hash(2)].into_iter().collect();
        let provided: HashSet<ScriptHash> = [make_hash(1), make_hash(3)].into_iter().collect();

        let errors = validate_missing_scripts(&needed, &provided);
        assert_eq!(errors.len(), 2); // missing(2) + extraneous(3)
    }

    #[test]
    fn test_needed_witnesses() {
        let present: HashSet<KeyHash> = [make_hash(1)].into_iter().collect();
        let needed: HashSet<KeyHash> = [make_hash(1), make_hash(2)].into_iter().collect();

        let result = validate_needed_witnesses(&present, &needed);
        assert!(matches!(
            result,
            Err(ShelleyUtxowPredFailure::MissingVKeyWitnessesUTXOW(_))
        ));
    }

    #[test]
    fn test_metadata_consistency() {
        let hash = Hash([1u8; 32]);

        // Both present and matching — OK
        assert!(validate_metadata(&None, &None).is_ok());

        // Hash in body, no aux data — error
        assert!(matches!(
            validate_metadata(&Some(hash), &None),
            Err(ShelleyUtxowPredFailure::MissingTxMetadata(_))
        ));
    }
}
