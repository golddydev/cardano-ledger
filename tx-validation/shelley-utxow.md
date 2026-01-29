# Shelley Era UTXOW Rule

## Overview

**UTXOW** (Unspent Transaction Output Witnessing) is Cardano's **Phase 1 transaction validation rule** that checks:
1. **VKey witnesses** - Ed25519 signatures authorizing transaction actions
2. **Native scripts** - Multi-signature scripts (RequireSignature, RequireAllOf, etc.)
3. **Metadata integrity** - Hash matching between body and auxiliary data
4. **Genesis signatures** - Quorum for MIR certificates

**Source File**: `eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxow.hs` (450 lines)

---

## Rule Hierarchy

```
LEDGER rule (Ledger.hs:322)
    │
    ├─ DELEGS rule (processes certificates FIRST)
    │
    └─ UTXOW rule (this file) ← Phase 1 Witnesses
        │
        └─ UTXO rule ← Structural validation
```

UTXOW embeds UTXO - witness checks happen **before** structural checks.

---

## Predicate Failures (Errors)

**Reference**: `Utxow.hs:85-112`

```haskell
data ShelleyUtxowPredFailure era
  = InvalidWitnessesUTXOW [VKey Witness]
  | MissingVKeyWitnessesUTXOW (Set (KeyHash Witness))
  | MissingScriptWitnessesUTXOW (Set ScriptHash)
  | ScriptWitnessNotValidatingUTXOW (Set ScriptHash)
  | UtxoFailure (PredicateFailure (EraRule "UTXO" era))
  | MissingTxBodyMetadataHash TxAuxDataHash
  | MissingTxMetadata TxAuxDataHash
  | ConflictingMetadataHash (Mismatch RelEQ TxAuxDataHash)
  | InvalidMetadata
  | ExtraneousScriptWitnessesUTXOW (Set ScriptHash)
  | MIRInsufficientGenesisSigsUTXOW (Set (KeyHash Witness))
```

### Error Explanations

| Error | When It Occurs |
|-------|----------------|
| `InvalidWitnessesUTXOW` | VKey signature verification failed (wrong key or corrupted signature) |
| `MissingVKeyWitnessesUTXOW` | Required VKey signatures not provided |
| `MissingScriptWitnessesUTXOW` | Scripts needed but not provided in witnesses |
| `ScriptWitnessNotValidatingUTXOW` | Native script evaluated to `false` |
| `UtxoFailure` | Error from embedded UTXO rule |
| `MissingTxBodyMetadataHash` | Auxiliary data provided but no hash in body |
| `MissingTxMetadata` | Hash in body but no auxiliary data provided |
| `ConflictingMetadataHash` | Auxiliary data hash doesn't match body hash |
| `InvalidMetadata` | Auxiliary data fails validation (Shelley: never used) |
| `ExtraneousScriptWitnessesUTXOW` | Scripts provided but not needed |
| `MIRInsufficientGenesisSigsUTXOW` | MIR certificate without enough genesis signatures |

---

## Main Transition Function

**Reference**: `Utxow.hs:296-333`

```haskell
transitionRulesUTXOW ::
  forall era.
  ( ... constraints ... ) =>
  TransitionRule (EraRule "UTXOW" era)
transitionRulesUTXOW = do
  TRC (utxoEnv@(UtxoEnv _ pp certState), u, tx) <- judgmentContext

  let utxo = utxosUtxo u
      txBody = tx ^. bodyTxL
      witsKeyHashes = keyHashWitnessesTxWits (tx ^. witsTxL)
      scriptsProvided = getScriptsProvided utxo tx

  -- Step 1: Validate native scripts (line 308)
  runTestOnSignal $ validateFailedNativeScripts scriptsProvided tx

  -- Step 2: Check script presence (line 311)
  let scriptsNeeded = getScriptsNeeded utxo txBody
  runTest $ validateMissingScripts scriptsNeeded scriptsProvided

  -- Step 3: Verify VKey signatures (line 316)
  runTestOnSignal $ validateVerifiedWits tx

  -- Step 4: Check required witnesses (line 319)
  runTest $ validateNeededWitnesses witsKeyHashes certState utxo txBody

  -- Step 5: Validate metadata (line 323)
  runTestOnSignal $ validateMetadata pp tx

  -- Step 6: Check MIR genesis signatures (line 328)
  let genDelegs = certState ^. certDStateL . dsGenDelegsL
  coreNodeQuorum <- liftSTS $ asks quorum
  runTest $ validateMIRInsufficientGenesisSigs genDelegs coreNodeQuorum witsKeyHashes tx

  -- Step 7: Call UTXO rule (line 333)
  trans @(EraRule "UTXO" era) $ TRC (utxoEnv, u, tx)
```

### Plain English Explanation

1. **Extract context**: Get the UTxO set, transaction body, and witnesses
2. **Run native scripts**: Check that all native scripts evaluate to `true`
3. **Check scripts provided**: Ensure all needed scripts are in witnesses
4. **Verify signatures**: Cryptographically verify Ed25519 signatures
5. **Check witnesses complete**: All required VKey witnesses are present
6. **Validate metadata**: Metadata hash matches if metadata provided
7. **Check MIR quorum**: Genesis key quorum for MIR certificates
8. **Call UTXO**: Proceed to structural validation

---

## Validation Functions

### 1. validateFailedNativeScripts

**Reference**: `Utxow.hs:184-197`

```haskell
validateFailedNativeScripts ::
  ScriptsProvided era -> Tx l era -> Test (ShelleyUtxowPredFailure era)
validateFailedNativeScripts (ScriptsProvided scriptsProvided) tx =
  let failedScripts =
        Map.filterWithKey
          (\scriptHash script ->
            isNativeScript script && not (validateScript scriptHash tx script)
          )
          scriptsProvided
   in failureUnless
        (Map.null failedScripts)
        (ScriptWitnessNotValidatingUTXOW (Map.keysSet failedScripts))
```

**What it does**:
1. Gets all scripts provided in transaction witnesses
2. Filters to only native scripts (not Plutus)
3. Evaluates each native script against the transaction
4. Fails if any script returns `false`

**Plain English**: "Run all native scripts. If any says 'no', fail the transaction."

---

### 2. validateMissingScripts

**Reference**: `Utxow.hs:382-389`

```haskell
validateMissingScripts ::
  ShelleyScriptsNeeded era ->
  ScriptsProvided era ->
  Test (ShelleyUtxowPredFailure era)
validateMissingScripts (ShelleyScriptsNeeded sNeeded) (ScriptsProvided scriptsProvided) =
  sequenceA_
    [ failureUnless (Set.null sMissing) (MissingScriptWitnessesUTXOW sMissing)
    , failureUnless (Set.null sExtra) (ExtraneousScriptWitnessesUTXOW sExtra)
    ]
  where
    sReceived = Map.keysSet scriptsProvided
    sMissing = Set.difference sNeeded sReceived
    sExtra = Set.difference sReceived sNeeded
```

**What it does**:
1. Computes which scripts are needed (from inputs, withdrawals, certificates)
2. Computes which scripts are provided in witnesses
3. Checks that needed ⊆ provided (no missing)
4. Checks that provided ⊆ needed (no extras)

**Plain English**: "Check that you provided exactly the scripts you need - no more, no less."

---

### 3. validateVerifiedWits

**Reference**: `Utxow.hs:210-226`

```haskell
validateVerifiedWits ::
  EraTx era => Tx l era -> Test (ShelleyUtxowPredFailure era)
validateVerifiedWits tx =
  let txBodyHash = hashAnnotated (tx ^. bodyTxL)
      wits = tx ^. witsTxL
      failedWits =
        filter
          (not . verifyWitVKey txBodyHash)
          (tx ^. witsTxL . addrTxWitsL)
   in failureUnless
        (null failedWits)
        (InvalidWitnessesUTXOW $ map witVKeyKeyHash failedWits)
```

**What it does**:
1. Hashes the transaction body (what gets signed)
2. For each VKey witness, verifies: `verify(vkey, signature, txBodyHash)`
3. Collects any witnesses where verification failed
4. Fails if any signature is invalid

**Plain English**: "Check that every signature is valid. One bad signature = transaction rejected."

---

### 4. validateNeededWitnesses

**Reference**: `Utxow.hs:422-434`

```haskell
validateNeededWitnesses ::
  Set (KeyHash Witness) ->
  CertState era ->
  UTxO era ->
  TxBody l era ->
  Test (ShelleyUtxowPredFailure era)
validateNeededWitnesses witsKeyHashes certState utxo txBody =
  let needed = getWitsVKeyNeeded certState utxo txBody
      missingWitnesses = Set.difference needed witsKeyHashes
   in failureUnless
        (Set.null missingWitnesses)
        (MissingVKeyWitnessesUTXOW missingWitnesses)
```

**What it does**:
1. Computes which key hashes must sign (see `getWitsVKeyNeeded` below)
2. Compares against key hashes actually present in witnesses
3. Fails if any required key hash is missing

**Plain English**: "Check that everyone who needs to sign has signed."

---

### 5. validateMetadata

**Reference**: `Utxow.hs:234-261`

```haskell
validateMetadata ::
  EraTx era =>
  PParams era -> Tx l era -> Test (ShelleyUtxowPredFailure era)
validateMetadata pp tx =
  let txBody = tx ^. bodyTxL
      auxDataHash = txBody ^. auxDataHashTxBodyL
      auxData = tx ^. auxDataTxL
   in case (auxDataHash, auxData) of
        (SNothing, SNothing) -> pure ()  -- No metadata, no hash: OK
        (SJust mdh, SNothing) -> failure (MissingTxMetadata mdh)
        (SNothing, SJust md) -> failure (MissingTxBodyMetadataHash (hashTxAuxData md))
        (SJust mdh, SJust md)
          | hashTxAuxData md /= mdh ->
              failure $ ConflictingMetadataHash Mismatch {mismatchSupplied = mdh, mismatchExpected = hashTxAuxData md}
          | not (validateTxAuxData pp md) -> failure InvalidMetadata
          | otherwise -> pure ()
```

**What it does**:
1. Gets the auxiliary data hash from transaction body
2. Gets the actual auxiliary data from transaction
3. Checks four cases:
   - Neither present: OK
   - Hash but no data: Error
   - Data but no hash: Error
   - Both present: Check hash matches and data is valid

**Plain English**: "If you include metadata, you must commit to it with a hash. The hash must match."

---

### 6. validateMIRInsufficientGenesisSigs

**Reference**: `Utxow.hs:267-288`

```haskell
validateMIRInsufficientGenesisSigs ::
  GenDelegs ->
  Word64 ->
  Set (KeyHash Witness) ->
  Tx l era ->
  Test (ShelleyUtxowPredFailure era)
validateMIRInsufficientGenesisSigs (GenDelegs genDelegs) coreNodeQuorum witsKeyHashes tx =
  let txBody = tx ^. bodyTxL
      mirCerts = filter isMirCert (toList $ txBody ^. certsTxBodyL)
      genSig = Set.intersection witsKeyHashes (Set.map (asWitness . genDelegKeyHash) (Map.elems genDelegs))
   in failureUnless
        (null mirCerts || fromIntegral (Set.size genSig) >= coreNodeQuorum)
        (MIRInsufficientGenesisSigsUTXOW genSig)
```

**What it does**:
1. Checks if transaction contains MIR (Move Instantaneous Rewards) certificates
2. If yes, counts how many genesis delegates have signed
3. Requires at least `quorum` signatures
4. Fails if MIR present but not enough genesis signatures

**Plain English**: "MIR certificates move treasury/reserves funds. They need genesis key approval."

---

## Helper Functions

### getWitsVKeyNeeded (What Signatures Are Required)

**Reference**: `eras/shelley/impl/src/Cardano/Ledger/Shelley/UTxO.hs:223-280`

```haskell
getShelleyWitsVKeyNeeded ::
  CertState era -> UTxO era -> TxBody l era -> Set (KeyHash Witness)
getShelleyWitsVKeyNeeded certState utxo txBody =
  getShelleyWitsVKeyNeededNoGov utxo txBody
    `Set.union` witsVKeyNeededGenDelegs txBody (dsGenDelegs (certState ^. certDStateL))
```

This combines two sources:

#### getShelleyWitsVKeyNeededNoGov

**Reference**: `UTxO.hs:228-268`

```haskell
getShelleyWitsVKeyNeededNoGov ::
  UTxO era -> TxBody l era -> Set (KeyHash Witness)
getShelleyWitsVKeyNeededNoGov utxo' txBody =
  certAuthors
    `Set.union` inputAuthors
    `Set.union` owners
    `Set.union` wdrlAuthors
```

| Source | What it collects |
|--------|------------------|
| `inputAuthors` | Key hashes from UTxO inputs with key-locked addresses |
| `wdrlAuthors` | Key hashes from withdrawal credentials |
| `certAuthors` | Key hashes authorizing certificates |
| `owners` | Pool owner key hashes for pool registration |

**Plain English**: "Collect every key that needs to authorize this transaction."

#### witsVKeyNeededGenDelegs

**Reference**: `UTxO.hs:206-219`

```haskell
witsVKeyNeededGenDelegs ::
  ShelleyEraTxBody era =>
  TxBody TopTx era ->
  GenDelegs ->
  Set (KeyHash Witness)
witsVKeyNeededGenDelegs txBody (GenDelegs genDelegs) =
  asWitness `Set.map` proposedUpdatesWitnesses (txBody ^. updateTxBodyL)
```

**What it does**: For protocol parameter updates, requires signatures from genesis delegates who proposed the update.

---

### getShelleyScriptsNeeded (What Scripts Are Required)

**Reference**: `UTxO.hs:103-119`

```haskell
getShelleyScriptsNeeded ::
  EraTxBody era =>
  UTxO era ->
  TxBody l era ->
  ShelleyScriptsNeeded era
getShelleyScriptsNeeded u txBody =
  ShelleyScriptsNeeded
    ( scriptHashes
        `Set.union` Set.fromList
          [sh | w <- withdrawals, Just sh <- [credScriptHash (raCredential w)]]
        `Set.union` Set.fromList
          [sh | c <- certificates, Just sh <- [getScriptWitnessTxCert c]]
    )
  where
    withdrawals = Map.keys (unWithdrawals (txBody ^. withdrawalsTxBodyL))
    scriptHashes = txinsScriptHashes (txBody ^. inputsTxBodyL) u
    certificates = toList (txBody ^. certsTxBodyL)
```

| Source | What it collects |
|--------|------------------|
| `txinsScriptHashes` | Script hashes from UTxO inputs with script-locked addresses |
| Withdrawals | Script hashes from script-locked reward addresses |
| Certificates | Script hashes from script-authorized certificates |

**Plain English**: "Find all the scripts that need to authorize parts of this transaction."

---

## Native Script Validation

**Reference**: `eras/shelley/impl/src/Cardano/Ledger/Shelley/Scripts.hs:233-249`

```haskell
evalMultiSig :: Set (KeyHash Witness) -> NativeScript era -> Bool
evalMultiSig vhks = go
  where
    go = \case
      RequireSignature hk -> Set.member hk vhks
      RequireAllOf msigs -> all go msigs
      RequireAnyOf msigs -> any go msigs
      RequireMOf m msigs -> m <= sum [if go msig then 1 else 0 | msig <- msigs]
```

| Constructor | Behavior |
|-------------|----------|
| `RequireSignature hk` | True if key hash `hk` is in witnesses |
| `RequireAllOf scripts` | True if ALL sub-scripts are true |
| `RequireAnyOf scripts` | True if ANY sub-script is true |
| `RequireMOf m scripts` | True if at least `m` sub-scripts are true |

**Example - 2-of-3 MultiSig**:
```haskell
RequireMOf 2
  [ RequireSignature keyHash1
  , RequireSignature keyHash2
  , RequireSignature keyHash3
  ]
-- Requires signatures from any 2 of the 3 keys
```

---

## Formal Specification

From the Shelley formal spec, UTXOW checks these predicates:

```
1. ∀ (vk ↦ σ) ∈ txwitsVKey txw, V_vk⟦ txBodyHash ⟧_σ
   (All signatures verify correctly)

2. witsVKeyNeeded utxo tx genDelegs ⊆ witsKeyHashes
   (All required key witnesses are present)

3. { s | (_,s) ∈ scriptsNeeded utxo tx} = dom(txscripts txw)
   (Exactly the needed scripts are provided)

4. ∀ s ∈ range(txscripts txw), evalScript s tx
   (All scripts validate)

5. (adh = ◇ ∧ ad = ◇) ∨ (adh = hashAD ad)
   (Metadata hash consistent)

6. { c ∈ txcerts txb ∩ TxCert_mir } ≠ ∅ ⇒ |genSig| ≥ Quorum
   (MIR certificates have genesis quorum)
```

---

## Allegra/Mary Eras

**Allegra** adds timelock scripts (`RequireTimeStart`, `RequireTimeExpire`) but makes **no changes to UTXOW logic** - just the script type changes.

**Mary** adds multi-asset support but makes **no changes to UTXOW logic** - value changes happen in UTXO, not UTXOW.

Both eras completely reuse Shelley's `transitionRulesUTXOW` function.
