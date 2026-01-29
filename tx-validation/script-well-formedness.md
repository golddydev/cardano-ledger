# Script Well-Formedness Validation

## Important Note: Alonzo vs Babbage

**Alonzo Era**: Does NOT check script well-formedness in UTXOW validation.
- The `validScript` function exists in `Alonzo/Scripts.hs` but is **never called** in Alonzo's UTXOW transition
- Scripts are assumed to be well-formed if they deserialized successfully

**Babbage Era**: Introduces script well-formedness checking.
- This is a **NEW validation step** added in Babbage
- Checks both witness scripts and reference scripts

---

## Babbage Implementation

### Reference Code

**File**: `eras/babbage/impl/src/Cardano/Ledger/Babbage/Rules/Utxow.hs:254-280`

```haskell
validateScriptsWellFormed ::
  forall era.
  ( EraTx era
  , BabbageEraTxBody era
  ) =>
  PParams era ->
  Tx TopTx era ->
  Test (BabbageUtxowPredFailure era)
validateScriptsWellFormed pp tx =
  sequenceA_
    [ failureUnless (Map.null invalidScriptWits) $
        MalformedScriptWitnesses (Map.keysSet invalidScriptWits)
    , failureUnless (null invalidRefScripts) $ MalformedReferenceScripts invalidRefScriptHashes
    ]
  where
    scriptWits = tx ^. witsTxL . scriptTxWitsL
    invalidScriptWits = Map.filter (not . validScript (pp ^. ppProtocolVersionL)) scriptWits

    txBody = tx ^. bodyTxL
    normalOuts = toList $ txBody ^. outputsTxBodyL
    returnOut = txBody ^. collateralReturnTxBodyL
    outs = case returnOut of
      SNothing -> normalOuts
      SJust rOut -> rOut : normalOuts
    rScripts = mapMaybe (strictMaybeToMaybe . view referenceScriptTxOutL) outs
    invalidRefScripts = filter (not . validScript (pp ^. ppProtocolVersionL)) rScripts
    invalidRefScriptHashes = Set.fromList $ map (hashScript @era) invalidRefScripts
```

### Plain English Explanation

**What it does**:
1. **Witness Scripts**: Checks all scripts in `tx.wits.scripts` using `validScript`
2. **Reference Scripts**: Checks all scripts in transaction outputs' `reference_script` field
3. **Separate Errors**: Reports malformed witness scripts and malformed reference scripts separately

**Why separate errors?**
- Helps identify whether the problem is in witnesses (user-provided) or reference scripts (on-chain)

---

## The `validScript` Function

### Reference Code

**File**: `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Scripts.hs:635-642`

```haskell
-- | Verify that every `Script` represents a valid script. Force native scripts to Normal
-- Form, to ensure that there are no bottoms and deserialize `Plutus` scripts into a
-- `Cardano.Ledger.Plutus.Language.PlutusRunnable`.
validScript :: (HasCallStack, AlonzoEraScript era) => ProtVer -> Script era -> Bool
validScript pv script =
  case toPlutusScript script of
    Just plutusScript -> isValidPlutusScript (pvMajor pv) plutusScript
    Nothing ->
      case getNativeScript script of
        Just timelockScript -> deepseq timelockScript True
        Nothing -> error "Impossible: There are only Native and Plutus scripts available"
```

### Plain English Explanation

**For Native Scripts**:
- Uses `deepseq timelockScript True` to force evaluation to **Normal Form**
- This ensures:
  - No bottom values (undefined, error, etc.)
  - Script structure is valid
  - All nested scripts are evaluated

**For Plutus Scripts**:
- Calls `isValidPlutusScript` which attempts to **deserialize** the script
- If deserialization succeeds → script is well-formed
- If deserialization fails → script is malformed

---

## The `isValidPlutusScript` Function

### Reference Code

**File**: `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Scripts.hs:249-252`

```haskell
-- | Verifies whether Plutus script is well formed or not, which simply means whether it
-- deserializes successfully or not.
isValidPlutusScript :: AlonzoEraScript era => Version -> PlutusScript era -> Bool
isValidPlutusScript pv ps = withPlutusScript ps (isValidPlutus pv)
```

### The `isValidPlutus` Function

**File**: `libs/cardano-ledger-core/src/Cardano/Ledger/Plutus/Language.hs:192-194`

```haskell
-- | Verify that the binary version of the Plutus script is deserializable.
isValidPlutus :: PlutusLanguage l => Version -> Plutus l -> Bool
isValidPlutus v = isRight . decodePlutusRunnable v
```

**Plain English**: 
- Tries to deserialize the Plutus script binary into a `PlutusRunnable`
- Returns `True` if deserialization succeeds (`Right`), `False` if it fails (`Left`)

---

## The `decodePlutusRunnable` Function

### Reference Code

**File**: `libs/cardano-ledger-core/src/Cardano/Ledger/Plutus/Language.hs:429-434`

```haskell
decodePlutusRunnable ::
  -- | Which major protocol version to use for deserialization and further execution
  Version ->
  -- | Binary version of the script that will be deserialized
  Plutus l ->
  Either P.ScriptDecodeError (PlutusRunnable l)
```

### Implementation for Each Language

**PlutusV1** (`Language.hs:481-482`):
```haskell
decodePlutusRunnable pv (Plutus (PlutusBinary bs)) =
  PlutusRunnable <$> PV1.deserialiseScript (toMajorProtocolVersion pv) bs
```

**PlutusV2** (`Language.hs:501-502`):
```haskell
decodePlutusRunnable pv (Plutus (PlutusBinary bs)) =
  PlutusRunnable <$> PV2.deserialiseScript (toMajorProtocolVersion pv) bs
```

**PlutusV3** (`Language.hs:521-522`):
```haskell
decodePlutusRunnable pv (Plutus (PlutusBinary bs)) =
  PlutusRunnable <$> PV3.deserialiseScript (toMajorProtocolVersion pv) bs
```

### What Deserialization Checks

The `deserialiseScript` functions (from PlutusLedgerApi) validate:

1. **CBOR Structure**: Script bytes must be valid CBOR
2. **UPLC Format**: Must be valid Untyped Plutus Core (UPLC) program
3. **Version Compatibility**: Script must be compatible with the protocol version
4. **Well-Formedness**: 
   - Valid term structure
   - No malformed constants
   - Valid builtin function references
   - Proper variable scoping

**If any of these fail**: `Left ScriptDecodeError` → Script is malformed

---

## Summary

### Alonzo Era
- ❌ **No script well-formedness checking**
- Scripts are only validated during execution (if they run)
- Malformed scripts can exist in UTxO but will fail when executed

### Babbage Era
- ✅ **Script well-formedness checking introduced**
- Checks **witness scripts** (in `tx.wits.scripts`)
- Checks **reference scripts** (in output `reference_script` fields)
- Uses `validScript` which:
  - **Native scripts**: Forces evaluation to normal form (`deepseq`)
  - **Plutus scripts**: Attempts deserialization (`decodePlutusRunnable`)

### What Makes a Script Malformed?

**Native Scripts**:
- Contains bottom values (undefined, error)
- Invalid structure (malformed nesting)

**Plutus Scripts**:
- Invalid CBOR encoding
- Not valid UPLC program
- Incompatible with protocol version
- Malformed constants or builtins
- Invalid variable scoping

### Error Types

```haskell
-- Babbage/Rules/Utxow.hs:98-102
MalformedScriptWitnesses (Set ScriptHash)      -- Witness scripts
MalformedReferenceScripts (Set ScriptHash)    -- Reference scripts
```

Both errors contain the **script hashes** of malformed scripts, not the scripts themselves.
