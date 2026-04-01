# TxCertificate PlutusData Encoding

How Cardano ledger transaction certificates are encoded as PlutusData across Plutus versions.

Source files (cardano-ledger):
- `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Plutus/TxInfo.hs` — V1/V2 translation
- `eras/conway/impl/src/Cardano/Ledger/Conway/TxInfo.hs` — V3 translation, V1/V2 backward compat
- `libs/cardano-ledger-core/src/Cardano/Ledger/Plutus/TxInfo.hs` — shared helpers

Plutus upstream (constructor index definitions via `makeIsDataSchemaIndexed`):
- [`PlutusLedgerApi/V1/DCert.hs`](https://github.com/IntersectMBO/plutus/blob/master/plutus-ledger-api/src/PlutusLedgerApi/V1/DCert.hs) — `''DCert`
- [`PlutusLedgerApi/V3/Contexts.hs`](https://github.com/IntersectMBO/plutus/blob/master/plutus-ledger-api/src/PlutusLedgerApi/V3/Contexts.hs) — `''TxCert`, `''Delegatee`, `''DRep`
- [`PlutusLedgerApi/V1/Credential.hs`](https://github.com/IntersectMBO/plutus/blob/master/plutus-ledger-api/src/PlutusLedgerApi/V1/Credential.hs) — `''Credential`, `''StakingCredential`

---

## PlutusV1 / PlutusV2 — `DCert`

V1 and V2 share the same `DCert` type. Constructor index = `Constr <index> [fields...]`.

Translated by:
- `transTxCert` — Alonzo/Plutus/TxInfo.hs:322-330
- `transTxCertCommon` — Alonzo/Plutus/TxInfo.hs:333-348

| Index | Constructor             | Fields                            | Haskell Reference (Alonzo/Plutus/TxInfo.hs)  |
|-------|-------------------------|-----------------------------------|----------------------------------------------|
| 0     | `DCertDelegRegKey`      | `[StakingCredential]`             | L335-336: `PV1.StakingHash (transCred stakeCred)` |
| 1     | `DCertDelegDeRegKey`    | `[StakingCredential]`             | L337-338: `PV1.StakingHash (transCred stakeCred)` |
| 2     | `DCertDelegDelegate`    | `[StakingCredential, PubKeyHash]` | L339-340: `PV1.StakingHash (transCred stakeCred)` |
| 3     | `DCertPoolRegister`     | `[PubKeyHash, PubKeyHash]`        | L341-345: `transKeyHash sppId`, VRF hash     |
| 4     | `DCertPoolRetire`       | `[PubKeyHash, Integer]`           | L346-347: `transKeyHash poolId`, `transEpochNo` |
| 5     | `DCertGenesis`          | `[]`                              | L328: `PV1.DCertGenesis` (all data discarded) |
| 6     | `DCertMir`              | `[]`                              | L329: `PV1.DCertMir` (all data discarded)    |

### `StakingHash` wrapping in V1/V2

Staking credentials are **always wrapped** with `PV1.StakingHash` in V1/V2 paths.

Every occurrence of `PV1.StakingHash` in certificate code:

| Where | File | Line | Code |
|-------|------|------|------|
| RegTxCert | Alonzo/Plutus/TxInfo.hs | 336 | `PV1.DCertDelegRegKey (PV1.StakingHash (transCred stakeCred))` |
| UnRegTxCert | Alonzo/Plutus/TxInfo.hs | 338 | `PV1.DCertDelegDeRegKey (PV1.StakingHash (transCred stakeCred))` |
| DelegStakeTxCert | Alonzo/Plutus/TxInfo.hs | 340 | `PV1.DCertDelegDelegate (PV1.StakingHash (transCred stakeCred))` |
| Withdrawals | Alonzo/Plutus/TxInfo.hs | 274 | `Map.insert (PV1.StakingHash (transAccountAddress accountAddress)) n ans` |
| Rewarding purpose | Alonzo/Plutus/TxInfo.hs | 361 | `PV1.Rewarding (PV1.StakingHash (transAccountAddress accountAddress))` |
| Conway RegDeposit (V1/V2 compat) | Conway/TxInfo.hs | 392 | `PV1.DCertDelegRegKey (PV1.StakingHash (transCred stakeCred))` |
| Conway UnRegDeposit (V1/V2 compat) | Conway/TxInfo.hs | 394 | `PV1.DCertDelegDeRegKey (PV1.StakingHash (transCred stakeCred))` |

The wrapping path:
```
transCred stakeCred          -> Credential              (TxInfo.hs:139-143)
PV1.StakingHash (transCred)  -> StakingCredential       (added at each call site above)
```

So a registration certificate for a key-hash staking credential encodes as:

```
Constr 0 [                          -- DCertDelegRegKey
  Constr 0 [                        -- StakingHash
    Constr 0 [B <28-byte-keyhash>]  -- PubKeyCredential (PubKeyHash)
  ]
]
```

### Pool registration detail

Only `sppId` (pool key hash) and `sppVrf` (VRF key hash) from `StakePoolParams` are kept.
The VRF key hash is encoded as a `PubKeyHash` (same bytestring wrapper, different semantic):

Ref: Alonzo/Plutus/TxInfo.hs:341-345

```
Constr 3 [                          -- DCertPoolRegister (V1/V2, index 3)
  Constr 0 [B <28-byte-pool-id>],   -- PubKeyHash (pool key hash)
  Constr 0 [B <32-byte-vrf-hash>]   -- PubKeyHash (VRF verification key hash)
]
```

### Genesis and MIR — lossy encoding

`DCertGenesis` and `DCertMir` carry **no fields** — all internal data (genesis delegate key,
VRF hash, MIR pot, rewards map) is discarded. These certificates exist in Shelley through
Babbage but are removed in Conway.

Ref: Alonzo/Plutus/TxInfo.hs:328-329

---

## Conway era backward compatibility (V1/V2)

When Conway-era transactions use PlutusV1/V2 scripts, `transTxCertV1V2` applies.

Ref: Conway/TxInfo.hs:383-397

| Conway Certificate            | V1/V2 Mapping | Haskell Reference (Conway/TxInfo.hs) |
|-------------------------------|---------------|--------------------------------------|
| `RegDepositTxCert cred dep`   | `DCertDelegRegKey (StakingHash cred)` — deposit dropped | L391-392 |
| `UnRegDepositTxCert cred ref` | `DCertDelegDeRegKey (StakingHash cred)` — refund dropped | L393-394 |
| `RegTxCert`, `UnRegTxCert`, `DelegStakeTxCert`, pool certs | Same as pre-Conway (delegates to `Alonzo.transTxCertCommon`) | L395-396 |
| DRep certs, Committee certs, `RegDelegCert`, etc. | **Rejected** — `CertificateNotSupported` error | L397 |

Conway governance certificates cannot appear in V1/V2 script contexts.

---

## PlutusV3 — `TxCert`

Completely redesigned type. Supports deposits, DReps, committee, and combined delegation.

Translated by: `transTxCert` — Conway/TxInfo.hs:555-593

| Index | Constructor                  | Fields                                              | Haskell Reference (Conway/TxInfo.hs)       |
|-------|------------------------------|-----------------------------------------------------|--------------------------------------------|
| 0     | `TxCertRegStaking`           | `[Credential, Maybe Lovelace]`                      | L565-573: `transCred`, bootstrap check     |
| 1     | `TxCertUnRegStaking`         | `[Credential, Maybe Lovelace]`                      | L567-578: `transCred`, bootstrap check     |
| 2     | `TxCertDelegStaking`         | `[Credential, Delegatee]`                           | L579-580: `transCred`, `transDelegatee`    |
| 3     | `TxCertRegDeleg`             | `[Credential, Delegatee, Lovelace]`                 | L581-582: `transCred`, `transDelegatee`    |
| 4     | `TxCertRegDRep`              | `[DRepCredential, Lovelace]`                        | L587-588: `transDRepCred`                  |
| 5     | `TxCertUpdateDRep`           | `[DRepCredential]`                                  | L591-592: `transDRepCred`                  |
| 6     | `TxCertUnRegDRep`            | `[DRepCredential, Lovelace]`                        | L589-590: `transDRepCred`                  |
| 7     | `TxCertPoolRegister`         | `[PubKeyHash, PubKeyHash]`                          | L559-562: `transKeyHash sppId`, VRF hash   |
| 8     | `TxCertPoolRetire`           | `[PubKeyHash, Integer]`                             | L563-564: `transKeyHash poolId`            |
| 9     | `TxCertAuthHotCommittee`     | `[ColdCommitteeCredential, HotCommitteeCredential]` | L583-584: `transColdCommitteeCred`, `transHotCommitteeCred` |
| 10    | `TxCertResignColdCommittee`  | `[ColdCommitteeCredential]`                         | L585-586: `transColdCommitteeCred`         |

### No `StakingHash` wrapper in V3

Unlike V1/V2, staking credentials are encoded as **raw `Credential`** — no `StakingHash` wrapper.

The key evidence:
- Comment at core TxInfo.hs:155-156:
  > This function is the right one to use starting with PlutusV3, prior to that an extra
  > `PV1.StakingHash` wrapper is needed.
- V3 cert code (Conway/TxInfo.hs:565) uses `transCred stakeCred` directly
- V1/V2 cert code (Alonzo/Plutus/TxInfo.hs:336) wraps it: `PV1.StakingHash (transCred stakeCred)`
- Same pattern in withdrawals:
  - V1/V2 (Alonzo/Plutus/TxInfo.hs:274): `PV1.StakingHash (transAccountAddress accountAddress)`
  - V3 (Conway/TxInfo.hs:547): `transAccountAddress` directly (no wrapper)

V3 staking registration example:

```
Constr 0 [                          -- TxCertRegStaking
  Constr 0 [B <28-byte-keyhash>],   -- PubKeyCredential (Credential, NOT StakingCredential)
  Constr 0 [I <deposit-lovelace>]   -- Just deposit
]
```

Compare V1/V2 equivalent:

```
Constr 0 [                          -- DCertDelegRegKey
  Constr 0 [                        -- StakingHash wrapper
    Constr 0 [B <28-byte-keyhash>]  -- PubKeyCredential
  ]
]
```

### Bootstrap phase behavior

During Conway bootstrap phase, deposit/refund in `RegDepositTxCert` and `UnRegDepositTxCert`
is encoded as `Nothing` even though a value exists on-chain. After bootstrap, the actual `Coin`
value is passed through.

Ref:
- `hardforkConwayBootstrapPhase` check at Conway/TxInfo.hs:571 (RegDeposit) and :576 (UnRegDeposit)
- Legacy `RegTxCert` / `UnRegTxCert` (no deposit) always encode with `Nothing` — Conway/TxInfo.hs:565-568

---

## Supporting Types

### Credential

Plutus definition: `makeIsDataSchemaIndexed ''Credential [('PubKeyCredential, 0), ('ScriptCredential, 1)]`
Ledger translation: `transCred` — core TxInfo.hs:139-143

```
Constr 0 [B <28-byte-keyhash>]    -- PubKeyCredential (PubKeyHash)    — L140-141
Constr 1 [B <28-byte-scripthash>] -- ScriptCredential (ScriptHash)    — L142-143
```

### StakingCredential (V1/V2 only)

Plutus definition: `makeIsDataSchemaIndexed ''StakingCredential [('StakingHash, 0), ('StakingPtr, 1)]`
Ledger translation: `transStakeReference` — core TxInfo.hs:133-137

```
Constr 0 [<Credential>]                        -- StakingHash           — L134
Constr 1 [I <slot>, I <txIx>, I <certIx>]      -- StakingPtr (deprecated) — L135-136
```

### Delegatee (V3 only)

Plutus definition: `makeIsDataSchemaIndexed ''Delegatee [('DelegStake, 0), ('DelegVote, 1), ('DelegStakeVote, 2)]`
Ledger translation: `transDelegatee` — Conway/TxInfo.hs:604-608

```
Constr 0 [<PubKeyHash>]                -- DelegStake (pool only)       — L606
Constr 1 [<DRep>]                      -- DelegVote (DRep only)        — L607
Constr 2 [<PubKeyHash>, <DRep>]        -- DelegStakeVote (pool + DRep) — L608
```

### DRep (V3 only)

Plutus definition: `makeIsDataSchemaIndexed ''DRep [('DRep, 0), ('DRepAlwaysAbstain, 1), ('DRepAlwaysNoConfidence, 2)]`
Ledger translation: `transDRep` — Conway/TxInfo.hs:610-614

```
Constr 0 [<DRepCredential>]   -- DRep (identified by credential)    — L612
Constr 1 []                   -- DRepAlwaysAbstain                   — L613
Constr 2 []                   -- DRepAlwaysNoConfidence              — L614
```

### Wrapper newtypes (V3 only)

These are transparent in PlutusData — they encode identically to their inner `Credential`:

| Newtype | Ledger translation | Reference (Conway/TxInfo.hs) |
|---------|--------------------|------------------------------|
| `DRepCredential` | `transDRepCred` = `PV3.DRepCredential . transCred` | L595-596 |
| `ColdCommitteeCredential` | `transColdCommitteeCred` = `PV3.ColdCommitteeCredential . transCred` | L598-599 |
| `HotCommitteeCredential` | `transHotCommitteeCred` = `PV3.HotCommitteeCredential . transCred` | L601-602 |

---

## ScriptPurpose — `Certifying`

### V1/V2

Ref: `transPlutusPurpose` — Alonzo/Plutus/TxInfo.hs:350-361 (Certifying case at L359)

```haskell
AlonzoCertifying (AsItem txCert) -> PV1.Certifying <$> toPlutusTxCert proxy pv txCert
```

```
Constr 2 [<DCert>]   -- Certifying (index 2 among ScriptPurpose constructors)
```

Conway V1/V2 backward compat delegates to Alonzo: Conway/TxInfo.hs:732

### V3

Ref: `transPlutusPurposeV3` — Conway/TxInfo.hs:623-635 (Certifying case at L634-635)

```haskell
ConwayCertifying (AsIxItem ix txCert) -> PV3.Certifying (toInteger ix) <$> toPlutusTxCert proxy pv txCert
```

```
Constr 2 [I <cert-index>, <TxCert>]   -- Certifying with certificate index in tx body
```

V3 adds the **index** of the certificate within the transaction body. This is needed because
Conway certificates can carry anchors (URLs) that are not representable in Plutus, so ordering
by value is not possible — the index provides a stable reference.

---

## Version Comparison Summary

| Aspect                    | V1/V2 (`DCert`)           | V3 (`TxCert`)                     |
|---------------------------|---------------------------|------------------------------------|
| Staking cred wrapper      | `StakingHash(Credential)` | `Credential` (no wrapper)          |
| Registration deposit      | Not present               | `Maybe Lovelace`                   |
| Deregistration refund     | Not present               | `Maybe Lovelace`                   |
| Delegation target         | Pool only (`PubKeyHash`)  | `Delegatee` (pool / DRep / both)   |
| Combined register+deleg   | Not supported             | `TxCertRegDeleg`                   |
| Genesis delegation        | `DCertGenesis` (no data)  | Removed                            |
| MIR certificate           | `DCertMir` (no data)      | Removed                            |
| DRep certificates         | Not supported             | Register / Unregister / Update     |
| Committee certificates    | Not supported             | AuthHot / ResignCold               |
| Certifying ScriptPurpose  | `Certifying DCert`        | `Certifying Integer TxCert`        |
| Number of constructors    | 7                         | 11                                 |

---

## Ledger-to-Plutus Translation Map (all eras)

```
Shelley/Allegra/Mary/Alonzo/Babbage certificates:
  RegTxCert cred                 -> V1: DCertDelegRegKey(StakingHash(cred))
                                      Alonzo/Plutus/TxInfo.hs:335-336
                                    V3: TxCertRegStaking(cred, Nothing)
                                      Conway/TxInfo.hs:565-566

  UnRegTxCert cred               -> V1: DCertDelegDeRegKey(StakingHash(cred))
                                      Alonzo/Plutus/TxInfo.hs:337-338
                                    V3: TxCertUnRegStaking(cred, Nothing)
                                      Conway/TxInfo.hs:567-568

  DelegStakeTxCert cred pool     -> V1: DCertDelegDelegate(StakingHash(cred), pool)
                                      Alonzo/Plutus/TxInfo.hs:339-340
                                    V3: TxCertDelegStaking(cred, DelegStake(pool))
                                      Conway/TxInfo.hs:579-580

  RegPoolTxCert params           -> V1: DCertPoolRegister(poolId, vrfHash)
                                      Alonzo/Plutus/TxInfo.hs:341-345
                                    V3: TxCertPoolRegister(poolId, vrfHash)
                                      Conway/TxInfo.hs:559-562

  RetirePoolTxCert pool epoch    -> V1: DCertPoolRetire(pool, epoch)
                                      Alonzo/Plutus/TxInfo.hs:346-347
                                    V3: TxCertPoolRetire(pool, epoch)
                                      Conway/TxInfo.hs:563-564

  GenesisDelegTxCert             -> V1: DCertGenesis
                                      Alonzo/Plutus/TxInfo.hs:328
                                    V3: N/A (removed in Conway)

  MirTxCert                      -> V1: DCertMir
                                      Alonzo/Plutus/TxInfo.hs:329
                                    V3: N/A (removed in Conway)

Conway-only certificates:
  RegDepositTxCert cred deposit  -> V1: DCertDelegRegKey(StakingHash(cred))  [deposit dropped]
                                      Conway/TxInfo.hs:391-392
                                    V3: TxCertRegStaking(cred, Just deposit | Nothing*)
                                      Conway/TxInfo.hs:569-573

  UnRegDepositTxCert cred refund -> V1: DCertDelegDeRegKey(StakingHash(cred))  [refund dropped]
                                      Conway/TxInfo.hs:393-394
                                    V3: TxCertUnRegStaking(cred, Just refund | Nothing*)
                                      Conway/TxInfo.hs:574-578

  DelegTxCert cred delegatee    -> V1: REJECTED (Conway/TxInfo.hs:397)
                                    V3: TxCertDelegStaking(cred, delegatee)
                                      Conway/TxInfo.hs:579-580

  RegDepositDelegTxCert          -> V1: REJECTED (Conway/TxInfo.hs:397)
                                    V3: TxCertRegDeleg(cred, delegatee, deposit)
                                      Conway/TxInfo.hs:581-582

  AuthCommitteeHotKeyTxCert      -> V1: REJECTED (Conway/TxInfo.hs:397)
                                    V3: TxCertAuthHotCommittee(cold, hot)
                                      Conway/TxInfo.hs:583-584

  ResignCommitteeColdTxCert      -> V1: REJECTED (Conway/TxInfo.hs:397)
                                    V3: TxCertResignColdCommittee(cold)
                                      Conway/TxInfo.hs:585-586

  RegDRepTxCert                  -> V1: REJECTED (Conway/TxInfo.hs:397)
                                    V3: TxCertRegDRep(drep, deposit)
                                      Conway/TxInfo.hs:587-588

  UnRegDRepTxCert                -> V1: REJECTED (Conway/TxInfo.hs:397)
                                    V3: TxCertUnRegDRep(drep, refund)
                                      Conway/TxInfo.hs:589-590

  UpdateDRepTxCert               -> V1: REJECTED (Conway/TxInfo.hs:397)
                                    V3: TxCertUpdateDRep(drep)
                                      Conway/TxInfo.hs:591-592

  * Nothing during Conway bootstrap phase (hardforkConwayBootstrapPhase check at L571, L576),
    Just value after bootstrap
```
