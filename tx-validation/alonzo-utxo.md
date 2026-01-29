# Alonzo Era UTXO Validation

## Overview

The Alonzo era introduces **Plutus smart contracts** to Cardano. The UTXO rule gains significant new validations for:
- **Collateral** - protection against script failures
- **Execution units** - resource limits for script execution
- **Validity interval forecasting** - scripts need time translation
- **Network ID in tx body** - explicit network specification

**Haskell Reference**: `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Rules/Utxo.hs`

## Era Architecture

```
Alonzo UTXO builds on Allegra:
┌─────────────────────────────────────────────────────────────────────────┐
│ AlonzoUtxoPredFailure                                                   │
│ ├── All Allegra/Shelley errors (reused via injection)                   │
│ └── New Alonzo-specific errors:                                         │
│     ├── OutsideForecast                                                 │
│     ├── InsufficientCollateral                                          │
│     ├── CollateralContainsNonADA                                        │
│     ├── ScriptsNotPaidUTxO                                              │
│     ├── NoCollateralInputs                                              │
│     ├── TooManyCollateralInputs                                         │
│     ├── ExUnitsTooBigUTxO                                               │
│     ├── WrongNetworkInTxBody                                            │
│     └── UtxosFailure (Phase 2 failure wrapper)                          │
└─────────────────────────────────────────────────────────────────────────┘
```

## Transition Rule Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         ALONZO UTXO TRANSITION                               │
│                                                                              │
│  Input: (UtxoEnv, UTxOState, Tx)                                            │
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 1: validateOutsideValidityIntervalUTxO (from Allegra)          │    │
│  │   Formal: ininterval slot (txvld txb)                               │    │
│  │   Error: OutsideValidityIntervalUTxO                                │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 2: validateOutsideForecast (NEW in Alonzo)                     │    │
│  │   Formal: epochInfoSlotToUTCTime ≠ ◇ for validity interval end      │    │
│  │   Error: OutsideForecast                                            │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 3: validateInputSetEmptyUTxO (from Shelley)                    │    │
│  │   Formal: txins txb ≠ ∅                                             │    │
│  │   Error: InputSetEmptyUTxO                                          │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 4: feesOK (NEW comprehensive check in Alonzo)                  │    │
│  │   Includes:                                                         │    │
│  │   ├── 4a: validateFeeTooSmall                                       │    │
│  │   └── 4b: validateCollateral (if redeemers present)                 │    │
│  │       ├── validateScriptsNotPaidUTxO                                │    │
│  │       ├── validateInsufficientCollateral                            │    │
│  │       ├── validateCollateralContainsNonADA                          │    │
│  │       └── NoCollateralInputs                                        │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 5: validateBadInputsUTxO (from Shelley)                        │    │
│  │   Formal: (txins txb) ∪ (collateral txb) ⊆ dom utxo                 │    │
│  │   Error: BadInputsUTxO                                              │    │
│  │   NOTE: Now includes collateral inputs!                             │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 6: validateValueNotConservedUTxO (from Shelley)                │    │
│  │   Formal: consumed pp utxo txb = produced pp poolParams txb         │    │
│  │   Error: ValueNotConservedUTxO                                      │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 7: validateOutputTooSmallUTxO (Alonzo version)                 │    │
│  │   Formal: coin ≥ utxoEntrySizetxout * coinsPerUTxOWord pp           │    │
│  │   Error: OutputTooSmallUTxO                                         │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 8: validateOutputTooBigUTxO                                    │    │
│  │   Formal: serSize (getValue txout) ≤ maxValSize pp                  │    │
│  │   Error: OutputTooBigUTxO                                           │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 9: validateOutputBootAddrAttrsTooBig (from Shelley)            │    │
│  │   Error: OutputBootAddrAttrsTooBig                                  │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 10: validateWrongNetwork (from Shelley)                        │    │
│  │   Error: WrongNetwork                                               │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 11: validateWrongNetworkWithdrawal (from Shelley)              │    │
│  │   Error: WrongNetworkWithdrawal                                     │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 12: validateWrongNetworkInTxBody (NEW in Alonzo)               │    │
│  │   Formal: (txnetworkid txb = NetworkId) ∨ (txnetworkid txb = ◇)     │    │
│  │   Error: WrongNetworkInTxBody                                       │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 13: validateMaxTxSizeUTxO (from Shelley)                       │    │
│  │   Error: MaxTxSizeUTxO                                              │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 14: validateExUnitsTooBigUTxO (NEW in Alonzo)                  │    │
│  │   Formal: totExunits tx ≤ maxTxExUnits pp                           │    │
│  │   Error: ExUnitsTooBigUTxO                                          │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 15: UTXOS Sub-rule (Phase 2 - Script Execution)                │    │
│  │   This is where Plutus scripts are actually evaluated               │    │
│  │   Error: UtxosFailure (wraps Phase 2 errors)                        │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                                                              │
│  Output: Updated UTxOState                                                   │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Haskell Implementation

```haskell
-- Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Rules/Utxo.hs:472-556
utxoTransition ::
  forall era.
  ( EraUTxO era
  , AlonzoEraTx era
  , AtMostEra "Babbage" era
  ...
  ) =>
  TransitionRule (EraRule "UTXO" era)
utxoTransition = do
  TRC (UtxoEnv slot pp dpstate, utxos, tx) <- judgmentContext
  let utxo = utxosUtxo utxos
      txBody = tx ^. bodyTxL
      inputsAndCollateral =
        Set.union
          (txBody ^. inputsTxBodyL)
          (txBody ^. collateralInputsTxBodyL)

  {- ininterval slot (txvld txb) -}
  runTest $ Allegra.validateOutsideValidityIntervalUTxO slot txBody

  sysSt <- liftSTS $ asks systemStart
  ei <- liftSTS $ asks epochInfo

  {- epochInfoSlotToUTCTime epochInfo systemTime i_f ≠ ◇ -}
  runTest $ validateOutsideForecast ei slot sysSt tx

  {-   txins txb ≠ ∅   -}
  runTestOnSignal $ Shelley.validateInputSetEmptyUTxO txBody

  {-   feesOK pp tx utxo   -}
  runTest $ feesOK pp tx utxo

  {- (txins txb) ∪ (collateral txb) ⊆ dom utxo   -}
  runTest $ Shelley.validateBadInputsUTxO utxo inputsAndCollateral

  {- consumed pp utxo txb = produced pp poolParams txb -}
  runTest $ Shelley.validateValueNotConservedUTxO pp utxo dpstate txBody

  {-   ∀ txout ∈ txouts txb, getValue txout ≥ inject (...) -}
  runTest $ validateOutputTooSmallUTxO pp outputs

  {-   ∀ txout ∈ txouts txb, serSize (getValue txout) ≤ maxValSize pp   -}
  runTest $ validateOutputTooBigUTxO pp outputs

  {- ∀ ( _ ↦ (a,_)) ∈ txoutstxb,  a ∈ Addrbootstrap → bootstrapAttrsSize a ≤ 64 -}
  runTestOnSignal $ Shelley.validateOutputBootAddrAttrsTooBig outputs

  netId <- liftSTS $ asks networkId

  {- ∀(_ → (a, _)) ∈ txouts txb, netId a = NetworkId -}
  runTestOnSignal $ Shelley.validateWrongNetwork netId outputs

  {- ∀(a → ) ∈ txwdrls txb, netId a = NetworkId -}
  runTestOnSignal $ Shelley.validateWrongNetworkWithdrawal netId txBody

  {- (txnetworkid txb = NetworkId) ∨ (txnetworkid txb = ◇) -}
  runTestOnSignal $ validateWrongNetworkInTxBody netId txBody

  {- txsize tx ≤ maxTxSize pp -}
  runTestOnSignal $ Shelley.validateMaxTxSizeUTxO pp tx

  {-   totExunits tx ≤ maxTxExUnits pp    -}
  runTest $ validateExUnitsTooBigUTxO pp tx

  -- Call UTXOS (Phase 2 - script execution)
  trans @(EraRule "UTXOS" era) =<< coerce <$> judgmentContext
```

---

## New Alonzo Validation Functions (Detailed)

### 1. validateOutsideForecast

**Reference**: `Alonzo/Utxo.hs:352-385`

**Formal Specification**: `epochInfoSlotToUTCTime epochInfo systemTime i_f ≠ ◇`

```haskell
validateOutsideForecast ::
  ( MaryEraTxBody era
  , AlonzoEraTxWits era
  , EraTx era
  ) =>
  EpochInfo (Either a) ->
  SlotNo ->
  SystemStart ->
  Tx l era ->
  Test (AlonzoUtxoPredFailure era)
validateOutsideForecast ei slot sysSt tx =
  {- (m = SNothing) ∨ (move ≠ Nothing) -}
  when (mb /= SNothing && isNothing move) $
    failureOnJust SNothing (OutsideForecast . unSlotNo) ifSlot
  where
    mb = strictMaybeToMaybe ifSlot
    ifSlot = tx ^. bodyTxL . vldtTxBodyL . invalidHereafterTxBodyL
    move = do
      slotno <- mb
      epochInfoSlotToUTCTime ei sysSt slotno
```

**Step-by-Step Explanation**:

1. **Check for redeemers**: Only applies if transaction has Plutus scripts
2. **Get validity interval end**: `invalidHereafter` from tx body
3. **Try to translate slot to UTC time**: Using epoch info
4. **Fail if translation fails**: Slot is outside consensus forecast

**Why This Matters**:
- Plutus scripts receive time context as POSIXTime
- The ledger must translate slots to UTC time
- Consensus only provides forecasts for a limited window
- Scripts can't run if time translation is impossible

**Error**: `OutsideForecast SlotNo`

---

### 2. feesOK (Comprehensive Fee & Collateral Check)

**Reference**: `Alonzo/Utxo.hs:263-287`

```haskell
feesOK ::
  forall era.
  ( AlonzoEraTx era
  , EraUTxO era
  ) =>
  PParams era ->
  Tx TopTx era ->
  UTxO era ->
  Test (AlonzoUtxoPredFailure era)
feesOK pp tx u@(UTxO utxo) =
  let txBody = tx ^. bodyTxL
      collateral = txBody ^. collateralInputsTxBodyL
      utxoCollateral = Map.restrictKeys utxo collateral
      theFee = txBody ^. feeTxBodyL
      minFee = getMinFeeTxUtxo pp tx u
   in sequenceA_
        [ -- Part 1: minfee pp tx ≤ txfee txb
          failureUnless (minFee <= theFee) (FeeTooSmallUTxO ...)
        , -- Part 2: (txrdmrs tx ≠ ∅ ⇒ validateCollateral)
          unless (null $ tx ^. witsTxL . rdmrsTxWitsL . unRedeemersL) $
            validateCollateral pp txBody utxoCollateral
        ]
```

**Structure**:
- **Part 1**: Basic fee validation (same as Shelley)
- **Part 2**: Collateral validation (only if redeemers present)

---

### 3. validateCollateral (Sub-Function of feesOK)

**Reference**: `Alonzo/Utxo.hs:289-309`

```haskell
validateCollateral ::
  ( EraTxBody era
  , AlonzoEraPParams era
  ) =>
  PParams era ->
  TxBody TopTx era ->
  Map.Map TxIn (TxOut era) ->
  Test (AlonzoUtxoPredFailure era)
validateCollateral pp txb utxoCollateral =
  sequenceA_
    [ -- Part 3: ∀(a,_,_) ∈ range (collateral txb ◁ utxo), a ∈ Addrvkey
      validateScriptsNotPaidUTxO utxoCollateral
    , -- Part 4: balance * 100 ≥ txfee txb * collateralPercent pp
      validateInsufficientCollateral pp txb bal
    , -- Part 5: isAdaOnly balance
      validateCollateralContainsNonADA utxoCollateral
    , -- Part 6: collateral txb ≠ ∅
      failureIf (null utxoCollateral) NoCollateralInputs
    ]
  where
    bal = toDeltaCoin $ sumAllCoin utxoCollateral
```

---

### 4. validateScriptsNotPaidUTxO

**Reference**: `Alonzo/Utxo.hs:311-318`

**Formal Specification**: `∀(a,_,_) ∈ range (collateral txb ◁ utxo), a ∈ Addrvkey`

```haskell
validateScriptsNotPaidUTxO ::
  EraTxOut era =>
  Map.Map TxIn (TxOut era) ->
  Test (AlonzoUtxoPredFailure era)
validateScriptsNotPaidUTxO utxoCollateral =
  failureUnless (all vKeyLocked utxoCollateral) $
    ScriptsNotPaidUTxO (UTxO (Map.filter (not . vKeyLocked) utxoCollateral))
```

**Step-by-Step Explanation**:

1. **Get collateral UTxOs**: Map of collateral inputs to their outputs
2. **Check each output**: Must be locked by a verification key, NOT a script
3. **Fail if any script-locked**: Return the script-locked UTxOs

**Why This Matters**:
- Collateral must be immediately spendable if scripts fail
- Script-locked UTxOs require script execution
- If Phase 2 fails, we can't run more scripts to collect collateral
- Only VKey-locked outputs can be used as collateral

**Error**: `ScriptsNotPaidUTxO (UTxO of script-locked collateral)`

---

### 5. validateInsufficientCollateral

**Reference**: `Alonzo/Utxo.hs:320-336`

**Formal Specification**: `balance * 100 ≥ txfee txb * collateralPercent pp`

```haskell
validateInsufficientCollateral ::
  ( EraTxBody era
  , AlonzoEraPParams era
  ) =>
  PParams era ->
  TxBody TopTx era ->
  DeltaCoin ->
  Test (AlonzoUtxoPredFailure era)
validateInsufficientCollateral pp txBody bal =
  failureUnless (Val.scale (100 :: Int) bal >= Val.scale collPerc (toDeltaCoin txfee)) $
    InsufficientCollateral bal $
      rationalToCoinViaCeiling $
        (fromIntegral collPerc * unCoin txfee) %. knownNonZero @100
  where
    txfee = txBody ^. feeTxBodyL
    collPerc = pp ^. ppCollateralPercentageL
```

**Step-by-Step Explanation**:

1. **Get collateral balance**: Sum of all collateral input values
2. **Get transaction fee**: Declared fee in tx body
3. **Get collateral percentage**: From protocol parameters (150 on mainnet)
4. **Check formula**: `balance * 100 >= fee * collateralPercent`

**Example**:
```
Transaction fee: 1 ADA (1,000,000 lovelace)
Collateral percentage: 150%
Minimum collateral needed: 1 ADA * 150 / 100 = 1.5 ADA

Collateral provided: 2 ADA
Check: 2,000,000 * 100 >= 1,000,000 * 150
       200,000,000 >= 150,000,000 ✓ PASS
```

**Error**: `InsufficientCollateral { provided: DeltaCoin, required: Coin }`

---

### 6. validateCollateralContainsNonADA

**Reference**: `Alonzo/Utxo.hs:338-346`

**Formal Specification**: `isAdaOnly (balance collateral)`

```haskell
validateCollateralContainsNonADA ::
  (Foldable f, EraTxOut era) =>
  f (TxOut era) ->
  Test (AlonzoUtxoPredFailure era)
validateCollateralContainsNonADA collateralTxOuts =
  failureUnless (areAllAdaOnly collateralTxOuts) $
    CollateralContainsNonADA $ sumAllValue collateralTxOuts
```

**Step-by-Step Explanation**:

1. **Get all collateral outputs**: From UTxO lookup
2. **Check each value**: Must contain only ADA
3. **Fail if any contain tokens**: Report the total value

**Why This Matters**:
- Simplifies collateral collection if scripts fail
- Native tokens can't be easily distributed
- Protocol just takes ADA as collateral

**Note**: Babbage relaxes this with collateral return output.

**Error**: `CollateralContainsNonADA Value`

---

### 7. NoCollateralInputs

**Formal Specification**: `collInputs tx ≠ ∅` (when redeemers present)

**Step-by-Step Explanation**:

1. **Check if transaction has redeemers**: Indicates Plutus scripts
2. **If redeemers exist**: Collateral inputs must be present
3. **Fail if no collateral**: Can't protect against script failure

**Error**: `NoCollateralInputs`

---

### 8. validateWrongNetworkInTxBody

**Reference**: `Alonzo/Utxo.hs:421-432`

**Formal Specification**: `(txnetworkid txb = NetworkId) ∨ (txnetworkid txb = ◇)`

```haskell
validateWrongNetworkInTxBody ::
  AlonzoEraTxBody era =>
  Network ->
  TxBody t era ->
  Test (AlonzoUtxoPredFailure era)
validateWrongNetworkInTxBody netId txBody =
  case txBody ^. networkIdTxBodyL of
    SNothing -> pure ()
    SJust n ->
      failureUnless (n == netId) $
        WrongNetworkInTxBody Mismatch { mismatchSupplied = n, mismatchExpected = netId }
```

**Step-by-Step Explanation**:

1. **Check for explicit network ID**: New optional field in Alonzo tx body
2. **If absent**: Pass (network determined by addresses)
3. **If present**: Must match the network's ID
4. **Fail on mismatch**: Prevents cross-network transaction submission

**Why This Matters**:
- Explicit protection against submitting to wrong network
- Optional field added in Alonzo
- Addresses already encode network, but this adds extra safety

**Error**: `WrongNetworkInTxBody { supplied, expected }`

---

### 9. validateExUnitsTooBigUTxO

**Reference**: `Alonzo/Utxo.hs:439-456`

**Formal Specification**: `totExunits tx ≤ maxTxExUnits pp`

```haskell
validateExUnitsTooBigUTxO ::
  ( AlonzoEraTxWits era
  , EraTx era
  , AlonzoEraPParams era
  ) =>
  PParams era ->
  Tx t era ->
  Test (AlonzoUtxoPredFailure era)
validateExUnitsTooBigUTxO pp tx =
  failureUnless (pointWiseExUnits (<=) totalExUnits maxTxExUnits) $
    ExUnitsTooBigUTxO Mismatch { mismatchSupplied = totalExUnits, mismatchExpected = maxTxExUnits }
  where
    maxTxExUnits = pp ^. ppMaxTxExUnitsL
    totalExUnits = totExUnits tx
```

**ExUnits Structure**:
```haskell
data ExUnits = ExUnits
  { exUnitsMem :: Natural   -- Memory units
  , exUnitsSteps :: Natural -- CPU steps
  }
```

**Step-by-Step Explanation**:

1. **Sum all script execution units**: From all redeemers in tx
2. **Get maximum allowed**: From protocol parameters
3. **Compare both dimensions**: Memory AND steps must be within limits
4. **Fail if either exceeds**: Report both values

**Mainnet Values** (as of 2024):
- `maxTxExUnits.mem`: 14,000,000
- `maxTxExUnits.steps`: 10,000,000,000

**Error**: `ExUnitsTooBigUTxO { supplied: ExUnits, expected: ExUnits }`

---

### 10. validateTooManyCollateralInputs

**Reference**: `Alonzo/Utxo.hs:458-467`

**Formal Specification**: `‖collateral tx‖ ≤ maxCollInputs pp`

```haskell
validateTooManyCollateralInputs ::
  AlonzoEraTxBody era =>
  PParams era ->
  TxBody t era ->
  Test (AlonzoUtxoPredFailure era)
validateTooManyCollateralInputs pp txBody =
  failureUnless (numColl <= maxColl) $
    TooManyCollateralInputs Mismatch { mismatchSupplied = numColl, mismatchExpected = maxColl }
  where
    maxColl = pp ^. ppMaxCollateralInputsL
    numColl = fromIntegral . Set.size $ txBody ^. collateralInputsTxBodyL
```

**Step-by-Step Explanation**:

1. **Count collateral inputs**: Number of collateral TxIns
2. **Get maximum allowed**: From protocol parameters
3. **Compare**: Must be within limit
4. **Fail if too many**: Report both values

**Mainnet Value**: `maxCollateralInputs = 3`

**Error**: `TooManyCollateralInputs { supplied, expected }`

---

### 11. validateOutputTooSmallUTxO (Alonzo Version)

**Reference**: `Alonzo/Utxo.hs:377-392`

**Formal Specification**: `getValue txout ≥ inject (utxoEntrySizetxout * coinsPerUTxOWord pp)`

```haskell
validateOutputTooSmallUTxO ::
  (AlonzoEraTxOut era, Foldable f) =>
  PParams era ->
  f (TxOut era) ->
  Test (AlonzoUtxoPredFailure era)
validateOutputTooSmallUTxO pp outputs =
  failureUnless (null outputsTooSmall) $ OutputTooSmallUTxO outputsTooSmall
  where
    outputsTooSmall =
      filter
        (\txOut -> txOut ^. coinTxOutL < getMinCoinTxOut pp txOut)
        (toList outputs)
```

**Key Difference from Shelley**:
- Shelley: Fixed `minUTxOValue`
- Alonzo: Size-based calculation using `coinsPerUTxOWord`

**Formula**:
```
minCoin = utxoEntrySizeWithoutVal + (valueSize / 8) * coinsPerUTxOWord
```

Where:
- `utxoEntrySizeWithoutVal`: Base size (27 words for Alonzo)
- `valueSize`: Serialized size of the Value
- `coinsPerUTxOWord`: Protocol parameter (34,482 lovelace on mainnet)

---

### 12. validateOutputTooBigUTxO

**Reference**: `Alonzo/Utxo.hs:394-418`

**Formal Specification**: `serSize (getValue txout) ≤ maxValSize pp`

```haskell
validateOutputTooBigUTxO ::
  (EraTxOut era, AlonzoEraPParams era, Foldable f) =>
  PParams era ->
  f (TxOut era) ->
  Test (AlonzoUtxoPredFailure era)
validateOutputTooBigUTxO pp outputs =
  failureUnless (null outputsTooBig) $ OutputTooBigUTxO outputsTooBig
  where
    maxValSize = pp ^. ppMaxValSizeL
    outputsTooBig = ...
```

**Step-by-Step Explanation**:

1. **Get max value size**: From protocol parameters
2. **For each output**: Serialize the Value field
3. **Check size**: Must be ≤ maxValSize
4. **Collect violations**: Report with actual vs max sizes

**Mainnet Value**: `maxValSize = 5000 bytes`

**Error**: `OutputTooBigUTxO [(actualSize, maxSize, TxOut)]`

---

## Error Type Summary

```haskell
-- Reference: eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Rules/Utxo.hs:91-130
data AlonzoUtxoPredFailure era
  = -- Inherited from Shelley/Allegra
    BadInputsUTxO (Set TxIn)
  | OutsideValidityIntervalUTxO ValidityInterval SlotNo
  | MaxTxSizeUTxO (Mismatch RelLTEQ Word32)
  | InputSetEmptyUTxO
  | FeeTooSmallUTxO (Mismatch RelGTEQ Coin)
  | ValueNotConservedUTxO (Mismatch RelEQ (Value era))
  | WrongNetwork Network (Set Addr)
  | WrongNetworkWithdrawal Network (Set RewardAccount)
  | OutputTooSmallUTxO [TxOut era]
  | OutputBootAddrAttrsTooBig [TxOut era]
    -- NEW in Alonzo
  | UtxosFailure (PredicateFailure (EraRule "UTXOS" era))
  | OutputTooBigUTxO [(Int, Int, TxOut era)]
  | InsufficientCollateral DeltaCoin Coin
  | ScriptsNotPaidUTxO (UTxO era)
  | ExUnitsTooBigUTxO (Mismatch RelLTEQ ExUnits)
  | CollateralContainsNonADA (Value era)
  | WrongNetworkInTxBody (Mismatch RelEQ Network)
  | OutsideForecast SlotNo
  | TooManyCollateralInputs (Mismatch RelLTEQ Natural)
  | NoCollateralInputs
```

---

## Key Differences from Shelley/Allegra

| Feature | Shelley/Allegra | Alonzo |
|---------|----------------|--------|
| Scripts | Native only | + Plutus |
| Collateral | Not needed | Required for Plutus txs |
| Fee check | Simple | + includes collateral check |
| ExUnits | N/A | Limited per tx |
| minUTxOValue | Fixed | Size-based (coinsPerUTxOWord) |
| Network ID | Implicit | Explicit optional field |
| Validity | Just time | + must translate to UTC |
| Phase 2 | N/A | UTXOS sub-rule |

See `babbage-utxo.md` for reference inputs and collateral return.
