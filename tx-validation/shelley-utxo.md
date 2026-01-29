# Shelley/Allegra/Mary Era UTXO Validation

## Overview

The UTXO rule validates transaction structural integrity **before** any script execution. This document covers three eras that share a common validation structure:

- **Shelley**: Foundational Phase 1 validation
- **Allegra**: Adds validity intervals (replacing simple TTL) and output size limits
- **Mary**: Adds multi-asset (native token) support with scaled minimum UTxO

These eras share the `AllegraUtxoPredFailure` error type for Allegra/Mary, with Shelley using its own type.

**Haskell References**:
- `eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxo.hs`
- `eras/allegra/impl/src/Cardano/Ledger/Allegra/Rules/Utxo.hs`
- `eras/mary/impl/src/Cardano/Ledger/Mary/Rules/Utxo.hs`

## Environment, State, and Signal

```haskell
-- Environment (read-only context)
data UtxoEnv era = UtxoEnv
  { ueSlot :: SlotNo          -- Current slot number
  , uePParams :: PParams era  -- Protocol parameters
  , ueCertState :: CertState era -- Certificate state
  }

-- State (mutable ledger state)
data UTxOState era = UTxOState
  { utxosUtxo :: UTxO era           -- The actual UTxO set
  , utxosDeposited :: Coin          -- Total deposits held
  , utxosFees :: Coin               -- Accumulated fees
  , utxosGovState :: GovState era   -- Governance state (PPUP in Shelley)
  , utxosInstantStake :: InstantStake
  , utxosDonation :: Coin
  }

-- Signal (input to the rule)
type Signal (EraRule "UTXO" era) = Tx TopTx era
```

## Transition Rule Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         SHELLEY UTXO TRANSITION                              │
│                                                                              │
│  Input: (UtxoEnv, UTxOState, Tx)                                            │
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 1: validateTimeToLive                                          │    │
│  │   Formal: txttl txb ≥ slot                                          │    │
│  │   Error: ExpiredUTxO                                                │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 2: validateInputSetEmptyUTxO                                   │    │
│  │   Formal: txins txb ≠ ∅                                             │    │
│  │   Error: InputSetEmptyUTxO                                          │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 3: validateFeeTooSmallUTxO                                     │    │
│  │   Formal: minfee pp tx ≤ txfee txb                                  │    │
│  │   Error: FeeTooSmallUTxO                                            │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 4: validateBadInputsUTxO                                       │    │
│  │   Formal: txins txb ⊆ dom utxo                                      │    │
│  │   Error: BadInputsUTxO                                              │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 5: validateWrongNetwork                                        │    │
│  │   Formal: ∀(_ → (a, _)) ∈ txouts txb, netId a = NetworkId           │    │
│  │   Error: WrongNetwork                                               │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 6: validateWrongNetworkWithdrawal                              │    │
│  │   Formal: ∀(a → ) ∈ txwdrls txb, netId a = NetworkId                │    │
│  │   Error: WrongNetworkWithdrawal                                     │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 7: validateValueNotConservedUTxO                               │    │
│  │   Formal: consumed pp utxo txb = produced pp poolParams txb         │    │
│  │   Error: ValueNotConservedUTxO                                      │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 8: PPUP Sub-rule (Protocol Parameter Updates)                  │    │
│  │   Error: UpdateFailure                                              │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 9: validateOutputTooSmallUTxO                                  │    │
│  │   Formal: ∀(_ → (_, c)) ∈ txouts txb, c ≥ minUTxOValue pp           │    │
│  │   Error: OutputTooSmallUTxO                                         │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 10: validateOutputBootAddrAttrsTooBig                          │    │
│  │   Formal: ∀(a,_) ∈ txouts, a ∈ Addrbootstrap → attrsSize a ≤ 64     │    │
│  │   Error: OutputBootAddrAttrsTooBig                                  │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 11: validateMaxTxSizeUTxO                                      │    │
│  │   Formal: txsize tx ≤ maxTxSize pp                                  │    │
│  │   Error: MaxTxSizeUTxO                                              │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 12: updateUTxOState                                            │    │
│  │   Apply the transaction to the UTxO set                             │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                                                              │
│  Output: Updated UTxOState                                                   │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Haskell Implementation

```haskell
-- Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxo.hs:343-416
utxoInductive ::
  forall era.
  ( EraUTxO era
  , EraStake era
  , ShelleyEraTxBody era
  , ExactEra ShelleyEra era
  ...
  ) =>
  TransitionRule (EraRule "UTXO" era)
utxoInductive = do
  TRC (UtxoEnv slot pp certState, utxos, tx) <- judgmentContext
  let utxo = utxos ^. utxoL
      UTxOState _ _ _ ppup _ _ = utxos
      txBody = tx ^. bodyTxL
      outputs = txBody ^. outputsTxBodyL
      genDelegs = dsGenDelegs (certState ^. certDStateL)

  {- txttl txb ≥ slot -}
  runTest $ validateTimeToLive txBody slot

  {- txins txb ≠ ∅ -}
  runTest $ validateInputSetEmptyUTxO txBody

  {- minfee pp tx ≤ txfee txb -}
  runTest $ validateFeeTooSmallUTxO pp tx utxo

  {- txins txb ⊆ dom utxo -}
  runTest $ validateBadInputsUTxO utxo $ txBody ^. inputsTxBodyL

  netId <- liftSTS $ asks networkId

  {- ∀(_ → (a, _)) ∈ txouts txb, netId a = NetworkId -}
  runTest $ validateWrongNetwork netId outputs

  {- ∀(a → ) ∈ txwdrls txb, netId a = NetworkId -}
  runTest $ validateWrongNetworkWithdrawal netId txBody

  {- consumed pp utxo txb = produced pp poolParams txb -}
  runTest $ validateValueNotConservedUTxO pp utxo certState txBody

  -- process Protocol Parameter Update Proposals
  ppup' <-
    trans @(EraRule "PPUP" era) $ TRC (PPUPEnv slot pp genDelegs, ppup, txBody ^. updateTxBodyL)

  {- ∀(_ → (_, c)) ∈ txouts txb, c ≥ (minUTxOValue pp) -}
  runTest $ validateOutputTooSmallUTxO pp outputs

  {- ∀ ( _ ↦ (a,_)) ∈ txoutstxb,  a ∈ Addrbootstrap → bootstrapAttrsSize a ≤ 64 -}
  runTest $ validateOutputBootAddrAttrsTooBig outputs

  {- txsize tx ≤ maxTxSize pp -}
  runTest $ validateMaxTxSizeUTxO pp tx

  updateUTxOState pp utxos txBody certState ppup' ...
```

---

## Validation Functions (Detailed)

### 1. validateTimeToLive

**Reference**: `Utxo.hs:421-430`

**Formal Specification**: `txttl txb ≥ slot`

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

**Step-by-Step Explanation**:

1. **Extract TTL**: Get the `timeToLive` field from transaction body
2. **Compare with slot**: Check if `ttl >= current_slot`
3. **Fail if expired**: If `ttl < current_slot`, the transaction has expired

**Why This Matters**:
- Prevents replay attacks (old transactions can't be re-submitted)
- Allows users to set expiration for their transactions
- Shelley uses simple TTL (upper bound only)

**Error**: `ExpiredUTxO { supplied: ttl, expected: slot }`

---

### 2. validateInputSetEmptyUTxO

**Reference**: `Utxo.hs:435-442`

**Formal Specification**: `txins txb ≠ ∅`

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

**Step-by-Step Explanation**:

1. **Get inputs**: Extract the set of transaction inputs
2. **Check non-empty**: Verify the set is not empty
3. **Fail if empty**: Return `InputSetEmptyUTxO` error

**Why This Matters**:
- Every transaction MUST consume at least one UTxO
- Prevents value creation from nothing
- Genesis transactions are handled differently (outside UTXO rule)

**Error**: `InputSetEmptyUTxO`

---

### 3. validateFeeTooSmallUTxO

**Reference**: `Utxo.hs:447-463`

**Formal Specification**: `minfee pp tx ≤ txfee txb`

```haskell
validateFeeTooSmallUTxO ::
  EraUTxO era =>
  PParams era ->
  Tx TopTx era ->
  UTxO era ->
  Test (ShelleyUtxoPredFailure era)
validateFeeTooSmallUTxO pp tx utxo =
  failureUnless (minFee <= txFee) $
    FeeTooSmallUTxO
      Mismatch { mismatchSupplied = txFee, mismatchExpected = minFee }
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

**Step-by-Step Explanation**:

1. **Calculate minimum fee**:
   ```
   minFee = (txSize * minFeeA) + minFeeB
   ```
   - `minFeeA` = 44 lovelace per byte (mainnet)
   - `minFeeB` = 155,381 lovelace base fee (mainnet)

2. **Get declared fee**: Extract `feeTxBodyL` from transaction body

3. **Compare**: Check `minFee <= txFee`

4. **Fail if insufficient**: Return error with expected vs supplied

**Example**:
```
Transaction size: 250 bytes
minFeeA: 44 lovelace/byte
minFeeB: 155,381 lovelace

minFee = (250 × 44) + 155,381 = 11,000 + 155,381 = 166,381 lovelace
txFee (declared): 200,000 lovelace

166,381 <= 200,000 ✓ PASS
```

**Error**: `FeeTooSmallUTxO { supplied: txFee, expected: minFee }`

---

### 4. validateBadInputsUTxO

**Reference**: `Utxo.hs:468-476`

**Formal Specification**: `txins txb ⊆ dom utxo`

```haskell
validateBadInputsUTxO ::
  UTxO era ->
  Set TxIn ->
  Test (ShelleyUtxoPredFailure era)
validateBadInputsUTxO utxo inputs =
  failureUnless (Set.null badInputs) $ BadInputsUTxO badInputs
  where
    {- inputs ➖ dom utxo -}
    badInputs = Set.filter (`Map.notMember` unUTxO utxo) inputs
```

**Step-by-Step Explanation**:

1. **Get all inputs**: Transaction input set
2. **Filter bad inputs**: Find inputs NOT in UTxO set
3. **Check result**: If any bad inputs exist, fail
4. **Return bad set**: Include all non-existent inputs in error

**Why This Matters**:
- Cannot spend non-existent UTxOs
- Prevents double-spending (already spent = removed from UTxO)
- All referenced inputs must exist at validation time

**Error**: `BadInputsUTxO { badInputs: Set<TxIn> }`

---

### 5. validateWrongNetwork

**Reference**: `Utxo.hs:481-492`

**Formal Specification**: `∀(_ → (a, _)) ∈ txouts txb, netId a = NetworkId`

```haskell
validateWrongNetwork ::
  (EraTxOut era, Foldable f) =>
  Network ->
  f (TxOut era) ->
  Test (ShelleyUtxoPredFailure era)
validateWrongNetwork netId outputs =
  failureUnless (null addrsWrongNetwork) $
    WrongNetwork netId (Set.fromList addrsWrongNetwork)
  where
    addrsWrongNetwork =
      filter
        (\a -> getNetwork a /= netId)
        (view addrTxOutL <$> toList outputs)
```

**Step-by-Step Explanation**:

1. **Get expected network**: From environment (Mainnet=1, Testnet=0)
2. **Check each output address**: Extract network ID from address
3. **Filter mismatches**: Collect addresses with wrong network
4. **Fail if any wrong**: Return set of wrong addresses

**Why This Matters**:
- Prevents sending to wrong network addresses
- Mainnet addresses can't be used on testnet and vice versa
- Network ID is encoded in the address itself

**Error**: `WrongNetwork { expected: Network, wrongAddresses: Set<Addr> }`

---

### 6. validateWrongNetworkWithdrawal

**Reference**: `Utxo.hs:497-509`

**Formal Specification**: `∀(a → ) ∈ txwdrls txb, netId a = NetworkId`

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

**Step-by-Step Explanation**:

1. **Get withdrawals**: Map of RewardAccount → Coin
2. **Check each reward account**: Extract network from account
3. **Filter mismatches**: Collect accounts with wrong network
4. **Fail if any wrong**: Return set of wrong accounts

**Error**: `WrongNetworkWithdrawal { expected: Network, wrongAccounts: Set<RewardAccount> }`

---

### 7. validateValueNotConservedUTxO

**Reference**: `Utxo.hs:514-526`

**Formal Specification**: `consumed pp utxo txb = produced pp poolParams txb`

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
    ValueNotConservedUTxO Mismatch {
      mismatchSupplied = consumedValue,
      mismatchExpected = producedValue
    }
  where
    consumedValue = consumed pp certState utxo txBody
    producedValue = produced pp certState txBody
```

**Consumed Calculation** (`Shelley/UTxO.hs`):
```haskell
consumed pp certState utxo txBody =
    balance (txInsFilter utxo (txBody ^. inputsTxBodyL))  -- Sum of input values
  <> (txBody ^. withdrawalsTxBodyL)                        -- Withdrawals
  <> Val.inject (certsTotalRefundsTxBody pp certState txBody)  -- Refunds
```

**Produced Calculation** (`Shelley/UTxO.hs`):
```haskell
produced pp certState txBody =
    F.fold (txBody ^. outputsTxBodyL)   -- Sum of output values
  <> Val.inject (txBody ^. feeTxBodyL)  -- Transaction fee
  <> Val.inject (certsTotalDepositsTxBody pp certState txBody)  -- Deposits
```

**Step-by-Step Explanation**:

1. **Calculate CONSUMED**:
   ```
   consumed = sum(inputs) + withdrawals + refunds
   ```
   - **Inputs**: Sum of all UTxO values being spent
   - **Withdrawals**: Staking rewards being claimed
   - **Refunds**: Deposits returned (deregistration)

2. **Calculate PRODUCED**:
   ```
   produced = sum(outputs) + fee + deposits
   ```
   - **Outputs**: Sum of all new UTxO values
   - **Fee**: Transaction fee
   - **Deposits**: New deposits (registration)

3. **Compare**: `consumed == produced`

4. **Fail if not equal**: This is the FUNDAMENTAL conservation law

**Example of Valid Transaction**:
```
CONSUMED:
  Input #1 (100 ADA) + Input #2 (50 ADA) = 150 ADA
  Withdrawal: 5 ADA (staking rewards)
  Refund: 2 ADA (deregister stake key)
  TOTAL: 157 ADA

PRODUCED:
  Output #1: 145 ADA (to recipient)
  Output #2: 5 ADA (change)
  Fee: 5 ADA
  Deposit: 2 ADA (register new stake key)
  TOTAL: 157 ADA

157 ADA == 157 ADA ✓ VALID
```

**Error**: `ValueNotConservedUTxO { consumed, produced }`

---

### 8. validateOutputTooSmallUTxO

**Reference**: `Utxo.hs:531-545`

**Formal Specification**: `∀(_ → (_, c)) ∈ txouts txb, c ≥ minUTxOValue pp`

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

**Step-by-Step Explanation**:

1. **Get minimum value**: From protocol parameters (`minUTxOValue`)
2. **Check each output**: Compare output coin with minimum
3. **Filter small outputs**: Collect outputs below minimum
4. **Fail if any too small**: Return list of offending outputs

**Why This Matters**:
- Prevents "dust" outputs that bloat the UTxO set
- Shelley: Fixed minimum (1 ADA initially)
- Users must include enough ADA in each output

**Error**: `OutputTooSmallUTxO [offending_outputs]`

---

### 9. validateOutputBootAddrAttrsTooBig

**Reference**: `Utxo.hs:551-565`

**Formal Specification**: `∀(a,_) ∈ txouts, a ∈ Addrbootstrap → bootstrapAttrsSize a ≤ 64`

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

**Step-by-Step Explanation**:

1. **Check for bootstrap addresses**: Only applies to Byron-era addresses
2. **Get attribute size**: Bootstrap addresses have variable-size attributes
3. **Check limit**: Attributes must be ≤ 64 bytes
4. **Filter violations**: Collect outputs with oversized attributes

**Why This Matters**:
- Bootstrap (Byron) addresses have an attributes section
- Large attributes waste space and could be abused
- Limit set at 64 bytes for safety

**Error**: `OutputBootAddrAttrsTooBig [offending_outputs]`

---

### 10. validateMaxTxSizeUTxO

**Reference**: `Utxo.hs:570-581`

**Formal Specification**: `txsize tx ≤ maxTxSize pp`

```haskell
validateMaxTxSizeUTxO ::
  EraTx era =>
  PParams era ->
  Tx t era ->
  Test (ShelleyUtxoPredFailure era)
validateMaxTxSizeUTxO pp tx =
  failureUnless (txSize <= maxTxSize) $
    MaxTxSizeUTxO Mismatch { mismatchSupplied = txSize, mismatchExpected = maxTxSize }
  where
    maxTxSize = pp ^. ppMaxTxSizeL
    txSize = tx ^. sizeTxF
```

**Step-by-Step Explanation**:

1. **Get max size**: From protocol parameters (`maxTxSize`)
2. **Get actual size**: Serialized transaction size
3. **Compare**: `txSize <= maxTxSize`
4. **Fail if too large**: Return both values in error

**Mainnet Value**: `maxTxSize = 16,384 bytes` (16 KB)

**Error**: `MaxTxSizeUTxO { supplied: txSize, expected: maxTxSize }`

---

## Error Type Summary

```haskell
-- Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxo.hs:82-93
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

---

---

## Allegra Era Extensions

**Reference**: `eras/allegra/impl/src/Cardano/Ledger/Allegra/Rules/Utxo.hs`

Allegra introduces two key changes to the UTXO validation:

### 1. Validity Intervals (Replaces TTL)

**Reference**: `eras/allegra/impl/src/Cardano/Ledger/Allegra/Scripts.hs:119-123`

```haskell
data ValidityInterval = ValidityInterval
  { invalidBefore :: !(StrictMaybe SlotNo)    -- First valid slot (inclusive)
  , invalidHereafter :: !(StrictMaybe SlotNo) -- First invalid slot (exclusive)
  }
```

The validity interval is a **half-open interval** `[invalidBefore, invalidHereafter)`:
- `invalidBefore <= current_slot` (inclusive lower bound)
- `current_slot < invalidHereafter` (exclusive upper bound)

**Validation Function**:

```haskell
-- Reference: Utxo.hs:242-249
validateOutsideValidityIntervalUTxO ::
  AllegraEraTxBody era =>
  SlotNo ->
  TxBody l era ->
  Test (AllegraUtxoPredFailure era)
validateOutsideValidityIntervalUTxO slot txb =
  failureUnless (inInterval slot (txb ^. vldtTxBodyL)) $
    OutsideValidityIntervalUTxO (txb ^. vldtTxBodyL) slot
```

**Interval Check Logic**:

```haskell
-- Reference: Scripts.hs:440-445
inInterval :: SlotNo -> ValidityInterval -> Bool
inInterval _slot (ValidityInterval SNothing SNothing) = True
inInterval slot (ValidityInterval SNothing (SJust top)) = slot < top
inInterval slot (ValidityInterval (SJust bottom) SNothing) = bottom <= slot
inInterval slot (ValidityInterval (SJust bottom) (SJust top)) =
  bottom <= slot && slot < top
```

**Why This Matters**:
- Enables time-locked transactions (e.g., vesting contracts)
- Scripts can check validity interval for time-based logic
- More flexible than simple TTL expiration

**Error**: `OutsideValidityIntervalUTxO { validityInterval, currentSlot }`

---

### 2. Output Value Size Limit (OutputTooBigUTxO)

**Reference**: `eras/allegra/impl/src/Cardano/Ledger/Allegra/Rules/Utxo.hs:254-270`

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
    maxValSize = 4000 :: Int64  -- Maximum serialized value size
    outputsTooBig =
      filter
        ( \out ->
            let v = out ^. valueTxOutL
             in BSL.length (serialize version v) > maxValSize
        )
        (Map.elems outputs)
```

**Formal Specification**: `∀ txout ∈ txouts txb, serSize (getValue txout) ≤ MaxValSize`

**Why This Matters**:
- Prevents UTxO bloat from extremely large multi-asset bundles
- Ensures outputs can be efficiently stored and transmitted
- The 4000 byte limit balances flexibility and efficiency

**Error**: `OutputTooBigUTxO [offending_outputs]`

---

### Allegra Error Type

```haskell
-- Reference: Utxo.hs:71-93
data AllegraUtxoPredFailure era
  = BadInputsUTxO (Set TxIn)
  | OutsideValidityIntervalUTxO ValidityInterval SlotNo  -- NEW
  | MaxTxSizeUTxO (Mismatch RelLTEQ Word32)
  | InputSetEmptyUTxO
  | FeeTooSmallUTxO (Mismatch RelGTEQ Coin)
  | ValueNotConservedUTxO (Mismatch RelEQ (Value era))
  | WrongNetwork Network (Set Addr)
  | WrongNetworkWithdrawal Network (Set RewardAccount)
  | OutputTooSmallUTxO [TxOut era]
  | UpdateFailure (EraRuleFailure "PPUP" era)
  | OutputBootAddrAttrsTooBig [TxOut era]
  | OutputTooBigUTxO [TxOut era]  -- NEW
```

---

### Allegra Transition Rule Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         ALLEGRA UTXO TRANSITION                              │
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 1: validateOutsideValidityIntervalUTxO (REPLACES TTL)          │    │
│  │   Formal: inInterval slot (txvldt txb)                              │    │
│  │   Error: OutsideValidityIntervalUTxO                                │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  Steps 2-8: Same as Shelley                                                  │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 9: validateOutputTooSmallUTxO                                  │    │
│  │   Uses fixed minUTxOValue (same as Shelley)                         │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 10: validateOutputTooBigUTxO (NEW)                             │    │
│  │   Formal: ∀ txout, serSize (getValue txout) ≤ 4000                  │    │
│  │   Error: OutputTooBigUTxO                                           │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  Steps 11-12: Same as Shelley                                                │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Mary Era Extensions

**Reference**: `eras/mary/impl/src/Cardano/Ledger/Mary/Rules/Utxo.hs`

Mary introduces **multi-asset (native token) support**. The UTXO rule reuses Allegra's structure with key differences in value handling:

```haskell
type instance EraRuleFailure "UTXO" MaryEra = AllegraUtxoPredFailure MaryEra
```

### 1. Multi-Asset Values

**Reference**: `eras/mary/impl/src/Cardano/Ledger/Mary/Value.hs`

```haskell
data MaryValue = MaryValue
  { mvCoin :: !Coin
  , mvMultiAsset :: !MultiAsset
  }

newtype MultiAsset = MultiAsset (Map PolicyId (Map AssetName Integer))
```

Outputs in Mary can contain both ADA and native tokens organized by PolicyId.

---

### 2. Scaled Minimum UTxO for Multi-Asset

**Reference**: `eras/mary/impl/src/Cardano/Ledger/Mary/TxOut.hs:35-77`

```haskell
-- The minimum ADA in a multi-asset output is scaled based on the value's size
scaledMinDeposit :: Val v => v -> Coin -> Coin
scaledMinDeposit v (Coin mv)
  | isAdaOnly v = Coin mv  -- Pure ADA: use minUTxOValue directly
  | otherwise = Coin $ max mv (coinsPerUTxOWord * (utxoEntrySizeWithoutVal + size v))
  where
    txoutLenNoVal = 14
    txinLen = 7
    coinSize = 0
    utxoEntrySizeWithoutVal = 6 + txoutLenNoVal + txinLen  -- = 27
    coinsPerUTxOWord = quot mv (utxoEntrySizeWithoutVal + coinSize)
```

**How It Works**:

1. For **ADA-only outputs**: minimum = `minUTxOValue` (protocol parameter)

2. For **multi-asset outputs**: minimum is scaled based on size:
   ```
   coinsPerWord = minUTxOValue / (utxoEntrySizeWithoutVal + coinSize)
   scaledMin = max(minUTxOValue, coinsPerWord * (utxoEntrySizeWithoutVal + valueSize))
   ```

**Example Calculation**:
```
minUTxOValue = 1,000,000 lovelace (1 ADA)
utxoEntrySizeWithoutVal = 27 words

For ADA-only:
  minDeposit = 1,000,000 lovelace

For multi-asset with size 40 words:
  coinsPerWord = 1,000,000 / 27 ≈ 37,037
  minDeposit = max(1,000,000, 37,037 * (27 + 40))
            = max(1,000,000, 37,037 * 67)
            = max(1,000,000, 2,481,479)
            = 2,481,479 lovelace (~2.48 ADA)
```

**Why This Matters**:
- Larger multi-asset bundles require proportionally more ADA
- Prevents UTxO bloat from outputs with minimal ADA but many tokens
- Ensures economic cost for storing large multi-asset values

---

### 3. Minting/Burning in Value Conservation

**Reference**: `eras/mary/impl/src/Cardano/Ledger/Mary/UTxO.hs:69-102`

In Mary, value conservation includes minted and burned tokens:

```haskell
-- Consumed value includes minted tokens
getConsumedMaryValue pp ... utxo txBody =
  consumedValue <> MaryValue mempty mintedMultiAsset
  where
    mintedMultiAsset = filterMultiAsset (\_ _ -> (> 0)) $ txBody ^. mintTxBodyL
    consumedValue = sumUTxO (txInsFilter utxo inputs) <> inject (refunds <> withdrawals)

-- Produced value includes burned tokens
getProducedMaryValue pp isPoolRegistered txBody =
  shelleyProducedValue pp isPoolRegistered txBody <> burnedMultiAssets txBody

burnedMultiAssets txBody =
  MaryValue mempty $
    mapMaybeMultiAsset (\_ _ v -> if v < 0 then Just (negate v) else Nothing) $
      txBody ^. mintTxBodyL
```

**Conservation Law with Minting**:

```
consumed = produced

where:
  consumed = sum(inputs) + withdrawals + refunds + minted_tokens
  produced = sum(outputs) + fee + deposits + burned_tokens
```

- **Minting** (positive quantities): Added to consumed side
- **Burning** (negative quantities): Added to produced side (as positive values)

**Important**: ADA cannot be minted or burned. The Haskell type system enforces this:

```haskell
{- adaPolicy ∉ supp mint tx
   above check not needed because mint field of type MultiAsset cannot contain ada -}
```

---

### 4. Policy Script Requirements

**Reference**: `eras/mary/impl/src/Cardano/Ledger/Mary/UTxO.hs:107-116`

Each PolicyId in the mint field requires a corresponding script witness:

```haskell
getMaryScriptsNeeded u txBody =
  case getShelleyScriptsNeeded u txBody of
    ShelleyScriptsNeeded shelleyScriptsNeeded ->
      ShelleyScriptsNeeded $
        shelleyScriptsNeeded `Set.union` Set.map policyID (txBody ^. mintedTxBodyF)
```

This means:
- For every policy in the mint field, there must be a native script with that hash
- The script must validate for the transaction to succeed

---

### Mary Transition Rule Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           MARY UTXO TRANSITION                               │
│                                                                              │
│  Steps 1-6: Same as Allegra                                                  │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 7: validateValueNotConservedUTxO (MODIFIED)                    │    │
│  │   Formal: consumed + minted = produced + burned                     │    │
│  │   Includes multi-asset tokens in conservation                       │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  Step 8: PPUP                                                                │
│                               ↓                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Step 9: validateOutputTooSmallUTxO (MODIFIED)                       │    │
│  │   Uses scaledMinDeposit for multi-asset outputs                     │    │
│  │   Formal: ∀ txout, coin(value) ≥ scaledMinDeposit(value, minUTxO)   │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  Steps 10-12: Same as Allegra                                                │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Era Comparison Summary

| Validation | Shelley | Allegra | Mary |
|------------|---------|---------|------|
| Time bounds | TTL only | ValidityInterval | ValidityInterval |
| Time error | ExpiredUTxO | OutsideValidityIntervalUTxO | OutsideValidityIntervalUTxO |
| Value type | Coin (ADA) | Coin (ADA) | MaryValue (ADA + tokens) |
| Min UTxO | Fixed minUTxOValue | Fixed minUTxOValue | scaledMinDeposit |
| Value size check | ❌ | ✅ (4000 bytes) | ✅ (4000 bytes) |
| Minting | ❌ | ❌ | ✅ |
| Conservation | ADA only | ADA only | ADA + multi-asset |

---

## UpdateFailure: Protocol Parameter Update Proposal (PPUP)

The `UpdateFailure` error is returned when the PPUP (Protocol Parameter Update Proposal) sub-rule fails during UTXO validation. This mechanism allows genesis key holders to propose changes to protocol parameters.

**Haskell Reference**: `eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Ppup.hs`

### PPUP Environment, State, and Signal

```haskell
-- Reference: Ppup.hs:66
data PpupEnv era = PPUPEnv SlotNo (PParams era) GenDelegs

-- State: ShelleyGovState containing current and future proposals
-- Signal: StrictMaybe (Update era) - optional update proposal
```

### PPUP Predicate Failures

```haskell
-- Reference: Ppup.hs:86-108
data ShelleyPpupPredFailure era
  = NonGenesisUpdatePPUP
      (Mismatch RelSubset (Set (KeyHash GenesisRole)))
      -- ^ Update proposed by non-genesis key
  | PPUpdateWrongEpoch
      EpochNo    -- Current epoch
      EpochNo    -- Target epoch in proposal
      VotingPeriod
      -- ^ Update targets wrong epoch for current voting period
  | PVCannotFollowPPUP
      ProtVer
      -- ^ Proposed protocol version is not a legal successor
```

### When UpdateFailure Occurs

The UTXO rule calls the PPUP sub-rule when processing a transaction with an update proposal:

```haskell
-- Reference: Utxo.hs:168-170
ppup' <-
  trans @(EraRule "PPUP" era) $ TRC (PPUPEnv slot pp genDelegs, ppup, txBody ^. updateTxBodyL)
```

If this sub-rule fails, the error is wrapped as `UpdateFailure`:

```haskell
-- Reference: Utxo.hs:596
UpdateFailure (EraRuleFailure "PPUP" era)
```

---

### 1. NonGenesisUpdatePPUP

**When it occurs**: A proposal is submitted by a key hash that is NOT one of the genesis delegate keys.

**Haskell Code**:

```haskell
-- Reference: Ppup.hs:166-171
Map.isSubmapOfBy (\_ _ -> True) pup genDelegs
  ?! NonGenesisUpdatePPUP
    Mismatch
      { mismatchSupplied = Map.keysSet pup
      , mismatchExpected = Map.keysSet genDelegs
      }
```

**Explanation**:
- `pup` is the map of genesis key hashes to their proposed updates
- `genDelegs` is the set of valid genesis delegates
- Every key in the proposal map MUST be a genesis delegate key
- If any key is not authorized, the proposal fails

**Error Details**:
- `mismatchSupplied`: The set of key hashes that submitted proposals
- `mismatchExpected`: The set of valid genesis delegate key hashes

---

### 2. PPUpdateWrongEpoch

**When it occurs**: The proposal targets the wrong epoch based on when it's submitted.

**Haskell Code**:

```haskell
-- Reference: Ppup.hs:180-198
(curEpochNo, tooLate, nextEpochNo) <- liftSTS $ getTheSlotOfNoReturn slot
tellEvent $ PpupNewEpoch nextEpochNo

if slot < tooLate
  then do
    -- Before "slot of no return": proposal is for CURRENT epoch
    (curEpochNo == targetEpochNo)
      ?! PPUpdateWrongEpoch curEpochNo targetEpochNo VoteForThisEpoch
    ...
  else do
    -- After "slot of no return": proposal is for NEXT epoch
    (succ curEpochNo == targetEpochNo)
      ?! PPUpdateWrongEpoch curEpochNo targetEpochNo VoteForNextEpoch
```

**The "Slot of No Return"**:

Each epoch has a deadline called the "slot of no return". This is calculated as `2 * stability_window` slots before the epoch boundary.

```
Epoch Timeline:
|-------- Current Epoch --------|-------- Next Epoch --------|
^                        ^      ^
start                 tooLate   boundary

Before tooLate: Proposals target CURRENT epoch
After tooLate:  Proposals target NEXT epoch
```

**Voting Period Rules**:

| Current Slot | Proposal Target | Valid? |
|--------------|-----------------|--------|
| slot < tooLate | current epoch | ✓ VoteForThisEpoch |
| slot < tooLate | next epoch | ✗ Wrong epoch |
| slot >= tooLate | next epoch | ✓ VoteForNextEpoch |
| slot >= tooLate | current epoch | ✗ Wrong epoch |

**Error Details**:
- First `EpochNo`: Current epoch number
- Second `EpochNo`: Target epoch in the proposal
- `VotingPeriod`: Either `VoteForThisEpoch` or `VoteForNextEpoch`

---

### 3. PVCannotFollowPPUP

**When it occurs**: The proposed protocol version is not a legal successor to the current version.

**Haskell Code**:

```haskell
-- Reference: Ppup.hs:173-178
let firstIllegalProtVerUpdate = do
      ppu <- F.find (not . hasLegalProtVerUpdate pp) pup
      SJust newBadProtVer <- Just (ppu ^. ppuProtocolVersionL)
      Just newBadProtVer
failOnJust firstIllegalProtVerUpdate PVCannotFollowPPUP
```

**Protocol Version Succession Rules** (`pvCanFollow`):

A new protocol version `(major', minor')` can follow `(major, minor)` if:

1. **Major version bump**: `major' = major + 1` AND `minor' = 0`
   - Example: `(3, 5) → (4, 0)` ✓

2. **Minor version bump**: `major' = major` AND `minor' = minor + 1`
   - Example: `(3, 5) → (3, 6)` ✓

**Invalid Examples**:
- `(3, 5) → (3, 7)` ✗ (minor must increment by exactly 1)
- `(3, 5) → (5, 0)` ✗ (major must increment by exactly 1)
- `(3, 5) → (4, 1)` ✗ (major bump requires minor = 0)
- `(3, 5) → (2, 0)` ✗ (cannot decrease version)

**Formal Specification**:

```
pvCanFollow : ProtVer → ProtVer → Bool
pvCanFollow (major, minor) (major', minor') =
  (major' = major ∧ minor' = minor + 1) ∨
  (major' = major + 1 ∧ minor' = 0)
```

---

### How Quorum Works

For a parameter change to be adopted, it must achieve **quorum**:

```haskell
-- Reference: Ppup.hs:205-242
votedFuturePParams ::
  ProposedPPUpdates era -> PParams era -> Word64 -> Maybe (PParams era)
votedFuturePParams (ProposedPPUpdates pppu) pp quorumN = do
  let votes =
        Map.foldr
          (\vote -> Map.insertWith (+) vote 1)
          (Map.empty :: Map.Map (PParamsUpdate era) Word64)
          pppu
      consensus = Map.filter (>= quorumN) votes
  [ppu] <- Just $ Map.keys consensus  -- Exactly one proposal must reach quorum
  let ppNew = applyPPUpdates pp ppu
  -- Additional size constraint check
  guard $ toInteger (ppNew ^. ppMaxTxSizeL) + toInteger (ppNew ^. ppMaxBHSizeL)
        < toInteger (ppNew ^. ppMaxBBSizeL)
  pure ppNew
```

**Quorum Requirement**:
- `quorumN` must be strictly greater than half the number of genesis nodes
- All votes for the EXACT SAME update are counted together
- If `quorumN` votes for identical parameters exist, the update is adopted

---

### Update Proposal Structure

```haskell
-- Reference: Shelley/PParams.hs
data Update era = Update
  { updatePropsal :: ProposedPPUpdates era
  , updateEpoch   :: EpochNo
  }

newtype ProposedPPUpdates era = ProposedPPUpdates
  (Map (KeyHash GenesisRole) (PParamsUpdate era))
```

Each proposal maps genesis key hashes to the parameters they want to change. Only the parameters specified in `PParamsUpdate` are changed; others remain at current values.

---

### UpdateFailure Summary Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         PPUP VALIDATION FLOW                                 │
│                                                                              │
│  Input: (slot, pp, genDelegs, proposals, update)                            │
│                                                                              │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │ Check 1: Is update present?                                            │  │
│  │   If SNothing: Return current state (no-op)                           │  │
│  │   If SJust (Update proposals targetEpoch): Continue                   │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                               ↓                                              │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │ Check 2: Are all proposers genesis delegates?                          │  │
│  │   pup_keys ⊆ genDelegs_keys                                           │  │
│  │   Error: NonGenesisUpdatePPUP                                          │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                               ↓                                              │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │ Check 3: Is protocol version update legal?                             │  │
│  │   For each ppu, check pvCanFollow(currentVer, proposedVer)            │  │
│  │   Error: PVCannotFollowPPUP                                            │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                               ↓                                              │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │ Check 4: Is target epoch correct for voting period?                    │  │
│  │   slot < tooLate: targetEpoch must equal currentEpoch                 │  │
│  │   slot ≥ tooLate: targetEpoch must equal currentEpoch + 1             │  │
│  │   Error: PPUpdateWrongEpoch                                            │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                               ↓                                              │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │ All checks pass: Merge proposal into current/future proposals          │  │
│  │   If quorum reached: Set future protocol parameters                   │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
│  Output: Updated ShelleyGovState                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

### Era Applicability

**Important**: The PPUP rule is only used in Shelley through Babbage eras. Starting with Conway era, governance is handled by the new DRep-based governance system (CIP-1694).

```haskell
-- Reference: Ppup.hs:123
instance (EraPParams era, AtMostEra "Babbage" era) => STS (ShelleyPPUP era)
```

---

See `alonzo-utxo.md` for Plutus-era additions.
