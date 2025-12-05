# Cardano Transaction Phase 1 Validation (Complete)

This document explains **ALL** Phase 1 (UTXO level) validations for Cardano transactions across all eras (Shelley, Allegra, Mary, Alonzo, Babbage, Conway).

## Overview

Phase 1 validation is performed by the UTXO rule and checks transaction structural validity **before** script execution (Phase 2). It ensures:
- Transaction structure is valid
- Inputs exist and fees are sufficient
- Value is conserved
- Network IDs are correct
- Size and execution limits are respected
- **Collateral is properly configured** (Alonzo+)
- **Reference inputs are valid** (Babbage+)

**Phase 1 does NOT execute scripts** - that happens in Phase 2 (UTXOS rule).

## Phase 1 vs Phase 2

| Phase | Rule | What It Does |
|-------|------|--------------|
| **Phase 1** | UTXO | Structural validation, fee checks, collateral validation, value conservation |
| **Phase 2** | UTXOS | Script execution, redeemer validation, datum checks, Plutus evaluation |

## Source Code References

- **Shelley**: `eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxo.hs:343-416`
- **Allegra**: `eras/allegra/impl/src/Cardano/Ledger/Allegra/Rules/Utxo.hs:160-238`
- **Mary**: `eras/mary/impl/src/Cardano/Ledger/Mary/Rules/Utxo.hs` (reuses Allegra)
- **Alonzo**: `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Rules/Utxo.hs:473-557`
- **Babbage**: `eras/babbage/impl/src/Cardano/Ledger/Babbage/Rules/Utxo.hs:350-444`
- **Conway**: `eras/conway/impl/src/Cardano/Ledger/Conway/Rules/Utxo.hs`

## Environment, State, and Signal

### UtxoEnv (Environment)
```haskell
data UtxoEnv era = UtxoEnv
  { ueSlot :: SlotNo          -- Current slot number
  , uePParams :: PParams era  -- Protocol parameters
  , ueCertState :: CertState era -- Certificate state
  }
```

### UTxOState (State)
```haskell
data UTxOState era = UTxOState
  { utxosUtxo :: UTxO era           -- The actual UTxO set
  , utxosDeposited :: Coin          -- Total deposits held
  , utxosFees :: Coin               -- Accumulated fees
  , utxosGovState :: GovState era   -- Governance state
  , utxosInstantStake :: InstantStake
  , utxosDonation :: Coin
  }
```

### Signal (Input)
```haskell
type Signal (EraRule "UTXO" era) = Tx TopTx era
```

---

## Complete Phase 1 Validation Functions

### SHELLEY ERA (Basic Validations)

#### 1. validateTimeToLive (Shelley) / validateOutsideValidityIntervalUTxO (Allegra+)

**Reference**:
- Shelley: `Utxo.hs:421-430`
- Allegra: `Allegra/Utxo.hs:242-249`

**Haskell Code (Shelley)**:
```haskell
validateTimeToLive ::
  (ShelleyEraTxBody era, ExactEra ShelleyEra era) =>
  TxBody TopTx era ->
  SlotNo ->
  Test (ShelleyUtxoPredFailure era)
validateTimeToLive txb slot =
  failureUnless (ttl >= slot) $
    ExpiredUTxO Mismatch {mismatchSupplied = ttl, mismatchExpected = slot}
  where
    ttl = txb ^. ttlTxBodyL
```

**Haskell Code (Allegra+)**:
```haskell
validateOutsideValidityIntervalUTxO ::
  AllegraEraTxBody era =>
  SlotNo ->
  TxBody l era ->
  Test (AllegraUtxoPredFailure era)
validateOutsideValidityIntervalUTxO slot txb =
  failureUnless (inInterval slot (txb ^. vldtTxBodyL)) $
    OutsideValidityIntervalUTxO (txb ^. vldtTxBodyL) slot
```

**Formal Specification**: `txttl txb ≥ slot` (Shelley) or `ininterval slot (txvld tx)` (Allegra+)

**Explanation**:
- **Shelley**: Checks Time To Live (TTL) field. Transaction expires when `slot >= ttl`.
- **Allegra+**: Checks validity interval `(invalidBefore, invalidHereafter)`. Current slot must be within interval using `inInterval`.
- **Key Difference**: Allegra allows specifying BOTH lower and upper bounds on validity, enabling time-locked transactions.
- Prevents old or future transactions from being included.

**Error**: `ExpiredUTxO` (Shelley) or `OutsideValidityIntervalUTxO` (Allegra+)

**Era Architecture Note**:
- Shelley defines this as `validateTimeToLive` returning `ShelleyUtxoPredFailure`
- Allegra defines this as `validateOutsideValidityIntervalUTxO` returning `AllegraUtxoPredFailure`
- Mary reuses Allegra's implementation (no new validation)

---

#### 2. validateInputSetEmptyUTxO

**Reference**: `Utxo.hs:435-442`

**Haskell Code**:
```haskell
validateInputSetEmptyUTxO ::
  EraTxBody era =>
  TxBody t era ->
  Test (ShelleyUtxoPredFailure era)
validateInputSetEmptyUTxO txb =
  failureUnless (inputs /= Set.empty) InputSetEmptyUTxO
  where
    inputs = txb ^. inputsTxBodyL
```

**Formal Specification**: `txins txb ≠ ∅`

**Explanation**:
- Every transaction must consume at least one UTxO
- Empty inputs would mean creating value from nothing
- Genesis transactions are an exception (not covered by this rule)

**Error**: `InputSetEmptyUTxO`

---

#### 3. validateFeeTooSmallUTxO

**Reference**: `Utxo.hs:447-463`

**Haskell Code**:
```haskell
validateFeeTooSmallUTxO ::
  EraUTxO era =>
  PParams era ->
  Tx TopTx era ->
  UTxO era ->
  Test (ShelleyUtxoPredFailure era)
validateFeeTooSmallUTxO pp tx utxo =
  failureUnless (minFee <= txFee) $
    FeeTooSmallUTxO Mismatch {mismatchSupplied = txFee, mismatchExpected = minFee}
  where
    minFee = getMinFeeTxUtxo pp tx utxo
    txFee = txb ^. feeTxBodyL
    txb = tx ^. bodyTxL
```

**Fee Calculation** (`Shelley/Tx.hs:256-258`):
```haskell
shelleyMinFeeTx :: EraTx era => PParams era -> Tx l era -> Coin
shelleyMinFeeTx pp tx =
  (tx ^. sizeTxF <×> pp ^. ppMinFeeAL) <+> pp ^. ppMinFeeBL
```

**Formula**: `minFee = (txSize * minFeeA) + minFeeB + refScriptFee` (Alonzo+ adds reference script costs)

**Formal Specification**: `minfee pp tx ≤ txfee txb`

**Explanation**:
- Calculates minimum fee based on transaction size
- `minFeeA`: Cost per byte (44 lovelace on mainnet)
- `minFeeB`: Base fee (155,381 lovelace on mainnet)
- Alonzo+ adds costs for reference scripts
- Ensures transactions pay for network resources

**Error**: `FeeTooSmallUTxO`

---

#### 4. validateBadInputsUTxO

**Reference**: `Utxo.hs:468-476`

**Haskell Code**:
```haskell
validateBadInputsUTxO ::
  UTxO era ->
  Set TxIn ->
  Test (ShelleyUtxoPredFailure era)
validateBadInputsUTxO utxo inputs =
  failureUnless (Set.null badInputs) $ BadInputsUTxO badInputs
  where
    badInputs = Set.filter (`Map.notMember` unUTxO utxo) inputs
```

**Formal Specification**: `inputs ⊆ dom utxo`

**Explanation**:
- Checks every transaction input exists in the current UTxO set
- In **Alonzo+**, this includes **collateral inputs** and **reference inputs** (Babbage+)
- Prevents double-spending and ensures inputs are valid
- Note: In Alonzo+, the check is `inputsAndCollateral = txins txb ∪ collateral txb`

**Error**: `BadInputsUTxO`

---

#### 5. validateWrongNetwork

**Reference**: `Utxo.hs:481-492`

**Haskell Code**:
```haskell
validateWrongNetwork ::
  (EraTxOut era, Foldable f) =>
  Network ->
  f (TxOut era) ->
  Test (ShelleyUtxoPredFailure era)
validateWrongNetwork netId outputs =
  failureUnless (null addrsWrongNetwork) $ WrongNetwork netId (Set.fromList addrsWrongNetwork)
  where
    addrsWrongNetwork =
      filter
        (\a -> getNetwork a /= netId)
        (view addrTxOutL <$> toList outputs)
```

**Formal Specification**: `∀(_ → (a, _)) ∈ txouts txb, netId a = NetworkId`

**Explanation**:
- Validates output addresses match the network (mainnet vs testnet)
- Prevents accidentally sending mainnet funds to testnet addresses
- In **Babbage+**, also checks **collateral return output**

**Error**: `WrongNetwork`

---

#### 6. validateWrongNetworkWithdrawal

**Reference**: `Utxo.hs:497-509`

**Haskell Code**:
```haskell
validateWrongNetworkWithdrawal ::
  EraTxBody era =>
  Network ->
  TxBody t era ->
  Test (ShelleyUtxoPredFailure era)
validateWrongNetworkWithdrawal netId txb =
  failureUnless (null withdrawalsWrongNetwork) $
    WrongNetworkWithdrawal netId (Set.fromList withdrawalsWrongNetwork)
  where
    withdrawalsWrongNetwork =
      filter
        (\a -> raNetwork a /= netId)
        (Map.keys . unWithdrawals $ txb ^. withdrawalsTxBodyL)
```

**Formal Specification**: `∀(a → ) ∈ txwdrls txb, netId a = NetworkId`

**Explanation**:
- Validates withdrawal reward account addresses match the network
- Prevents withdrawing rewards to wrong network

**Error**: `WrongNetworkWithdrawal`

---

#### 7. validateValueNotConservedUTxO

**Reference**: `Utxo.hs:514-526`

**Haskell Code**:
```haskell
validateValueNotConservedUTxO ::
  (EraUTxO era, EraCertState era) =>
  PParams era ->
  UTxO era ->
  CertState era ->
  TxBody TopTx era ->
  Test (ShelleyUtxoPredFailure era)
validateValueNotConservedUTxO pp utxo certState txBody =
  failureUnless (consumedValue == producedValue) $
    ValueNotConservedUTxO Mismatch {mismatchSupplied = consumedValue, mismatchExpected = producedValue}
  where
    consumedValue = consumed pp certState utxo txBody
    producedValue = produced pp certState txBody
```

**Consumed Calculation**:
```
Consumed = balance(inputs) + withdrawals + refunds
```

**Produced Calculation**:
```
Produced = sum(outputs) + fees + deposits
```

**Formal Specification**: `consumed pp utxo txb = produced pp poolParams txb`

**Explanation**:
- **Fundamental ledger invariant**: No value created or destroyed
- **Consumed** includes:
  1. All values from input UTxOs
  2. Withdrawal rewards from stake accounts
  3. Deposit refunds (from deregistrations)
- **Produced** includes:
  1. All values in output UTxOs
  2. Transaction fee (goes to fee pot)
  3. New deposits (from registrations)

**Error**: `ValueNotConservedUTxO`

---

#### 8. validateOutputTooSmallUTxO

**Reference**: `Utxo.hs:531-545` (Shelley), `Alonzo/Utxo.hs:377-392` (Alonzo), `Babbage/Utxo.hs:328-348` (Babbage)

**Haskell Code (Shelley)**:
```haskell
validateOutputTooSmallUTxO ::
  (EraTxOut era, Foldable f) =>
  PParams era ->
  f (TxOut era) ->
  Test (ShelleyUtxoPredFailure era)
validateOutputTooSmallUTxO pp outputs =
  failureUnless (null outputsTooSmall) $ OutputTooSmallUTxO outputsTooSmall
  where
    outputsTooSmall =
      filter
        (\txOut -> txOut ^. coinTxOutL < getMinCoinTxOut pp txOut)
        (toList outputs)
```

**Formal Specification**:
- **Shelley**: `∀(_ → (_, c)) ∈ txouts txb, c ≥ minUTxOValue pp`
- **Alonzo**: `∀ txout ∈ txouts txb, getValue txout ≥ inject (utxoEntrySize txout * coinsPerUTxOWord pp)`
- **Babbage**: `∀ txout ∈ allOuts txb, getValue txout ≥ inject (serSize txout * coinsPerUTxOByte pp)`

**Explanation**:
- Prevents "dust" outputs that bloat the UTxO set
- **Shelley**: Fixed minimum (1 ADA initially)
- **Alonzo**: Size-based minimum using `coinsPerUTxOWord`
- **Babbage**: More accurate size-based calculation using `coinsPerUTxOByte`
- In **Babbage+**, checks **all outputs** including collateral return

**Error**: `OutputTooSmallUTxO` (Shelley/Alonzo) or `BabbageOutputTooSmallUTxO` (Babbage+)

---

#### 9. validateOutputBootAddrAttrsTooBig

**Reference**: `Utxo.hs:551-565`

**Haskell Code**:
```haskell
validateOutputBootAddrAttrsTooBig ::
  (EraTxOut era, Foldable f) =>
  f (TxOut era) ->
  Test (ShelleyUtxoPredFailure era)
validateOutputBootAddrAttrsTooBig outputs =
  failureUnless (null outputsAttrsTooBig) $ OutputBootAddrAttrsTooBig outputsAttrsTooBig
  where
    outputsAttrsTooBig =
      filter
        ( \txOut ->
            case txOut ^. bootAddrTxOutF of
              Just addr -> bootstrapAddressAttrsSize addr > 64
              _ -> False
        )
        (toList outputs)
```

**Formal Specification**: `∀ ( _ ↦ (a,_)) ∈ txouts txb, a ∈ Addrbootstrap → bootstrapAttrsSize a ≤ 64`

**Explanation**:
- Bootstrap addresses are Byron-era (Cardano's original era) addresses
- They have variable-sized attributes that can contain arbitrary data
- 64 byte limit prevents abuse and maintains reasonable transaction sizes

**Error**: `OutputBootAddrAttrsTooBig`

---

#### 10. validateMaxTxSizeUTxO

**Reference**: `Utxo.hs:570-584`

**Haskell Code**:
```haskell
validateMaxTxSizeUTxO ::
  EraTx era =>
  PParams era ->
  Tx l era ->
  Test (ShelleyUtxoPredFailure era)
validateMaxTxSizeUTxO pp tx =
  failureUnless (txSize <= maxTxSize) $
    MaxTxSizeUTxO Mismatch {mismatchSupplied = txSize, mismatchExpected = maxTxSize}
  where
    maxTxSize = pp ^. ppMaxTxSizeL
    txSize = tx ^. sizeTxF
```

**Formal Specification**: `txsize tx ≤ maxTxSize pp`

**Explanation**:
- Limits serialized transaction size (16 KB on mainnet)
- Ensures blocks don't become too large
- Maintains fast network propagation
- Bounds node memory usage

**Error**: `MaxTxSizeUTxO`

---

### ALLEGRA ERA (Validity Intervals & Value Size Limits)

The Allegra era introduced **validity intervals** and **output value size limits**. Most validations are reused from Shelley.

#### Era-Specific Differences from Shelley

**New Error Types** (`Allegra/Utxo.hs:71-93`):
```haskell
data AllegraUtxoPredFailure era
  = BadInputsUTxO (Set TxIn)  -- Reused from Shelley
  | OutsideValidityIntervalUTxO ValidityInterval SlotNo  -- NEW: Replaces ExpiredUTxO
  | MaxTxSizeUTxO (Mismatch RelLTEQ Word32)
  | InputSetEmptyUTxO
  | FeeTooSmallUTxO (Mismatch RelGTEQ Coin)
  | ValueNotConservedUTxO (Mismatch RelEQ (Value era))
  | WrongNetwork Network (Set Addr)
  | WrongNetworkWithdrawal Network (Set RewardAccount)
  | OutputTooSmallUTxO [TxOut era]
  | UpdateFailure (EraRuleFailure "PPUP" era)
  | OutputBootAddrAttrsTooBig [TxOut era]
  | OutputTooBigUTxO [TxOut era]  -- NEW: Value size limit
```

**Key Architectural Points**:
1. **Error Type Replacement**: Allegra defines its own `AllegraUtxoPredFailure` (not nested in Shelley's)
2. **Conversion Function**: Provides `shelleyToAllegraUtxoPredFailure` (lines 386-399) to convert Shelley errors
3. **Validation Reuse**: Most validation functions are imported and reused from Shelley module (lines 190-228)
4. **Mary Reuse**: Mary era uses `AllegraUtxoPredFailure` directly without defining new errors

#### 11. validateOutputTooBigUTxO (Allegra+)

**Reference**: `Allegra/Utxo.hs:254-270`

**Haskell Code**:
```haskell
validateOutputTooBigUTxO ::
  EraTxOut era =>
  PParams era ->
  UTxO era ->
  Test (AllegraUtxoPredFailure era)
validateOutputTooBigUTxO pp (UTxO outputs) =
  failureUnless (null outputsTooBig) $ OutputTooBigUTxO outputsTooBig
  where
    version = pvMajor (pp ^. ppProtocolVersionL)
    maxValSize = 4000 :: Int64
    outputsTooBig =
      filter
        ( \out ->
            let v = out ^. valueTxOutL
             in BSL.length (serialize version v) > maxValSize
        )
        (Map.elems outputs)
```

**Formal Specification**: `∀ txout ∈ txouts txb, serSize (getValue txout) ≤ MaxValSize`

**Explanation**:
- **NEW in Allegra**: Limits serialized size of output **Value** to 4000 bytes
- Prevents outputs with too many different native tokens (Mary era introduced multi-asset)
- Checks the CBOR-serialized size of the entire Value structure
- **Hardcoded limit**: 4000 bytes (later became configurable in Alonzo as `maxValSize`)

**Error**: `OutputTooBigUTxO`

---

#### Shelley-to-Allegra Error Conversion

**Reference**: `Allegra/Utxo.hs:386-399`

When Allegra processes transactions that might fail with Shelley-era errors, it converts them:

```haskell
shelleyToAllegraUtxoPredFailure :: Shelley.ShelleyUtxoPredFailure era -> AllegraUtxoPredFailure era
shelleyToAllegraUtxoPredFailure = \case
  Shelley.BadInputsUTxO ins -> BadInputsUTxO ins
  Shelley.ExpiredUTxO Mismatch {mismatchSupplied = ttl, mismatchExpected = current} ->
    OutsideValidityIntervalUTxO (ValidityInterval SNothing (SJust ttl)) current
  Shelley.MaxTxSizeUTxO m -> MaxTxSizeUTxO m
  Shelley.InputSetEmptyUTxO -> InputSetEmptyUTxO
  Shelley.FeeTooSmallUTxO m -> FeeTooSmallUTxO m
  Shelley.ValueNotConservedUTxO m -> ValueNotConservedUTxO m
  Shelley.WrongNetwork n as -> WrongNetwork n as
  Shelley.WrongNetworkWithdrawal n as -> WrongNetworkWithdrawal n as
  Shelley.OutputTooSmallUTxO x -> OutputTooSmallUTxO x
  Shelley.UpdateFailure x -> UpdateFailure x
  Shelley.OutputBootAddrAttrsTooBig outs -> OutputTooBigUTxO outs
```

**Notable Conversions**:
- `ExpiredUTxO` → `OutsideValidityIntervalUTxO` with only upper bound set (`SNothing` for lower, `SJust ttl` for upper)
- `OutputBootAddrAttrsTooBig` → `OutputTooBigUTxO` (interesting choice!)

---

#### Allegra Validation Order

**Reference**: `Allegra/Utxo.hs:180-237` (`utxoTransition`)

1. **Validity interval** (`validateOutsideValidityIntervalUTxO`)
2. Input set not empty (`Shelley.validateInputSetEmptyUTxO`)
3. Fee sufficient (`Shelley.validateFeeTooSmallUTxO`)
4. Inputs exist (`Shelley.validateBadInputsUTxO`)
5. Output addresses correct network (`Shelley.validateWrongNetwork`)
6. Withdrawal addresses correct network (`Shelley.validateWrongNetworkWithdrawal`)
7. Value conserved (`Shelley.validateValueNotConservedUTxO`)
8. PPUP (Protocol parameter updates)
9. Outputs meet minimum value (`validateOutputTooSmallUTxO` - Allegra version)
10. **Output value size limit** (`validateOutputTooBigUTxO` - NEW)
11. Bootstrap addresses not too big (`Shelley.validateOutputBootAddrAttrsTooBig`)
12. Transaction size within limit (`Shelley.validateMaxTxSizeUTxO`)

**Validation Function Reuse Pattern**:
- Allegra imports and directly calls most Shelley validation functions using qualified imports: `Shelley.validateX`
- Only defines new validations for era-specific features
- This pattern is carried forward in Mary, which reuses Allegra entirely

---

### MARY ERA (Multi-Asset Support)

Mary era introduces native tokens (multi-asset support) but **does NOT add any new UTXO validation rules**.

**Architecture** (`Mary/Utxo.hs:1-26`):
```haskell
module Cardano.Ledger.Mary.Rules.Utxo () where

type instance EraRuleFailure "UTXO" MaryEra = AllegraUtxoPredFailure MaryEra

instance InjectRuleFailure "UTXO" AllegraUtxoPredFailure MaryEra
instance InjectRuleFailure "UTXO" ShelleyUtxoPredFailure MaryEra where
  injectFailure = shelleyToAllegraUtxoPredFailure
instance InjectRuleFailure "UTXO" ShelleyPpupPredFailure MaryEra where
  injectFailure = UpdateFailure
```

**Key Points**:
- Mary uses `AllegraUtxoPredFailure` directly (no new error type)
- No new validation functions defined
- Mary's contribution is in the **Value type** (supporting multi-asset), not in UTXO validation logic
- The existing Allegra validations (especially `validateOutputTooBigUTxO`) handle multi-asset validation

---

### ALONZO ERA (Plutus Scripts & Collateral)

Alonzo introduces Plutus scripts, which require collateral to prevent DoS attacks. Phase 1 validates collateral structure (Phase 2 executes scripts).

#### 11. validateOutsideForecast

**Reference**: `Alonzo/Utxo.hs:353-372`

**Haskell Code**:
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
validateOutsideForecast ei slotNo sysSt tx =
  case tx ^. bodyTxL . vldtTxBodyL of
    ValidityInterval _ (SJust ifj)
      | not . null $ tx ^. witsTxL . rdmrsTxWitsL . unRedeemersL ->
          let ei' = unsafeLinearExtendEpochInfo slotNo ei
           in failureUnless (isRight (epochInfoSlotToUTCTime ei' sysSt ifj)) $ OutsideForecast ifj
    _ -> pure ()
```

**Formal Specification**: `◇ ∉ { txrdmrs tx, i_f } ⇒ epochInfoSlotToUTCTime epochInfo systemTime i_f ≠ ◇`

**Explanation**:
- If transaction has redeemers (Plutus scripts), the validity interval end must be within consensus forecast
- This ensures the script execution deadline can be converted to real time
- Required for Plutus script timeout calculations

**Error**: `OutsideForecast`

---

#### 12. validateExUnitsTooBigUTxO

**Reference**: `Alonzo/Utxo.hs:438-452`

**Haskell Code**:
```haskell
validateExUnitsTooBigUTxO ::
  ( AlonzoEraTxWits era
  , EraTx era
  , AlonzoEraPParams era
  ) =>
  PParams era ->
  Tx l era ->
  Test (AlonzoUtxoPredFailure era)
validateExUnitsTooBigUTxO pp tx =
  failureUnless (pointWiseExUnits (<=) totalExUnits maxTxExUnits) $
    ExUnitsTooBigUTxO Mismatch {mismatchSupplied = totalExUnits, mismatchExpected = maxTxExUnits}
  where
    maxTxExUnits = pp ^. ppMaxTxExUnitsL
    totalExUnits = totExUnits tx
```

**Formal Specification**: `totExunits tx ≤ maxTxExUnits pp`

**Explanation**:
- **ExUnits** = Execution units (CPU steps + memory)
- Limits total script execution resources per transaction
- Prevents transactions from consuming too many resources
- **This is a Phase 1 check** - actual execution happens in Phase 2
- Mainnet limit: 10,000,000,000 CPU steps, 14,000,000 memory units

**Error**: `ExUnitsTooBigUTxO`

---

#### 13. validateOutputTooBigUTxO

**Reference**: `Alonzo/Utxo.hs:399-418`

**Haskell Code**:
```haskell
validateOutputTooBigUTxO ::
  ( EraTxOut era
  , AlonzoEraPParams era
  , Foldable f
  ) =>
  PParams era ->
  f (TxOut era) ->
  Test (AlonzoUtxoPredFailure era)
validateOutputTooBigUTxO pp outputs =
  failureUnless (null outputsTooBig) $ OutputTooBigUTxO outputsTooBig
  where
    maxValSize = pp ^. ppMaxValSizeL
    protVer = pp ^. ppProtocolVersionL
    outputsTooBig = F.foldl' accum [] outputs
    accum ans txOut =
      let v = txOut ^. valueTxOutL
          serSize = fromIntegral $ BSL.length $ serialize (pvMajor protVer) v
       in if serSize > maxValSize
            then (fromIntegral serSize, fromIntegral maxValSize, txOut) : ans
            else ans
```

**Formal Specification**: `∀ txout ∈ txouts txb, serSize (getValue txout) ≤ maxValSize pp`

**Explanation**:
- Limits the serialized size of **Value** (ADA + native tokens)
- Prevents outputs with too many different tokens
- Mainnet limit: 5000 bytes per output value

**Error**: `OutputTooBigUTxO`

---

#### 14. validateWrongNetworkInTxBody

**Reference**: `Alonzo/Utxo.hs:423-433`

**Haskell Code**:
```haskell
validateWrongNetworkInTxBody ::
  AlonzoEraTxBody era =>
  Network ->
  TxBody l era ->
  Test (AlonzoUtxoPredFailure era)
validateWrongNetworkInTxBody netId txBody =
  case txBody ^. networkIdTxBodyL of
    SNothing -> pure ()
    SJust bid ->
      failureUnless (netId == bid) $
        WrongNetworkInTxBody Mismatch {mismatchSupplied = bid, mismatchExpected = netId}
```

**Formal Specification**: `(txnetworkid txb = NetworkId) ∨ (txnetworkid txb = ◇)`

**Explanation**:
- Alonzo adds optional network ID field in transaction body
- If present, must match the actual network
- Provides additional protection against wrong-network transactions

**Error**: `WrongNetworkInTxBody`

---

#### 15. validateTooManyCollateralInputs

**Reference**: `Alonzo/Utxo.hs:457-468`

**Haskell Code**:
```haskell
validateTooManyCollateralInputs ::
  AlonzoEraTxBody era =>
  PParams era ->
  TxBody TopTx era ->
  Test (AlonzoUtxoPredFailure era)
validateTooManyCollateralInputs pp txBody =
  failureUnless (numColl <= maxColl) $
    TooManyCollateralInputs Mismatch {mismatchSupplied = numColl, mismatchExpected = maxColl}
  where
    maxColl, numColl :: Natural
    maxColl = pp ^. ppMaxCollateralInputsL
    numColl = fromIntegral . Set.size $ txBody ^. collateralInputsTxBodyL
```

**Formal Specification**: `‖collateral tx‖ ≤ maxCollInputs pp`

**Explanation**:
- Limits number of collateral inputs
- Prevents transactions with excessive collateral inputs
- Mainnet limit: 3 collateral inputs

**Error**: `TooManyCollateralInputs`

---

#### 16. validateScriptsNotPaidUTxO

**Reference**: `Alonzo/Utxo.hs:313-319`

**Haskell Code**:
```haskell
validateScriptsNotPaidUTxO ::
  EraTxOut era =>
  Map.Map TxIn (TxOut era) ->
  Test (AlonzoUtxoPredFailure era)
validateScriptsNotPaidUTxO utxoCollateral =
  failureUnless (all vKeyLocked utxoCollateral) $
    ScriptsNotPaidUTxO (UTxO (Map.filter (not . vKeyLocked) utxoCollateral))
```

**Formal Specification**: `∀(a,_,_) ∈ range (collateral txb ◁ utxo), a ∈ Addrvkey`

**Explanation**:
- **Collateral inputs must be locked by verification keys (VKeys), not scripts**
- Prevents recursive script execution for collateral
- If scripts could guard collateral, validation could fail recursively
- `vKeyLocked` checks if address is key hash (not script hash)

**Error**: `ScriptsNotPaidUTxO`

---

#### 17. validateInsufficientCollateral

**Reference**: `Alonzo/Utxo.hs:322-337`

**Haskell Code**:
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

**Formal Specification**: `balance * 100 ≥ txfee * collateralPercent pp`

**Explanation**:
- Collateral must be at least a percentage of the transaction fee
- Mainnet: 150% of fee (`collateralPercent = 150`)
- If scripts fail in Phase 2, this collateral is collected
- Protects against DoS attacks with failing scripts

**Error**: `InsufficientCollateral`

---

#### 18. validateCollateralContainsNonADA

**Reference**: `Alonzo/Utxo.hs:340-347`

**Haskell Code**:
```haskell
validateCollateralContainsNonADA ::
  (Foldable f, EraTxOut era) =>
  f (TxOut era) ->
  Test (AlonzoUtxoPredFailure era)
validateCollateralContainsNonADA collateralTxOuts =
  failureUnless (areAllAdaOnly collateralTxOuts) $
    CollateralContainsNonADA $ sumAllValue collateralTxOuts
```

**Formal Specification**: `isAdaOnly (balance collateral)`

**Explanation**:
- **Alonzo**: Collateral inputs must contain only ADA (no native tokens)
- Simplifies collateral collection if scripts fail
- **Babbage**: Relaxed with collateral return output (see below)

**Error**: `CollateralContainsNonADA`

---

#### 19. NoCollateralInputs

**Reference**: `Alonzo/Utxo.hs:307` (part of `validateCollateral`)

**Haskell Code**:
```haskell
failureIf (null utxoCollateral) NoCollateralInputs
```

**Formal Specification**: `collInputs tx ≠ ∅` (when redeemers present)

**Explanation**:
- If transaction has redeemers (Plutus scripts), it must have collateral
- Ensures failed scripts can still collect collateral
- Collateral not required for simple transactions without scripts

**Error**: `NoCollateralInputs`

---

### BABBAGE ERA (Reference Inputs & Collateral Return)

#### 20. disjointRefInputs

**Reference**: `Babbage/Utxo.hs:225-239`

**Haskell Code**:
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

**Formal Specification**: `inputs ∩ refInputs = ∅`

**Explanation**:
- **Reference inputs** (Babbage feature) allow reading UTxOs without spending them
- Regular inputs and reference inputs must be disjoint
- Prevents confusion between spending and reading

**Error**: `BabbageNonDisjointRefInputs`

---

#### 21. validateCollateralEqBalance

**Reference**: `Babbage/Utxo.hs:320-325`

**Haskell Code**:
```haskell
validateCollateralEqBalance ::
  DeltaCoin -> StrictMaybe Coin -> Validation (NonEmpty (BabbageUtxoPredFailure era)) ()
validateCollateralEqBalance bal txcoll =
  case txcoll of
    SNothing -> pure ()
    SJust tc -> failureUnless (bal == toDeltaCoin tc) (IncorrectTotalCollateralField bal tc)
```

**Formal Specification**: `(txcoll tx ≠ ◇) ⇒ balance = txcoll tx`

**Explanation**:
- Babbage adds optional `totalCollateral` field in transaction body
- If present, must exactly match the computed collateral balance
- Allows wallets to specify exact collateral amount
- Protects users from losing more collateral than necessary

**Error**: `IncorrectTotalCollateralField`

---

#### 22. validateCollateralContainsNonADA (Babbage Version)

**Reference**: `Babbage/Utxo.hs:278-317`

**Haskell Code** (simplified):
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
    -- Collateral inputs can contain non-ADA if collateral return output consumes it
    allNonAdaIsConsumedByReturn = Val.isAdaOnly totalCollateralBalance
    totalCollateralBalance = case txBody ^. collateralReturnTxBodyL of
      SNothing -> collateralBalance
      SJust retTxOut -> collateralBalance <-> (retTxOut ^. valueTxOutL @era)
```

**Explanation**:
- **Babbage improvement**: Adds optional **collateral return output**
- Collateral inputs CAN contain native tokens if return output handles them
- Only the **net collateral** (after return) must be ADA-only
- Simplifies UTxO selection for collateral

**Error**: `CollateralContainsNonADA`

---

## Validation Order Summary

The validations execute in this order (varies slightly by era):

### Shelley/Allegra/Mary Order:
1. Time to live / Validity interval
2. Input set not empty
3. Fee sufficient
4. Inputs exist in UTxO
5. Output addresses correct network
6. Withdrawal addresses correct network
7. Value conserved
8. PPUP (Protocol parameter updates)
9. Outputs meet minimum value
10. Bootstrap addresses not too big
11. Transaction size within limit

### Alonzo/Babbage/Conway Order:
1. **Reference inputs disjoint** (Babbage+)
2. Validity interval
3. **Outside forecast** (Alonzo+)
4. Input set not empty
5. **feesOK** (includes all collateral checks when scripts present)
6. Inputs/collateral/reference exist in UTxO
7. Value conserved
8. Outputs meet minimum value
9. **Output value size limit** (Alonzo+)
10. Bootstrap addresses not too big
11. Output addresses correct network
12. Withdrawal addresses correct network
13. **Network ID in tx body** (Alonzo+)
14. Transaction size within limit
15. **Execution units limit** (Alonzo+)
16. **Collateral input count limit** (Alonzo+)
17. Call UTXOS (Phase 2 - script execution)

---

## Complete Error Types

### Shelley Era Errors
```haskell
data ShelleyUtxoPredFailure era
  = BadInputsUTxO (Set TxIn)
  | ExpiredUTxO (Mismatch RelLTEQ SlotNo)
  | MaxTxSizeUTxO (Mismatch RelLTEQ Word32)
  | InputSetEmptyUTxO
  | FeeTooSmallUTxO (Mismatch RelGTEQ Coin)
  | ValueNotConservedUTxO (Mismatch RelEQ (Value era))
  | WrongNetwork Network (Set Addr)
  | WrongNetworkWithdrawal Network (Set RewardAccount)
  | OutputTooSmallUTxO [TxOut era]
  | UpdateFailure (EraRuleFailure "PPUP" era)
  | OutputBootAddrAttrsTooBig [TxOut era]
```

### Allegra Era Errors (Replaces Shelley Structure)
**Reference**: `Allegra/Utxo.hs:71-93`

```haskell
data AllegraUtxoPredFailure era
  = BadInputsUTxO (Set TxIn)
  | OutsideValidityIntervalUTxO ValidityInterval SlotNo  -- Replaces ExpiredUTxO
  | MaxTxSizeUTxO (Mismatch RelLTEQ Word32)
  | InputSetEmptyUTxO
  | FeeTooSmallUTxO (Mismatch RelGTEQ Coin)
  | ValueNotConservedUTxO (Mismatch RelEQ (Value era))
  | WrongNetwork Network (Set Addr)
  | WrongNetworkWithdrawal Network (Set RewardAccount)
  | OutputTooSmallUTxO [TxOut era]
  | UpdateFailure (EraRuleFailure "PPUP" era)
  | OutputBootAddrAttrsTooBig [TxOut era]
  | OutputTooBigUTxO [TxOut era]  -- NEW: Value size validation
```

**Key Changes from Shelley**:
- `ExpiredUTxO` → `OutsideValidityIntervalUTxO` (supports both bounds)
- Added `OutputTooBigUTxO` for value size limits

### Mary Era Errors
**Reference**: `Mary/Utxo.hs:17`

```haskell
type instance EraRuleFailure "UTXO" MaryEra = AllegraUtxoPredFailure MaryEra
```

**Mary uses `AllegraUtxoPredFailure` directly** - no new error constructors.

### Alonzo Era Errors (Adds to Allegra)
```haskell
data AlonzoUtxoPredFailure era
  = -- ... all Shelley errors ...
  | OutsideValidityIntervalUTxO ValidityInterval SlotNo
  | OutputTooBigUTxO [(Int, Int, TxOut era)]
  | InsufficientCollateral DeltaCoin Coin
  | ScriptsNotPaidUTxO (UTxO era)
  | ExUnitsTooBigUTxO (Mismatch RelLTEQ ExUnits)
  | CollateralContainsNonADA (Value era)
  | WrongNetworkInTxBody (Mismatch RelEQ Network)
  | OutsideForecast SlotNo
  | TooManyCollateralInputs (Mismatch RelLTEQ Natural)
  | NoCollateralInputs
  | UtxosFailure (PredicateFailure (EraRule "UTXOS" era))
```

### Babbage Era Errors (Adds to Alonzo)
```haskell
data BabbageUtxoPredFailure era
  = AlonzoInBabbageUtxoPredFailure (AlonzoUtxoPredFailure era)
  | IncorrectTotalCollateralField DeltaCoin Coin
  | BabbageOutputTooSmallUTxO [(TxOut era, Coin)]
  | BabbageNonDisjointRefInputs (NonEmpty TxIn)
```

### Conway Era Errors (Flattened Structure)
```haskell
data ConwayUtxoPredFailure era
  = UtxosFailure (PredicateFailure (EraRule "UTXOS" era))
  | BadInputsUTxO (Set TxIn)
  | OutsideValidityIntervalUTxO ValidityInterval SlotNo
  | MaxTxSizeUTxO (Mismatch RelLTEQ Word32)
  | InputSetEmptyUTxO
  | FeeTooSmallUTxO (Mismatch RelGTEQ Coin)
  | ValueNotConservedUTxO (Mismatch RelEQ (Value era))
  | WrongNetwork Network (Set Addr)
  | WrongNetworkWithdrawal Network (Set RewardAccount)
  | OutputTooSmallUTxO [TxOut era]
  | OutputBootAddrAttrsTooBig [TxOut era]
  | OutputTooBigUTxO [(Int, Int, TxOut era)]
  | InsufficientCollateral DeltaCoin Coin
  | ScriptsNotPaidUTxO (UTxO era)
  | ExUnitsTooBigUTxO (Mismatch RelLTEQ ExUnits)
  | CollateralContainsNonADA (Value era)
  | WrongNetworkInTxBody (Mismatch RelEQ Network)
  | OutsideForecast SlotNo
  | TooManyCollateralInputs (Mismatch RelLTEQ Natural)
  | NoCollateralInputs
  | IncorrectTotalCollateralField DeltaCoin Coin
  | BabbageOutputTooSmallUTxO [(TxOut era, Coin)]
  | BabbageNonDisjointRefInputs (NonEmpty TxIn)
```

**Important**: Conway **flattens** all previous era errors into a single data type. No nesting! This is the recommended approach for new implementations.

---

## Key Concepts

### Collateral (Alonzo+)
- **Purpose**: Prevents DoS attacks with failing Plutus scripts
- **Requirements**:
  - Must be VKey-locked (not script-locked)
  - Must be >= percentage of fee (150% on mainnet)
  - Must be ADA-only (Alonzo) or net-ADA-only (Babbage with return)
- **Collected if**: Scripts fail in Phase 2
- **Returned if**: All scripts succeed

### Reference Inputs (Babbage+)
- Allows reading UTxOs without spending them
- Useful for sharing datum or reference scripts
- Must be disjoint from regular inputs
- Must exist in UTxO set

### Execution Units (ExUnits)
- Measures Plutus script resource consumption
- Two dimensions: CPU steps and memory
- Phase 1 checks budget limits
- Phase 2 measures actual usage

### Phase 1 vs Phase 2
- **Phase 1 (UTXO)**: Structural validation - happens for ALL transactions
- **Phase 2 (UTXOS)**: Script execution - only for transactions with redeemers
- Phase 1 must pass before Phase 2 runs
- If Phase 2 fails, collateral is collected

---

## Summary

Phase 1 validation ensures transactions are structurally valid before expensive script execution. It checks:

1. **Basic structure**: inputs, outputs, fees, sizes
2. **Value conservation**: No value created or destroyed
3. **Network correctness**: All addresses match network
4. **Collateral setup** (Alonzo+): Properly configured for script failure
5. **Resource limits** (Alonzo+): ExUnits and collateral counts
6. **Reference inputs** (Babbage+): Disjoint and valid

After Phase 1 passes, Phase 2 (UTXOS rule) executes Plutus scripts. Phase 1 failures reject the transaction. Phase 2 failures collect collateral but still update UTxO state.
