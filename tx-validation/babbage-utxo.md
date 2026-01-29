# Babbage Era UTXO Validation

## Overview

The Babbage era introduces significant improvements to Cardano's transaction model:
- **Reference Inputs** - read UTxOs without spending them
- **Inline Datums** - store datums directly in outputs
- **Reference Scripts** - reuse scripts via reference
- **Collateral Return** - get change back from collateral

**Haskell Reference**: `eras/babbage/impl/src/Cardano/Ledger/Babbage/Rules/Utxo.hs`

## Era Architecture

```
Babbage UTXO builds on Alonzo:
┌─────────────────────────────────────────────────────────────────────────┐
│ BabbageUtxoPredFailure                                                  │
│ ├── AlonzoInBabbageUtxoPredFailure (wraps all Alonzo errors)           │
│ └── New Babbage-specific errors:                                        │
│     ├── IncorrectTotalCollateralField                                   │
│     ├── BabbageOutputTooSmallUTxO                                       │
│     └── BabbageNonDisjointRefInputs                                     │
└─────────────────────────────────────────────────────────────────────────┘
```

## Transition Rule Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         BABBAGE UTXO TRANSITION                              │
│                                                                              │
│  Input: (UtxoEnv, UTxOState, Tx)                                            │
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 1: disjointRefInputs (NEW in Babbage)                          │    │
│  │   Formal: inputs ∩ refInputs = ∅                                    │    │
│  │   Error: BabbageNonDisjointRefInputs                                │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 2: validateOutsideValidityIntervalUTxO (from Allegra)          │    │
│  │   Error: OutsideValidityIntervalUTxO                                │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 3: validateOutsideForecast (from Alonzo)                       │    │
│  │   Error: OutsideForecast                                            │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 4: validateInputSetEmptyUTxO (from Shelley)                    │    │
│  │   Error: InputSetEmptyUTxO                                          │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 5: feesOK (Babbage version - includes collateral return)       │    │
│  │   Includes:                                                         │    │
│  │   ├── validateFeeTooSmall                                           │    │
│  │   └── validateCollateral (Babbage version)                          │    │
│  │       ├── validateScriptsNotPaidUTxO                                │    │
│  │       ├── validateInsufficientCollateral                            │    │
│  │       ├── validateCollateralContainsNonADA (relaxed!)               │    │
│  │       ├── NoCollateralInputs                                        │    │
│  │       └── validateCollateralEqBalance (NEW)                         │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 6: validateBadInputsUTxO (Babbage version)                     │    │
│  │   Formal: (spendInputs ∪ collInputs ∪ refInputs) ⊆ dom utxo         │    │
│  │   Error: BadInputsUTxO                                              │    │
│  │   NOTE: Now includes reference inputs!                              │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 7: validateValueNotConservedUTxO (from Shelley)                │    │
│  │   Error: ValueNotConservedUTxO                                      │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 8: validateOutputTooSmallUTxO (Babbage version)                │    │
│  │   Formal: coin ≥ serSize txout * coinsPerUTxOByte pp                │    │
│  │   Error: BabbageOutputTooSmallUTxO (includes min value)             │    │
│  │   NOTE: Checks ALL outputs including collateral return!             │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 9: validateOutputTooBigUTxO (from Alonzo)                      │    │
│  │   Error: OutputTooBigUTxO                                           │    │
│  │   NOTE: Checks ALL outputs including collateral return!             │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 10-13: Network and size checks (from Shelley/Alonzo)           │    │
│  │   NOTE: Checks ALL outputs including collateral return!             │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 14: validateExUnitsTooBigUTxO (from Alonzo)                    │    │
│  │   Error: ExUnitsTooBigUTxO                                          │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 15: validateTooManyCollateralInputs (from Alonzo)              │    │
│  │   Error: TooManyCollateralInputs                                    │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 16: UTXOS Sub-rule (Phase 2 - Script Execution)                │    │
│  │   Error: UtxosFailure                                               │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                                                              │
│  Output: Updated UTxOState                                                   │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Haskell Implementation

```haskell
-- Reference: eras/babbage/impl/src/Cardano/Ledger/Babbage/Rules/Utxo.hs:350-465
babbageUtxoValidation ::
  forall era.
  ( EraUTxO era
  , BabbageEraTxBody era
  , AlonzoEraTxWits era
  ...
  ) =>
  Rule (EraRule "UTXO" era) 'Transition ()
babbageUtxoValidation = do
  TRC (Shelley.UtxoEnv slot pp certState, utxos, tx) <- judgmentContext
  let utxo = utxosUtxo utxos
      txBody = tx ^. bodyTxL
      allInputs = txBody ^. allInputsTxBodyF  -- NEW: includes ref inputs
      refInputs = txBody ^. referenceInputsTxBodyL
      inputs = txBody ^. inputsTxBodyL

  {- inputs ∩ refInputs = ∅ -}
  runTest $ disjointRefInputs pp inputs refInputs

  {- ininterval slot (txvld txb) -}
  runTest $ Allegra.validateOutsideValidityIntervalUTxO slot txBody

  {- epochInfoSlotToUTCTime ≠ ◇ -}
  runTest $ Alonzo.validateOutsideForecast ei slot sysSt tx

  {-   txins txb ≠ ∅   -}
  runTestOnSignal $ Shelley.validateInputSetEmptyUTxO txBody

  {-   feesOK pp tx utxo   -}
  validate $ feesOK pp tx utxo  -- Babbage version

  {- allInputs ⊆ dom utxo -}
  runTest $ Shelley.validateBadInputsUTxO utxo allInputs

  {- consumed == produced -}
  runTest $ Shelley.validateValueNotConservedUTxO pp utxo certState txBody

  let allSizedOutputs = txBody ^. allSizedOutputsTxBodyF  -- includes collateral return
  
  {- ∀ txout ∈ allOuts, getValue txout ≥ inject (serSize txout * coinsPerUTxOByte pp) -}
  runTest $ validateOutputTooSmallUTxO pp allSizedOutputs

  let allOutputs = fmap sizedValue allSizedOutputs
  
  {- ∀ txout ∈ allOuts, serSize (getValue txout) ≤ maxValSize pp -}
  runTest $ Alonzo.validateOutputTooBigUTxO pp allOutputs

  -- ... remaining checks on allOutputs ...

  {- totExunits tx ≤ maxTxExUnits pp -}
  runTest $ Alonzo.validateExUnitsTooBigUTxO pp tx

  {- ‖collateral tx‖ ≤ maxCollInputs pp -}
  runTest $ Alonzo.validateTooManyCollateralInputs pp txBody

utxoTransition = do
  babbageUtxoValidation
  trans @(EraRule "UTXOS" era) =<< coerce <$> judgmentContext
```

---

## New Babbage Validation Functions (Detailed)

### 1. disjointRefInputs

**Reference**: `Babbage/Utxo.hs:225-239`

**Formal Specification**: `inputs ∩ refInputs = ∅`

```haskell
disjointRefInputs ::
  forall era.
  EraPParams era =>
  PParams era ->
  Set TxIn ->
  Set TxIn ->
  Test (BabbageUtxoPredFailure era)
disjointRefInputs pp inputs refInputs =
  when
    ( pvMajor (pp ^. ppProtocolVersionL) > eraProtVerHigh @BabbageEra
        && pvMajor (pp ^. ppProtocolVersionL) < natVersion @11
    )
    (failureOnNonEmpty common BabbageNonDisjointRefInputs)
  where
    common = inputs `Set.intersection` refInputs
```

**Step-by-Step Explanation**:

1. **Get both input sets**: Regular spending inputs and reference inputs
2. **Compute intersection**: Find any TxIns that appear in both
3. **Check for overlap**: If any overlap, fail
4. **Protocol version check**: Only enforced in certain versions

**Why This Matters**:
- Reference inputs are READ-ONLY - they don't get spent
- Spending inputs are CONSUMED - they get removed from UTxO
- Same input can't be both read and consumed
- Prevents confusion in transaction semantics

**Error**: `BabbageNonDisjointRefInputs (NonEmpty TxIn)`

---

### 2. feesOK (Babbage Version with Collateral Return)

**Reference**: `Babbage/Utxo.hs:136-219`

The Babbage version of `feesOK` adds support for **collateral return output**.

```haskell
feesOK ::
  forall era.
  ( BabbageEraTxBody era
  , AlonzoEraTxWits era
  , EraUTxO era
  ) =>
  PParams era ->
  Tx TopTx era ->
  UTxO era ->
  Test (AlonzoUtxoPredFailure era)
feesOK pp tx (UTxO utxo) = ...
  -- Same as Alonzo, but collateral validation is different
```

Key difference: Collateral validation now considers **collateral return**.

---

### 3. validateCollateralContainsNonADA (Babbage Version - Relaxed!)

**Reference**: `Babbage/Utxo.hs:278-317`

**This is a major UX improvement in Babbage!**

```haskell
validateCollateralContainsNonADA ::
  forall era.
  BabbageEraTxBody era =>
  TxBody TopTx era ->
  Map.Map TxIn (TxOut era) ->
  Test (AlonzoUtxoPredFailure era)
validateCollateralContainsNonADA txBody utxoCollateral =
  failureUnless onlyAdaInCollateral $ CollateralContainsNonADA valueWithNonAda
  where
    onlyAdaInCollateral =
      utxoCollateralAndReturnHaveOnlyAda || allNonAdaIsConsumedByReturn

    -- NEW: Non-ADA is OK if collateral return consumes it
    allNonAdaIsConsumedByReturn = Val.isAdaOnly totalCollateralBalance

    totalCollateralBalance = case txBody ^. collateralReturnTxBodyL of
      SNothing -> collateralBalance
      SJust retTxOut -> collateralBalance <-> (retTxOut ^. valueTxOutL @era)
```

**Step-by-Step Explanation**:

1. **Sum collateral input values**: Get total value of all collateral inputs
2. **Subtract collateral return**: If present, subtract return output value
3. **Check NET collateral**: Only the net (what would be taken) must be ADA-only
4. **Pass if net is ADA-only**: Even if inputs have tokens

**Example (Alonzo vs Babbage)**:

**Alonzo (strict)**:
```
Collateral input: 10 ADA + 100 TokenA
Result: FAIL - CollateralContainsNonADA

User had to find ADA-only UTxOs for collateral!
```

**Babbage (relaxed)**:
```
Collateral input: 10 ADA + 100 TokenA
Collateral return: 8 ADA + 100 TokenA
Net collateral: 2 ADA (ADA-only!)

Result: PASS - Native tokens returned to user
```

**Why This Matters**:
- In Alonzo, users had to carefully select ADA-only UTxOs for collateral
- Many wallets didn't have "pure ADA" UTxOs available
- Babbage allows using any UTxO as collateral
- Collateral return gets back the tokens if script succeeds
- If script fails, only the NET collateral (ADA-only) is taken

---

### 4. validateCollateralEqBalance

**Reference**: `Babbage/Utxo.hs:320-325`

**Formal Specification**: `(txcoll tx ≠ ◇) ⇒ balance = txcoll tx`

```haskell
validateCollateralEqBalance ::
  DeltaCoin -> StrictMaybe Coin -> Validation (NonEmpty (BabbageUtxoPredFailure era)) ()
validateCollateralEqBalance bal txcoll =
  case txcoll of
    SNothing -> pure ()
    SJust tc -> failureUnless (bal == toDeltaCoin tc) (IncorrectTotalCollateralField bal tc)
```

**Step-by-Step Explanation**:

1. **Check for totalCollateral field**: New optional field in Babbage tx body
2. **If absent**: No check needed (computed automatically)
3. **If present**: Must exactly match computed collateral balance
4. **Fail on mismatch**: Report both values

**Why This Matters**:
- Wallets can specify exact collateral amount upfront
- Provides certainty about maximum loss if scripts fail
- Protects against wallet bugs that might miscalculate
- Optional field - can be omitted for backward compatibility

**Error**: `IncorrectTotalCollateralField { computed: DeltaCoin, declared: Coin }`

---

### 5. validateOutputTooSmallUTxO (Babbage Version)

**Reference**: `Babbage/Utxo.hs:328-348`

**Formal Specification**: `∀ txout ∈ allOuts txb, getValue txout ≥ inject (serSize txout * coinsPerUTxOByte pp)`

```haskell
validateOutputTooSmallUTxO ::
  forall era f.
  (BabbageEraTxOut era, Foldable f) =>
  PParams era ->
  f (Sized (TxOut era)) ->
  Test (BabbageUtxoPredFailure era)
validateOutputTooSmallUTxO pp outputs =
  failureUnless (null outputsTooSmall) $
    BabbageOutputTooSmallUTxO (map (\(out, Coin minSize) -> (sizedValue out, Coin minSize)) outputsTooSmall)
  where
    outputsTooSmall =
      filter
        (\(sizedTxOut, _) ->
          sizedValue sizedTxOut ^. coinTxOutL < getMinCoinSizedTxOut pp sizedTxOut
        )
        [(sized, getMinCoinSizedTxOut pp sized) | sized <- toList outputs]
```

**Key Differences from Alonzo**:

1. **Checks ALL outputs**: Including collateral return output (`allOuts`)
2. **Uses coinsPerUTxOByte**: Instead of Alonzo's `coinsPerUTxOWord`
3. **Returns min value in error**: Helpful for wallets to fix the issue
4. **Sized outputs**: Pre-computed sizes for efficiency

**Formula Change**:
```
Alonzo:  minCoin = (utxoEntrySize + valueSize/8) * coinsPerUTxOWord
Babbage: minCoin = serSize(txOut) * coinsPerUTxOByte
```

The Babbage formula is more accurate because it uses the actual serialized size.

**Error**: `BabbageOutputTooSmallUTxO [(TxOut, minCoin)]`

---

### 6. allInputsTxBodyF (New Field Accessor)

**Not a validation function, but important for understanding**:

```haskell
allInputsTxBodyF :: SimpleGetter (TxBody era) (Set TxIn)
allInputsTxBodyF = to $ \txBody ->
  Set.union
    (txBody ^. inputsTxBodyL)
    (Set.union
      (txBody ^. collateralInputsTxBodyL)
      (txBody ^. referenceInputsTxBodyL))
```

This combines:
- **Spending inputs** (`inputsTxBodyL`)
- **Collateral inputs** (`collateralInputsTxBodyL`)
- **Reference inputs** (`referenceInputsTxBodyL`)

All three must exist in the UTxO set for the transaction to be valid.

---

### 7. allSizedOutputsTxBodyF (New Field Accessor)

```haskell
allSizedOutputsTxBodyF :: SimpleGetter (TxBody era) (StrictSeq (Sized (TxOut era)))
allSizedOutputsTxBodyF = to $ \txBody ->
  case txBody ^. collateralReturnTxBodyL of
    SNothing -> txBody ^. sizedOutputsTxBodyL
    SJust retOut -> txBody ^. sizedOutputsTxBodyL :|> mkSized (eraProtVerLow @era) retOut
```

This includes:
- **Regular outputs** (`sizedOutputsTxBodyL`)
- **Collateral return output** (if present)

Both are validated for min UTxO value, max value size, etc.

---

## Error Type Summary

```haskell
-- Reference: eras/babbage/impl/src/Cardano/Ledger/Babbage/Rules/Utxo.hs:68-79
data BabbageUtxoPredFailure era
  = -- | Wraps all Alonzo errors
    AlonzoInBabbageUtxoPredFailure (AlonzoUtxoPredFailure era)
    -- | totalCollateral field doesn't match computed balance
  | IncorrectTotalCollateralField DeltaCoin Coin
    -- | Outputs below minimum with minimum value included
  | BabbageOutputTooSmallUTxO [(TxOut era, Coin)]
    -- | Reference inputs overlap with spending inputs
  | BabbageNonDisjointRefInputs (NonEmpty TxIn)
```

---

## Key Differences from Alonzo

| Feature | Alonzo | Babbage |
|---------|--------|---------|
| Reference Inputs | N/A | Supported (read-only) |
| Inline Datums | N/A | Supported (in outputs) |
| Reference Scripts | N/A | Supported (in outputs) |
| Collateral Return | N/A | Supported (get change back) |
| Collateral Tokens | Must be ADA-only | OK if return handles tokens |
| totalCollateral field | N/A | Optional explicit field |
| minUTxO calculation | coinsPerUTxOWord | coinsPerUTxOByte (more accurate) |
| allInputs check | spend + collateral | spend + collateral + reference |
| allOutputs check | regular outputs | regular + collateral return |

---

## Reference Inputs Deep Dive

### What Are Reference Inputs?

Reference inputs allow a transaction to **read** a UTxO without **spending** it:

```
Regular Input:     UTxO → Transaction → UTxO consumed (removed from set)
Reference Input:   UTxO → Transaction → UTxO unchanged (still in set)
```

### Why Reference Inputs Matter

1. **Script Sharing**:
   - Store a Plutus script in a UTxO once
   - Many transactions reference it instead of including it
   - Saves transaction size and fees

2. **State Reading**:
   - DeFi protocols can read oracle data without consuming it
   - Multiple transactions can read the same state

3. **Datum Sharing**:
   - Complex datums stored once, referenced many times

### Validation Rules

1. **Must exist**: All reference inputs must be in UTxO set
2. **Must be disjoint**: Can't both spend and reference same UTxO
3. **No spending scripts**: Reference inputs don't require script execution

---

## Collateral Return Deep Dive

### The Problem (Alonzo)

In Alonzo, finding suitable collateral was painful:
- Collateral must be VKey-locked
- Collateral must contain only ADA
- Must provide 150% of fee
- Most UTxOs have native tokens attached!

### The Solution (Babbage)

Collateral return output lets you "make change":

```
BEFORE (Alonzo):
  Collateral Input: 100 ADA + 50 TokenX
  → FAIL: CollateralContainsNonADA

AFTER (Babbage):
  Collateral Input: 100 ADA + 50 TokenX
  Collateral Return: 98 ADA + 50 TokenX
  Net Collateral: 2 ADA
  
  If script succeeds: Normal execution, collateral untouched
  If script fails: User loses 2 ADA, gets back 98 ADA + 50 TokenX
```

### totalCollateral Field

Optional field that declares the exact net collateral:

```
totalCollateral = 2 ADA

Validation checks:
  sum(collateral_inputs) - collateral_return = totalCollateral
  2 ADA = 2 ADA ✓
```

Benefits:
- Explicit maximum loss amount
- Protection against wallet bugs
- Clear user expectation

See `conway-utxo.md` for governance-era changes.
