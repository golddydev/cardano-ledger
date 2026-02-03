# Babbage Era UTXOW Rule

## Overview

Babbage introduces **reference scripts** and **inline datums**, reducing transaction sizes and improving dApp UX. UTXOW gains:

1. **Reference script support** - Scripts can be referenced from UTxOs instead of included in witnesses
2. **Inline datum support** - Datums can be embedded directly in outputs
3. **Script well-formedness checks** - Validate script structure before execution

**Source File**: `eras/babbage/impl/src/Cardano/Ledger/Babbage/Rules/Utxow.hs` (420 lines)

---

## Key Features

### Reference Scripts (CIP-33)

**Before Babbage**: Every transaction using a script must include the full script in witnesses.

**With Babbage**: Scripts can be stored in UTxO outputs and referenced by transactions.

```
┌─────────────────────────────────────────────────────────────┐
│ Traditional (Alonzo):                                       │
│                                                             │
│   Tx1: includes full script (10KB)                         │
│   Tx2: includes full script (10KB)                         │
│   Tx3: includes full script (10KB)                         │
│   Total: 30KB                                               │
├─────────────────────────────────────────────────────────────┤
│ With Reference Scripts (Babbage):                           │
│                                                             │
│   Setup Tx: creates UTxO with script (10KB)                │
│   Tx1: references UTxO (32 bytes)                          │
│   Tx2: references UTxO (32 bytes)                          │
│   Tx3: references UTxO (32 bytes)                          │
│   Total: ~10.1KB                                            │
└─────────────────────────────────────────────────────────────┘
```

### Inline Datums (CIP-32)

**Before Babbage**: Datums stored as hashes in UTxOs; actual datums in witnesses.

**With Babbage**: Datums can be embedded directly in outputs.

```
┌─────────────────────────────────────────────────────────────┐
│ Traditional (Alonzo):                                       │
│                                                             │
│   Output: { address, value, datumHash }                    │
│   To spend: Must provide datum in witnesses                │
├─────────────────────────────────────────────────────────────┤
│ With Inline Datum (Babbage):                                │
│                                                             │
│   Output: { address, value, inlineDatum: <actual data> }   │
│   To spend: Datum already present, no witness needed       │
└─────────────────────────────────────────────────────────────┘
```

### Reference Inputs (CIP-31)

Transactions can reference UTxOs without consuming them, enabling read-only access to on-chain data.

---

## Predicate Failures (Errors)

**Reference**: `Utxow.hs:77-109`

```haskell
data BabbageUtxowPredFailure era
  = AlonzoInBabbageUtxowPredFailure (AlonzoUtxowPredFailure era)  -- Wrapped Alonzo errors
  | UtxoFailure (PredicateFailure (EraRule "UTXO" era))
  | MalformedScriptWitnesses (Set ScriptHash)      -- NEW
  | MalformedReferenceScripts (Set ScriptHash)     -- NEW
  | ScriptIntegrityHashMismatch                    -- NEW (replaces PPViewHashesDontMatch)
      (Mismatch RelEQ (StrictMaybe ScriptIntegrityHash))
      (StrictMaybe ByteString)
```

### New Babbage Errors

| Error | When It Occurs |
|-------|----------------|
| `MalformedScriptWitnesses` | Scripts in witnesses have invalid CBOR structure |
| `MalformedReferenceScripts` | Scripts in reference UTxOs have invalid CBOR structure |
| `ScriptIntegrityHashMismatch` | Replaces `PPViewHashesDontMatch` with better error info |

---

## Main Transition Function

**Reference**: `Utxow.hs:315-401`

```haskell
babbageUtxowTransition ::
  forall era.
  ( ... constraints ... ) =>
  TransitionRule (EraRule "UTXOW" era)
babbageUtxowTransition = do
  TRC (utxoEnv@(UtxoEnv _ pp certState), u, tx) <- judgmentContext

  let utxo = utxosUtxo u
      txBody = tx ^. bodyTxL
      witsKeyHashes = keyHashWitnessesTxWits (tx ^. witsTxL)
      scriptsProvided = getScriptsProvided utxo tx  -- CHANGED: includes reference scripts

  -- REUSED: Native script validation (line 343)
  runTestOnSignal $ Shelley.validateFailedNativeScripts scriptsProvided tx

  -- CHANGED: Script presence check with reference scripts (line 347-359)
  let scriptsNeeded = getScriptsNeeded utxo txBody
      scriptHashesNeeded = getScriptsHashesNeeded scriptsNeeded
  runTest $ babbageMissingScripts scriptsProvided scriptHashesNeeded

  -- REUSED: Check required datums (line 362)
  runTest $ Alonzo.missingRequiredDatums utxo tx

  -- REUSED: Check exact redeemers (line 364)
  runTest $ Alonzo.hasExactSetOfRedeemers tx scriptsProvided scriptsNeeded

  -- REUSED: VKey verification (line 368)
  runTestOnSignal $ Shelley.validateVerifiedWits tx

  -- REUSED: Required witnesses (line 372)
  runTest $ Alonzo.validateNeededWitnesses witsKeyHashes certState utxo txBody

  -- REUSED: MIR signatures (line 379-380)
  let genDelegs = certState ^. certDStateL . dsGenDelegsL
  coreNodeQuorum <- liftSTS $ asks quorum
  runTest $ Shelley.validateMIRInsufficientGenesisSigs genDelegs coreNodeQuorum witsKeyHashes tx

  -- REUSED: Metadata (line 386)
  runTestOnSignal $ Shelley.validateMetadata pp tx

  -- NEW: Validate script well-formedness (line 392-396)
  runTest $ validateScriptsWellFormed scriptsProvided tx

  -- REUSED: Script integrity hash (line 399)
  let scriptIntegrity = mkScriptIntegrity pp tx scriptsProvided scriptHashesNeeded
  runTest $ checkScriptIntegrityHash tx pp scriptIntegrity

  -- Call UTXO rule (line 401)
  trans @(EraRule "UTXO" era) $ TRC (utxoEnv, u, tx)
```

### Validation Steps Summary

| Step | Function | Change from Alonzo |
|------|----------|-------------------|
| 1 | Native scripts | Same |
| 2 | Script presence | **Changed**: Reference scripts |
| 3 | Required datums | Same |
| 4 | Exact redeemers | Same |
| 5 | VKey verification | Same |
| 6 | Required witnesses | Same |
| 7 | MIR signatures | Same |
| 8 | Metadata (Shelley full: hash + validMetadatum when pv > (2,0)) | Same |
| 9 | **Script well-formedness** | **New** |
| 10 | Script integrity | Same |
| 11 | UTXO rule | Same |

---

## New/Changed Validation Functions

### 1. getScriptsProvided (Changed)

**Reference**: `eras/babbage/impl/src/Cardano/Ledger/Babbage/UTxO.hs:63-81`

```haskell
getBabbageScriptsProvided ::
  (BabbageEraTxBody era, AlonzoEraScript era) =>
  UTxO era -> Tx l era -> ScriptsProvided era
getBabbageScriptsProvided utxo tx =
  ScriptsProvided
    ( tx ^. witsTxL . scriptTxWitsL            -- Scripts from witnesses
        `Map.union` refScripts                  -- NEW: Scripts from reference inputs
    )
  where
    refScripts =
      Map.fromList
        [(hashScript s, s) | s <- mapMaybe (^. referenceScriptTxOutL) refUTxOOutputs]
    inputs = (tx ^. bodyTxL . referenceInputsTxBodyL)
               `Set.union` (tx ^. bodyTxL . inputsTxBodyL)
    refUTxOOutputs = Map.elems $ unUTxO $ txInsFilter utxo inputs
```

**What's new**:
1. Collects scripts from transaction witnesses (same as Alonzo)
2. **Also collects scripts from referenced UTxOs** (new in Babbage)
3. Scripts from both regular inputs and reference inputs are included

**Plain English**: "Scripts can now come from either witnesses OR UTxO outputs."

---

### 2. babbageMissingScripts

**Reference**: `Utxow.hs:208-233`

```haskell
babbageMissingScripts ::
  ScriptsProvided era ->
  Set ScriptHash ->
  Test (BabbageUtxowPredFailure era)
babbageMissingScripts (ScriptsProvided scriptsProvided) scriptHashesNeeded = do
  let scriptsReceived = Map.keysSet scriptsProvided
      neededAndNotReceived = scriptHashesNeeded `Set.difference` scriptsReceived
      receivedAndNotNeeded = scriptsReceived `Set.difference` scriptHashesNeeded
      -- Reference scripts are allowed to be "extra" (they're not in witnesses)
      receivedAsWitAndNotNeeded = receivedAndNotNeeded `Set.intersection` witnessScripts
  sequenceA_
    [ failureUnless (Set.null neededAndNotReceived)
        (injectFailure $ MissingScriptWitnessesUTXOW neededAndNotReceived)
    , failureUnless (Set.null receivedAsWitAndNotNeeded)
        (injectFailure $ ExtraneousScriptWitnessesUTXOW receivedAsWitAndNotNeeded)
    ]
```

**What's different from Alonzo**:
- Reference scripts can be "extra" without error (they're in UTxOs, not witnesses)
- Only witness scripts are checked for extraneous

**Plain English**: "Check scripts are provided (from witnesses OR references). Only warn about extra witness scripts."

---

### 3. validateScriptsWellFormed (New)

**Reference**: `Utxow.hs:248-277`

```haskell
validateScriptsWellFormed ::
  (AlonzoEraScript era, BabbageEraTxBody era) =>
  ScriptsProvided era -> Tx l era -> Test (BabbageUtxowPredFailure era)
validateScriptsWellFormed (ScriptsProvided scriptsProvided) tx = do
  let scriptWits = tx ^. witsTxL . scriptTxWitsL
      malformedWitnesses =
        Map.keysSet $ Map.filter (not . isPlutusScriptWellFormed) $ Map.mapMaybe toPlutusScript scriptWits
      malformedReferences =
        Map.keysSet $ Map.filter (not . isPlutusScriptWellFormed) $ Map.mapMaybe toPlutusScript refScripts
  sequenceA_
    [ failureUnless (Set.null malformedWitnesses) (MalformedScriptWitnesses malformedWitnesses)
    , failureUnless (Set.null malformedReferences) (MalformedReferenceScripts malformedReferences)
    ]
```

**What it does**:
1. Checks each Plutus script in witnesses for valid CBOR structure
2. Checks each Plutus script in reference UTxOs for valid CBOR structure
3. Separates errors by source (witness vs reference)

**Why needed?**
- Reference scripts are stored on-chain by users
- Malformed scripts could cause issues during execution
- Better to catch in Phase 1 than fail in Phase 2

**Plain English**: "All Plutus scripts must be properly formatted, whether from witnesses or on-chain."

---

### 4. getSupplementalDataHashes (Changed)

**Reference**: `eras/babbage/impl/src/Cardano/Ledger/Babbage/UTxO.hs:83-92`

```haskell
getBabbageSupplementalDataHashes ::
  (BabbageEraTxBody era, AlonzoEraTxOut era) =>
  UTxO era -> TxBody l era -> Set.Set DataHash
getBabbageSupplementalDataHashes utxo txBody =
  outputHashes `Set.union` refInputHashes  -- NEW: includes reference inputs
  where
    outputHashes = Set.fromList [dh | out <- outputs, Just dh <- [out ^. dataHashTxOutL]]
    refInputHashes = Set.fromList
      [ dh
      | txin <- Set.toList (txBody ^. referenceInputsTxBodyL)
      , Just out <- [txinLookup txin utxo]
      , Just dh <- [out ^. dataHashTxOutL]
      ]
```

**What's new**: Datums for reference inputs are now allowed as supplemental datums.

**Plain English**: "You can provide datums for reference inputs, not just outputs."

---

## Inline Datums

**Reference**: `eras/babbage/impl/src/Cardano/Ledger/Babbage/TxOut.hs`

```haskell
data Datum era
  = NoDatum
  | DatumHash DataHash
  | Datum (BinaryData era)  -- NEW: Inline datum
```

**How inline datums work**:
1. Output contains full datum data (not just hash)
2. When spending, datum is read directly from UTxO
3. No need to include datum in witnesses
4. `getInputDataHashesTxBody` recognizes inline datums and doesn't add to `inputHashes`

**Plain English**: "With inline datums, the datum is already there in the UTxO. No need to provide it separately."

---

## Reference Inputs

**Reference**: `eras/babbage/impl/src/Cardano/Ledger/Babbage/TxBody.hs`

```haskell
referenceInputsTxBodyL :: Lens' (BabbageTxBody era) (Set TxIn)
```

**How reference inputs work**:
1. Listed in `referenceInputs` field of transaction body
2. Not consumed (stay in UTxO)
3. Data accessible to Plutus scripts via `ScriptContext`
4. Scripts in referenced outputs become available

**Use cases**:
- Read oracle data without consuming it
- Access shared scripts without including in witnesses
- Read configuration data stored on-chain

---

## Summary: What Babbage Adds to UTXOW

| Feature | Purpose |
|---------|---------|
| **Reference scripts** | Reduce tx size by referencing on-chain scripts |
| **Inline datums** | Simplify dApp UX, datums embedded in outputs |
| **Reference inputs** | Read-only access to UTxO data |
| **Script well-formedness** | Validate script structure before execution |
| **Changed error type** | `BabbageUtxowPredFailure` wraps `AlonzoUtxowPredFailure` |

**Validation changes**:
- `getScriptsProvided` now includes reference scripts
- `babbageMissingScripts` allows extra reference scripts
- New `validateScriptsWellFormed` check
- `getSupplementalDataHashes` includes reference input datums
