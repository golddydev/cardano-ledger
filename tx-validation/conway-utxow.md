# Conway Era UTXOW Rule

## Overview

Conway introduces **on-chain governance** (CIP-1694) and makes key architectural changes to UTXOW:

1. **Flattened error hierarchy** - All errors at the same level, not nested
2. **MIR removal** - No more Move Instantaneous Rewards certificates
3. **Governance witnesses** - Voters (DReps, Committee, SPOs) need authorization
4. **PlutusV3 optional datums** - Spending scripts no longer require datums (CIP-0069)
5. **New script purposes** - Voting and Proposing purposes for governance

**Source File**: `eras/conway/impl/src/Cardano/Ledger/Conway/Rules/Utxow.hs` (348 lines)

---

## Key Changes from Babbage

### 1. Flattened Error Hierarchy

**Before Conway** (nested):
```
BabbageUtxowPredFailure
  └─ AlonzoInBabbageUtxowPredFailure
      └─ ShelleyInAlonzoUtxowPredFailure
          └─ InvalidWitnessesUTXOW [vkeys]   ← Actual error buried 3 levels deep
```

**Conway** (flat):
```
ConwayUtxowPredFailure
  └─ InvalidWitnessesUTXOW [vkeys]           ← Directly accessible
```

### 2. MIR Certificate Removal

**Before**: MIR certificates moved funds from treasury/reserves, requiring genesis key quorum.

**Conway**: MIR is gone. Treasury withdrawals now use governance proposals.

### 3. PlutusV3 Optional Datums (CIP-0069)

**Before**: All Plutus spending scripts required datums.

**Conway**: PlutusV3 spending scripts can work without datums.

```haskell
-- From UTxO.hs:184-188
case txOut ^. datumTxOutF of
  NoDatum
    | Just lang <- spendingPlutusScriptLanguage addr
    , lang < PlutusV3 ->  -- Only V1/V2 require datum
        (hashSet, Set.insert txIn inputSet)
```

---

## Predicate Failures (Errors)

**Reference**: `Utxow.hs:75-136`

```haskell
data ConwayUtxowPredFailure era
  -- Shelley errors (embedded directly, not wrapped):
  = UtxoFailure (PredicateFailure (EraRule "UTXO" era))
  | InvalidWitnessesUTXOW [VKey Witness]
  | MissingVKeyWitnessesUTXOW (Set (KeyHash Witness))
  | MissingScriptWitnessesUTXOW (Set ScriptHash)
  | ScriptWitnessNotValidatingUTXOW (Set ScriptHash)
  | MissingTxBodyMetadataHash TxAuxDataHash
  | MissingTxMetadata TxAuxDataHash
  | ConflictingMetadataHash (Mismatch RelEQ TxAuxDataHash)
  | InvalidMetadata
  | ExtraneousScriptWitnessesUTXOW (Set ScriptHash)
  -- NOTE: MIRInsufficientGenesisSigsUTXOW REMOVED

  -- Alonzo errors (embedded directly):
  | MissingRedeemers [(PlutusPurpose AsItem era, ScriptHash)]
  | MissingRequiredDatums (Set DataHash) (Set DataHash)
  | NotAllowedSupplementalDatums (Set DataHash) (Set DataHash)
  | PPViewHashesDontMatch (Mismatch RelEQ (StrictMaybe ScriptIntegrityHash))
  | UnspendableUTxONoDatumHash (Set TxIn)
  | ExtraRedeemers [PlutusPurpose AsIx era]

  -- Babbage errors (embedded directly):
  | MalformedScriptWitnesses (Set ScriptHash)
  | MalformedReferenceScripts (Set ScriptHash)
  | ScriptIntegrityHashMismatch
      (Mismatch RelEQ (StrictMaybe ScriptIntegrityHash))
      (StrictMaybe ByteString)
```

### Key Differences from Babbage

| Babbage | Conway |
|---------|--------|
| `AlonzoInBabbageUtxowPredFailure(ShelleyInAlonzoUtxowPredFailure(error))` | `error` |
| `MIRInsufficientGenesisSigsUTXOW` | **Removed** |
| Nested unwrapping needed | Direct pattern matching |

---

## Error Conversion Functions

Conway provides explicit functions to convert errors from previous eras:

### From Shelley

**Reference**: `Utxow.hs:327-345`

```haskell
shelleyToConwayUtxowPredFailure :: ShelleyUtxowPredFailure era -> ConwayUtxowPredFailure era
shelleyToConwayUtxowPredFailure = \case
  Shelley.InvalidWitnessesUTXOW xs -> InvalidWitnessesUTXOW xs
  Shelley.MissingVKeyWitnessesUTXOW xs -> MissingVKeyWitnessesUTXOW xs
  Shelley.MissingScriptWitnessesUTXOW xs -> MissingScriptWitnessesUTXOW xs
  Shelley.ScriptWitnessNotValidatingUTXOW xs -> ScriptWitnessNotValidatingUTXOW xs
  Shelley.UtxoFailure x -> UtxoFailure x
  Shelley.MissingTxBodyMetadataHash x -> MissingTxBodyMetadataHash x
  Shelley.MissingTxMetadata x -> MissingTxMetadata x
  Shelley.ConflictingMetadataHash x -> ConflictingMetadataHash x
  Shelley.InvalidMetadata -> InvalidMetadata
  Shelley.ExtraneousScriptWitnessesUTXOW xs -> ExtraneousScriptWitnessesUTXOW xs
  Shelley.MIRInsufficientGenesisSigsUTXOW _xs ->
    error "Impossible: MIR has been removed in Conway"  -- Can never happen!
```

### From Alonzo

**Reference**: `Utxow.hs:317-326`

```haskell
alonzoToConwayUtxowPredFailure :: AlonzoUtxowPredFailure era -> ConwayUtxowPredFailure era
alonzoToConwayUtxowPredFailure = \case
  Alonzo.ShelleyInAlonzoUtxowPredFailure f -> shelleyToConwayUtxowPredFailure f
  Alonzo.MissingRedeemers rs -> MissingRedeemers rs
  Alonzo.MissingRequiredDatums mds rds -> MissingRequiredDatums mds rds
  Alonzo.NotAllowedSupplementalDatums uds ads -> NotAllowedSupplementalDatums uds ads
  Alonzo.PPViewHashesDontMatch m -> PPViewHashesDontMatch m
  Alonzo.UnspendableUTxONoDatumHash ins -> UnspendableUTxONoDatumHash ins
  Alonzo.ExtraRedeemers xs -> ExtraRedeemers xs
  Alonzo.ScriptIntegrityHashMismatch x y -> ScriptIntegrityHashMismatch x y
```

### From Babbage

**Reference**: `Utxow.hs:306-316`

```haskell
babbageToConwayUtxowPredFailure :: BabbageUtxowPredFailure era -> ConwayUtxowPredFailure era
babbageToConwayUtxowPredFailure = \case
  Babbage.AlonzoInBabbageUtxowPredFailure x -> alonzoToConwayUtxowPredFailure x
  Babbage.UtxoFailure x -> UtxoFailure x
  Babbage.MalformedScriptWitnesses xs -> MalformedScriptWitnesses xs
  Babbage.MalformedReferenceScripts xs -> MalformedReferenceScripts xs
  Babbage.ScriptIntegrityHashMismatch x y -> ScriptIntegrityHashMismatch x y
```

---

## Transition Function

**Reference**: `Utxow.hs:199-226`

```haskell
instance STS (ConwayUTXOW era) where
  type PredicateFailure (ConwayUTXOW era) = ConwayUtxowPredFailure era
  
  -- IMPORTANT: Reuses Babbage transition function!
  transitionRules = [babbageUtxowTransition @era]
```

**Key insight**: Conway **reuses** Babbage's `babbageUtxowTransition` completely. The validation logic is the same; only the error types change.

**Why no MIR check?**
- MIR certificates are rejected at the CERT level in Conway
- They never reach UTXOW
- The conversion function has an `error` case for MIR that can never be reached

---

## Scripts Needed (Conway Extensions)

**Reference**: `eras/conway/impl/src/Cardano/Ledger/Conway/UTxO.hs:59-102`

```haskell
getConwayScriptsNeeded ::
  ConwayEraTxBody era =>
  UTxO era -> TxBody l era -> AlonzoScriptsNeeded era
getConwayScriptsNeeded utxo txBody =
  getSpendingScriptsNeeded utxo txBody
    <> getRewardingScriptsNeeded txBody
    <> certifyingScriptsNeeded
    <> getMintingScriptsNeeded txBody
    <> votingScriptsNeeded        -- NEW in Conway
    <> proposingScriptsNeeded     -- NEW in Conway
```

### New Purposes

#### VotingPurpose

**Reference**: `UTxO.hs:79-89`

```haskell
votingScriptsNeeded =
  AlonzoScriptsNeeded $
    catMaybes $
      zipAsIxItem (Map.keys (unVotingProcedures (txBody ^. votingProceduresTxBodyL))) $
        \asIxItem@(AsIxItem _ voter) ->
          (VotingPurpose asIxItem,) <$> getVoterScriptHash voter
  where
    getVoterScriptHash = \case
      CommitteeVoter cred -> credScriptHash cred
      DRepVoter cred -> credScriptHash cred
      StakePoolVoter _ -> Nothing  -- SPOs use key witnesses, not scripts
```

**What it does**: Collects script hashes from voters (Committee members, DReps) who vote using scripts.

#### ProposingPurpose

**Reference**: `UTxO.hs:91-102`

```haskell
proposingScriptsNeeded =
  AlonzoScriptsNeeded $
    catMaybes $
      zipAsIxItem (txBody ^. proposalProceduresTxBodyL) $
        \asIxItem@(AsIxItem _ proposal) ->
          (ProposingPurpose asIxItem,) <$> getProposalScriptHash proposal
  where
    getProposalScriptHash ProposalProcedure {pProcGovAction} =
      case pProcGovAction of
        ParameterChange _ _ (SJust govPolicyHash) -> Just govPolicyHash
        TreasuryWithdrawals _ (SJust govPolicyHash) -> Just govPolicyHash
        _ -> Nothing
```

**What it does**: Collects the governance policy script (constitution script) for proposals that use guardrails.

---

## VKey Witnesses Needed (Conway)

**Reference**: `eras/conway/impl/src/Cardano/Ledger/Conway/UTxO.hs:177-199`

```haskell
getConwayWitsVKeyNeeded ::
  (EraTx era, ConwayEraTxBody era) =>
  UTxO era -> TxBody l era -> Set (KeyHash Witness)
getConwayWitsVKeyNeeded utxo txBody =
  getShelleyWitsVKeyNeededNoGov utxo txBody       -- No genesis delegates!
    `Set.union` Set.map asWitness (txBody ^. reqSignerHashesTxBodyG)
    `Set.union` voterWitnesses txBody             -- NEW: voter key witnesses

voterWitnesses ::
  ConwayEraTxBody era =>
  TxBody l era -> Set (KeyHash Witness)
voterWitnesses txb =
  Map.foldrWithKey' accum mempty (unVotingProcedures (txb ^. votingProceduresTxBodyL))
  where
    accum voter _ khs =
      maybe khs (`Set.insert` khs) $
        case voter of
          CommitteeVoter cred -> credKeyHashWitness cred
          DRepVoter cred -> credKeyHashWitness cred
          StakePoolVoter poolId -> Just $ asWitness poolId
```

### Key Differences from Babbage/Alonzo

| Component | Alonzo/Babbage | Conway |
|-----------|----------------|--------|
| Base function | `getShelleyWitsVKeyNeeded` | `getShelleyWitsVKeyNeededNoGov` |
| Genesis delegates | ✅ Required for updates | ❌ Removed |
| Voter witnesses | ❌ N/A | ✅ Added |

**Why `NoGov`?**
- Protocol parameter updates no longer use genesis delegate signatures
- Governance is fully on-chain via DReps, Committee, and SPOs
- No more off-chain genesis key ceremonies

---

## PlutusV3 Changes (CIP-0069)

### Optional Spending Datums

**Reference**: `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/UTxO.hs:184-188`

```haskell
NoDatum
  | Just lang <- spendingPlutusScriptLanguage addr
  , lang < PlutusV3 ->  -- Only V1/V2 require datum
      (hashSet, Set.insert txIn inputSet)
```

| Script Version | Datum Required for Spending? |
|----------------|------------------------------|
| PlutusV1 | ✅ Yes |
| PlutusV2 | ✅ Yes |
| PlutusV3 | ❌ No (optional) |

### Test Evidence

**Reference**: `eras/conway/impl/testlib/Test/Cardano/Ledger/Conway/Imp/UtxosSpec.hs:72-78`

```haskell
if lang >= PlutusV3
  then submitTx_ tx    -- Success! No datum needed
  else submitFailingTx tx [injectFailure $ UnspendableUTxONoDatumHash [txIn]]
```

---

## Formal Spec Differences

### Removed Predicates
```
-- No longer checked in Conway UTXOW:
{ c ∈ txcerts txb ∩ TxCert_mir } ≠ ∅ ⇒ |genSig| ≥ Quorum
```

### Added Predicates (from governance)
```
-- Voter authorization:
∀ voter ∈ dom(votingProcedures txb):
  voter is authorized (key witness or script)

-- Proposal authorization:
∀ proposal ∈ proposalProcedures txb:
  if proposal requires guardrails → constitution script validates
```

---

## Summary: What Conway Changes in UTXOW

| Change | Impact |
|--------|--------|
| **Flattened errors** | Simpler error handling, direct pattern matching |
| **MIR removal** | No genesis delegate checks in UTXOW |
| **Voter witnesses** | DReps/Committee/SPOs need authorization |
| **New script purposes** | VotingPurpose, ProposingPurpose |
| **PlutusV3 datums** | Spending scripts can work without datums |
| **No genesis delegates** | Uses `getShelleyWitsVKeyNeededNoGov` |

**Validation logic is identical to Babbage** - only error types and witness computation change.

---

## Governance Proposals (CIP-1694) - Deep Dive

### What is a Proposal?

In Conway era, a **Governance Proposal** (`ProposalProcedure`) is a formal on-chain request to change something in the protocol. Anyone can submit a proposal - this is intentionally **permissionless**.

**Reference**: `eras/conway/impl/src/Cardano/Ledger/Conway/Governance/Procedures.hs:487-492`

```haskell
data ProposalProcedure era = ProposalProcedure
  { pProcDeposit :: !Coin             -- Deposit amount (refundable)
  , pProcReturnAddr :: !RewardAccount -- Where to refund deposit
  , pProcGovAction :: !(GovAction era) -- The actual proposal content
  , pProcAnchor :: !Anchor            -- Metadata URL + hash
  }
```

### Types of Governance Actions

**Reference**: `Procedures.hs:794-833`

```haskell
data GovAction era
  = ParameterChange !(StrictMaybe GovPurposeId) !(PParamsUpdate era) !(StrictMaybe ScriptHash)
  | HardForkInitiation !(StrictMaybe GovPurposeId) !ProtVer
  | TreasuryWithdrawals !(Map RewardAccount Coin) !(StrictMaybe ScriptHash)
  | NoConfidence !(StrictMaybe GovPurposeId)
  | UpdateCommittee !(StrictMaybe GovPurposeId) !(Set Credential) !(Map Credential EpochNo) !UnitInterval
  | NewConstitution !(StrictMaybe GovPurposeId) !(Constitution era)
  | InfoAction  -- Non-binding, informational only
```

| Action Type | Description | Who Votes |
|------------|-------------|-----------|
| **ParameterChange** | Modify protocol parameters | DReps + Committee |
| **HardForkInitiation** | Initiate a hard fork | SPOs + DReps + Committee |
| **TreasuryWithdrawals** | Withdraw funds from treasury | DReps + Committee |
| **NoConfidence** | Motion of no confidence in committee | SPOs + DReps |
| **UpdateCommittee** | Add/remove committee members | SPOs + DReps |
| **NewConstitution** | Update the constitution | DReps + Committee |
| **InfoAction** | Non-binding information | All (advisory) |

---

### Why No VKey Witness for Proposals?

**Critical Design Decision**: The `pProcReturnAddr` (return address) does NOT require a key witness.

**Reference**: `Conway/UTxO.hs:177-185`

```haskell
getConwayWitsVKeyNeeded utxo txBody =
  getShelleyWitsVKeyNeededNoGov utxo txBody
    `Set.union` Set.map asWitness (txBody ^. reqSignerHashesTxBodyG)
    `Set.union` voterWitnesses txBody
    -- NOTE: NO proposal witnesses! Proposals don't require return address signature
```

#### Reasons

1. **Permissionless by Design (CIP-1694)**
   - Anyone can propose changes to the protocol
   - The community (DReps, SPOs, Committee) decides through voting
   - No gatekeeping on WHO can propose

2. **Economic Barrier via Deposit**
   - Proposals require a large deposit (e.g., 100,000 ADA from `ppGovActionDepositL`)
   - This deposit comes from transaction **inputs** (which ARE signed)
   - Creates economic skin-in-the-game to prevent spam

3. **Return Address is Just a Recipient**
   - The return address specifies where the deposit refund goes
   - Similar to sending ADA: you can send to anyone without their signature
   - You cannot "steal" by setting someone else's address - you're giving THEM money

4. **No Security Risk**
   ```
   Example:
   Alice creates proposal with:
   - Deposit: 100,000 ADA (from her signed inputs)
   - Return Address: Bob's reward account

   Result:
   - Alice's inputs are spent (she signed)
   - If proposal enacted/expires: Bob receives 100,000 ADA refund
   - Alice GAVE Bob money - no theft possible
   ```

#### Comparison: Voters vs Proposers

| Component | VKey Witness Required? | Why? |
|-----------|----------------------|------|
| **Voter** | ✅ YES | Voters exercise power - must prove identity to prevent impersonation |
| **Proposer** | ❌ NO | Proposers pay deposit from signed inputs - no additional auth needed |

---

### Why Scripts ARE Required for Some Proposals?

While no KEY witness is needed, **governance policy scripts** (guardrails) can be required.

#### The Constitution's Policy Script

**Reference**: `Procedures.hs:893-904`

```haskell
data Constitution era = Constitution
  { constitutionAnchor :: !Anchor
  , constitutionScript :: !(StrictMaybe ScriptHash)  -- Optional guardrail script!
  }
```

The constitution can include a **governance policy script** that acts as a programmatic check on certain proposals.

#### Which Proposals Need Scripts?

**Reference**: `Conway/UTxO.hs:91-102`

```haskell
proposingScriptsNeeded =
  AlonzoScriptsNeeded $
    catMaybes $
      zipAsIxItem (txBody ^. proposalProceduresTxBodyL) $
        \asIxItem@(AsIxItem _ proposal) ->
          (ProposingPurpose asIxItem,) <$> getProposalScriptHash proposal
  where
    getProposalScriptHash ProposalProcedure {pProcGovAction} =
      case pProcGovAction of
        ParameterChange _ _ (SJust govPolicyHash) -> Just govPolicyHash
        TreasuryWithdrawals _ (SJust govPolicyHash) -> Just govPolicyHash
        _ -> Nothing  -- Other actions don't use guardrails
```

**Only ParameterChange and TreasuryWithdrawals can have policy scripts!**

#### How Policy Script Validation Works

**Step 1: GOV Rule checks policy hash matches constitution**

**Reference**: `Conway/Rules/Gov.hs:533-545`

```haskell
case pProcGovAction of
  TreasuryWithdrawals wdrls proposalPolicy ->
    runTest $ checkPolicy @era constitutionPolicy proposalPolicy
  ParameterChange _ _ proposalPolicy ->
    runTest $ checkPolicy @era constitutionPolicy proposalPolicy
  _ -> pure ()

checkPolicy expectedPolicyHash actualPolicyHash =
  failureUnless (actualPolicyHash == expectedPolicyHash) $
    InvalidPolicyHash actualPolicyHash expectedPolicyHash
```

If the constitution has a policy script, the proposal **MUST** include the same script hash.

**Step 2: UTXOW Rule validates script executes successfully**

The script is collected with `ProposingPurpose` and executed like any Plutus script. If it fails, the transaction fails with `ScriptWitnessNotValidatingUTXOW`.

#### Purpose of Governance Policy Scripts (Guardrails)

These scripts enforce programmatic rules:

```
Example: Constitution has a guardrail script that:
- Checks parameter changes don't exceed ±10% of current values
- Ensures treasury withdrawals don't exceed 1M ADA per proposal
- Validates any constitutional requirements

When proposer submits ParameterChange:
1. Must include the guardrail script hash in proposal
2. Must provide the script in witness set  
3. Script executes and checks the proposal
4. If script returns False → Transaction FAILS
```

---

### Proposal Witness Summary

| Proposal Type | VKey Witness | Script Witness | Notes |
|--------------|--------------|----------------|-------|
| Any proposal | ❌ NO | - | Permissionless submission |
| ParameterChange (no policy) | ❌ NO | ❌ NO | |
| ParameterChange (with policy) | ❌ NO | ✅ YES | Constitution script required |
| TreasuryWithdrawals (no policy) | ❌ NO | ❌ NO | |
| TreasuryWithdrawals (with policy) | ❌ NO | ✅ YES | Constitution script required |
| HardForkInitiation | ❌ NO | ❌ NO | |
| NoConfidence | ❌ NO | ❌ NO | |
| UpdateCommittee | ❌ NO | ❌ NO | |
| NewConstitution | ❌ NO | ❌ NO | |
| InfoAction | ❌ NO | ❌ NO | |

---

### Proposal Lifecycle

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                       PROPOSAL LIFECYCLE                                     │
│                                                                              │
│  SUBMISSION (in transaction)                                                 │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ 1. Proposer creates tx with ProposalProcedure                        │    │
│  │ 2. Deposit paid from signed inputs                                   │    │
│  │ 3. Return address set (no witness needed)                           │    │
│  │ 4. If ParameterChange/TreasuryWithdrawals with policy:              │    │
│  │    - Policy hash must match constitution                            │    │
│  │    - Script must be provided and execute successfully               │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  COLLECTION (ongoing during epoch)                                           │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ - Proposal stored in `Proposals` state                               │    │
│  │ - Assigned GovActionId (txId#index)                                 │    │
│  │ - Voters (DReps, SPOs, Committee) submit votes                      │    │
│  │ - Votes accumulated in GovActionState                               │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  RATIFICATION (at epoch boundary)                                            │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ - Votes tallied against stake distribution                          │    │
│  │ - Different thresholds per action type                              │    │
│  │ - Must meet or exceed required threshold                            │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                               ↓                                              │
│  OUTCOME                                                                     │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ IF RATIFIED:                                                         │    │
│  │   - Action is ENACTED (params changed, funds withdrawn, etc.)       │    │
│  │   - Deposit refunded to pProcReturnAddr                             │    │
│  │                                                                      │    │
│  │ IF EXPIRED (not ratified within govActionLifetime):                 │    │
│  │   - Proposal removed from active set                                │    │
│  │   - Deposit refunded to pProcReturnAddr                             │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

### Key Insight: Proposals vs Voters

The design philosophy clearly separates:

1. **Proposal Submission** - Permissionless, anyone with deposit can propose
   - Economic barrier (deposit) prevents spam
   - No identity verification needed
   - Return address is just a refund destination

2. **Voting** - Requires authentication
   - Voters exercise real power over the protocol
   - Must prove identity (key witness or script)
   - Prevents vote manipulation/impersonation

This is similar to how in a democracy:
- Anyone can petition for a ballot measure (proposal)
- But only registered voters can vote (authentication required)

---

## Voting Mechanics in Conway Era (CIP-1694) - Deep Dive

### The Three Voting Bodies

Conway establishes a tricameral governance structure with three distinct voting bodies, each with different roles and voting power calculation methods:

**Reference**: `eras/conway/impl/src/Cardano/Ledger/Conway/Governance/Internal.hs`

| Voting Body | Members | Power Calculation | Key Role |
|------------|---------|-------------------|----------|
| **Constitutional Committee** | Elected members | 1 vote per member (head count) | Technical oversight |
| **DReps** (Delegated Representatives) | Registered representatives | Stake-weighted (delegated ADA) | Community voice |
| **SPOs** (Stake Pool Operators) | Pool operators | Stake-weighted (total pool stake) | Security & infrastructure |

---

### How Voting Power is Calculated

#### 1. Constitutional Committee Voting Power

**Reference**: `Rules/Ratify.hs:108-162`

Committee voting is **NOT stake-weighted** - each valid member has exactly 1 vote.

```haskell
-- From committeeAcceptedRatio
committeeAcceptedRatio members votes committeeState currentEpoch =
  yesVotes %? totalExcludingAbstain
  where
    accumVotes (!yes, !tot) member expiry
      | currentEpoch > expiry = (yes, tot)           -- expired: abstain
      | otherwise =
          case Map.lookup member (csCommitteeCreds committeeState) of
            Nothing -> (yes, tot)                     -- not registered: abstain
            Just (CommitteeMemberResigned _) -> (yes, tot)  -- resigned: abstain
            Just (CommitteeHotCredential hotKey) ->
              case Map.lookup hotKey votes of
                Nothing -> (yes, tot + 1)             -- no vote: counts as NO
                Just Abstain -> (yes, tot)            -- abstain: removed from total
                Just VoteNo -> (yes, tot + 1)         -- no: counted
                Just VoteYes -> (yes + 1, tot + 1)    -- yes: counted
```

**Important Rules**:
- **No vote = No** (unlike DReps where no vote affects denominator)
- **Abstain** removes the member from the calculation entirely
- **Expired/Resigned/Unregistered** members are excluded
- **Minimum size** (`ppCommitteeMinSizeL`) must be met, or committee is considered absent

#### 2. DRep Voting Power

**Reference**: `Rules/Ratify.hs:239-280` & `Governance/DRepPulser.hs:206-248`

DRep voting power is **stake-weighted** based on delegated ADA.

```haskell
-- Stake components for DRep voting power:
-- 1. Instant stake (delegated stake distribution)
-- 2. Rewards (in reward accounts)  
-- 3. Proposal deposits (locked deposits for governance)

addToDRepDistr accountState stakeAndDeposits distr = fromMaybe distr $ do
  dRep <- accountState ^. dRepDelegationAccountStateL
  let
    balance = accountState ^. balanceAccountStateL        -- rewards
    updatedDistr = Map.insertWith (<>) dRep (stakeAndDeposits <> balance) distr
  Just $ case dRep of
    DRepAlwaysAbstain -> updatedDistr
    DRepAlwaysNoConfidence -> updatedDistr
    DRepCredential cred
      | Map.member cred regDReps -> updatedDistr          -- only if registered
      | otherwise -> distr                                 -- skip unregistered
```

**DRep Voting Power Formula**:
```
DRep_power = Σ (delegator_stake + delegator_rewards + delegator_proposal_deposits)
             for all stake credentials delegated to this DRep
```

**DRep Accepted Ratio**:
```haskell
-- From dRepAcceptedRatio
dRepAcceptedRatio = yesStake %? totalExcludingAbstainStake
  where
    accumStake (!yes, !tot) drep (CompactCoin stake) =
      case drep of
        DRepCredential cred ->
          case Map.lookup cred reDRepState of
            Nothing -> (yes, tot)                    -- not registered: skip
            Just drepState
              | currentEpoch > drepExpiry drepState -> (yes, tot)  -- expired: skip
              | otherwise ->
                  case Map.lookup cred gasDRepVotes of
                    Nothing -> (yes, tot + stake)    -- no vote: counts toward NO
                    Just VoteYes -> (yes + stake, tot + stake)
                    Just Abstain -> (yes, tot)       -- abstain: removed from total
                    Just VoteNo -> (yes, tot + stake)
        DRepAlwaysNoConfidence ->
          case govAction of
            NoConfidence _ -> (yes + stake, tot + stake)  -- auto-YES for NoConfidence
            _ -> (yes, tot + stake)                       -- otherwise: NO
        DRepAlwaysAbstain -> (yes, tot)                   -- removed from calculation
```

**Special Pseudo-DReps**:
| Pseudo-DRep | Behavior | Purpose |
|-------------|----------|---------|
| `DRepAlwaysAbstain` | Always abstain (removed from ratio) | Stake holders who don't want to participate |
| `DRepAlwaysNoConfidence` | Auto-YES on NoConfidence, NO otherwise | Signal distrust in committee |

#### 3. SPO Voting Power

**Reference**: `Rules/Ratify.hs:177-225`

SPO voting power is **stake-weighted** based on total pool stake.

```haskell
-- From spoAcceptedRatio
spoAcceptedRatio = yesStake %? (totalActiveStake - abstainStake)
  where
    accumStake (!yes, !abstain) poolId distr =
      let CompactCoin stake = individualTotalPoolStake distr
          vote = Map.lookup poolId gasStakePoolVotes
       in case vote of
            Nothing
              | HardForkInitiation {} <- govAction -> (yes, abstain)      -- No = count
              | bootstrapPhase -> (yes, abstain + stake)                   -- Abstain
              | otherwise -> case defaultStakePoolVote poolId ... of
                  DefaultNoConfidence
                    | NoConfidence {} <- govAction -> (yes + stake, abstain)  -- Auto-YES
                  DefaultAbstain -> (yes, abstain + stake)
                  _ -> (yes, abstain)                                      -- Default NO
            Just Abstain -> (yes, abstain + stake)
            Just VoteNo -> (yes, abstain)                                  -- NO counted
            Just VoteYes -> (yes + stake, abstain)                         -- YES counted
```

**SPO Default Vote** (post-bootstrap):
```haskell
-- Reference: Governance.hs:557-574
defaultStakePoolVote poolId poolParams accounts =
  toDefaultVote $ do
    spp <- Map.lookup poolId poolParams
    accountState <- Map.lookup (spsRewardAccount spp) (accounts ^. accountsMapL)
    accountState ^. dRepDelegationAccountStateL
  where
    toDefaultVote (Just DRepAlwaysAbstain) = DefaultAbstain
    toDefaultVote (Just DRepAlwaysNoConfidence) = DefaultNoConfidence
    toDefaultVote _ = DefaultNo
```

**SPO Default Vote Logic**:
| Pool Reward Account Delegation | Default Vote |
|-------------------------------|--------------|
| Delegated to `DRepAlwaysAbstain` | Abstain |
| Delegated to `DRepAlwaysNoConfidence` | Yes on NoConfidence, No otherwise |
| Any other (including no delegation) | No |

---

### Voting Thresholds by Action Type

Each governance action type has specific thresholds defined in protocol parameters.

**Reference**: `PParams.hs:302-403` & `Governance/Internal.hs:370-540`

#### Pool Voting Thresholds (`PoolVotingThresholds`)

```haskell
data PoolVotingThresholds = PoolVotingThresholds
  { pvtMotionNoConfidence :: !UnitInterval     -- NoConfidence action
  , pvtCommitteeNormal :: !UnitInterval        -- UpdateCommittee (committee exists)
  , pvtCommitteeNoConfidence :: !UnitInterval  -- UpdateCommittee (no committee)
  , pvtHardForkInitiation :: !UnitInterval     -- HardForkInitiation
  , pvtPPSecurityGroup :: !UnitInterval        -- Security-relevant param changes
  }
```

#### DRep Voting Thresholds (`DRepVotingThresholds`)

```haskell
data DRepVotingThresholds = DRepVotingThresholds
  { dvtMotionNoConfidence :: !UnitInterval     -- NoConfidence action
  , dvtCommitteeNormal :: !UnitInterval        -- UpdateCommittee (committee exists)
  , dvtCommitteeNoConfidence :: !UnitInterval  -- UpdateCommittee (no committee)
  , dvtUpdateToConstitution :: !UnitInterval   -- NewConstitution
  , dvtHardForkInitiation :: !UnitInterval     -- HardForkInitiation
  , dvtPPNetworkGroup :: !UnitInterval         -- Network param changes
  , dvtPPEconomicGroup :: !UnitInterval        -- Economic param changes
  , dvtPPTechnicalGroup :: !UnitInterval       -- Technical param changes
  , dvtPPGovGroup :: !UnitInterval             -- Governance param changes
  , dvtTreasuryWithdrawal :: !UnitInterval     -- TreasuryWithdrawals
  }
```

#### Which Group Votes on What?

**Reference**: `Governance/Internal.hs:381-540`

| Gov Action | Committee | DReps | SPOs | Notes |
|-----------|-----------|-------|------|-------|
| **NoConfidence** | ❌ No | ✅ Yes | ✅ Yes | Committee cannot vote on their own removal |
| **UpdateCommittee** | ❌ No | ✅ Yes | ✅ Yes | Same reason |
| **NewConstitution** | ✅ Yes | ✅ Yes | ❌ No | |
| **HardForkInitiation** | ✅ Yes | ✅ Yes | ✅ Yes | All three required |
| **ParameterChange** | ✅ Yes | ✅ Yes | Conditional* | *Only for security-relevant params |
| **TreasuryWithdrawals** | ✅ Yes | ✅ Yes | ❌ No | |
| **InfoAction** | Advisory | Advisory | Advisory | Non-binding, no threshold |

**Security-Relevant Parameters** (SPOs vote):
```haskell
-- From votingStakePoolThresholdInternal
paramChangeThreshold ppu
  | any isSecurityRelevant (modifiedPPGroups ppu) =
      VotingThreshold pvtPPSecurityGroup
  | otherwise = NoVotingAllowed

isSecurityRelevant (PPGroups _ s) =
  case s of
    SecurityGroup -> True       -- SPOs vote
    NoStakePoolGroup -> False   -- SPOs don't vote
```

---

### Ratification Process

**Reference**: `Rules/Ratify.hs:307-359`

```haskell
ratifyTransition = do
  TRC (env@RatifyEnv {reCurrentEpoch}, st, RatifySignal rsig) <- judgmentContext
  case rsig of
    gas@GovActionState {gasId, gasExpiresAfter} :<| sigs -> do
      let govAction = gasAction gas
      if prevActionAsExpected gas ensPrevGovActionIds    -- 1. Parent check
        && validCommitteeTerm govAction pp reCurrentEpoch -- 2. Term length check
        && not rsDelayed                                   -- 3. Not delayed
        && withdrawalCanWithdraw govAction ensTreasury    -- 4. Treasury check
        && acceptedByEveryone env st gas                   -- 5. ALL groups accept
        then do
          newEnactState <- trans @(EraRule "ENACT" era) $ ...
          let st' = st
                & rsEnactStateL .~ newEnactState
                & rsDelayedL .~ delayingAction govAction   -- Mark as delaying
                & rsEnactedL %~ (:|> gas)                  -- Add to enacted
          trans @(ConwayRATIFY era) $ TRC (env, st', RatifySignal sigs)
        else do
          -- Check for expiry
          if gasExpiresAfter < reCurrentEpoch
            then pure $ st' & rsExpiredL %~ Set.insert gasId
            else pure st'
```

#### Ratification Conditions

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    RATIFICATION CONDITIONS                                   │
│                                                                              │
│  A proposal is RATIFIED when ALL of these are TRUE:                         │
│                                                                              │
│  1. ┌─────────────────────────────────────────────────────────────────────┐ │
│     │ PARENT CHECK (prevActionAsExpected)                                  │ │
│     │ - Proposal's "previous action" matches last enacted action           │ │
│     │ - Prevents conflicting forks in governance                           │ │
│     └─────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
│  2. ┌─────────────────────────────────────────────────────────────────────┐ │
│     │ COMMITTEE TERM CHECK (validCommitteeTerm)                            │ │
│     │ - New committee members' terms ≤ current_epoch + maxTermLength       │ │
│     │ - Only for UpdateCommittee actions                                   │ │
│     └─────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
│  3. ┌─────────────────────────────────────────────────────────────────────┐ │
│     │ NOT DELAYED                                                          │ │
│     │ - No "delaying action" was enacted this epoch                        │ │
│     │ - Delaying actions: NoConfidence, HardFork, UpdateCommittee,        │ │
│     │   NewConstitution                                                    │ │
│     └─────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
│  4. ┌─────────────────────────────────────────────────────────────────────┐ │
│     │ TREASURY CHECK (withdrawalCanWithdraw)                               │ │
│     │ - For TreasuryWithdrawals: sum of withdrawals ≤ treasury balance     │ │
│     └─────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
│  5. ┌─────────────────────────────────────────────────────────────────────┐ │
│     │ ACCEPTED BY EVERYONE                                                 │ │
│     │ - committeeAccepted AND spoAccepted AND dRepAccepted                 │ │
│     │ - Each group's ratio ≥ their threshold                               │ │
│     └─────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Delaying Actions

```haskell
-- From delayingAction
delayingAction :: GovAction era -> Bool
delayingAction NoConfidence {} = True
delayingAction HardForkInitiation {} = True
delayingAction UpdateCommittee {} = True
delayingAction NewConstitution {} = True
delayingAction TreasuryWithdrawals {} = False
delayingAction ParameterChange {} = False
delayingAction InfoAction {} = False
```

When a delaying action is enacted:
- `rsDelayed` flag is set to `True`
- No other proposals can be ratified in the same epoch
- Active proposals have their expiry extended by one epoch

#### Action Priority (Processing Order)

```haskell
-- From actionPriority
actionPriority :: GovAction era -> Int
actionPriority NoConfidence {} = 0        -- Highest priority
actionPriority UpdateCommittee {} = 1
actionPriority NewConstitution {} = 2
actionPriority HardForkInitiation {} = 3
actionPriority ParameterChange {} = 4
actionPriority TreasuryWithdrawals {} = 5
actionPriority InfoAction {} = 6          -- Lowest priority
```

---

### Expiry and Activity Rules

#### DRep Expiry

**Reference**: `DRep.hs` & `Rules/GovCert.hs:219-233`

```haskell
-- DRepState contains expiry
data DRepState = DRepState
  { drepExpiry :: !EpochNo     -- Epoch when DRep expires
  , drepAnchor :: !(StrictMaybe Anchor)
  , drepDeposit :: !Coin
  , drepDelegs :: !(Set (Credential Staking))
  }
```

**DRep Activity Rules**:
- DReps have `ppDRepActivityL` epochs to remain active
- Activity is refreshed when DRep votes or updates registration
- Expired DReps are excluded from voting power calculation
- Dormant epochs (no active proposals) don't count against expiry

#### Committee Member Terms

```haskell
-- Committee definition
data Committee era = Committee
  { committeeMembers :: !(Map (Credential ColdCommitteeRole) EpochNo)  -- member → expiry
  , committeeThreshold :: !UnitInterval                                 -- quorum threshold
  }
```

**Committee Rules**:
- Each member has an expiry epoch
- Expired members cannot vote (treated as abstain)
- `ppCommitteeMinSizeL` defines minimum active members
- If active members < minimum, committee is considered absent

#### Proposal Expiry

```haskell
-- GovActionState contains expiry
data GovActionState era = GovActionState
  { gasId :: !GovActionId
  , gasCommitteeVotes :: !(Map (Credential HotCommitteeRole) Vote)
  , gasDRepVotes :: !(Map (Credential DRepRole) Vote)
  , gasStakePoolVotes :: !(Map (KeyHash StakePool) Vote)
  , gasProposalProcedure :: !(ProposalProcedure era)
  , gasProposedIn :: !EpochNo
  , gasExpiresAfter :: !EpochNo   -- After this epoch, proposal expires
  }
```

**Proposal Lifecycle**:
- `gasExpiresAfter = gasProposedIn + ppGovActionLifetimeL`
- If not ratified by expiry → proposal removed, deposit refunded
- Delaying actions extend all active proposals by one epoch

---

### Voting Power Calculation Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    VOTING POWER CALCULATION                                  │
│                                                                              │
│  AT EPOCH BOUNDARY (via DRepPulser):                                        │
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────────┐│
│  │ STEP 1: Collect Stake Distribution                                       ││
│  │                                                                          ││
│  │ For each stake credential in Accounts:                                   ││
│  │   stake = instant_stake + rewards + proposal_deposits                    ││
│  │                                                                          ││
│  │   IF delegated to DRep:                                                  ││
│  │     dRepDistr[drep] += stake                                             ││
│  │                                                                          ││
│  │   IF delegated to SPO (proposal deposits only):                          ││
│  │     poolDistr[pool] += proposal_deposits                                 ││
│  └─────────────────────────────────────────────────────────────────────────┘│
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────────┐│
│  │ STEP 2: Calculate Ratios (for each proposal)                            ││
│  │                                                                          ││
│  │ DRep Ratio = yes_stake / (total_stake - abstain_stake)                  ││
│  │                                                                          ││
│  │ SPO Ratio = yes_stake / (total_stake - abstain_stake)                   ││
│  │                                                                          ││
│  │ Committee Ratio = yes_votes / (total_votes - abstain_votes)             ││
│  └─────────────────────────────────────────────────────────────────────────┘│
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────────┐│
│  │ STEP 3: Compare Against Thresholds                                       ││
│  │                                                                          ││
│  │ For each required voting body:                                           ││
│  │   IF ratio >= threshold THEN accepted = True                             ││
│  │   IF threshold == 0 THEN accepted = True (auto-pass)                    ││
│  │   IF body not required (NoVotingAllowed) THEN accepted = True           ││
│  └─────────────────────────────────────────────────────────────────────────┘│
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────────┐│
│  │ STEP 4: Final Decision                                                   ││
│  │                                                                          ││
│  │ acceptedByEveryone = committeeAccepted                                   ││
│  │                      && spoAccepted                                      ││
│  │                      && dRepAccepted                                     ││
│  │                                                                          ││
│  │ IF acceptedByEveryone && all other conditions THEN RATIFY               ││
│  └─────────────────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────────────────┘
```

---

### Vote Options

```haskell
data Vote = VoteNo | VoteYes | Abstain
  deriving (Eq, Ord, Show, Generic, Enum, Bounded)
```

| Vote | Effect on Ratio |
|------|-----------------|
| **VoteYes** | Counts toward numerator AND denominator |
| **VoteNo** | Counts toward denominator only |
| **Abstain** | Excluded from calculation entirely |

**Key Insight**: Abstaining removes your stake from the calculation, making it easier for Yes votes to reach the threshold (denominator shrinks).

---

### Bootstrap Phase Special Rules

During the bootstrap phase (protocol version < 10), special rules apply:

```haskell
-- From hardforkConwayBootstrapPhase
hardforkConwayBootstrapPhase :: ProtVer -> Bool
hardforkConwayBootstrapPhase pv = pvMajor pv < natVersion @10
```

**Bootstrap Phase Differences**:

| Aspect | Bootstrap Phase | Post-Bootstrap |
|--------|----------------|----------------|
| DRep thresholds | All reset to 0 (except InfoAction) | From PParams |
| SPO default vote | Abstain (if not voted) | Based on DRep delegation |
| Active DReps required | No | Yes |

```haskell
-- DRep thresholds during bootstrap (from votingDRepThresholdInternal)
let thresholds
      | hardforkConwayBootstrapPhase (pp ^. ppProtocolVersionL) = def  -- All zeros!
      | otherwise = pp ^. ppDRepVotingThresholdsL
```

---

### Summary: Voting System

| Component | Calculation | Threshold Source |
|-----------|-------------|------------------|
| **Committee** | Yes_count / (Total - Abstain) | Committee definition |
| **DReps** | Yes_stake / (Total_stake - Abstain_stake) | `DRepVotingThresholds` in PParams |
| **SPOs** | Yes_stake / (Total_stake - Abstain_stake) | `PoolVotingThresholds` in PParams |

**Ratification requires ALL applicable groups to meet their thresholds simultaneously.**
