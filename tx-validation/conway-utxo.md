# Conway Era UTXO Validation

## Overview

The Conway era introduces **on-chain governance** to Cardano (CIP-1694). From a UTXO perspective, Conway:
- **Reuses Babbage UTXO validation entirely** for Phase 1
- Introduces new **governance actions** (voting, proposals)
- Has a **flattened error type** (all errors in one enum)

**Haskell Reference**: `eras/conway/impl/src/Cardano/Ledger/Conway/Rules/Utxo.hs`

## Era Architecture

```
Conway UTXO reuses Babbage:
┌─────────────────────────────────────────────────────────────────────────┐
│ ConwayUtxoPredFailure                                                   │
│ FLATTENED structure - all errors defined directly, no wrapping!         │
│                                                                         │
│ ├── UtxosFailure (Phase 2)                                             │
│ ├── BadInputsUTxO                                                      │
│ ├── OutsideValidityIntervalUTxO                                        │
│ ├── MaxTxSizeUTxO                                                      │
│ ├── InputSetEmptyUTxO                                                  │
│ ├── FeeTooSmallUTxO                                                    │
│ ├── ValueNotConservedUTxO                                              │
│ ├── WrongNetwork                                                       │
│ ├── WrongNetworkWithdrawal                                             │
│ ├── OutputTooSmallUTxO                                                 │
│ ├── OutputBootAddrAttrsTooBig                                          │
│ ├── OutputTooBigUTxO                                                   │
│ ├── InsufficientCollateral                                             │
│ ├── ScriptsNotPaidUTxO                                                 │
│ ├── ExUnitsTooBigUTxO                                                  │
│ ├── CollateralContainsNonADA                                           │
│ ├── WrongNetworkInTxBody                                               │
│ ├── OutsideForecast                                                    │
│ ├── TooManyCollateralInputs                                            │
│ ├── NoCollateralInputs                                                 │
│ ├── IncorrectTotalCollateralField                                      │
│ ├── BabbageOutputTooSmallUTxO                                          │
│ └── BabbageNonDisjointRefInputs                                        │
└─────────────────────────────────────────────────────────────────────────┘
```

## Key Insight: Conway Reuses Babbage for UTXO

The most important thing to understand about Conway UTXO is:

```haskell
-- Reference: eras/conway/impl/src/Cardano/Ledger/Conway/Rules/Utxo.hs:252-256
instance ... STS (ConwayUTXO era) where
  ...
  transitionRules = [Babbage.utxoTransition @era]  -- REUSES BABBAGE!
```

**Conway doesn't add any new UTXO validations!** It simply reuses all Babbage validation.

The new governance features (voting, proposals) affect:
- **UTXOS** (Phase 2) - new script purposes for voting/proposing
- **UTXOW** - witness requirements for governance actions
- **LEDGER** - governance state transitions

But UTXO Phase 1 validation is unchanged from Babbage.

## Transition Rule Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         CONWAY UTXO TRANSITION                               │
│                                                                              │
│  === IDENTICAL TO BABBAGE ===                                               │
│                                                                              │
│  Step 1:  disjointRefInputs                                                 │
│  Step 2:  validateOutsideValidityIntervalUTxO                               │
│  Step 3:  validateOutsideForecast                                           │
│  Step 4:  validateInputSetEmptyUTxO                                         │
│  Step 5:  feesOK (with collateral return)                                   │
│  Step 6:  validateBadInputsUTxO (spend + collateral + ref)                  │
│  Step 7:  validateValueNotConservedUTxO                                     │
│  Step 8:  validateOutputTooSmallUTxO (all outputs)                          │
│  Step 9:  validateOutputTooBigUTxO                                          │
│  Step 10: validateOutputBootAddrAttrsTooBig                                 │
│  Step 11: validateWrongNetwork                                              │
│  Step 12: validateWrongNetworkWithdrawal                                    │
│  Step 13: validateWrongNetworkInTxBody                                      │
│  Step 14: validateMaxTxSizeUTxO                                             │
│  Step 15: validateExUnitsTooBigUTxO                                         │
│  Step 16: validateTooManyCollateralInputs                                   │
│  Step 17: UTXOS Sub-rule (Phase 2) → ConwayUTXOS                            │
│                                                                              │
│  Output: Updated UTxOState                                                   │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Haskell Implementation

```haskell
-- Reference: eras/conway/impl/src/Cardano/Ledger/Conway/Rules/Utxo.hs:223-256

instance
  forall era.
  ( EraTx era
  , EraUTxO era
  , ConwayEraTxBody era
  , AlonzoEraTxWits era
  , EraRule "UTXO" era ~ ConwayUTXO era
  , InjectRuleFailure "UTXO" ShelleyUtxoPredFailure era
  , InjectRuleFailure "UTXO" AllegraUtxoPredFailure era
  , InjectRuleFailure "UTXO" AlonzoUtxoPredFailure era
  , InjectRuleFailure "UTXO" BabbageUtxoPredFailure era
  , InjectRuleFailure "UTXO" ConwayUtxoPredFailure era
  , Embed (EraRule "UTXOS" era) (ConwayUTXO era)
  , Environment (EraRule "UTXOS" era) ~ Shelley.UtxoEnv era
  , State (EraRule "UTXOS" era) ~ Shelley.UTxOState era
  , Signal (EraRule "UTXOS" era) ~ Tx TopTx era
  , PredicateFailure (EraRule "UTXO" era) ~ ConwayUtxoPredFailure era
  , EraCertState era
  , SafeToHash (TxWits era)
  ) =>
  STS (ConwayUTXO era)
  where
  type State (ConwayUTXO era) = Shelley.UTxOState era
  type Signal (ConwayUTXO era) = Tx TopTx era
  type Environment (ConwayUTXO era) = Shelley.UtxoEnv era
  type BaseM (ConwayUTXO era) = ShelleyBase
  type PredicateFailure (ConwayUTXO era) = ConwayUtxoPredFailure era
  type Event (ConwayUTXO era) = AlonzoUtxoEvent era

  initialRules = []

  -- KEY: Conway uses Babbage's transition rules!
  transitionRules = [Babbage.utxoTransition @era]

  assertions = [Shelley.validSizeComputationCheck]
```

---

## The Flattened Error Type

Conway introduces a **flattened** error type that contains all possible UTXO errors directly, rather than wrapping previous era errors.

### Why Flattened?

In previous eras, errors were nested:
```
Shelley → Allegra wraps Shelley → Alonzo wraps Allegra → Babbage wraps Alonzo
```

This led to deeply nested error structures like:
```
BabbageUtxoPredFailure
  └── AlonzoInBabbageUtxoPredFailure
      └── ShelleyInAlonzoUtxoPredFailure
          └── BadInputsUTxO
```

Conway flattens everything into a single enum:
```
ConwayUtxoPredFailure
  └── BadInputsUTxO (directly!)
```

### Error Conversion Functions

```haskell
-- Reference: eras/conway/impl/src/Cardano/Ledger/Conway/Rules/Utxo.hs:345-399

babbageToConwayUtxoPredFailure ::
  BabbageUtxoPredFailure era ->
  ConwayUtxoPredFailure era
babbageToConwayUtxoPredFailure = \case
  Babbage.AlonzoInBabbageUtxoPredFailure a -> alonzoToConwayUtxoPredFailure a
  Babbage.IncorrectTotalCollateralField c1 c2 -> IncorrectTotalCollateralField c1 c2
  Babbage.BabbageOutputTooSmallUTxO ts -> BabbageOutputTooSmallUTxO ts
  Babbage.BabbageNonDisjointRefInputs ts -> BabbageNonDisjointRefInputs ts

alonzoToConwayUtxoPredFailure ::
  AlonzoUtxoPredFailure era ->
  ConwayUtxoPredFailure era
alonzoToConwayUtxoPredFailure = \case
  Alonzo.BadInputsUTxO x -> BadInputsUTxO x
  Alonzo.OutsideValidityIntervalUTxO vi slotNo -> OutsideValidityIntervalUTxO vi slotNo
  Alonzo.MaxTxSizeUTxO m -> MaxTxSizeUTxO m
  Alonzo.InputSetEmptyUTxO -> InputSetEmptyUTxO
  Alonzo.FeeTooSmallUTxO m -> FeeTooSmallUTxO m
  Alonzo.ValueNotConservedUTxO m -> ValueNotConservedUTxO m
  -- ... all other cases mapped directly

allegraToConwayUtxoPredFailure ::
  Allegra.AllegraUtxoPredFailure era ->
  ConwayUtxoPredFailure era
allegraToConwayUtxoPredFailure = \case
  Allegra.BadInputsUTxO x -> BadInputsUTxO x
  Allegra.OutsideValidityIntervalUTxO vi slotNo -> OutsideValidityIntervalUTxO vi slotNo
  -- ... etc
```

---

## Complete Error Type

```haskell
-- Reference: eras/conway/impl/src/Cardano/Ledger/Conway/Rules/Utxo.hs:86-158
data ConwayUtxoPredFailure era
  = -- | Subtransition Failures (Phase 2)
    UtxosFailure (PredicateFailure (EraRule "UTXOS" era))

  -- From Shelley/Allegra
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

  -- From Alonzo
  | OutputTooBigUTxO [(Int, Int, TxOut era)]
  | InsufficientCollateral DeltaCoin Coin
  | ScriptsNotPaidUTxO (UTxO era)
  | ExUnitsTooBigUTxO (Mismatch RelLTEQ ExUnits)
  | CollateralContainsNonADA (Value era)
  | WrongNetworkInTxBody (Mismatch RelEQ Network)
  | OutsideForecast SlotNo
  | TooManyCollateralInputs (Mismatch RelLTEQ Natural)
  | NoCollateralInputs

  -- From Babbage
  | IncorrectTotalCollateralField DeltaCoin Coin
  | BabbageOutputTooSmallUTxO [(TxOut era, Coin)]
  | BabbageNonDisjointRefInputs (NonEmpty TxIn)
```

---

## What Conway DOES Add (Governance)

While Conway doesn't change UTXO Phase 1 validation, it adds significant governance features that affect other rules:

### New Transaction Body Fields

```haskell
-- Conway adds to TxBody:
data ConwayTxBody era = ConwayTxBody
  { -- ... all Babbage fields ...
  , ctbVotingProcedures :: VotingProcedures era    -- NEW: voting
  , ctbProposalProcedures :: OSet (ProposalProcedure era)  -- NEW: proposals
  , ctbCurrentTreasuryValue :: StrictMaybe Coin    -- NEW: treasury donation
  , ctbTreasuryDonation :: Coin                    -- NEW: treasury donation
  }
```

### New Script Purposes

Conway adds two new script purposes for governance:

```haskell
-- Reference: libs/cardano-ledger-core/src/Cardano/Ledger/Plutus/Language.hs
data ConwayPlutusPurpose f era
  = ConwaySpending !(f TxIn)        -- Existing
  | ConwayMinting !(f PolicyId)     -- Existing
  | ConwayRewarding !(f RewardAccount)  -- Existing
  | ConwayCertifying !(f TxCert)    -- Existing
  | ConwayVoting !(f Voter)         -- NEW: authorize voting
  | ConwayProposing !(f ProposalProcedure)  -- NEW: authorize proposals
```

These affect:
- **UTXOS** (Phase 2) - scripts can now authorize voting/proposing
- **UTXOW** - `getScriptsNeeded` includes voting/proposing scripts
- But NOT UTXO Phase 1 validation

### Governance Actions

Conway transactions can include:
1. **Voting procedures** - DReps, SPOs, and Constitutional Committee vote
2. **Proposal procedures** - Submit governance actions
3. **Treasury donations** - Donate ADA to the treasury

These are validated in LEDGER and GOV rules, not UTXO.

---

## Phase 2 Changes (UTXOS)

While Phase 1 (UTXO) is unchanged, Phase 2 (UTXOS) has changes for governance:

```haskell
-- Reference: eras/conway/impl/src/Cardano/Ledger/Conway/Rules/Utxos.hs

-- Conway adds new error types for governance scripts
data ConwayUtxosPredFailure era
  = ValidationTagMismatch IsValid
  | CollectErrors [CollectError era]
  deriving (Generic)
```

The key difference is how `scriptsNeeded` is calculated:

```haskell
-- Reference: eras/conway/impl/src/Cardano/Ledger/Conway/UTxO.hs:59-102
getConwayScriptsNeeded ::
  ConwayEraTxBody era =>
  UTxO era ->
  TxBody l era ->
  AlonzoScriptsNeeded era
getConwayScriptsNeeded utxo txBody =
  getSpendingScriptsNeeded utxo txBody
    <> getRewardingScriptsNeeded txBody
    <> certifyingScriptsNeeded
    <> getMintingScriptsNeeded txBody
    <> votingScriptsNeeded       -- NEW in Conway
    <> proposingScriptsNeeded    -- NEW in Conway
```

---

## Summary: UTXO Validation Across Eras

| Era | Key UTXO Changes |
|-----|------------------|
| **Shelley** | Foundation: TTL, value conservation, min UTxO |
| **Allegra** | Validity intervals (both bounds) |
| **Mary** | Multi-asset (Value type), no new validations |
| **Alonzo** | Collateral, execution units, Phase 2, Plutus |
| **Babbage** | Reference inputs, collateral return, coinsPerUTxOByte |
| **Conway** | **No UTXO changes!** Reuses Babbage. Governance in other rules. |

---

## Error Injection Pattern

Conway uses `InjectRuleFailure` to convert errors from previous eras:

```haskell
instance InjectRuleFailure "UTXO" BabbageUtxoPredFailure ConwayEra where
  injectFailure = babbageToConwayUtxoPredFailure

instance InjectRuleFailure "UTXO" AlonzoUtxoPredFailure ConwayEra where
  injectFailure = alonzoToConwayUtxoPredFailure

instance InjectRuleFailure "UTXO" ShelleyUtxoPredFailure ConwayEra where
  injectFailure =
    allegraToConwayUtxoPredFailure
      . shelleyToAllegraUtxoPredFailure

instance InjectRuleFailure "UTXO" Allegra.AllegraUtxoPredFailure ConwayEra where
  injectFailure = allegraToConwayUtxoPredFailure
```

When Babbage validation produces an error, it gets automatically converted to the flat Conway format.

---

## Practical Implications

For validators implementing Conway UTXO:

1. **Use Babbage validation** - Conway UTXO is identical to Babbage
2. **Flatten errors** - Convert nested errors to flat Conway format
3. **Handle governance in other rules** - Voting/proposals aren't UTXO concerns
4. **Phase 2 has changes** - New script purposes for governance

For wallet developers:

1. **No new UTXO requirements** - Same as Babbage
2. **New tx body fields** - Must handle voting/proposals
3. **Flat error types** - Easier error handling in Conway
