# Alonzo Era UTXOW Rule

## Overview

Alonzo introduces **Plutus scripts** (Phase 2 smart contracts). The UTXOW rule gains new **Phase 1 setup checks** to ensure Plutus scripts can execute correctly:

1. **Datum validation** - UTxO inputs with Plutus scripts must have datums
2. **Redeemer validation** - Exactly one redeemer per Plutus script
3. **Script integrity hash** - Commitment to redeemers, datums, and cost models

**Source File**: `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Rules/Utxow.hs` (450 lines)

---

## Key Concepts

### Phase 1 vs Phase 2

| Aspect | Phase 1 (UTXOW) | Phase 2 (UTXOS) |
|--------|-----------------|-----------------|
| **When** | Always | Only if Plutus scripts present |
| **Scripts** | Native scripts validated | Plutus scripts executed |
| **On Failure** | Transaction rejected | Collateral collected |
| **Cost** | Minimal | Resource-limited (ExUnits) |

**Alonzo UTXOW handles Phase 1 setup for Plutus, not Plutus execution itself.**

### Plutus Script Arguments

| Purpose | Arguments | Has Datum? |
|---------|-----------|------------|
| **Spending** | Datum + Redeemer + Context | ✅ Yes (stored in UTxO) |
| **Minting** | Redeemer + Context | ❌ No |
| **Rewarding** | Redeemer + Context | ❌ No |
| **Certifying** | Redeemer + Context | ❌ No |

**Datums are only required for spending** because only UTxOs store datum hashes.

---

## Predicate Failures (Errors)

**Reference**: `Utxow.hs:97-129`

```haskell
data AlonzoUtxowPredFailure era
  = ShelleyInAlonzoUtxowPredFailure (ShelleyUtxowPredFailure era)  -- Wrapped Shelley errors
  | MissingRedeemers [(PlutusPurpose AsItem era, ScriptHash)]
  | MissingRequiredDatums (Set DataHash) (Set DataHash)
  | NotAllowedSupplementalDatums (Set DataHash) (Set DataHash)
  | PPViewHashesDontMatch (Mismatch RelEQ (StrictMaybe ScriptIntegrityHash))
  | UnspendableUTxONoDatumHash (Set TxIn)
  | ExtraRedeemers [PlutusPurpose AsIx era]
  | ScriptIntegrityHashMismatch (Mismatch RelEQ (StrictMaybe ScriptIntegrityHash)) (StrictMaybe ByteString)
```

### Error Explanations

| Error | When It Occurs |
|-------|----------------|
| `ShelleyInAlonzoUtxowPredFailure` | Wrapped error from Shelley validation (signatures, native scripts, etc.) |
| `MissingRedeemers` | Plutus script needs a redeemer but none provided |
| `MissingRequiredDatums` | Datum hash in UTxO but no matching datum in witnesses |
| `NotAllowedSupplementalDatums` | Extra datums provided that aren't needed |
| `PPViewHashesDontMatch` | Script integrity hash mismatch (pre-version 11) |
| `UnspendableUTxONoDatumHash` | Trying to spend Plutus V1/V2 UTxO without datum hash |
| `ExtraRedeemers` | Redeemers provided for non-existent scripts |
| `ScriptIntegrityHashMismatch` | Script integrity hash mismatch (version 11+) |

---

## Main Transition Function

**Reference**: `Utxow.hs:340-396`

```haskell
alonzoStyleWitness :: TransitionRule (EraRule "UTXOW" era)
alonzoStyleWitness = do
  TRC (utxoEnv@(UtxoEnv _ pp certState), u, tx) <- judgmentContext

  let utxo = utxosUtxo u
      txBody = tx ^. bodyTxL
      witsKeyHashes = keyHashWitnessesTxWits (tx ^. witsTxL)
      scriptsProvided = getScriptsProvided utxo tx

  -- REUSED FROM SHELLEY: Native script validation (line 348)
  runTestOnSignal $ Shelley.validateFailedNativeScripts scriptsProvided tx

  -- REUSED FROM SHELLEY: Script presence check (line 351-354)
  let scriptsNeeded = getScriptsNeeded utxo txBody
      scriptsHashesNeeded = getScriptsHashesNeeded scriptsNeeded
      shelleyScriptsNeeded = ShelleyScriptsNeeded scriptsHashesNeeded
  runTest $ Shelley.validateMissingScripts shelleyScriptsNeeded scriptsProvided

  -- NEW: Check required datums (line 358)
  runTest $ missingRequiredDatums utxo tx

  -- NEW: Check exact set of redeemers (line 366)
  runTest $ hasExactSetOfRedeemers tx scriptsProvided scriptsNeeded

  -- REUSED FROM SHELLEY: VKey verification (line 370)
  runTestOnSignal $ Shelley.validateVerifiedWits tx

  -- REUSED FROM SHELLEY: Required witnesses (line 373)
  runTest $ validateNeededWitnesses witsKeyHashes certState utxo txBody

  -- REUSED FROM SHELLEY: MIR signatures (line 378-380)
  let genDelegs = certState ^. certDStateL . dsGenDelegsL
  coreNodeQuorum <- liftSTS $ asks quorum
  runTest $ Shelley.validateMIRInsufficientGenesisSigs genDelegs coreNodeQuorum witsKeyHashes tx

  -- REUSED FROM SHELLEY: Metadata (line 386)
  runTestOnSignal $ Shelley.validateMetadata pp tx

  -- NEW: Script integrity hash (line 392-394)
  let scriptIntegrity = mkScriptIntegrity pp tx scriptsProvided scriptsHashesNeeded
  runTest $ checkScriptIntegrityHash tx pp scriptIntegrity

  -- Call UTXO rule (line 396)
  trans @(EraRule "UTXO" era) $ TRC (utxoEnv, u, tx)
```

### Validation Steps Summary

| Step | Function | From Era | Purpose |
|------|----------|----------|---------|
| 1 | `validateFailedNativeScripts` | Shelley | Run native scripts |
| 2 | `validateMissingScripts` | Shelley | Check script presence |
| 3 | `missingRequiredDatums` | **Alonzo** | Datum validation |
| 4 | `hasExactSetOfRedeemers` | **Alonzo** | Redeemer validation |
| 5 | `validateVerifiedWits` | Shelley | Verify signatures |
| 6 | `validateNeededWitnesses` | Shelley | Check required witnesses |
| 7 | `validateMIRInsufficientGenesisSigs` | Shelley | MIR quorum |
| 8 | `validateMetadata` | Shelley | Metadata: hash consistency + validMetadatum when pv > (2,0) (InvalidMetadata) |
| 9 | `checkScriptIntegrityHash` | **Alonzo** | Script integrity |
| 10 | UTXO rule | All | Structural validation |

---

## New Validation Functions

### 1. missingRequiredDatums

**Reference**: `Utxow.hs:229-257`

```haskell
missingRequiredDatums ::
  (AlonzoEraTx era, AlonzoEraUTxO era) =>
  UTxO era -> Tx l era -> Test (AlonzoUtxowPredFailure era)
missingRequiredDatums utxo tx = do
  let txBody = tx ^. bodyTxL
      scriptsProvided = getScriptsProvided utxo tx
      (inputHashes, txInsNoDataHash) = getInputDataHashesTxBody utxo txBody scriptsProvided
      txHashes = Map.keysSet (tx ^. witsTxL . datsTxWitsL . unTxDatsL)
      unmatchedDatumHashes = Set.difference inputHashes txHashes
      allowedSupplementalDataHashes = getSupplementalDataHashes utxo txBody
      supplimentalDatumHashes = Set.difference txHashes inputHashes
      (okSupplimentalDHs, notOkSupplimentalDHs) =
        Set.partition (`Set.member` allowedSupplementalDataHashes) supplimentalDatumHashes
  sequenceA_
    [ failureUnless (Set.null txInsNoDataHash)
        (UnspendableUTxONoDatumHash txInsNoDataHash)
    , failureUnless (Set.null unmatchedDatumHashes)
        (MissingRequiredDatums unmatchedDatumHashes txHashes)
    , failureUnless (Set.null notOkSupplimentalDHs)
        (NotAllowedSupplementalDatums notOkSupplimentalDHs okSupplimentalDHs)
    ]
```

**What it does** (step by step):

1. **Get input datum hashes**: For each spending input locked by a Plutus script, extract the datum hash stored in the UTxO
2. **Get problematic inputs**: Inputs locked by Plutus V1/V2 that have **no datum hash at all** (unspendable!)
3. **Get provided datums**: Datum hashes from transaction witnesses
4. **Check three conditions**:
   - `txInsNoDataHash` must be empty → else `UnspendableUTxONoDatumHash`
   - `inputHashes ⊆ txHashes` → else `MissingRequiredDatums`
   - Extra datums must be for outputs/ref inputs → else `NotAllowedSupplementalDatums`

**Plain English**: "Every Plutus V1/V2 spending input needs a datum. You must provide the actual datums in witnesses."

#### getInputDataHashesTxBody

**Reference**: `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/UTxO.hs:165-194`

```haskell
getInputDataHashesTxBody ::
  UTxO era -> TxBody l era -> ScriptsProvided era -> (Set DataHash, Set TxIn)
getInputDataHashesTxBody (UTxO utxo) txBody (ScriptsProvided scriptsProvided) =
  Map.foldlWithKey' accum (Set.empty, Set.empty) spendUTxO
  where
    accum ans@(!hashSet, !inputSet) txIn txOut =
      let addr = txOut ^. addrTxOutL
       in case txOut ^. datumTxOutF of
            NoDatum
              | Just lang <- spendingPlutusScriptLanguage addr
              , lang < PlutusV3 ->  -- CIP-0069: V3 doesn't require datum
                  (hashSet, Set.insert txIn inputSet)  -- PROBLEM!
            DatumHash dataHash
              | isSpendingPlutusScript addr ->
                  (Set.insert dataHash hashSet, inputSet)  -- Collect hash
            _ -> ans  -- OK (native script, inline datum, or V3)
```

**Key insight**: PlutusV3 (Conway) does **not** require datums for spending. Only V1/V2 do.

---

### 2. hasExactSetOfRedeemers

**Reference**: `Utxow.hs:262-285`

```haskell
hasExactSetOfRedeemers ::
  AlonzoEraTx era =>
  Tx l era -> ScriptsProvided era -> AlonzoScriptsNeeded era ->
  Test (AlonzoUtxowPredFailure era)
hasExactSetOfRedeemers tx (ScriptsProvided scriptsProvided) (AlonzoScriptsNeeded scriptsNeeded) = do
  let redeemersNeeded =
        [ (hoistPlutusPurpose toAsIx sp, (hoistPlutusPurpose toAsItem sp, sh))
        | (sp, sh) <- scriptsNeeded         -- For each script needed
        , Just script <- [Map.lookup sh scriptsProvided]
        , not (isNativeScript script)       -- Only Plutus scripts need redeemers!
        ]
      (extraRdmrs, missingRdmrs) =
        extSymmetricDifference
          (Map.keys $ tx ^. witsTxL . rdmrsTxWitsL . unRedeemersL)
          id
          redeemersNeeded
          fst
  sequenceA_
    [ failureUnless (null extraRdmrs) (ExtraRedeemers extraRdmrs)
    , failureUnless (null missingRdmrs) (MissingRedeemers (map snd missingRdmrs))
    ]
```

**What it does**:

1. **Get scripts needed**: All (purpose, scriptHash) pairs for the transaction
2. **Filter to Plutus only**: Native scripts don't need redeemers
3. **Compare with provided redeemers**: Must be exactly 1:1 match
4. **Fail if**:
   - Extra redeemers (waste of space/fees)
   - Missing redeemers (Plutus can't execute)

**Important**: This checks ALL purposes (Spending, Minting, Rewarding, Certifying), not just spending!

**Plain English**: "Every Plutus script needs exactly one redeemer. No more, no less."

---

### 3. checkScriptIntegrityHash

**Reference**: `Utxow.hs:289-310`

```haskell
checkScriptIntegrityHash ::
  AlonzoEraTx era =>
  Tx l era -> PParams era -> StrictMaybe (ScriptIntegrity era) ->
  Test (AlonzoUtxowPredFailure era)
checkScriptIntegrityHash tx pp scriptIntegrity = do
  let computedScriptIntegrityHash = hashScriptIntegrity <$> scriptIntegrity
      suppliedScriptIntegrityHash = tx ^. bodyTxL . scriptIntegrityHashTxBodyL
      mismatch = Mismatch
        { mismatchSupplied = suppliedScriptIntegrityHash
        , mismatchExpected = computedScriptIntegrityHash
        }
  failureUnless (suppliedScriptIntegrityHash == computedScriptIntegrityHash)
    $ if pvMajor (pp ^. ppProtocolVersionL) < natVersion @11
      then PPViewHashesDontMatch mismatch
      else ScriptIntegrityHashMismatch mismatch expectedScriptIntegrity
```

**What it does**:

1. **Compute expected hash**: Hash of (redeemers, datums, language views)
2. **Get supplied hash**: From transaction body
3. **Compare**: Must match exactly

**What's in the script integrity hash**:
- All redeemers (with execution units)
- All datums (for scripts that need them)
- Cost model parameters for each Plutus version used

**Plain English**: "The transaction commits to exactly what Plutus scripts will receive. No tampering."

---

## Scripts Needed (Alonzo Style)

**Reference**: `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/UTxO.hs:228-277`

```haskell
newtype AlonzoScriptsNeeded era
  = AlonzoScriptsNeeded [(PlutusPurpose AsIxItem era, ScriptHash)]

getAlonzoScriptsNeeded ::
  UTxO era -> TxBody l era -> AlonzoScriptsNeeded era
getAlonzoScriptsNeeded utxo txBody =
  getSpendingScriptsNeeded utxo txBody
    <> getRewardingScriptsNeeded txBody
    <> certifyingScriptsNeeded
    <> getMintingScriptsNeeded txBody
```

**Difference from Shelley**: Returns `(Purpose, ScriptHash)` pairs, not just `Set ScriptHash`.

| Purpose | Source |
|---------|--------|
| `SpendingPurpose (idx, txIn)` | UTxO inputs locked by scripts |
| `RewardingPurpose (idx, rewardAccount)` | Withdrawals from script-locked accounts |
| `CertifyingPurpose (idx, cert)` | Certificates authorized by scripts |
| `MintingPurpose (idx, policyId)` | Minting policies |

**Why the index?** Redeemers are identified by `(purpose, index)`, not by script hash. This allows the same script to be used multiple times with different redeemers.

---

## VKey Witnesses Needed (Alonzo)

**Reference**: `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/UTxO.hs:322-338`

```haskell
getAlonzoWitsVKeyNeeded ::
  CertState era -> UTxO era -> TxBody l era -> Set (KeyHash Witness)
getAlonzoWitsVKeyNeeded certState utxo txBody =
  getShelleyWitsVKeyNeeded certState utxo txBody
    `Set.union` Set.map asWitness (txBody ^. reqSignerHashesTxBodyG)
```

**What's new in Alonzo**: The `reqSignerHashes` field.

**Why?** Plutus scripts can use `txSignedBy` to check for specific signatures. The `reqSignerHashes` field tells the ledger which keys the Plutus scripts expect to sign, so we can require those witnesses in Phase 1.

**Plain English**: "Alonzo adds required signers - keys that Plutus scripts will check for."

---

## Formal Specification

From the Alonzo formal spec, additional UTXOW predicates:

```
1. { h | (_ → (a,_,h)) ∈ txins tx ◁ utxo, isTwoPhaseScriptAddress tx a } ⊆ dom(txdats txw)
   (Plutus spending inputs have datums in witnesses)

2. dom(txdats txw) ⊆ inputHashes ∪ { h | (_,_,h,_) ∈ txouts tx ∪ utxo(refInputs tx) }
   (Extra datums must be for outputs or reference inputs)

3. dom(txrdmrs tx) = { rdptr txb sp | (sp, h) ∈ scriptsNeeded utxo tx,
                                        h ↦ s ∈ txscripts txw, s ∈ Scriptph2 }
   (Exactly one redeemer per Plutus script)

4. scriptIntegrityHash txb = hashScriptIntegrity pp (languages txw) (txrdmrs txw)
   (Script integrity hash matches)
```

---

## Summary: What Alonzo Adds to UTXOW

| Feature | Purpose |
|---------|---------|
| **Datum validation** | Ensure Plutus scripts get their required data |
| **Redeemer matching** | Ensure each Plutus script has execution input |
| **Script integrity hash** | Prevent tampering with Plutus execution context |
| **Required signers** | Let Plutus check for signatures in Phase 1 |
| **Wrapped error type** | `AlonzoUtxowPredFailure` wraps `ShelleyUtxowPredFailure` |

**Note**: Plutus script **execution** happens in UTXOS (Phase 2), not UTXOW (Phase 1). Alonzo UTXOW just sets up the execution context.
