# Conway DRep Delegator Lifecycle

This document describes how DRep (Delegated Representative) delegators work in the Conway era: registration, delegation, re-delegation, unregistration, and what happens when a DRep is deregistered. It is based on the Haskell ledger implementation with explicit file and line references.

---

## 1. Overview: Two Sides of DRep State

DRep-related state is split into two places:

| Location | What it stores | Haskell reference |
|----------|-----------------|--------------------|
| **Accounts (DState)** | Per **stake credential**: current DRep (or none), pool, balance, deposit | `ConwayAccountState`, `casDRepDelegation` |
| **VState (vsDReps)** | Per **DRep credential**: `DRepState` (expiry, anchor, deposit, **set of delegators**) | `VState`, `vsDReps`, `DRepState.drepDelegs` |

**Invariant (correct behavior):** For every stake credential that has a DRep delegation, that credential must appear in exactly one DRep’s `drepDelegs` set. When a user changes DRep, the ledger must remove them from the old DRep’s set and add them to the new DRep’s set.

**References:**
- `libs/cardano-ledger-core/src/Cardano/Ledger/DRep.hs` — `DRep`, `DRepState`, `drepDelegs`
- `eras/conway/impl/src/Cardano/Ledger/Conway/State/Account.hs` — `ConwayAccountState`, `casDRepDelegation`, `dRepDelegationAccountStateL`
- `eras/conway/impl/src/Cardano/Ledger/Conway/State/VState.hs` — `VState`, `vsDReps`, `unDelegReDelegDRep`

---

## 2. DRep and DRepState (Core Types)

**Reference:** `libs/cardano-ledger-core/src/Cardano/Ledger/DRep.hs:63-69, 148-156`

```haskell
data DRep
  = DRepKeyHash !(KeyHash DRepRole)
  | DRepScriptHash !ScriptHash
  | DRepAlwaysAbstain
  | DRepAlwaysNoConfidence
  deriving (Show, Eq, Ord, ...)

data DRepState = DRepState
  { drepExpiry   :: !EpochNo
  , drepAnchor   :: !(StrictMaybe Anchor)
  , drepDeposit  :: !(CompactForm Coin)
  , drepDelegs   :: !(Set (Credential Staking))  -- delegators currently delegated to this DRep
  }
```

**Plain English:**
- **DRep** is who you delegate your **voting power** to: a key, a script, or the virtual DReps “AlwaysAbstain” / “AlwaysNoConfidence”.
- **DRepState** is the on-chain record for a **registered** DRep: expiry, optional anchor, deposit, and the **set of stake credentials** that are currently delegating to this DRep (`drepDelegs`).

---

## 3. Certificate Processing Order (CERTS → CERT → DELEG / GOVCERT)

Certificates in a transaction are processed **one by one**, in **sequence**. Each cert is dispatched by type:

**Reference:** `eras/conway/impl/src/Cardano/Ledger/Conway/Rules/Certs.hs:218-250`  
`eras/conway/impl/src/Cardano/Ledger/Conway/Rules/Cert.hs:219-231`

- **CERTS** rule: signal = `Seq (TxCert era)`. For each cert it calls the **CERT** rule with the **current** cert and the **accumulated** cert state so far.
- **CERT** rule: on each `TxCert`:
  - `ConwayTxCertDeleg delegCert` → **DELEG** (stake registration, unregistration, delegation)
  - `ConwayTxCertPool poolCert` → **POOL**
  - `ConwayTxCertGov govCert` → **GOVCERT** (RegDRep, UnRegDRep, UpdateDRep, committee certs)

So: **delegation certs (DELEG) and governance certs (GOVCERT) are processed in the same sequence as they appear in the transaction.** Order within a tx matters.

---

## 4. DRep Delegator Lifecycle (Step by Step)

### 4.1 Registering a stake credential and delegating to a DRep

**Certificates:** `ConwayRegDelegCert stakeCred delegatee deposit` (register + delegate in one) or `ConwayRegCert` then `ConwayDelegCert`.

**Reference:** `eras/conway/impl/src/Cardano/Ledger/Conway/Rules/Deleg.hs:276-303`

- **ConwayRegDelegCert:** DELEG rule checks deposit, “stake key not already registered”, and “delegatee valid” (pool registered if present, DRep registered or virtual). Then it calls `processDelegationInternal` with the **new** account and the given `Delegatee`.
- **ConwayDelegCert:** DELEG rule looks up the stake credential’s account; if present, calls `processDelegationInternal` with that account and the new `Delegatee`.

**Reference:** `eras/conway/impl/src/Cardano/Ledger/Conway/Rules/Deleg.hs:321-357`

```haskell
processDelegationInternal preserveIncorrectDelegation stakeCred mAccountState newDelegatee =
  case newDelegatee of
    DelegStake sPool     -> delegStake sPool
    DelegVote dRep       -> delegVote dRep
    DelegStakeVote sPool dRep -> delegVote dRep . delegStake sPool
  where
    delegVote dRep cState =
      cState
        & certDStateL . accountsL %~ adjustAccountState (dRepDelegationAccountStateL ?~ dRep) stakeCred
        & maybe
            (certVStateL %~ insertDRepDeleg dRep)
            (\accountState -> certVStateL %~ unDelegReDelegDRep stakeCred accountState (Just dRep))
            (guard (not preserveIncorrectDelegation) >> mAccountState)
    insertDRepDeleg dRep = case dRep of
      DRepCredential dRepCred ->
        vsDRepsL %~ Map.adjust (drepDelegsL %~ Set.insert stakeCred) dRepCred
      _ -> id
```

**Plain English:**
1. **Account (DState):** The stake credential’s account is updated so its **DRep delegation** is set to the new DRep (`dRepDelegationAccountStateL ?~ dRep`).
2. **VState (vsDReps):**
   - If the DRep is a **credential** (KeyHash/ScriptHash), the credential is added to that DRep’s `drepDelegs` set (`insertDRepDeleg`).
   - If we have a **previous** account state and we are **not** preserving the buggy behavior (`preserveIncorrectDelegation` is False), we also call **`unDelegReDelegDRep stakeCred accountState (Just dRep)`**: remove this stake credential from the **old** DRep’s `drepDelegs` and, if the new DRep is a credential, add it to the new DRep’s set.

So under **correct** behavior (see below for the bug), changing DRep updates both the account and both DReps’ delegator sets.

---

### 4.2 Removing delegation from the old DRep: `unDelegReDelegDRep`

**Reference:** `eras/conway/impl/src/Cardano/Ledger/Conway/State/VState.hs:115-142`

```haskell
unDelegReDelegDRep ::
  ConwayEraAccounts era =>
  Credential Staking ->
  AccountState era ->
  Maybe DRep ->        -- Nothing = unregister / remove vote; Just dRep = new DRep
  VState era ->
  VState era
unDelegReDelegDRep stakeCred accountState mNewDRep =
  fromMaybe (vsDRepsL %~ addNewDelegation) $ do
    dRep@(DRepCredential dRepCred) <- accountState ^. dRepDelegationAccountStateL
    pure $
      if Just dRep == mNewDRep
        then id
        else
          vsDRepsL %~ addNewDelegation . Map.adjust (drepDelegsL %~ Set.delete stakeCred) dRepCred
  where
    addNewDelegation =
      case mNewDRep of
        Just (DRepCredential dRepCred) ->
          Map.adjust (drepDelegsL %~ Set.insert stakeCred) dRepCred
        _ -> id
```

**Plain English:**
- If the account **has no** DRep credential (e.g. Abstain/NoConfidence or no vote), only `addNewDelegation` runs (add to new DRep if it’s a credential).
- If the account **has** a DRep credential:
  - If the **new** DRep is the **same** as the current one, nothing is done.
  - Otherwise: **remove** this stake credential from the **current** DRep’s `drepDelegs`, then **add** it to the **new** DRep’s `drepDelegs` (if the new one is a credential).

So `unDelegReDelegDRep` keeps the invariant: each delegator appears in at most one credential DRep’s `drepDelegs`.

---

### 4.3 Unregistering a stake credential

**Reference:** `eras/conway/impl/src/Cardano/Ledger/Conway/Rules/Deleg.hs:228-280`

When processing **ConwayUnRegCert**:
- The account is removed from the accounts map.
- **VState** is updated with `unDelegReDelegDRep stakeCred accountState **Nothing**`.

So: the credential is **removed** from its current DRep’s `drepDelegs` (and not added to any other). The DRep’s delegator set stays consistent.

---

## 5. What Happens When a DRep Is Deregistered (UnRegDRep)

**Reference:** `eras/conway/impl/src/Cardano/Ledger/Conway/Rules/GovCert.hs:237-257`

```haskell
ConwayUnRegDRep cred refund -> do
  let mDRepState = Map.lookup cred (certState ^. certVStateL . vsDRepsL)
      ...
  isJust mDRepState ?! (injectFailure . ConwayDRepNotRegistered) cred
  failOnJust drepRefundMismatch $ ...
  let
    certState' =
      certState & certVStateL . vsDRepsL %~ Map.delete cred
    clearDRepDelegations delegs accountsMap =
      foldr (Map.adjust (dRepDelegationAccountStateL .~ Nothing)) accountsMap delegs
  pure $
    case mDRepState of
      Nothing -> certState'
      Just dRepState ->
        certState'
          & certDStateL . accountsL . accountsMapL
            %~ clearDRepDelegations (drepDelegs dRepState)
```

**Plain English:**
1. The DRep credential is **removed** from `vsDReps` (`Map.delete cred`).
2. For **every** stake credential in that DRep’s **current** delegator set (`drepDelegs dRepState`), the ledger **clears** their DRep delegation in the accounts map: `dRepDelegationAccountStateL .~ Nothing`.

So: when a DRep unregisters, **only** the accounts that are **currently** listed in that DRep’s `drepDelegs` have their DRep field set to “no DRep”. Anyone who had already switched to another DRep should, under correct logic, **not** be in `drepDelegs` anymore, so they are **not** cleared.

The bug (next section) is that under protocol version &lt; 10, when you switch DRep, the ledger **does not** remove you from the old DRep’s `drepDelegs`. So when that old DRep later unregisters, you are still in its `drepDelegs` and your (correct) delegation to the new DRep gets wrongly cleared.

---

## 6. Protocol Version &lt; 10 Bug (#4772): Delegation Cleared After Switching DRep

**Reference:** `eras/conway/impl/src/Cardano/Ledger/Conway/Rules/Deleg.hs:288-290, 300, 319-324, 334, 350-354`

For **ConwayDelegCert** and **ConwayRegDelegCert**, the code passes a boolean into `processDelegationInternal`:

```haskell
processDelegationInternal (pvMajor pv < natVersion @10) stakeCred ...
```

So when **protocol version major &lt; 10** (e.g. version 9), that boolean is **True** and is used as **`preserveIncorrectDelegation`**.

In `delegVote`:

```haskell
delegVote dRep cState =
  cState
    & certDStateL . accountsL %~ adjustAccountState (dRepDelegationAccountStateL ?~ dRep) stakeCred
    & maybe
        (certVStateL %~ insertDRepDeleg dRep)
        (\accountState -> certVStateL %~ unDelegReDelegDRep stakeCred accountState (Just dRep))
        (guard (not preserveIncorrectDelegation) >> mAccountState)
```

- **Account** is always updated: the stake credential’s DRep delegation is set to the new DRep.
- **VState** update:
  - The third argument to `maybe` is `(guard (not preserveIncorrectDelegation) >> mAccountState)`.
  - When **`preserveIncorrectDelegation` is True** (pv &lt; 10), that expression is **Nothing** (because `guard False >> x` is Nothing). So we take the **first** branch of `maybe`: **only** `insertDRepDeleg dRep` runs. We **do not** call `unDelegReDelegDRep`.
  - So we **add** the stake credential to the **new** DRep’s `drepDelegs`, but we **never remove** it from the **old** DRep’s `drepDelegs`.

**Consequence:**
- Under **pv &lt; 10**: If you first delegate to DRep A, then later submit a cert delegating to DRep B, your **account** correctly shows DRep B, and you are **added** to B’s `drepDelegs`. But you remain in **A’s** `drepDelegs` (bug).
- When **DRep A** is later **unregistered**, GovCert runs `clearDRepDelegations (drepDelegs dRepState)`. That set still contains **everyone who ever delegated to A**, including you. So the ledger **clears your DRep delegation** in the accounts map (`dRepDelegationAccountStateL .~ Nothing`), even though you had already switched to B. You end up with **no** DRep instead of staying with B.

**Fixed from protocol version 10:** For `pvMajor pv >= natVersion @10`, `preserveIncorrectDelegation` is False. Then we pass the current account state and call `unDelegReDelegDRep`, so when you switch from A to B you are removed from A’s `drepDelegs`. When A unregisters, you are no longer in A’s set, so your delegation to B is left unchanged.

**References:**
- Bug flag: `Deleg.hs:324` — “Preserve the buggy behavior where DRep delegations are not updated correctly (See #4772)”
- Version check: `Deleg.hs:290`, `300` — `pvMajor pv < natVersion @10`
- Same version used elsewhere: `eras/conway/impl/src/Cardano/Ledger/Conway/Era.hs` (e.g. `hardforkConwayDELEGIncorrectDepositsAndRefunds`, `hardforkConwayMoveWithdrawalsAndDRepChecksToLedgerRule` use `natVersion @10`)

---

## 7. Summary Table

| Event | Account (DState) | VState (vsDReps) |
|-------|------------------|------------------|
| Delegate to DRep B (first time) | Set `casDRepDelegation = B` | Add cred to B’s `drepDelegs` |
| Switch from DRep A to B (pv ≥ 10) | Set `casDRepDelegation = B` | Remove cred from A’s `drepDelegs`; add to B’s `drepDelegs` |
| Switch from DRep A to B (pv &lt; 10, bug) | Set `casDRepDelegation = B` | Only add cred to B’s `drepDelegs`; **stay in A’s** `drepDelegs` |
| Unregister stake cred | Remove account | Remove cred from current DRep’s `drepDelegs` via `unDelegReDelegDRep _ _ Nothing` |
| UnRegDRep (DRep A) | For each cred in A’s `drepDelegs`: set `casDRepDelegation = Nothing` | Delete A from `vsDReps` |

Under the pv &lt; 10 bug, “each cred in A’s `drepDelegs`” includes people who had already switched to another DRep, so they are incorrectly cleared.

---

## 8. Haskell File Reference Index

| Topic | File | Key symbols |
|-------|------|-------------|
| DRep, DRepState, drepDelegs | `libs/cardano-ledger-core/src/Cardano/Ledger/DRep.hs` | `DRep`, `DRepState`, `drepDelegs`, `drepDelegsL` |
| Account DRep field | `eras/conway/impl/src/Cardano/Ledger/Conway/State/Account.hs` | `ConwayAccountState`, `casDRepDelegation`, `dRepDelegationAccountStateL` |
| VState, unDelegReDelegDRep | `eras/conway/impl/src/Cardano/Ledger/Conway/State/VState.hs` | `VState`, `vsDReps`, `unDelegReDelegDRep` |
| DELEG rule, processDelegationInternal, bug flag | `eras/conway/impl/src/Cardano/Ledger/Conway/Rules/Deleg.hs` | `processDelegationInternal`, `preserveIncorrectDelegation`, `delegVote`, `insertDRepDeleg` |
| UnRegDRep, clearDRepDelegations | `eras/conway/impl/src/Cardano/Ledger/Conway/Rules/GovCert.hs` | `ConwayUnRegDRep`, `clearDRepDelegations`, `drepDelegs dRepState` |
| CERT dispatch (DELEG vs GOVCERT) | `eras/conway/impl/src/Cardano/Ledger/Conway/Rules/Cert.hs` | `certTransition`, `ConwayTxCertDeleg`, `ConwayTxCertGov` |
| CERTS sequence | `eras/conway/impl/src/Cardano/Ledger/Conway/Rules/Certs.hs` | `conwayCertsTransition` |
| Protocol version 10 | `eras/conway/impl/src/Cardano/Ledger/Conway/Era.hs` | `natVersion @10` |
| DRepPulser note on #4772 | `eras/conway/impl/src/Cardano/Ledger/Conway/Governance/DRepPulser.hs` | Comment re #4772 |
