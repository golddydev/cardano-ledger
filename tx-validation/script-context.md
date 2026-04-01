# ScriptContext Encoding to PlutusData

How the ledger constructs `ScriptContext` and encodes it to `PlutusData` for Plutus
script evaluation across all Plutus language versions.

**Source references:**

| Module | Path |
|--------|------|
| Shared translators | `libs/cardano-ledger-core/src/Cardano/Ledger/Plutus/TxInfo.hs` |
| PlutusArgs / evaluation | `libs/cardano-ledger-core/src/Cardano/Ledger/Plutus/Language.hs` |
| ToPlutusData class | `libs/cardano-ledger-core/src/Cardano/Ledger/Plutus/ToPlutusData.hs` |
| V1 TxInfo (Alonzo) | `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Plutus/TxInfo.hs` |
| V2 TxInfo (Babbage) | `eras/babbage/impl/src/Cardano/Ledger/Babbage/TxInfo.hs` |
| V3 TxInfo (Conway) | `eras/conway/impl/src/Cardano/Ledger/Conway/TxInfo.hs` |
| Type families | `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Plutus/Context.hs` |

---

## Script arguments shape

### PlutusV1 / PlutusV2 (legacy args)

Scripts receive **separate** Data arguments:

- **Spending scripts**: 3 args — `[Datum, Redeemer, ScriptContext]`
- **All other scripts**: 2 args — `[Redeemer, ScriptContext]`

Where `ScriptContext` wraps `TxInfo` + `ScriptPurpose`:

```haskell
-- PV1/PV2
data ScriptContext = ScriptContext
  { scriptContextTxInfo    :: TxInfo
  , scriptContextPurpose   :: ScriptPurpose
  }
```

The encoding pipeline:

```haskell
legacyPlutusArgsToData = \case
  LegacyPlutusArgs2 redeemer scriptContext -> [redeemer, PV3.toData scriptContext]
  LegacyPlutusArgs3 datum redeemer scriptContext -> [datum, redeemer, PV3.toData scriptContext]
```

### PlutusV3 (single arg)

Scripts receive a **single** `ScriptContext` that embeds the redeemer and script-specific info:

```haskell
-- PV3
data ScriptContext = ScriptContext
  { scriptContextTxInfo    :: TxInfo
  , scriptContextRedeemer  :: Redeemer
  , scriptContextScriptInfo :: ScriptInfo
  }
```

The encoding pipeline:

```haskell
-- PlutusV3 evaluatePlutusRunnable:
PV3.evaluateScriptRestricting pv vm ec exBudget rs . PV3.toData . unPlutusV3Args
```

### PlutusData encoding

All versions use `PV3.toData` (from `plutus-ledger-api`) to convert `ScriptContext` into
`PlutusData`. The exact constructor tags and field order are determined by the `ToData`
(Generic-derived) instances in the `plutus-ledger-api` package, not in `cardano-ledger`.

---

## TxInfo fields per version

### PlutusV1 TxInfo

```haskell
PV1.TxInfo
  { txInfoInputs      :: [TxInInfo]                     -- sorted by TxIn (Set.toList)
  , txInfoOutputs     :: [TxOut]                         -- transaction body order
  , txInfoFee         :: Value                           -- ADA-only Value (see below)
  , txInfoMint        :: Value                           -- zero ADA + multi-asset
  , txInfoDCert       :: [DCert]                         -- transaction body order
  , txInfoWdrl        :: [(StakingCredential, Integer)]  -- ascending by StakingCredential
  , txInfoValidRange  :: POSIXTimeRange
  , txInfoSignatories :: [PubKeyHash]                    -- ascending (Set.toList)
  , txInfoData        :: [(DatumHash, Datum)]            -- ascending by DatumHash
  , txInfoId          :: TxId
  }
```

**Alonzo-specific behavior**: Byron addresses in inputs are silently filtered out
(`catMaybes` after `transTxOut` returns `Nothing` for Bootstrap addresses).

### PlutusV2 TxInfo

```haskell
PV2.TxInfo
  { txInfoInputs          :: [TxInInfo]                              -- sorted by TxIn
  , txInfoOutputs         :: [TxOut]                                 -- tx body order
  , txInfoReferenceInputs :: [TxInInfo]                              -- sorted by TxIn  (NEW)
  , txInfoFee             :: Value                                   -- ADA-only Value
  , txInfoMint            :: Value                                   -- zero ADA + multi-asset
  , txInfoDCert           :: [DCert]                                 -- tx body order
  , txInfoWdrl            :: Map StakingCredential Integer           -- ascending keys  (now Map)
  , txInfoValidRange      :: POSIXTimeRange
  , txInfoSignatories     :: [PubKeyHash]                            -- ascending
  , txInfoRedeemers       :: Map ScriptPurpose Redeemer              -- ascending keys  (NEW)
  , txInfoData            :: Map DatumHash Datum                     -- ascending keys  (now Map)
  , txInfoId              :: TxId
  }
```

**Changes from V1**: reference inputs, redeemers map, and withdrawals/data switched from
list-of-pairs to Plutus `Map` (via `PV2.unsafeFromList`). Outputs carry inline datums and
reference script hashes. Byron address inputs are now a hard error rather than silently
filtered.

### PlutusV3 TxInfo

```haskell
PV3.TxInfo
  { txInfoInputs                :: [TxInInfo]                        -- sorted by TxIn
  , txInfoOutputs               :: [TxOut]                           -- tx body order
  , txInfoReferenceInputs       :: [TxInInfo]                        -- sorted by TxIn
  , txInfoFee                   :: Lovelace                          -- plain integer  (CHANGED)
  , txInfoMint                  :: MintValue                         -- no ADA entry   (CHANGED)
  , txInfoTxCerts               :: [TxCert]                          -- tx body order  (RENAMED)
  , txInfoWdrl                  :: Map Credential Lovelace           -- ascending keys (CHANGED)
  , txInfoValidRange            :: POSIXTimeRange
  , txInfoSignatories           :: [PubKeyHash]                      -- ascending
  , txInfoRedeemers             :: Map ScriptPurpose Redeemer        -- ascending keys
  , txInfoData                  :: Map DatumHash Datum               -- ascending keys
  , txInfoId                    :: TxId
  , txInfoVotes                 :: Map Voter (Map GovernanceActionId Vote)  -- (NEW)
  , txInfoProposalProcedures    :: [ProposalProcedure]               -- OSet order     (NEW)
  , txInfoCurrentTreasuryAmount :: Maybe Lovelace                    -- (NEW)
  , txInfoTreasuryDonation      :: Maybe Lovelace                    -- (NEW)
  }
```

**Changes from V2**: fee is `Lovelace` (plain integer) instead of `Value`; mint uses
`MintValue` (no zero-ADA entry); withdrawals keyed by `Credential` (not
`StakingCredential` with `StakingHash` wrapper) and values are `Lovelace`; certificates
use `TxCert` (richer Conway types). Governance fields added.

---

## Fee encoding

### PlutusV1 / PlutusV2

Fee is encoded as a **full `Value`** containing only ADA:

```haskell
transCoinToValue :: Coin -> PV1.Value
transCoinToValue (Coin c) = PV1.singleton PV1.adaSymbol PV1.adaToken c
```

As `PlutusData`, this becomes a `Map` with a single entry:
`Map[(adaSymbol, Map[(adaToken, fee_amount)])]` — where `adaSymbol` is the empty
bytestring `""` and `adaToken` is the empty bytestring `""`.

### PlutusV3

Fee is encoded as a plain **`Lovelace`** (integer):

```haskell
transCoinToLovelace :: Coin -> PV1.Lovelace
transCoinToLovelace (Coin c) = PV1.Lovelace c
```

As `PlutusData`, this is simply `I(fee_amount)`.

---

## Datums encoding and sorting

### What goes into `txInfoData`

Only datums explicitly present in the transaction **witness set** (`TxDats`) are included.
Inline datums on outputs are **not** duplicated here — they appear only on the output itself
(V2/V3 outputs).

```haskell
transTxWitsDatums :: AlonzoEraTxWits era => TxWits era -> [(PV1.DatumHash, PV1.Datum)]
transTxWitsDatums txWits = transDataPair <$> Map.toList (txWits ^. datsTxWitsL . unTxDatsL)
```

### Sort order

The witness datum map is a `Map.Strict DataHash (Data era)`. `Map.toList` produces pairs
in **ascending `DataHash` order** (standard `Ord` on the hash bytes).

### V1

Encoded as a **list of pairs** `[(DatumHash, Datum)]` — the `ToData` instance for a list
of pairs produces a Plutus `Map` node.

### V2 / V3

Wrapped in `PV2.unsafeFromList` / `PV3.unsafeFromList` — produces a Plutus `Map` value
directly. The underlying order is preserved from `Map.toList` (ascending datum hash).

### Spending datum (passed separately)

For spending scripts, the datum attached to the UTxO being spent is resolved:

- **Alonzo (V1)**: look up the datum hash on the UTxO output, then find the datum by
  that hash in the witness set.
- **Babbage+ (V2/V3)**: prefer **inline datum** on the UTxO output; fall back to
  datum-by-hash from the witness set.

For V1/V2 this datum is passed as a **separate first argument** to the script.
For V3 it is embedded in `ScriptInfo` as `SpendingScript txOutRef (Maybe Datum)`.

---

## Redeemers encoding and sorting

### Redeemer indexing (how redeemers map to purposes)

In the transaction witness set, redeemers are keyed by `PlutusPurpose AsIx era` — a
(tag, index) pair. The index is the **position** of the item within its category in the
transaction body:

| Purpose | Container | Index semantics |
|---------|-----------|-----------------|
| Spending | `inputsTxBodyL` (`Set TxIn`) | `Set.lookupIndex` — ascending `TxIn` order |
| Minting | `mintedTxBodyF` (`Set PolicyID`) | `Set.lookupIndex` — ascending `PolicyID` order |
| Certifying | `certsTxBodyL` (`StrictSeq TxCert`) | `StrictSeq.findIndexL` — sequential position |
| Rewarding | `withdrawalsTxBodyL` (`Map RewardAccount Coin`) | `Map.lookupIndex` — ascending key order |
| Voting (V3) | `votingProceduresTxBodyL` | sequential position in the `VotingProcedures` container |
| Proposing (V3) | `proposalProceduresTxBodyL` (`OSet`) | sequential position in the `OSet` |

### V1

Redeemers are **not** included in `TxInfo`. Each redeemer is passed as a separate script
argument (the second arg for non-spending, the second of three for spending).

### V2

Redeemers appear in `TxInfo` as `Map ScriptPurpose Redeemer`. Built by:

```haskell
transTxRedeemers proxy pv tx =
  PV2.unsafeFromList
    <$> mapM (transRedeemerPtr proxy pv $ tx ^. bodyTxL)
            (Map.toList $ tx ^. witsTxL . rdmrsTxWitsL . unRedeemersL)
```

The **sort order** follows `Map.toList` on the redeemer map — ascending by
`PlutusPurpose AsIx` key. The `Ord` instance orders first by **constructor tag**
(Spending < Minting < Certifying < Rewarding), then by index within each tag.

Each `(AsIx, (Data, ExUnits))` entry is resolved to `(ScriptPurpose, Redeemer)` via
`redeemerPointerInverse` (mapping index back to the actual item in the tx body).

### V3

Same as V2 — `Map ScriptPurpose Redeemer` in `TxInfo`, plus the redeemer is also embedded
in the `ScriptContext` directly as `scriptContextRedeemer`.

The `Ord` on Conway's `PlutusPurpose AsIx` adds Voting and Proposing constructors:
Spending < Minting < Certifying < Rewarding < Voting < Proposing.

### Redeemer Data translation

```haskell
transRedeemer :: Data era -> PV2.Redeemer
transRedeemer = PV2.Redeemer . PV2.dataToBuiltinData . getPlutusData
```

---

## Required signers (signatories)

### All versions

Required signer hashes are translated identically across V1, V2, and V3:

```haskell
transTxBodyReqSignerHashes :: AlonzoEraTxBody era => TxBody t era -> [PV1.PubKeyHash]
transTxBodyReqSignerHashes txBody = transKeyHash <$> Set.toList (txBody ^. reqSignerHashesTxBodyG)
```

The `reqSignerHashesTxBodyG` getter returns a `Set (KeyHash WitVKey)`. `Set.toList`
produces the hashes in **ascending order** by the underlying byte representation (standard
`Ord` for `KeyHash`).

Each `KeyHash` is translated to `PubKeyHash`:

```haskell
transKeyHash :: KeyHash d -> PV1.PubKeyHash
transKeyHash (KeyHash h) = PV1.PubKeyHash (PV1.toBuiltin (hashToBytes h))
```

---

## ScriptPurpose encoding

### PlutusV1 / PlutusV2

Four constructors:

```haskell
data ScriptPurpose
  = Minting CurrencySymbol
  | Spending TxOutRef
  | Rewarding StakingCredential      -- wrapped in StakingHash
  | Certifying DCert
```

Translation from ledger types:

```haskell
transPlutusPurpose proxy pv = \case
  AlonzoSpending (AsItem txIn) -> PV1.Spending (transTxIn txIn)
  AlonzoMinting (AsItem policyId) -> PV1.Minting (transPolicyID policyId)
  AlonzoCertifying (AsItem txCert) -> PV1.Certifying <$> toPlutusTxCert proxy pv txCert
  AlonzoRewarding (AsItem account) -> PV1.Rewarding (PV1.StakingHash (transAccountAddress account))
```

Note: `Rewarding` wraps the credential in `StakingHash` in V1/V2.

### PlutusV3

Six constructors — adds `Voting` and `Proposing`, and `Certifying`/`Proposing` carry
an index:

```haskell
data ScriptPurpose
  = Minting CurrencySymbol
  | Spending TxOutRef
  | Rewarding Credential              -- direct Credential (no StakingHash wrapper)
  | Certifying Integer TxCert         -- includes index
  | Voting Voter
  | Proposing Integer ProposalProcedure  -- includes index
```

The index is necessary because Conway certificates and proposals contain `Anchor`s that
are not translated to the Plutus context, making it impossible to uniquely identify items
by content alone.

Translation:

```haskell
transPlutusPurposeV3 proxy pv = \case
  ConwaySpending (AsIxItem _ txIn) -> PV3.Spending (transTxIn txIn)
  ConwayMinting (AsIxItem _ policyId) -> PV3.Minting (transPolicyID policyId)
  ConwayCertifying (AsIxItem ix txCert) -> PV3.Certifying (toInteger ix) <$> toPlutusTxCert ...
  ConwayRewarding (AsIxItem _ account) -> PV3.Rewarding (transAccountAddress account)
  ConwayVoting (AsIxItem _ voter) -> PV3.Voting (transVoter voter)
  ConwayProposing (AsIxItem ix proposal) -> PV3.Proposing (toInteger ix) (transProposal ...)
```

### V3 ScriptInfo (in ScriptContext)

`ScriptPurpose` is further converted to `ScriptInfo` which is what actually appears in the
V3 `ScriptContext`. For spending, this includes the optional datum:

```haskell
scriptPurposeToScriptInfo :: ScriptPurpose -> Maybe Datum -> ScriptInfo
scriptPurposeToScriptInfo sp maybeDatum = case sp of
  Spending txIn         -> SpendingScript txIn maybeDatum
  Minting policyId      -> MintingScript policyId
  Certifying ix txCert  -> CertifyingScript ix txCert
  Rewarding cred        -> RewardingScript cred
  Voting voter          -> VotingScript voter
  Proposing ix proposal -> ProposingScript ix proposal
```

---

## Mint field encoding

### PlutusV1 / PlutusV2

Mint is encoded as a `Value` with a **zero ADA entry prepended** (historical quirk from
the Mary era when the mint field was `MaryValue` instead of `MultiAsset`):

```haskell
transMintValue :: MultiAsset -> PV1.Value
transMintValue m = transCoinToValue zero <> transMultiAsset m
```

The multi-asset map is built from `Map.foldrWithKey'`, producing pairs in the map's
natural key order (ascending `PolicyID`, then ascending `AssetName` within each policy).

### PlutusV3

Uses `MintValue` — the multi-asset map **without** the zero ADA entry:

```haskell
transMintValue :: MultiAsset -> PV3.MintValue
transMintValue = PV3.UnsafeMintValue . PV1.getValue . Alonzo.transMultiAsset
```

---

## Withdrawals encoding

### PlutusV1

List of pairs `[(StakingCredential, Integer)]` — the `StakingCredential` wraps the
underlying `Credential` in `StakingHash`:

```haskell
transWithdrawals (Withdrawals mp) = Map.foldlWithKey' accum Map.empty mp
  where
    accum ans account (Coin n) = Map.insert (PV1.StakingHash (transAccountAddress account)) n ans
```

Result order: ascending by `StakingCredential` (from `Map.toList` on the accumulated map).

### PlutusV2

Same content but wrapped as a Plutus `Map` via `PV2.unsafeFromList`.

### PlutusV3

Keyed by `Credential` directly (no `StakingHash` wrapper), values are `Lovelace`
(not `Integer`):

```haskell
transTxBodyWithdrawals txBody =
  transMap transAccountAddress transCoinToLovelace (unWithdrawals $ txBody ^. withdrawalsTxBodyL)
```

`transMap` uses `Map.toList` — ascending by ledger `RewardAccount` key order.

---

## Inputs sorting

### All versions

Inputs (both regular and reference) come from `Set TxIn` in the transaction body.
`Set.toList` produces them in **ascending order** by `(TxId, TxIx)` — lexicographic
on the pair, with `TxId` compared by hash bytes and `TxIx` by numeric value.

### Alonzo V1 quirk

Byron address inputs are silently dropped (`catMaybes` after `transTxOut` returns
`Nothing` for Bootstrap addresses). This bug is preserved for backward compatibility.

### Babbage+ V1

Byron address inputs produce a hard error (`ByronTxOutInContext`) instead of being
silently filtered.

---

## Validity interval differences

### Alonzo (used by V1/V2 in Alonzo/Babbage eras)

```haskell
ValidityInterval SNothing  (SJust i) -> PV1.to <$> transSlotToPOSIXTime i
```

`PV1.to t` = `Interval (LowerBound NegInf True) (UpperBound (Finite t) True)` — upper
bound is **inclusive**.

### Conway (used by all versions in Conway era)

```haskell
ValidityInterval SNothing (SJust i) -> do
  t <- transSlotToPOSIXTime i
  pure $ PV1.Interval (PV1.LowerBound PV1.NegInf True) (PV1.strictUpperBound t)
```

Upper bound uses `strictUpperBound` — **exclusive**. This is a behavioral change for V1/V2
scripts running in the Conway era when only an upper bound is specified.

When both bounds are specified, both eras use `lowerBound` (inclusive) and
`strictUpperBound` (exclusive).

---

## Conway-era guards for V1/V2

When running V1 or V2 scripts in the Conway era, the following fields must be empty or
absent, otherwise script context translation fails:

- `votingProcedures` — must be empty
- `proposalProcedures` — must be empty
- `treasuryDonation` — must be zero
- `currentTreasuryValue` — must be `SNothing`

Additionally, from protocol version 11 onward, `inputs ∩ referenceInputs` must be
**disjoint** (applies to V3 only; V1/V2 don't expose reference inputs).

---

## Summary table

| Field | V1 | V2 | V3 |
|-------|-----|-----|-----|
| Fee | `Value` (ADA singleton) | `Value` (ADA singleton) | `Lovelace` (integer) |
| Mint | `Value` (zero ADA + assets) | `Value` (zero ADA + assets) | `MintValue` (no ADA) |
| Datums in TxInfo | `[(DatumHash, Datum)]` | `Map DatumHash Datum` | `Map DatumHash Datum` |
| Datum sort | ascending `DataHash` | ascending `DataHash` | ascending `DataHash` |
| Redeemers in TxInfo | not present | `Map ScriptPurpose Redeemer` | `Map ScriptPurpose Redeemer` |
| Redeemer sort | n/a | ascending `(Tag, Index)` | ascending `(Tag, Index)` |
| Required signers | `[PubKeyHash]` ascending | `[PubKeyHash]` ascending | `[PubKeyHash]` ascending |
| Withdrawals | `[(StakingCredential, Integer)]` | `Map StakingCredential Integer` | `Map Credential Lovelace` |
| Reference inputs | not present | `[TxInInfo]` ascending | `[TxInInfo]` ascending |
| Certificates | `[DCert]` | `[DCert]` | `[TxCert]` (richer types) |
| Votes | not present | not present | `Map Voter (Map GovActionId Vote)` |
| Proposals | not present | not present | `[ProposalProcedure]` (OSet order) |
| Treasury | not present | not present | `Maybe Lovelace` |
| Script args | `[Datum?, Redeemer, ScriptContext]` | `[Datum?, Redeemer, ScriptContext]` | single `ScriptContext` |
| Rewarding credential | `StakingHash Credential` | `StakingHash Credential` | `Credential` (direct) |
| ScriptPurpose constructors | 4 | 4 | 6 (+ Voting, Proposing with index) |

---

# PlutusData Encoding Details

Every type passed to a Plutus script is converted to `PlutusData` (`Data` in Plutus Core)
via `ToData` instances defined in `plutus-ledger-api`. The encoding uses these `Data`
constructors:

- **`Constr tag [fields...]`** — tagged product/sum, where `tag` is an integer and
  fields are encoded in declaration order
- **`Map [(key, value)...]`** — association list of key-value pairs
- **`List [items...]`** — ordered sequence
- **`I integer`** — arbitrary-precision integer
- **`B bytestring`** — raw bytes

The `makeIsDataSchemaIndexed` TH macro generates `ToData`/`FromData` instances that
encode each constructor as `Constr tag [field1, field2, ...]` with fields in Haskell
declaration order.

**Source:** `plutus-ledger-api` package — `PlutusLedgerApi.V1.*`, `PlutusLedgerApi.V2.*`,
`PlutusLedgerApi.V3.*` modules. Tags below are from the `makeIsDataSchemaIndexed` splices
in those modules.

---

## ScriptContext structure

### V1 / V2

```
Constr 0 [txInfo, scriptPurpose]
```

| Field index | Field | Type |
|:-----------:|-------|------|
| 0 | `scriptContextTxInfo` | `TxInfo` |
| 1 | `scriptContextPurpose` | `ScriptPurpose` |

### V3

```
Constr 0 [txInfo, redeemer, scriptInfo]
```

| Field index | Field | Type |
|:-----------:|-------|------|
| 0 | `scriptContextTxInfo` | `TxInfo` |
| 1 | `scriptContextRedeemer` | `Redeemer` (encoded as raw `Data`) |
| 2 | `scriptContextScriptInfo` | `ScriptInfo` |

---

## TxInfo field count and field order

### V1 TxInfo — 10 fields

```
Constr 0 [inputs, outputs, fee, mint, dCert, wdrl, validRange, signatories, data, id]
```

| Field index | Field | PlutusData shape |
|:-----------:|-------|------------------|
| 0 | `txInfoInputs` | `List [TxInInfo...]` |
| 1 | `txInfoOutputs` | `List [TxOut...]` |
| 2 | `txInfoFee` | `Value` (Map encoding) |
| 3 | `txInfoMint` | `Value` (Map encoding) |
| 4 | `txInfoDCert` | `List [DCert...]` |
| 5 | `txInfoWdrl` | `Map [(StakingCredential, I amount)...]` |
| 6 | `txInfoValidRange` | `Interval POSIXTime` |
| 7 | `txInfoSignatories` | `List [PubKeyHash...]` |
| 8 | `txInfoData` | `Map [(DatumHash, Datum)...]` |
| 9 | `txInfoId` | `TxId` |

Note: V1 `txInfoWdrl` and `txInfoData` are Haskell `[(k,v)]` lists but `ToData` for
`[(a,b)]` produces a `Map` node (not `List`).

### V2 TxInfo — 12 fields

```
Constr 0 [inputs, refInputs, outputs, fee, mint, dCert, wdrl, validRange,
          signatories, redeemers, data, id]
```

| Field index | Field | PlutusData shape |
|:-----------:|-------|------------------|
| 0 | `txInfoInputs` | `List [TxInInfo...]` |
| 1 | `txInfoReferenceInputs` | `List [TxInInfo...]` |
| 2 | `txInfoOutputs` | `List [TxOut...]` |
| 3 | `txInfoFee` | `Value` (Map encoding) |
| 4 | `txInfoMint` | `Value` (Map encoding) |
| 5 | `txInfoDCert` | `List [DCert...]` |
| 6 | `txInfoWdrl` | `Map [(StakingCredential, I amount)...]` |
| 7 | `txInfoValidRange` | `Interval POSIXTime` |
| 8 | `txInfoSignatories` | `List [PubKeyHash...]` |
| 9 | `txInfoRedeemers` | `Map [(ScriptPurpose, Redeemer)...]` |
| 10 | `txInfoData` | `Map [(DatumHash, Datum)...]` |
| 11 | `txInfoId` | `TxId` |

### V3 TxInfo — 16 fields

```
Constr 0 [inputs, refInputs, outputs, fee, mint, txCerts, wdrl, validRange,
          signatories, redeemers, data, id, votes, proposalProcedures,
          currentTreasuryAmount, treasuryDonation]
```

| Field index | Field | PlutusData shape |
|:-----------:|-------|------------------|
| 0 | `txInfoInputs` | `List [TxInInfo...]` |
| 1 | `txInfoReferenceInputs` | `List [TxInInfo...]` |
| 2 | `txInfoOutputs` | `List [TxOut...]` |
| 3 | `txInfoFee` | `I lovelace` |
| 4 | `txInfoMint` | `MintValue` (Map encoding, no ADA entry) |
| 5 | `txInfoTxCerts` | `List [TxCert...]` |
| 6 | `txInfoWdrl` | `Map [(Credential, I lovelace)...]` |
| 7 | `txInfoValidRange` | `Interval POSIXTime` |
| 8 | `txInfoSignatories` | `List [PubKeyHash...]` |
| 9 | `txInfoRedeemers` | `Map [(ScriptPurpose, Redeemer)...]` |
| 10 | `txInfoData` | `Map [(DatumHash, Datum)...]` |
| 11 | `txInfoId` | `TxId` |
| 12 | `txInfoVotes` | `Map [(Voter, Map [(GovernanceActionId, Vote)...])...]` |
| 13 | `txInfoProposalProcedures` | `List [ProposalProcedure...]` |
| 14 | `txInfoCurrentTreasuryAmount` | `Maybe Lovelace` |
| 15 | `txInfoTreasuryDonation` | `Maybe Lovelace` |

---

## TxId encoding

### V1 / V2

`TxId` has a `makeIsDataSchemaIndexed` instance:

```
Constr 0 [B txid_bytes]
```

The 32-byte BLAKE2b-256 hash is wrapped in a `Constr 0` tag.

### V3

`TxId` uses `deriving newtype ToData` — **no `Constr` wrapper**:

```
B txid_bytes
```

This is a **breaking change** from V1/V2. The V3 `TxId` is a bare bytestring in
PlutusData, not wrapped in `Constr 0`.

---

## TxOutRef encoding

### V1 / V2

```
Constr 0 [Constr 0 [B txid_bytes], I index]
         ^^^^^^^^^^^^^^^^^^^^^^^^^^
         V1/V2 TxId (wrapped)
```

### V3

```
Constr 0 [B txid_bytes, I index]
          ^^^^^^^^^^^^^
          V3 TxId (bare bytestring)
```

---

## TxInInfo encoding

All versions:

```
Constr 0 [txOutRef, resolvedTxOut]
```

| Field index | Field | Type |
|:-----------:|-------|------|
| 0 | `txInInfoOutRef` | `TxOutRef` (version-specific) |
| 1 | `txInInfoResolved` | `TxOut` (version-specific) |

---

## Value encoding

`Value` is a newtype over `Map CurrencySymbol (Map TokenName Integer)` with
`deriving newtype ToData`. It encodes as a **nested `Map`**, not a `Constr`:

```
Map [
  (B currency_symbol_bytes, Map [
    (B token_name_bytes, I amount),
    ...
  ]),
  ...
]
```

- `CurrencySymbol` encodes as `B` (bare bytestring, newtype-derived)
- `TokenName` encodes as `B` (bare bytestring, newtype-derived)
- ADA is represented by `CurrencySymbol ""` (empty bytestring) and `TokenName ""`

For fee in V1/V2 (ADA-only):

```
Map [(B "", Map [(B "", I fee_lovelace)])]
```

---

## MintValue encoding (V3)

`MintValue` is a newtype over the same `Map CurrencySymbol (Map TokenName Integer)`
structure with `deriving newtype ToData`. Identical `Map` encoding as `Value`, but
**without the zero-ADA entry** that V1/V2 mint fields include.

---

## Lovelace encoding

`Lovelace` is a newtype over `Integer` with `deriving newtype ToData`:

```
I lovelace_amount
```

---

## TxOut encoding

### V1

```
Constr 0 [address, value, maybeDatumHash]
```

| Field index | Field | Type |
|:-----------:|-------|------|
| 0 | `txOutAddress` | `Address` |
| 1 | `txOutValue` | `Value` |
| 2 | `txOutDatumHash` | `Maybe DatumHash` |

### V2 / V3

```
Constr 0 [address, value, outputDatum, maybeReferenceScript]
```

| Field index | Field | Type |
|:-----------:|-------|------|
| 0 | `txOutAddress` | `Address` |
| 1 | `txOutValue` | `Value` |
| 2 | `txOutDatum` | `OutputDatum` |
| 3 | `txOutReferenceScript` | `Maybe ScriptHash` |

---

## OutputDatum encoding (V2/V3)

| Constructor | Constr tag | Fields |
|-------------|:----------:|--------|
| `NoOutputDatum` | 0 | `[]` |
| `OutputDatumHash` | 1 | `[B datum_hash_bytes]` |
| `OutputDatum` | 2 | `[datum_data]` |

`DatumHash` is a newtype over `BuiltinByteString` — encodes as `B`.
`Datum` is a newtype over `BuiltinData` — encodes as the raw `Data` value directly.

---

## Address encoding

```
Constr 0 [credential, maybeStakingCredential]
```

| Field index | Field | Type |
|:-----------:|-------|------|
| 0 | `addressCredential` | `Credential` |
| 1 | `addressStakingCredential` | `Maybe StakingCredential` |

### Credential

| Constructor | Constr tag | Fields |
|-------------|:----------:|--------|
| `PubKeyCredential` | 0 | `[B pubkeyhash_bytes]` |
| `ScriptCredential` | 1 | `[B scripthash_bytes]` |

`PubKeyHash` and `ScriptHash` are newtypes over `BuiltinByteString` — bare `B`.

### StakingCredential (V1/V2)

| Constructor | Constr tag | Fields |
|-------------|:----------:|--------|
| `StakingHash` | 0 | `[Credential]` |
| `StakingPtr` | 1 | `[I slot, I txIx, I certIx]` |

### V3 credential newtypes

`ColdCommitteeCredential`, `HotCommitteeCredential`, and `DRepCredential` all use
`deriving newtype ToData` from `Credential` — identical encoding as `Credential`
(Constr 0 or Constr 1, no extra wrapper).

---

## POSIXTimeRange / Interval encoding

`POSIXTimeRange = Interval POSIXTime`

### Interval

```
Constr 0 [lowerBound, upperBound]
```

### LowerBound

```
Constr 0 [extended, I closure]
```

### UpperBound

```
Constr 0 [extended, I closure]
```

Where `closure` is a `Bool`: `False` = `Constr 0 []`, `True` = `Constr 1 []`.

### Extended

| Constructor | Constr tag | Fields |
|-------------|:----------:|--------|
| `NegInf` | 0 | `[]` |
| `Finite` | 1 | `[I posix_time_ms]` |
| `PosInf` | 2 | `[]` |

`POSIXTime` is a newtype over `Integer` — encodes as `I`.

### Full example: `always` = `Interval (LowerBound NegInf True) (UpperBound PosInf True)`

```
Constr 0 [
  Constr 0 [Constr 0 [], Constr 1 []],    -- LowerBound NegInf True
  Constr 0 [Constr 2 [], Constr 1 []]     -- UpperBound PosInf True
]
```

### Full example: `[t1, t2)` = `Interval (LowerBound (Finite t1) True) (UpperBound (Finite t2) False)`

```
Constr 0 [
  Constr 0 [Constr 1 [I t1], Constr 1 []],   -- LowerBound (Finite t1) True
  Constr 0 [Constr 1 [I t2], Constr 0 []]    -- UpperBound (Finite t2) False
]
```

---

## ScriptPurpose Constr tags

### V1 / V2

| Constructor | Constr tag | Fields |
|-------------|:----------:|--------|
| `Minting` | 0 | `[B currency_symbol]` |
| `Spending` | 1 | `[TxOutRef]` |
| `Rewarding` | 2 | `[StakingCredential]` |
| `Certifying` | 3 | `[DCert]` |

### V3

| Constructor | Constr tag | Fields |
|-------------|:----------:|--------|
| `Minting` | 0 | `[B currency_symbol]` |
| `Spending` | 1 | `[TxOutRef]` |
| `Rewarding` | 2 | `[Credential]` |
| `Certifying` | 3 | `[I index, TxCert]` |
| `Voting` | 4 | `[Voter]` |
| `Proposing` | 5 | `[I index, ProposalProcedure]` |

---

## ScriptInfo encoding (V3)

| Constructor | Constr tag | Fields |
|-------------|:----------:|--------|
| `MintingScript` | 0 | `[B currency_symbol]` |
| `SpendingScript` | 1 | `[TxOutRef, Maybe Datum]` |
| `RewardingScript` | 2 | `[Credential]` |
| `CertifyingScript` | 3 | `[I index, TxCert]` |
| `VotingScript` | 4 | `[Voter]` |
| `ProposingScript` | 5 | `[I index, ProposalProcedure]` |

---

## DCert encoding (V1/V2)

| Constructor | Constr tag | Fields |
|-------------|:----------:|--------|
| `DCertDelegRegKey` | 0 | `[StakingCredential]` |
| `DCertDelegDeRegKey` | 1 | `[StakingCredential]` |
| `DCertDelegDelegate` | 2 | `[StakingCredential, B pub_key_hash]` |
| `DCertPoolRegister` | 3 | `[B pool_id_hash, B pool_vrf_hash]` |
| `DCertPoolRetire` | 4 | `[B pool_id_hash, I epoch]` |
| `DCertGenesis` | 5 | `[]` |
| `DCertMir` | 6 | `[]` |

---

## TxCert encoding (V3)

| Constructor | Constr tag | Fields |
|-------------|:----------:|--------|
| `TxCertRegStaking` | 0 | `[Credential, Maybe Lovelace]` |
| `TxCertUnRegStaking` | 1 | `[Credential, Maybe Lovelace]` |
| `TxCertDelegStaking` | 2 | `[Credential, Delegatee]` |
| `TxCertRegDeleg` | 3 | `[Credential, Delegatee, I lovelace]` |
| `TxCertRegDRep` | 4 | `[DRepCredential, I lovelace]` |
| `TxCertUpdateDRep` | 5 | `[DRepCredential]` |
| `TxCertUnRegDRep` | 6 | `[DRepCredential, I lovelace]` |
| `TxCertPoolRegister` | 7 | `[B pool_id, B pool_vrf]` |
| `TxCertPoolRetire` | 8 | `[B pool_id, I epoch]` |
| `TxCertAuthHotCommittee` | 9 | `[ColdCommitteeCredential, HotCommitteeCredential]` |
| `TxCertResignColdCommittee` | 10 | `[ColdCommitteeCredential]` |

Conway-era protocol version 9 bug: `RegDepositTxCert` and `UnRegDepositTxCert` omit the
deposit/refund field (translated as `Nothing`). From protocol version 10 onward, the
deposit/refund is included as `Just lovelace`.

---

## Delegatee encoding (V3)

| Constructor | Constr tag | Fields |
|-------------|:----------:|--------|
| `DelegStake` | 0 | `[B pool_key_hash]` |
| `DelegVote` | 1 | `[DRep]` |
| `DelegStakeVote` | 2 | `[B pool_key_hash, DRep]` |

---

## DRep encoding (V3)

| Constructor | Constr tag | Fields |
|-------------|:----------:|--------|
| `DRep` | 0 | `[DRepCredential]` |
| `DRepAlwaysAbstain` | 1 | `[]` |
| `DRepAlwaysNoConfidence` | 2 | `[]` |

---

## Voter encoding (V3)

| Constructor | Constr tag | Fields |
|-------------|:----------:|--------|
| `CommitteeVoter` | 0 | `[HotCommitteeCredential]` |
| `DRepVoter` | 1 | `[DRepCredential]` |
| `StakePoolVoter` | 2 | `[B pool_key_hash]` |

---

## Vote encoding (V3)

| Constructor | Constr tag | Fields |
|-------------|:----------:|--------|
| `VoteNo` | 0 | `[]` |
| `VoteYes` | 1 | `[]` |
| `Abstain` | 2 | `[]` |

---

## GovernanceActionId encoding (V3)

```
Constr 0 [txId, I gov_action_ix]
```

Where `txId` is **V3 `TxId`** = bare `B txid_bytes` (no `Constr 0` wrapper).

---

## GovernanceAction encoding (V3)

| Constructor | Constr tag | Fields |
|-------------|:----------:|--------|
| `ParameterChange` | 0 | `[Maybe GovernanceActionId, ChangedParameters, Maybe ScriptHash]` |
| `HardForkInitiation` | 1 | `[Maybe GovernanceActionId, ProtocolVersion]` |
| `TreasuryWithdrawals` | 2 | `[Map Credential Lovelace, Maybe ScriptHash]` |
| `NoConfidence` | 3 | `[Maybe GovernanceActionId]` |
| `UpdateCommittee` | 4 | `[Maybe GovernanceActionId, List [ColdCommitteeCredential...], Map ColdCommitteeCredential Integer, Rational]` |
| `NewConstitution` | 5 | `[Maybe GovernanceActionId, Constitution]` |
| `InfoAction` | 6 | `[]` |

### ProtocolVersion

```
Constr 0 [I major, I minor]
```

### Constitution

```
Constr 0 [Maybe ScriptHash]
```

### ChangedParameters

Newtype over `BuiltinData` — encodes as raw `Data` directly (identity encoding).
Contains a `Map` from parameter ID integers to new values.

### Rational (PlutusTx)

Encoded as a pair: `Constr 0 [I numerator, I denominator]`.

---

## ProposalProcedure encoding (V3)

```
Constr 0 [I deposit_lovelace, credential, governanceAction]
```

| Field index | Field | Type |
|:-----------:|-------|------|
| 0 | `ppDeposit` | `Lovelace` (`I`) |
| 1 | `ppReturnAddr` | `Credential` |
| 2 | `ppGovernanceAction` | `GovernanceAction` |

---

## Committee encoding (V3)

```
Constr 0 [Map [(ColdCommitteeCredential, I epoch)...], Constr 0 [I num, I denom]]
```

| Field index | Field | Type |
|:-----------:|-------|------|
| 0 | `committeeMembers` | `Map ColdCommitteeCredential Integer` |
| 1 | `committeeQuorum` | `Rational` |

---

## Map encoding (PlutusTx.AssocMap)

Plutus `Map` is encoded using the `Data` constructor `Map` (not `Constr`):

```
Map [(key_data, value_data), ...]
```

This applies to: `txInfoWdrl`, `txInfoRedeemers`, `txInfoData`, `txInfoVotes`,
`Value` internals, and all other `Map` fields.

---

## Maybe encoding

| Constructor | Constr tag | Fields |
|-------------|:----------:|--------|
| `Just x` | 0 | `[x]` |
| `Nothing` | 1 | `[]` |

Note: this is the **opposite** of the Haskell convention where `Nothing` is typically the
first constructor. The Plutus `makeIsDataIndexed` explicitly assigns `Just = 0`,
`Nothing = 1`.

---

## Bool encoding

| Constructor | Constr tag | Fields |
|-------------|:----------:|--------|
| `False` | 0 | `[]` |
| `True` | 1 | `[]` |

Used in `Closure` (interval bounds openness).

---

## Newtype encoding rules

Types using `deriving newtype ToData` encode **identically** to their underlying type
with no extra `Constr` wrapper:

| Newtype | Underlying | PlutusData |
|---------|-----------|------------|
| `Lovelace` | `Integer` | `I n` |
| `POSIXTime` | `Integer` | `I n` |
| `CurrencySymbol` | `BuiltinByteString` | `B bytes` |
| `TokenName` | `BuiltinByteString` | `B bytes` |
| `PubKeyHash` | `BuiltinByteString` | `B bytes` |
| `ScriptHash` | `BuiltinByteString` | `B bytes` |
| `DatumHash` | `BuiltinByteString` | `B bytes` |
| `Datum` | `BuiltinData` | raw `Data` (identity) |
| `Redeemer` | `BuiltinData` | raw `Data` (identity) |
| `V3.TxId` | `BuiltinByteString` | `B bytes` |
| `V1/V2 TxId` | (via `makeIsDataSchemaIndexed`) | `Constr 0 [B bytes]` |
| `Value` | `Map CS (Map TN Integer)` | `Map [...]` |
| `MintValue` | `Map CS (Map TN Integer)` | `Map [...]` |
| `ColdCommitteeCredential` | `Credential` | same as `Credential` |
| `HotCommitteeCredential` | `Credential` | same as `Credential` |
| `DRepCredential` | `Credential` | same as `Credential` |
| `ChangedParameters` | `BuiltinData` | raw `Data` (identity) |
