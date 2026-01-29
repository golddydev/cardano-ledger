# Shelley Era Timing: A Comprehensive Guide

This document explains in detail how timing works in the Cardano Shelley era, covering epoch boundaries, pool retirement, snapshots, reward calculation, and all the related mechanisms.

## Table of Contents

1. [Overview](#overview)
2. [The Slot and Epoch Concepts](#the-slot-and-epoch-concepts)
3. [The TICK Transition: Every Slot Processing](#the-tick-transition-every-slot-processing)
4. [The NEWEPOCH Transition: Epoch Boundary](#the-newepoch-transition-epoch-boundary)
5. [The EPOCH Rule: Snapshots and Pools](#the-epoch-rule-snapshots-and-pools)
6. [Snapshot Mechanisms (SNAP)](#snapshot-mechanisms-snap)
7. [Pool Retirement (POOLREAP)](#pool-retirement-poolreap)
8. [Reward Calculation (RUPD)](#reward-calculation-rupd)
9. [Complete Timing Flow](#complete-timing-flow)
10. [Key Timing Parameters](#key-timing-parameters)
11. [Visual Timeline](#visual-timeline)

---

## Overview

The Shelley era ledger operates on a **slot-by-slot basis**, where each slot represents a fixed period of time (1 second in Cardano). Multiple slots are grouped into **epochs**, which are the primary unit for major state transitions like:

- Taking stake snapshots
- Calculating and distributing rewards
- Retiring stake pools
- Updating protocol parameters

Understanding the timing means understanding **when** these events happen and **how** they coordinate.

---

## The Slot and Epoch Concepts

### What is a Slot?

A **slot** is the fundamental unit of time in Cardano:
- Each slot is 1 second long
- Slots are numbered consecutively: 0, 1, 2, 3, ...
- In each slot, at most one block can be produced (though some slots may be empty)

### What is an Epoch?

An **epoch** is a group of slots:
- In mainnet, 1 epoch = 432,000 slots = 5 days
- Epochs are numbered: 0, 1, 2, 3, ...
- Major state transitions happen at epoch boundaries (when one epoch ends and the next begins)

### Why Epochs Matter

Epochs provide a regular rhythm for:
- **Stake snapshots**: Capturing who has staked what amount
- **Reward distribution**: Paying staking rewards based on previous performance
- **Pool management**: Allowing pools to retire or update parameters
- **Protocol updates**: Activating new protocol parameter changes

---

## The TICK Transition: Every Slot Processing

**File**: `eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Tick.hs`

**Key Functions**:
- [`bheadTransition`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Tick.hs:261) - Main TICK transition function
- [`validatingTickTransition`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Tick.hs:177) - Full validation with epoch boundary handling
- [`solidifyNextEpochPParams`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Tick.hs:151) - Finalize future protocol parameters
- [`adoptGenesisDelegs`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Tick.hs:118) - Activate scheduled genesis delegations

The TICK rule is the "heartbeat" of the ledger. It processes **every single slot**, regardless of whether a block is produced.

### What TICK Does

Think of TICK as a clock that ticks once per slot and checks:

1. **"Has the epoch changed?"**
   - If yes → trigger NEWEPOCH transition (see next section)
   - If no → continue with regular slot processing

2. **"Should we solidify future protocol parameters?"**
   - Once we pass a certain point in the epoch (the "point of no return"), the protocol parameters for the next epoch become final and can no longer be changed
   - This ensures stability and predictability

3. **"Are there any genesis delegations to activate?"**
   - Genesis delegations are special delegation certificates scheduled to activate at specific slots
   - TICK checks if any are scheduled for this slot and activates them

4. **"Should we pulse the reward calculation?"**
   - Rewards aren't calculated instantly; instead, they're computed incrementally over many slots
   - TICK triggers one "pulse" of the calculation each slot (more on this later)

5. **"Should we force evaluation of snapshots?"**
   - After a stability window passes, TICK forces the stake snapshots to be fully computed
   - This prevents expensive computations during the actual epoch transition

### Key Function: `bheadTransition`

This is the main TICK function. Here's what it does in plain language:

```
For each slot:
  1. Check if we're transitioning to a new epoch
     - If yes, run NEWEPOCH transition

  2. Check if we've passed the "point of no return" for next epoch's parameters
     - If yes, mark future parameters as solidified (can't change anymore)

  3. Check for genesis delegations scheduled for this slot
     - Move any scheduled delegations from "future" to "current"

  4. Pulse the reward calculation
     - Do a small chunk of the reward computation

  5. Force evaluation of stake snapshots (if past stability window)
     - Make sure snapshots are computed before we need them at epoch boundary

  6. Return the updated ledger state
```

### Why TICK Runs Every Slot

TICK needs to run every slot (not just when blocks are produced) because:
- Epoch boundaries occur at specific slot numbers, regardless of blocks
- Reward calculations need to progress incrementally
- Genesis delegations are scheduled by slot number
- Parameter solidification depends on reaching specific slots

---

## The NEWEPOCH Transition: Epoch Boundary

**File**: `eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/NewEpoch.hs`

**Key Functions**:
- [`newEpochTransition`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/NewEpoch.hs:150) - Main epoch boundary transition
- [`updateRewards`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/NewEpoch.hs:235) - Apply calculated rewards to accounts

NEWEPOCH is triggered by TICK when we cross from one epoch to the next. This is where all the major epoch-boundary work happens.

### When NEWEPOCH Triggers

```
Current epoch number = e
Last processed epoch = e_last

If e ≠ (e_last + 1):
  → We've crossed an epoch boundary
  → Run NEWEPOCH transition
```

### What NEWEPOCH Does

Think of NEWEPOCH as the "new year's eve" of the ledger. At midnight (the epoch boundary), several important things happen:

#### Step 1: Complete Reward Calculation (RUPD)

If rewards were being calculated incrementally (pulsing), we need to finish that calculation now:

```
If reward calculation is still in progress:
  - Force it to complete immediately
  - Get the final reward amounts for all stake accounts
```

#### Step 2: Apply Rewards to Accounts

Now that we have the final reward amounts, add them to everyone's stake accounts:

```
For each stake account:
  current_balance = current_balance + calculated_reward
```

This is when stakers actually receive their rewards!

#### Step 3: Process Instantaneous Reserves (MIR Rule)

**File**: [`Mir.hs`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Mir.hs)

MIR stands for "Move Instantaneous Reserves". This handles special treasury operations.

**Important**: MIR certificates come from transactions submitted during the **previous epoch** (epoch N-1). When we're at the epoch boundary transitioning from N-1 to N, the MIR rule processes the accumulated MIR instructions that were collected in `dsIRewards` during epoch N-1.

**How MIR Works**:

1. **During Epoch N-1** (Certificate Submission via DELEG rule):
   - Governance submits MIRCert certificates in transactions
   - These must be submitted before the stability window (to prevent last-minute manipulation)
   - Each MIRCert updates the `dsIRewards` (InstantaneousRewards) in the DState
   - MIRCerts can specify:
     - `StakeAddressesMIR`: Pay specific amounts to specific stake accounts
     - `SendToOppositePotMIR`: Transfer ADA from reserves to treasury (or vice versa)

2. **At Epoch Boundary (N-1 → N)** (MIR Rule Execution):

   **File**: [`Mir.hs:94-158`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Mir.hs:94)

   The MIR rule is triggered by NEWEPOCH at the epoch boundary ([`NewEpoch.hs:167`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/NewEpoch.hs:167)):
   ```haskell
   es'' <- trans @(EraRule "MIR" era) $ TRC ((), es', ())
   ```

   **Haskell Implementation** (`mirTransition`):
   ```haskell
   mirTransition = do
     TRC (_, es@EpochState { esChainAccountState = chainAccountState, ... }, ()) <- judgmentContext
     let ds = dpState ^. certDStateL
         reserves = casReserves chainAccountState
         treasury = casTreasury chainAccountState
         -- Get pending MIR rewards, but ONLY for accounts that still exist
         irwdR = iRReserves (dsIRewards ds) `Map.intersection` accountsMap
         irwdT = iRTreasury (dsIRewards ds) `Map.intersection` accountsMap
         totR = fold irwdR  -- Total to pay from reserves
         totT = fold irwdT  -- Total to pay from treasury
         -- Account for any pot-to-pot transfers (delta adjustments)
         availableReserves = reserves `addDeltaCoin` deltaReserves (dsIRewards ds)
         availableTreasury = treasury `addDeltaCoin` deltaTreasury (dsIRewards ds)
         -- Combined update map for all accounts
         update = Map.unionWith (<>) irwdR irwdT

     if totR <= availableReserves && totT <= availableTreasury
       then do
         -- SUCCESS: Sufficient funds exist
         tellEvent $ MirTransfer (...)
         pure $ EpochState
           ChainAccountState
             { casReserves = availableReserves <-> totR  -- Deduct from reserves
             , casTreasury = availableTreasury <-> totT  -- Deduct from treasury
             }
           ( ls
               -- Add rewards to stake accounts
               & lsCertStateL . certDStateL . accountsL
                 %~ addToBalanceAccounts (Map.map compactCoinOrError update)
               -- Clear the pending MIR rewards
               & lsCertStateL . certDStateL . dsIRewardsL .~ emptyInstantaneousRewards
           )
           ...
       else do
         -- FAILURE: Insufficient funds - just clear without transfer
         tellEvent $ NoMirTransfer (...) availableReserves availableTreasury
         pure $ EpochState
           chainAccountState  -- Unchanged
           ( ls
               -- Still clear the pending rewards (no transfer happens)
               & lsCertStateL . certDStateL . dsIRewardsL .~ emptyInstantaneousRewards
           )
           ...
   ```

   **Key Operations at Epoch Boundary**:
   - Reads the accumulated `dsIRewards` from DState
   - Filters to only include accounts that still exist (`Map.intersection accountsMap`)
   - Checks if sufficient funds exist in reserves/treasury
   - If sufficient funds:
     - Transfers ADA from reserves/treasury to specified stake accounts via `addToBalanceAccounts`
     - Updates `casReserves` and `casTreasury` to reflect the withdrawals
     - Clears `dsIRewards` (resets to `emptyInstantaneousRewards`)
     - Emits `MirTransfer` event
   - If insufficient funds:
     - Just clears `dsIRewards` without making transfers
     - Emits `NoMirTransfer` event with available amounts

**Key Constraint**: MIR certificates can only be submitted during the first part of an epoch (before the stability window ends), enforced by the `checkSlotNotTooLate` function in the DELEG rule.

#### The `checkSlotNotTooLate` Function

**File**: [`Deleg.hs:380-397`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Deleg.hs:380)

This function enforces the critical timing constraint that prevents MIR certificates from being submitted too close to the epoch boundary.

**Haskell Implementation**:

```haskell
checkSlotNotTooLate ::
  ( EraCertState era
  , ShelleyEraAccounts era
  , ShelleyEraTxCert era
  , EraPParams era
  , AtMostEra "Babbage" era
  ) =>
  SlotNo ->
  EpochNo ->
  Rule (ShelleyDELEG era) 'Transition ()
checkSlotNotTooLate slot curEpochNo = do
  sp <- liftSTS $ asks stabilityWindow
  ei <- liftSTS $ asks epochInfoPure
  let firstSlot = epochInfoFirst ei newEpoch
      tooLate = firstSlot *- Duration sp
      newEpoch = addEpochInterval curEpochNo (EpochInterval 1)
  tellEvent (DelegNewEpoch newEpoch)
  slot < tooLate ?! MIRCertificateTooLateinEpochDELEG (Mismatch slot tooLate)
```

**Step-by-Step Breakdown**:

1. **Get the Stability Window** (`sp <- liftSTS $ asks stabilityWindow`):
   - The stability window is a global parameter representing slots before epoch boundary where chain is considered "stable"
   - On mainnet: approximately 3,600 slots (~1 hour)
   - Computed from security parameter `k` and active slot coefficient `f`: `stabilityWindow = 3k/f`

2. **Get Epoch Info** (`ei <- liftSTS $ asks epochInfoPure`):
   - Retrieves epoch information structure for slot/epoch conversions

3. **Calculate Next Epoch's First Slot**:
   ```haskell
   newEpoch = addEpochInterval curEpochNo (EpochInterval 1)  -- next epoch
   firstSlot = epochInfoFirst ei newEpoch                     -- first slot of next epoch
   ```

4. **Calculate the "Too Late" Threshold**:
   ```haskell
   tooLate = firstSlot *- Duration sp
   ```
   The `*-` operator subtracts a duration from a slot (from [`Slot.hs:56-57`](libs/cardano-ledger-core/src/Cardano/Ledger/Slot.hs:56)):
   ```haskell
   (*-) :: SlotNo -> Duration -> SlotNo
   (SlotNo s) *- (Duration d) = SlotNo (if s > d then s - d else 0)
   ```
   So `tooLate` = first slot of next epoch - stability window

5. **Enforce the Constraint**:
   ```haskell
   slot < tooLate ?! MIRCertificateTooLateinEpochDELEG (Mismatch slot tooLate)
   ```
   If current slot ≥ `tooLate`, the transaction fails with `MIRCertificateTooLateinEpochDELEG`

**Visual Timeline**:

```
Epoch N:
├─ Slot 0 ────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  [═══════════════════════ SAFE ZONE ═══════════════════]  [══ FORBIDDEN ══] │
│                                                           ↑                  │
│                                                    tooLate slot              │
│                                             (firstSlotNextEpoch - sp)        │
├──────────────────────────────────────────────────────────────────────────────┤
                                                            │← sp slots →│
                                                                               │
Epoch N+1:                                                                     ↓
├─ Slot 0 ─────────────────────────────────────────────────────────────────────┤
```

**Mainnet Example** (with actual values):
- Current epoch: 300
- Epoch length: 432,000 slots (5 days)
- Stability window (`sp`): ~3,600 slots (~1 hour)
- First slot of epoch 301: 130,032,000
- `tooLate` = 130,032,000 - 3,600 = **130,028,400**
- MIR certificates submitted at slot ≥ 130,028,400 in epoch 300 will be **rejected**

**Why This Constraint Exists**:

1. **Consensus Stability**: The stability window represents time needed for chain consensus to be "settled". Within this window, chain rollbacks are theoretically possible (up to k blocks). If a MIR certificate were included in a block that gets rolled back near the epoch boundary, the ledger state at the boundary could be inconsistent.

2. **Predictable Epoch Boundary Processing**: MIR certificates are "IOUs" settled at epoch boundaries. The system needs to know the complete set of pending MIRs well before the epoch ends to:
   - Calculate total amounts to be moved from reserves/treasury
   - Ensure sufficient funds exist
   - Update stake addresses with their rewards

3. **Preventing Gaming**: Without this constraint, a malicious actor could try to submit MIR certificates very late in an epoch, exploit timing differences between nodes, and cause chain forks.

**Error Type** ([`Deleg.hs:108-109`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Deleg.hs:108)):

```haskell
| MIRCertificateTooLateinEpochDELEG
    (Mismatch RelLT SlotNo)
```

The `Mismatch` provides:
- `mismatchSupplied`: The actual slot of the transaction
- `mismatchExpected`: The `tooLate` threshold that should NOT have been reached

**Era Constraint** (`AtMostEra "Babbage"`):

This function only applies through the Babbage era. In **Conway era**, MIR certificates are **deprecated** and replaced by governance actions, so this timing constraint no longer applies to MIRs.

**Example Timeline**:
```
Epoch N-1:
  Slot 100: Governance submits MIRCert to pay 1000 ADA to stake_addr_1
    → Updates dsIRewards.iRReserves[stake_addr_1] = 1000 ADA
  Slot 200: Governance submits MIRCert to pay 500 ADA to stake_addr_2
    → Updates dsIRewards.iRReserves[stake_addr_2] = 500 ADA
  Slot 3,600: Stability window ends
    → No more MIR certificates allowed for this epoch

Epoch N-1 → N boundary:
  MIR rule executes:
    → Checks reserves have >= 1500 ADA
    → Transfers 1000 ADA to stake_addr_1
    → Transfers 500 ADA to stake_addr_2
    → Clears dsIRewards (reset to empty)

Epoch N:
  → New MIR certificates can be submitted for processing at N → N+1 boundary
```

**Data Structure**: [`InstantaneousRewards`](libs/cardano-ledger-core/src/Cardano/Ledger/State/CertState.hs:134)
```haskell
InstantaneousRewards:
  - iRReserves: Map of (stake account → ADA amount) to pay from reserves
  - iRTreasury: Map of (stake account → ADA amount) to pay from treasury
  - deltaReserves: Net change to reserves (for pot-to-pot transfers)
  - deltaTreasury: Net change to treasury (for pot-to-pot transfers)
```

#### Step 4: Run the EPOCH Rule

This is the big one! The EPOCH rule handles three major sub-tasks:
- **SNAP**: Take new stake snapshots
- **POOLREAP**: Retire pools that are scheduled to retire this epoch
- **UPEC**: Activate pending protocol parameter updates

(We'll cover each of these in detail below)

#### Step 5: Update Epoch State

After all the above, update the ledger state:

```
- Set last_processed_epoch = current_epoch
- Move "current epoch blocks" to "previous epoch blocks"
- Reset current epoch block counter to 0
- Update pool distribution for leader selection
```

### The NewEpochState Data Structure

**Data Type Definition**: [`NewEpochState`](eras/shelley/impl/src/Cardano/Ledger/Shelley/LedgerState/Types.hs:309)

Here's what the ledger tracks for epoch boundaries:

```haskell
NewEpochState:
  - nesEL: Last epoch we processed
  - nesBprev: How many blocks each pool made in the previous epoch
  - nesBcur: How many blocks each pool has made so far in current epoch
  - nesEs: The main epoch state (accounts, pools, snapshots, etc.)
  - nesRu: Current status of reward calculation (in progress or complete)
  - nesPd: Pool stake distribution (used to determine who can make blocks)
```

---

## The EPOCH Rule: Snapshots and Pools

**File**: `eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Epoch.hs`

**Key Function**:
- [`epochTransition`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Epoch.hs:143) - Coordinates SNAP, POOLREAP, and UPEC sub-rules

The EPOCH rule runs as part of NEWEPOCH and coordinates three important sub-rules:

### The Three Sub-Rules

1. **SNAP** (Snapshot)
   - Takes a new snapshot of the current stake distribution
   - Rotates previous snapshots forward in time
   - Computes pool distribution for future epochs

2. **POOLREAP** (Pool Reap)
   - Identifies pools scheduled to retire this epoch
   - Refunds their deposits
   - Removes them from the active pool set
   - Clears delegations to retired pools

3. **UPEC** (Update Epoch Change)
   - Activates pending protocol parameter updates
   - Updates the current protocol parameters to the new values

These run in order: SNAP → POOLREAP → UPEC

---

## Snapshot Mechanisms (SNAP)

**File**: `eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Snap.hs`

**Key Function**:
- [`snapTransition`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Snap.hs:77) - Creates new snapshot and rotates existing ones

Snapshots are "photographs" of the stake distribution at specific points in time. They're crucial for determining rewards and leader selection.

### Why We Need Snapshots

The stake distribution changes constantly as people:
- Delegate to different pools
- Add or withdraw ADA
- Register or deregister stake accounts

But for rewards and leader selection, we need a **stable** stake distribution that doesn't change during an epoch. That's what snapshots provide.

### The Three Snapshots

At any given time, the ledger maintains **three** snapshots:

#### 1. Mark Snapshot (`ssStakeMark`)

- **When created**: At the beginning of each new epoch
- **What it captures**: The stake distribution at that moment
- **What it's used for**: Calculating rewards for the upcoming epoch
- **Example**:
  - Mark snapshot taken at start of epoch 100
  - Used to calculate rewards during epoch 100
  - Those rewards are distributed at the start of epoch 101

#### 2. Set Snapshot (`ssStakeSet`)

- **When created**: This was the Mark snapshot from the previous epoch
- **What it's used for**: Determines reward distribution in the current epoch
- **Example**:
  - At start of epoch 100, Mark becomes Set
  - Set snapshot from epoch 99 determines who gets rewards at start of epoch 100

#### 3. Go Snapshot (`ssStakeGo`)

- **When created**: This was the Set snapshot from the previous epoch
- **What it's used for**: Historical reference (two epochs old)
- **Example**:
  - At start of epoch 100, Set becomes Go
  - Go snapshot is from epoch 98

### Snapshot Rotation at Epoch Boundary

Here's how snapshots rotate at each epoch boundary:

```
Before epoch boundary (epoch 99):
  Mark = snapshot from start of epoch 99
  Set  = snapshot from start of epoch 98
  Go   = snapshot from start of epoch 97

EPOCH BOUNDARY (epoch 99 → 100)

After epoch boundary (epoch 100):
  Mark = NEW snapshot (stake distribution right now at start of epoch 100)
  Set  = OLD Mark (snapshot from start of epoch 99)
  Go   = OLD Set (snapshot from start of epoch 98)
```

This rotation happens **automatically** during the SNAP rule.

### What's in a Snapshot?

**Data Type Definition**: [`SnapShots`](libs/cardano-ledger-core/src/Cardano/Ledger/State/SnapShots.hs:223)

Each snapshot contains:

```
For each stake credential (stake account):
  - The amount of ADA staked
  - Which pool it's delegated to
  - The associated stake pool parameters

Aggregate data:
  - Total ADA staked across all accounts
  - Distribution of stake across all pools
```

### Pool Distribution Calculation

The SNAP rule also computes the **pool distribution** (`ssStakeMarkPoolDistr`):

```
For each active pool:
  - Calculate total stake delegated to this pool
  - Calculate the pool's "relative stake" (its fraction of total stake)
  - Store pool operator's parameters (cost, margin, pledge)
```

This pool distribution is used for:
- **Leader selection**: Determining which pools can make blocks (probability proportional to stake)
- **Reward calculation**: Determining how rewards are split among pools

### Lazy vs. Strict Evaluation

Technical detail (but important for performance):

- **Mark snapshot**: Computed lazily (only when needed)
- **Set snapshot**: Computed strictly (fully evaluated immediately)
- **Go snapshot**: Computed strictly (fully evaluated immediately)

Why the difference?
- Mark is used soon after epoch boundary for reward calculation, so we can delay its full computation
- Set and Go need to be ready immediately for various checks and validations

The Mark snapshot is **forced to be fully computed** after the stability window passes (see "Key Timing Parameters" section).

---

## Pool Retirement (POOLREAP)

**File**: `eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/PoolReap.hs`

**Key Function**:
- [`poolReapTransition`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/PoolReap.hs:134) - Executes scheduled pool retirements and refunds deposits

Pool retirement is a two-phase process: **scheduling** and **execution**.

### Phase 1: Scheduling Retirement (POOL Rule)

**File**: `eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Pool.hs`

**Key Function**:
- [`poolDelegationTransition`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Pool.hs:215) - Handles pool registration and retirement scheduling
- [Retirement validation](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Pool.hs:300) - Checks retirement epoch constraints

When a pool operator wants to retire their pool, they submit a **RetirePool certificate** to the blockchain.

#### Requirements for Scheduling

1. **Pool must be registered**
   - Can't retire a pool that doesn't exist

2. **Retirement must be in the future**
   ```
   retirement_epoch > current_epoch
   ```
   - Can't retire in the current or past epochs

3. **Retirement can't be too far in the future**
   ```
   retirement_epoch ≤ current_epoch + eMax
   ```
   - `eMax` is a protocol parameter (typically 18 epochs)
   - This prevents pools from scheduling retirement centuries in the future

#### What Happens When Scheduling

```
When RetirePool(pool_id, retirement_epoch) is submitted:
  1. Verify pool_id exists in registered pools
  2. Verify current_epoch < retirement_epoch ≤ current_epoch + eMax
  3. Add to retirement schedule:
     psRetiring[pool_id] = retirement_epoch
  4. Pool continues operating normally until retirement_epoch
```

The pool is now scheduled to retire but **continues operating** until the retirement epoch arrives.

### Phase 2: Executing Retirement (POOLREAP Rule)

At the epoch boundary when `current_epoch == retirement_epoch`, the POOLREAP rule:

#### Step 1: Identify Retiring Pools

```
For each pool in psRetiring:
  If psRetiring[pool_id] == current_epoch:
    → This pool retires now
    Add pool_id to "retired" set
```

#### Step 2: Calculate Deposit Refunds

Each pool paid a deposit when registering (typically 500 ADA). Now we refund it:

```
For each retiring pool:
  deposit_amount = pool deposit (from protocol parameters)
  refund_account = pool's registered reward account

  If refund_account is registered and active:
    → Refund deposit to this account
  Else:
    → Send deposit to treasury (unclaimed)
```

#### Step 3: Process Refunds and Unclaimed Deposits

```
refunds = { account → amount } for all claimable deposits
unclaimed_deposits = total deposits for pools with unregistered accounts

Treasury balance += unclaimed_deposits
For each (account, amount) in refunds:
  account.balance += amount
```

#### Step 4: Remove Pool from State

```
For each retiring pool:
  1. Remove from psStakePools (pool parameters)
  2. Remove from psRetiring (retirement schedule)
  3. Clear any pool-specific state
```

#### Step 5: Clear Delegations to Retired Pools

```
For each stake account:
  If delegated_to_pool ∈ retired_pools:
    → Remove delegation (account becomes undelegated)
    → Future rewards won't be affected (rewards were already calculated based on Set snapshot)
```

### Example Timeline

```
Epoch 100: Pool operator submits RetirePool(my_pool, epoch=105)
  → psRetiring[my_pool] = 105
  → Pool continues operating normally

Epoch 101-104: Pool continues making blocks, receiving delegations
  → Business as usual

Epoch 105 boundary: POOLREAP executes
  → Identifies my_pool with psRetiring[my_pool] == 105
  → Refunds deposit to reward account
  → Removes my_pool from registered pools
  → Clears delegations to my_pool
  → Pool is now retired and cannot make blocks

Epoch 106+: Pool is gone from the system
```

### Important Notes

1. **Delegators aren't penalized**: If you're delegated to a retiring pool, your delegation is simply cleared. You can re-delegate to another pool.

2. **Rewards are safe**: Rewards are calculated based on the Set snapshot (from the previous epoch), so retirement doesn't affect already-earned rewards.

3. **Immediate effect**: Once retired, the pool cannot make blocks starting from the first slot of the next epoch.

4. **Re-registration allowed**: A retired pool can be re-registered with a new registration certificate (paying the deposit again).

---

## Reward Calculation (RUPD)

**File**: `eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Rupd.hs`

**Key Functions**:
- [`rupdTransition`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Rupd.hs:118) - Manages incremental reward calculation (pulsing)
- [`determineRewardTiming`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Rupd.hs:112) - Determines if rewards should start, pulse, or force complete
- [`RewardTiming` data type](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Rupd.hs:109) - States: RewardsTooEarly, RewardsJustRight, RewardsTooLate

Reward calculation is one of the most complex timing mechanisms in Shelley. Instead of calculating all rewards at once (which would take too long), rewards are calculated **incrementally** over many slots.

### Why Incremental Calculation?

Calculating rewards involves:
- Processing every stake account
- Processing every stake pool
- Computing performance metrics
- Calculating pool rewards
- Distributing rewards to delegators

For a network with millions of accounts, this would take too long to do in a single slot. The solution: **pulsing**.

### What is Pulsing?

**Pulsing** means breaking the calculation into small chunks:
- Each chunk is called a "pulse"
- One pulse executes per slot
- Each pulse processes a portion of the accounts/pools
- After enough pulses, the calculation is complete

Think of it like filling a bucket with water:
- You can't pour all the water at once (too heavy)
- Instead, you pour a little bit each second
- Eventually, the bucket is full

### The Reward Timing Window

Rewards are calculated during a specific window within each epoch:

```
Epoch starts at slot S
Stability window = W slots

Reward calculation window:
  Start: S + W (after randomness stabilizes)
  End: S + 2W (after another stability window)
  Force completion: S + 3W (if not yet complete)
```

The stability windows ensure that:
- VRF randomness has stabilized (for leader selection)
- Block production data is final
- No more epoch boundary changes will occur

### The Three Timing States

The RUPD rule uses three states to determine what to do:

#### 1. RewardsTooEarly

```
Current slot ≤ epoch_start + stability_window

State: SNothing (no calculation started)
Action: Do nothing, wait for stability window to pass
```

#### 2. RewardsJustRight

```
epoch_start + stability_window < current_slot ≤ force_completion_slot

State: Pulsing (calculation in progress)
Action: Execute one pulse of the calculation
```

#### 3. RewardsTooLate

```
current_slot > force_completion_slot

State: Should be Complete (but might still be Pulsing if unlucky)
Action: Force completion immediately
```

### Pulsing States

The `nesRu` field in NewEpochState tracks the reward calculation state:

```haskell
nesRu :: StrictMaybe PulsingRewUpdate

Can be:
  - SNothing: Haven't started yet
  - SJust (Pulsing state blocks): In progress
  - SJust (Complete rewards): Finished
```

### Pulsing Progression

Here's how reward calculation progresses through an epoch:

#### Stage 1: Before Stability Window

```
Epoch 100 starts at slot 43,200,000
Stability window ends at slot 43,203,600

Slots 43,200,000 - 43,203,600:
  TICK runs each slot
  RUPD rule: Current time is RewardsTooEarly
  Action: Stay in SNothing state
  Reason: VRF randomness not yet stable
```

#### Stage 2: Start Pulsing

```
Slot 43,203,601 (first slot after stability window):
  TICK runs
  RUPD rule: Current time is RewardsJustRight
  Action: Call startStep
    - Initialize reward calculation state
    - Set up iterators over all accounts/pools
    - Enter Pulsing state
    - Process first chunk of accounts
  Result: nesRu = SJust (Pulsing state1 blocks)
```

#### Stage 3: Continue Pulsing

```
Slots 43,203,602 - 43,207,200:
  Each slot:
    TICK runs
    RUPD rule: Current time is RewardsJustRight
    Action: Call pulseStep
      - Process next chunk of accounts
      - Update Pulsing state
    Result: nesRu = SJust (Pulsing state_i blocks)
```

#### Stage 4: Completion (Two Scenarios)

**Scenario A: Natural Completion** (calculation finishes before deadline)

```
Slot 43,206,500:
  TICK runs
  RUPD rule: Call pulseStep
  Action: Process final chunk of accounts
  Result: nesRu = SJust (Complete reward_update)

  → Calculation finished naturally!
```

**Scenario B: Forced Completion** (deadline reached)

```
Slot 43,207,201 (force completion deadline):
  TICK runs
  RUPD rule: Current time is RewardsTooLate
  State: Still SJust (Pulsing ...) ← calculation not finished yet
  Action: Call completeStep
    - Process all remaining accounts immediately
    - Force completion regardless of time
  Result: nesRu = SJust (Complete reward_update)

  → Calculation forced to finish!
```

#### Stage 5: Application at Epoch Boundary

```
Slot 43,632,000 (epoch 100 → 101 boundary):
  TICK detects epoch change
  NEWEPOCH runs:
    Check nesRu state:
      If SJust (Complete reward_update):
        → Apply rewards to all accounts
        → Update account balances
        → Reset nesRu to SNothing for next epoch
```

### What Gets Calculated?

During pulsing, the calculation computes:

1. **Pool Rewards**
   ```
   For each pool:
     - How many blocks did it produce?
     - What was its performance (actual blocks / expected blocks)?
     - Total stake delegated to pool
     - Pool costs and margin
     → Calculate pool operator reward
     → Calculate pool delegator rewards
   ```

2. **Delegator Rewards**
   ```
   For each delegator to this pool:
     - How much did they delegate?
     - What fraction of pool stake is this?
     → Calculate their share of pool rewards
   ```

3. **Reserve/Treasury Accounting**
   ```
   - How much total ADA to distribute?
   - How much goes to pools?
   - How much stays in reserves?
   - How much goes to treasury?
   ```

### Key Insight: Rewards Are Applied Later

Important: Reward **calculation** happens during the epoch, but reward **application** happens at the epoch boundary:

```
Epoch 100:
  - Slots 0-3,600: RewardsTooEarly (waiting)
  - Slots 3,601-7,200: Pulsing (calculating)
  - Slot 7,201+: Complete (calculation done)

Epoch 100 → 101 boundary:
  - NEWEPOCH applies calculated rewards to accounts
  - Delegators see increased balances
```

### Why This Timing Matters

1. **No disruption**: Pulsing spreads computation across many slots, so no single slot takes too long

2. **Predictable**: Rewards always calculated in the same window within each epoch

3. **Stable data**: Calculation uses stable data (after VRF stabilization) so results are deterministic

4. **Guaranteed completion**: Force deadline ensures rewards are always ready by epoch boundary

---

## Complete Timing Flow

Let's walk through a complete epoch to see how all these mechanisms coordinate.

### Epoch 100: Complete Timeline

#### Phase 1: Epoch Boundary (Slot 0 of Epoch 100)

```
TICK runs at slot 43,200,000 (first slot of epoch 100)

Detects: current_epoch (100) ≠ last_epoch + 1 (99 + 1)
  → Epoch boundary!

NEWEPOCH runs:
  1. Check nesRu:
     - State: SJust (Complete reward_update_for_epoch_99)
     - Action: Apply rewards calculated during epoch 99
       → Update all account balances
       → People receive their epoch 99 rewards

  2. MIR: Process treasury operations

  3. EPOCH:
     a) SNAP:
        - Take new Mark snapshot (stake distribution right now)
        - Mark snapshot from epoch 99 becomes Set snapshot
        - Set snapshot from epoch 98 becomes Go snapshot
        - Calculate pool distribution from new Mark

     b) POOLREAP:
        - Check if any pools scheduled to retire at epoch 100
        - For each retiring pool:
          → Refund deposit to reward account
          → Remove from pool registry
          → Clear delegations

     c) UPEC:
        - Activate any pending protocol parameter updates
        - Update current parameters

  4. Update NewEpochState:
     - nesEL = 100 (last processed epoch)
     - nesBprev = nesBcur (blocks from epoch 99)
     - nesBcur = empty (new block counter)
     - nesRu = SNothing (reset reward calculation)
     - nesPd = new pool distribution (from SNAP)

RUPD runs:
  - Timing: RewardsTooEarly (still in stability window)
  - Action: Nothing (stay in SNothing)

Result: Epoch 100 begins with fresh snapshots, updated parameters, retired pools removed
```

#### Phase 2: Early Epoch (Slots 0 - 3,600)

```
Slots 43,200,000 - 43,203,600

Each slot:
  TICK runs:
    - Check for epoch change: No
    - Check if past point of no return: No (too early)
    - Check for genesis delegations: Activate any scheduled for this slot
    - Run RUPD:
      → Timing: RewardsTooEarly
      → Action: Nothing (remain in SNothing state)
    - Force snapshots: No (too early)

Regular activity:
  - Blocks produced by various pools
  - Block counters (nesBcur) incremented
  - Transactions processed
  - Pool registrations/retirements scheduled
  - Delegations changed
  - But these don't affect snapshots (already taken) or rewards (calculated later)
```

#### Phase 3: Reward Calculation Begins (Slot 3,601)

```
Slot 43,203,601

TICK runs:
  - Check for epoch change: No
  - Check if past point of no return: No
  - Run RUPD:
    → Timing: RewardsJustRight (just entered calculation window)
    → State: SNothing → transition to Pulsing
    → Action: startStep
      - Initialize reward calculation
      - Load Set snapshot (from epoch 99)
      - Load block production data (nesBprev)
      - Create iterators over all pools and delegators
      - Process first chunk of accounts
    → New state: SJust (Pulsing state_1 nesBprev)
  - Force snapshots: Yes (stability window passed)
    → Mark snapshot forced to WHNF (fully evaluated)
    → Pool distribution forced to WHNF

Result: Reward calculation begins
```

#### Phase 4: Reward Calculation Continues (Slots 3,602 - 7,200)

```
Slots 43,203,602 - 43,207,200

Each slot:
  TICK runs:
    - Check for epoch change: No
    - Run RUPD:
      → Timing: RewardsJustRight
      → State: SJust (Pulsing state_i nesBprev)
      → Action: pulseStep
        - Process next chunk of accounts
        - Calculate rewards for processed accounts
        - Update state
      → New state: SJust (Pulsing state_i+1 nesBprev)

Regular activity continues:
  - Blocks produced
  - Transactions processed
  - Reward calculation progresses in background
```

#### Phase 5: Reward Calculation Completes (Slot ~7,000)

```
Slot 43,207,000 (example - actual completion varies)

TICK runs:
  - Run RUPD:
    → Timing: RewardsJustRight
    → State: SJust (Pulsing state_last nesBprev)
    → Action: pulseStep processes final accounts
    → Calculation complete!
    → New state: SJust (Complete reward_update_for_epoch_100)

Result: Rewards calculated for entire epoch
```

#### Phase 6: Protocol Parameter Solidification (Slot ~129,600)

```
Slot 43,329,600 (30 days from epoch start, example)

TICK runs:
  - Check if past point of no return: Yes
  - Action: solidifyNextEpochPParams
    → Future protocol parameters marked as finalized
    → No more updates allowed for epoch 101 parameters
```

#### Phase 7: Rest of Epoch (Slots 7,201 - 431,999)

```
Slots 43,207,201 - 43,631,999

Each slot:
  TICK runs:
    - Check for epoch change: No
    - Run RUPD:
      → Timing: RewardsTooLate (past calculation window)
      → State: SJust (Complete reward_update_for_epoch_100)
      → Action: Nothing (already complete)

Regular activity:
  - Blocks produced
  - Transactions processed
  - Rewards are calculated but NOT YET APPLIED
  - Wait for epoch boundary...
```

#### Phase 8: Next Epoch Boundary (Slot 0 of Epoch 101)

```
Slot 43,632,000 (first slot of epoch 101)

TICK runs:

Detects: current_epoch (101) ≠ last_epoch + 1 (100 + 1)
  → Epoch boundary again!

NEWEPOCH runs:
  1. Apply rewards calculated during epoch 100
     → Everyone gets their epoch 100 rewards

  2. Rotate snapshots again

  3. Retire any pools scheduled for epoch 101

  4. Reset reward calculation to SNothing

Cycle repeats for epoch 101...
```

---

## Key Timing Parameters

These parameters control when various timing events occur. They're defined in the protocol parameters.

### 1. Randomness Stabilisation Window

**What it is**: The number of slots after epoch boundary before VRF randomness is considered stable.

**Typical value**: 3,600 slots (1 hour)

**Why it matters**:
- VRF (Verifiable Random Function) is used to determine which pools can produce blocks
- VRF values depend on blocks produced near the epoch boundary
- We need to wait for enough blocks to ensure randomness is final and can't be manipulated

**What it affects**:
- When reward calculation can begin (after stability window)
- When snapshots are forced to evaluate (after stability window)
- When future protocol parameters are solidified

### 2. Point of No Return

**What it is**: The last slot at which protocol parameters can still be updated for the next epoch.

**Calculation**: `first_slot_of_epoch + stabilisation_window + randomness_stabilisation_window`

**Why it matters**:
- Protocol parameters need to be stable before an epoch begins
- Nodes need time to prepare for parameter changes
- This ensures everyone knows the parameters before the epoch starts

**What it affects**:
- When `futureProposals` becomes empty (no more changes allowed)
- When nodes can safely cache next epoch's parameters

### 3. eMax (Maximum Retirement Epoch)

**What it is**: Maximum number of epochs in the future that a pool can schedule retirement.

**Typical value**: 18 epochs (90 days)

**Why it matters**:
- Prevents pools from scheduling retirement too far in the future
- Ensures retirement schedules are manageable
- Limits long-term commitment uncertainty for delegators

**What it affects**:
- Validation of RetirePool certificates
- Maximum planning horizon for pool operators

### 4. Epoch Length

**What it is**: Number of slots per epoch.

**Mainnet value**: 432,000 slots (5 days)

**Why it matters**:
- Determines frequency of snapshots
- Determines frequency of reward distribution
- Affects how often pools can be retired
- Affects how often protocol parameters can be updated

### 5. Slot Length

**What it is**: Duration of each slot.

**Value**: 1 second

**Why it matters**:
- Fundamental time unit for all timing calculations
- Determines real-world duration of epochs (432,000 slots × 1 sec = 5 days)

---

## Visual Timeline

Here's a visual representation of timing within a single epoch:

```
Epoch 100
═══════════════════════════════════════════════════════════════════════════════

Slot:    0              3,600          7,200                                432,000
         ↓              ↓              ↓                                    ↓
Time:    |─────────────────────────────────────────────────────────────────|
         Epoch          Stability      Force                                Epoch
         Boundary       Window         Completion                           Boundary
         (NEWEPOCH)     Ends           Window                               (NEWEPOCH)

Activities:
═══════════════════════════════════════════════════════════════════════════════

0 (Epoch Boundary):
├─ NEWEPOCH
│  ├─ Apply epoch 99 rewards ← Delegators receive rewards
│  ├─ MIR (process MIR certificates from epoch 99) ← Treasury operations
│  └─ EPOCH
│     ├─ SNAP (rotate snapshots)
│     ├─ POOLREAP (retire scheduled pools)
│     └─ UPEC (activate parameter updates)
└─ RUPD: RewardsTooEarly (wait)

1 - 3,600:
├─ TICK every slot
├─ RUPD: RewardsTooEarly (still waiting)
└─ Regular block production

3,601 (Stability Window Ends):
├─ TICK
├─ RUPD: RewardsJustRight
│  └─ startStep (begin reward calculation)
├─ Force evaluation of Mark snapshot
└─ Force evaluation of pool distribution

3,602 - 7,200:
├─ TICK every slot
├─ RUPD: RewardsJustRight
│  └─ pulseStep (continue calculation)
└─ Regular block production

~7,000 (Variable):
└─ RUPD: Calculation completes
   └─ State: Complete (rewards ready)

7,201 - 432,000:
├─ TICK every slot
├─ RUPD: RewardsTooLate (no action, already complete)
├─ Regular block production
└─ Wait for next epoch boundary...

432,000 (Next Epoch Boundary):
└─ Cycle repeats for epoch 101
```

### Snapshot Timeline Across Multiple Epochs

```
Epoch 98          Epoch 99          Epoch 100         Epoch 101
════════════════  ════════════════  ════════════════  ════════════════

Snapshot A taken  Snapshot B taken  Snapshot C taken  Snapshot D taken
at epoch 98       at epoch 99       at epoch 100      at epoch 101
boundary          boundary          boundary          boundary

At Epoch 100:
─────────────
Mark = C (just taken, for epoch 101 rewards)
Set  = B (from epoch 99, for epoch 100 rewards)
Go   = A (from epoch 98, historical)

At Epoch 101:
─────────────
Mark = D (just taken, for epoch 102 rewards)
Set  = C (from epoch 100, for epoch 101 rewards)
Go   = B (from epoch 99, historical)
```

### Reward Calculation and Distribution Timeline

```
Epoch 99                                    Epoch 100
════════════════════════════════════════════════════════════════════

             Rewards for Epoch 99          Rewards for Epoch 100
             ───────────────────           ──────────────────────

Slots:       3,601 →→→→ 7,000              3,601 →→→→ 7,000
             Calculate                     Calculate
             (pulsing)                     (pulsing)
                            ↓                                ↓
Slot:                    432,000                         432,000
                       (Boundary)                       (Boundary)
                           ↓                                ↓
                        Applied                          Applied
                        to accounts                      to accounts
                        ↓                                ↓
                    Delegators                       Delegators
                    receive                          receive
                    rewards                          rewards
```

---

## Summary: Key Takeaways

1. **TICK runs every slot** and is the heartbeat of timing
   - Detects epoch boundaries
   - Pulses reward calculation
   - Activates genesis delegations
   - Solidifies future parameters

2. **NEWEPOCH runs at epoch boundaries** and coordinates major transitions
   - Applies rewards from previous epoch
   - Triggers MIR (treasury operations)
   - Triggers EPOCH rule

3. **EPOCH rule** handles three major tasks at boundaries
   - SNAP: Takes and rotates snapshots
   - POOLREAP: Retires scheduled pools
   - UPEC: Activates protocol parameter updates

4. **Snapshots** are rotated at each epoch boundary
   - Mark (just taken) → used for next epoch's rewards
   - Set (previous Mark) → used for current epoch's rewards
   - Go (previous Set) → historical reference

5. **Pool retirement** is two-phase
   - Scheduling: RetirePool certificate marks future epoch
   - Execution: POOLREAP removes pool at that epoch boundary

6. **Reward calculation** is incremental (pulsing)
   - Calculation during epoch (after stability window)
   - Application at next epoch boundary
   - Based on Set snapshot (one epoch delayed)

7. **Timing windows** ensure stability
   - Stability window: VRF randomness finalized
   - Point of no return: Future parameters solidified
   - Forced completion: Rewards guaranteed ready

8. **MIR certificates have timing constraints** (Shelley through Babbage)
   - Must be submitted before stability window ends
   - Enforced by `checkSlotNotTooLate` function
   - Prevents manipulation near epoch boundaries
   - Deprecated in Conway era (replaced by governance actions)

9. **Everything coordinates** around epoch boundaries
   - Snapshots rotated
   - Rewards applied
   - Pools retired
   - Parameters updated
   - MIR transfers executed
   - All in synchronized sequence

---

## File Reference

For deeper understanding, refer to these files:

| Component | File Path | Key Functions/Types |
|-----------|-----------|---------------------|
| TICK | [`Tick.hs`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Tick.hs) | [`bheadTransition:261`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Tick.hs:261), [`validatingTickTransition:177`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Tick.hs:177), [`solidifyNextEpochPParams:151`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Tick.hs:151) |
| NEWEPOCH | [`NewEpoch.hs`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/NewEpoch.hs) | [`newEpochTransition:150`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/NewEpoch.hs:150), [`updateRewards:235`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/NewEpoch.hs:235) |
| EPOCH | [`Epoch.hs`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Epoch.hs) | [`epochTransition:143`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Epoch.hs:143) |
| SNAP | [`Snap.hs`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Snap.hs) | [`snapTransition:77`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Snap.hs:77) |
| POOLREAP | [`PoolReap.hs`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/PoolReap.hs) | [`poolReapTransition:134`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/PoolReap.hs:134) |
| POOL | [`Pool.hs`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Pool.hs) | [`poolDelegationTransition:215`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Pool.hs:215) |
| RUPD | [`Rupd.hs`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Rupd.hs) | [`rupdTransition:118`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Rupd.hs:118), [`determineRewardTiming:112`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Rupd.hs:112), [`RewardTiming:109`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Rupd.hs:109) |
| DELEG | [`Deleg.hs`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Deleg.hs) | [`delegationTransition:248`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Deleg.hs:248), [`checkSlotNotTooLate:380`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Deleg.hs:380), [`MIRCertificateTooLateinEpochDELEG:108`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Deleg.hs:108) |
| MIR | [`Mir.hs`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Mir.hs) | [`mirTransition:94`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Mir.hs:94), [`ShelleyMirEvent:56`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Mir.hs:56), [`emptyInstantaneousRewards:160`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Mir.hs:160) |
| UPEC | [`Upec.hs`](eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Upec.hs) | Protocol parameter update activation |
| State Types | [`LedgerState/Types.hs`](eras/shelley/impl/src/Cardano/Ledger/Shelley/LedgerState/Types.hs) | [`NewEpochState:309`](eras/shelley/impl/src/Cardano/Ledger/Shelley/LedgerState/Types.hs:309), [`EpochState:67`](eras/shelley/impl/src/Cardano/Ledger/Shelley/LedgerState/Types.hs:67) |
| Snapshots | [`State/SnapShots.hs`](libs/cardano-ledger-core/src/Cardano/Ledger/State/SnapShots.hs) | [`SnapShots:223`](libs/cardano-ledger-core/src/Cardano/Ledger/State/SnapShots.hs:223) |
| Slot Arithmetic | [`Slot.hs`](libs/cardano-ledger-core/src/Cardano/Ledger/Slot.hs) | [`Duration:39`](libs/cardano-ledger-core/src/Cardano/Ledger/Slot.hs:39), [`(*-):56`](libs/cardano-ledger-core/src/Cardano/Ledger/Slot.hs:56), [`epochInfoFirst:71`](libs/cardano-ledger-core/src/Cardano/Ledger/Slot.hs:71) |
| Globals | [`BaseTypes.hs`](libs/cardano-ledger-core/src/Cardano/Ledger/BaseTypes.hs) | [`Globals:711`](libs/cardano-ledger-core/src/Cardano/Ledger/BaseTypes.hs:711), [`stabilityWindow:713`](libs/cardano-ledger-core/src/Cardano/Ledger/BaseTypes.hs:713) |

---

This document provides a comprehensive understanding of Shelley era timing mechanisms. The actual implementation is in Haskell, but the concepts described here apply regardless of programming language familiarity.
