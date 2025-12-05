# Cardano Transaction UTXOW Rule (Witnessing Validation)

## Overview

**UTXOW** (Unspent Transaction Output Witnessing) is Cardano's **Phase 1 transaction validation rule** that checks:
1. **Transaction witnesses** (signatures and scripts)
2. **Native script validation** (multi-signature, timelock)
3. **Metadata integrity**
4. **Genesis signatures** for MIR certificates

After UTXOW passes, the transaction proceeds to **UTXO rule** for structural validation, then **UTXOS rule** (Phase 2) for Plutus script execution in later eras.

## Rule Hierarchy

**File**: `eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Ledger.hs:322-343`

```
Transaction Validation Hierarchy:
│
├─ LEDGER (Top-level ledger rule)
│  ├─ DELEGS (Certificate processing - executed FIRST)
│  └─ UTXOW (Witnessing - Phase 1 signatures/scripts) ← THIS DOCUMENT (executed SECOND)
│     └─ UTXO (Structural validation - executed THIRD)
│        └─ UTXOS (Plutus execution - Phase 2, Alonzo+)
```

**Execution Order in LEDGER rule** (`Ledger.hs:322-343`):
```haskell
ledgerTransition :: TransitionRule (ShelleyLEDGER era)
ledgerTransition = do
  TRC (LedgerEnv slot mbCurEpochNo txIx pp account, LedgerState utxoSt certState, tx) <-
    judgmentContext

  -- Step 1: Process certificates (DELEGS rule)
  certState' <-
    trans @(EraRule "DELEGS" era) $
      TRC (DelegsEnv slot curEpochNo txIx pp tx account, certState, certs)

  -- Step 2: Process transaction witnesses and UTxO (UTXOW rule)
  utxoSt' <-
    trans @(EraRule "UTXOW" era) $
      TRC (UtxoEnv slot pp certState, utxoSt, tx)

  pure (LedgerState utxoSt' certState')
```

**Then UTXOW calls UTXO** (`Utxow.hs:333`):
```haskell
transitionRulesUTXOW = do
  -- ... UTXOW validations (steps 1-6) ...

  -- Step 7: Call UTXO rule
  trans @(EraRule "UTXO" era) $ TRC (utxoEnv, u, tx)
```

**Key Point**: UTXOW **embeds** UTXO, so UTXOW validation happens **immediately before** UTXO validation within the same rule. Witness checking (UTXOW) happens before structural checks (UTXO).

## Complete Call Chain with Evidence

This shows the **exact execution order** with file/line references:

```
User submits transaction
    ↓
LEDGER rule starts (Ledger.hs:322)
    ↓
├─ Line 329-334: DELEGS rule (processes certificates)
│  trans @(EraRule "DELEGS" era) $ TRC (DelegsEnv ..., certState, certs)
│
└─ Line 336-342: UTXOW rule (processes witnesses) ← THIS DOCUMENT
   trans @(EraRule "UTXOW" era) $ TRC (UtxoEnv slot pp certState, utxoSt, tx)
       ↓
       UTXOW validation starts (Utxow.hs:296)
           ↓
       ├─ Line 308: Validate native scripts
       ├─ Line 311: Check script presence
       ├─ Line 316: Verify VKey signatures
       ├─ Line 319: Check required witnesses
       ├─ Line 323: Validate metadata
       ├─ Line 328: Check MIR genesis signatures
       │
       └─ Line 333: UTXO rule (structural validation)
          trans @(EraRule "UTXO" era) $ TRC (utxoEnv, u, tx)
              ↓
              UTXO validation (Utxo.hs:343)
              ├─ Input validation
              ├─ Fee validation
              ├─ Value conservation
              ├─ Size limits
              └─ Other structural checks
```

**Evidence Summary**:
- **LEDGER calls UTXOW**: `Ledger.hs:336-342`
- **UTXOW calls UTXO**: `Utxow.hs:333`
- **UTXO embeds in UTXOW**: `Utxow.hs:341-344` (Embed instance with `wrapFailed = UtxoFailure`)

So the answer to "Is UTXOW before UTXO?" is **YES**:
- UTXOW executes lines 296-332 (witness validation)
- Then line 333 calls UTXO (structural validation)
- This is **sequential within the same rule transition**

## Phase 1 vs Phase 2 Distinction

| Aspect | Phase 1 (UTXOW + UTXO) | Phase 2 (UTXOS) |
|--------|------------------------|-----------------|
| **What** | Signatures, native scripts, structure | Plutus script execution |
| **When** | All transactions | Only txs with redeemers |
| **Scripts** | Native scripts only | Plutus scripts |
| **Validation** | Lightweight, deterministic | Expensive, resource-limited |
| **On Failure** | Transaction rejected | Collateral collected |

**UTXOW handles Native Scripts (Phase 1) - NOT Plutus Scripts (Phase 2)**

## Native Scripts (Phase 1)

Native scripts are simple, deterministic scripts validated in UTXOW:

### Types of Native Scripts

#### 1. Shelley Era: MultiSig
**Reference**: `eras/shelley/impl/src/Cardano/Ledger/Shelley/Scripts.hs`

```haskell
data MultiSigRaw era
  = MultiSigSignature !(KeyHash Witness)    -- Require specific signature
  | MultiSigAllOf !(StrictSeq (MultiSig era))  -- Require ALL sub-scripts
  | MultiSigAnyOf !(StrictSeq (MultiSig era))  -- Require ANY sub-script
  | MultiSigMOf !Int !(StrictSeq (MultiSig era)) -- Require M of N sub-scripts
```

**Example - 2-of-3 MultiSig**:
```haskell
RequireMOf 2
  [ RequireSignature keyHash1
  , RequireSignature keyHash2
  , RequireSignature keyHash3
  ]
```

**Validation Logic** (`Scripts.hs:233-249`):
```haskell
evalMultiSig :: Set (KeyHash Witness) -> NativeScript era -> Bool
evalMultiSig vhks = go
  where
    go = \case
      RequireSignature hk -> Set.member hk vhks  -- Check signature present
      RequireAllOf msigs -> all go msigs         -- All must validate
      RequireAnyOf msigs -> any go msigs         -- At least one validates
      RequireMOf m msigs -> isValidMOf m msigs   -- At least M validate
```

#### 2. Allegra Era: Timelock (Extends MultiSig)
**Reference**: `eras/allegra/impl/src/Cardano/Ledger/Allegra/Scripts.hs`

Allegra adds **time-based conditions**:

```haskell
data TimelockRaw era
  = TimelockSignature !(KeyHash Witness)      -- Require signature
  | TimelockAllOf !(StrictSeq (Timelock era)) -- All must validate
  | TimelockAnyOf !(StrictSeq (Timelock era)) -- Any must validate
  | TimelockMOf !Int !(StrictSeq (Timelock era)) -- M of N validate
  | TimelockTimeStart !SlotNo    -- Valid AFTER this slot (inclusive)
  | TimelockTimeExpire !SlotNo   -- Valid BEFORE this slot (exclusive)
```

**Example - Time-Locked 2-of-3**:
```haskell
RequireAllOf
  [ RequireTimeStart (SlotNo 1000)     -- Valid after slot 1000
  , RequireTimeExpire (SlotNo 2000)    -- Valid before slot 2000
  , RequireMOf 2 [ sig1, sig2, sig3 ]  -- 2 of 3 signatures
  ]
```

**Validation with Time** (`Allegra/Scripts.hs:428-451`):
```haskell
evalTimelock :: Set (KeyHash Witness) -> SlotNo -> Timelock era -> Bool
evalTimelock vhks slot = go
  where
    go = \case
      RequireTimeStart s  -> s <= slot      -- Current slot >= start
      RequireTimeExpire s -> slot < s       -- Current slot < expire
      RequireSignature hk -> Set.member hk vhks
      RequireAllOf ts     -> all go ts
      RequireAnyOf ts     -> any go ts
      RequireMOf m ts     -> isValidMOf m ts
```

**Key Difference from Plutus**:
- Native scripts are **evaluated**, not **executed**
- No computational resources consumed
- Deterministic, fast validation
- No redeemers or datums

## Source Code References

### Core Files
- **UTXOW Rule**: `eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxow.hs`
- **MultiSig Scripts**: `eras/shelley/impl/src/Cardano/Ledger/Shelley/Scripts.hs:233-260`
- **Timelock Scripts**: `eras/allegra/impl/src/Cardano/Ledger/Allegra/Scripts.hs:428-451`
- **Scripts Needed**: `eras/shelley/impl/src/Cardano/Ledger/Shelley/UTxO.hs:104-120`

## UTXOW Environment, State, and Signal

### Environment (UtxoEnv)
**Reference**: `Shelley/Rules/Utxo.hs:109-113`

```haskell
data UtxoEnv era = UtxoEnv
  { ueSlot :: SlotNo              -- Current slot for timelock validation
  , uePParams :: PParams era      -- Protocol parameters
  , ueCertState :: CertState era  -- Certificate state (for genesis keys)
  }
```

### State (UTxOState)
```haskell
data UTxOState era = UTxOState
  { utxosUtxo :: UTxO era           -- Current UTxO set
  , utxosDeposited :: Coin          -- Deposits
  , utxosFees :: Coin               -- Accumulated fees
  , utxosGovState :: GovState era   -- Governance state
  , utxosInstantStake :: InstantStake
  , utxosDonation :: Coin
  }
```

### Signal (Input)
```haskell
type Signal (EraRule "UTXOW" era) = Tx TopTx era
```

## UTXOW Validation Steps

**Reference**: `Shelley/Rules/Utxow.hs:296-333` (`transitionRulesUTXOW`)

### Step 1: Validate Native Scripts
**Reference**: `Shelley/Rules/Utxow.hs:373-381`

```haskell
validateFailedNativeScripts ::
  EraTx era => ScriptsProvided era -> Tx l era -> Test (ShelleyUtxowPredFailure era)
validateFailedNativeScripts (ScriptsProvided scriptsProvided) tx = do
  let failedScripts =
        Map.filter -- Keep only non-validating native scripts
          (maybe False (not . validateNativeScript tx) . getNativeScript)
          scriptsProvided
  failureUnless (Map.null failedScripts) $
    ScriptWitnessNotValidatingUTXOW (Map.keysSet failedScripts)
```

**Called from UTXOW rule** (`Shelley/Rules/Utxow.hs:308`):
```haskell
runTestOnSignal $ validateFailedNativeScripts scriptsProvided tx
```

**Formal Specification**:
```
∀ s ∈ range(txscripts txw) ∩ Scriptnative, runNativeScript s tx
```

**What It Does**:
1. Gets all provided scripts from transaction witnesses
2. Filters to native scripts only (excludes Plutus in later eras via `getNativeScript`)
3. Validates each native script against transaction context via `validateNativeScript`
4. Fails if any native script returns false

**Key Functions**:

1. **`getNativeScript`** (`Cardano.Ledger.Core`):
   - Extracts native script from generic Script type
   - Returns `Just script` for native, `Nothing` for Plutus
   - Shelley: All scripts are native
   - Alonzo+: Filters out Plutus scripts

2. **`validateNativeScript`** (`Shelley/Tx.hs:217-218`):
   ```haskell
   validateNativeScript = validateMultiSig
   {-# INLINE validateNativeScript #-}
   ```
   For Shelley, redirects to `validateMultiSig`.

3. **`validateMultiSig`** (`Shelley/Scripts.hs:253-260`):
   ```haskell
   validateMultiSig ::
     (ShelleyEraScript era, EraTx era, NativeScript era ~ MultiSig era) =>
     Tx t era -> NativeScript era -> Bool
   validateMultiSig tx =
     evalMultiSig $ Set.map witVKeyHash (tx ^. witsTxL . addrTxWitsL)
   {-# INLINE validateMultiSig #-}
   ```
   - Extracts key hashes from VKey witnesses
   - Calls `evalMultiSig` with those hashes and the script

4. **`evalMultiSig`** (`Shelley/Scripts.hs:233-249`):
   ```haskell
   evalMultiSig ::
     (ShelleyEraScript era, NativeScript era ~ MultiSig era) =>
     Set.Set (KeyHash Witness) -> NativeScript era -> Bool
   evalMultiSig vhks = go
     where
       isValidMOf n StrictSeq.Empty = n <= 0
       isValidMOf n (msig StrictSeq.:<| msigs) =
         n <= 0 || if go msig then isValidMOf (n - 1) msigs else isValidMOf n msigs
       go = \case
         RequireSignature hk -> Set.member hk vhks
         RequireAllOf msigs -> all go msigs
         RequireAnyOf msigs -> any go msigs
         RequireMOf m msigs -> isValidMOf m msigs
   ```
   - Recursively evaluates script tree
   - Checks signatures against provided key hashes
   - Short-circuits on failure for efficiency

**Validation Context**:
- **Shelley**: Checks signatures in `tx.witnesses.vkeyWitnesses` against `RequireSignature` constraints
- **Allegra+**: Additionally checks current slot against `RequireTimeStart`/`RequireTimeExpire`
  - Uses `validateTimelock` instead of `validateMultiSig` (`Allegra/Tx.hs:74-75`)

**Error**: `ScriptWitnessNotValidatingUTXOW (Set ScriptHash)`

---

### Step 2: Validate Script Presence
**Reference**: `Shelley/Rules/Utxow.hs:383-398`

```haskell
validateMissingScripts ::
  ShelleyScriptsNeeded era ->
  ScriptsProvided era ->
  Test (ShelleyUtxowPredFailure era)
validateMissingScripts (ShelleyScriptsNeeded sNeeded) scriptsprovided =
  sequenceA_
    [ failureUnless (sNeeded `Set.isSubsetOf` sProvided) $
        MissingScriptWitnessesUTXOW (sNeeded `Set.difference` sProvided)
    , failureUnless (sProvided `Set.isSubsetOf` sNeeded) $
        ExtraneousScriptWitnessesUTXOW (sProvided `Set.difference` sNeeded)
    ]
  where
    sProvided = Map.keysSet $ unScriptsProvided scriptsprovided
```

**Called from UTXOW rule** (`Shelley/Rules/Utxow.hs:311-312`):
```haskell
let scriptsNeeded = getScriptsNeeded utxo (tx ^. bodyTxL)
runTest $ validateMissingScripts scriptsNeeded scriptsProvided
```

**Formal Specification**:
```
{ s | (_,s) ∈ scriptsNeeded utxo tx } = dom(txscripts txw)
```

**What It Does**:
1. Computes `scriptsNeeded` - all scripts required to unlock inputs and certificates
2. Compares with `scriptsProvided` in transaction witnesses
3. Performs **bidirectional check**: scripts can't be missing AND can't be extraneous
4. Uses `sequenceA_` to collect both types of errors if they occur

**Scripts Are Needed For**:
- **Inputs locked by script addresses** (`Shelley/UTxO.hs:89-102`):
  ```haskell
  txinsScriptHashes :: Set TxIn -> UTxO era -> Set ScriptHash
  txinsScriptHashes txInps (UTxO u) = foldr add Set.empty txInps
    where
      add input ans = case Map.lookup input u of
        Just txOut -> case txOut ^. addrTxOutL of
          Addr _ (ScriptHashObj h) _ -> Set.insert h ans  -- Script address!
          _ -> ans
        Nothing -> ans
  ```

- **Withdrawals from script stake addresses** (`Shelley/UTxO.hs:118`):
  ```haskell
  withdrawals = Map.keys (unWithdrawals (txBody ^. withdrawalsTxBodyL))
  -- Extract script hashes from stake credentials
  [sh | w <- withdrawals, Just sh <- [credScriptHash (raCredential w)]]
  ```

- **Certificates with script credentials** (`Utxow.hs:115-120`):
  ```haskell
  certificates = toList (txBody ^. certsTxBodyL)
  -- Extract script hashes from certificate credentials
  [sh | c <- certificates, Just sh <- [getScriptWitnessTxCert c]]
  ```

**Errors**:
- `MissingScriptWitnessesUTXOW (Set ScriptHash)` - Scripts needed but not provided
- `ExtraneousScriptWitnessesUTXOW (Set ScriptHash)` - Scripts provided but not needed

---

### Step 3: Validate VKey Witnesses (Signatures)
**Reference**: `Utxow.hs:400-419`

```haskell
validateVerifiedWits :: EraTx era => Tx l era -> Test (ShelleyUtxowPredFailure era)
validateVerifiedWits tx =
  case failed <> failedBootstrap of
    [] -> pure ()
    nonEmpty -> failure $ InvalidWitnessesUTXOW nonEmpty
  where
    txBody = tx ^. bodyTxL
    txBodyHash = extractHash (hashAnnotated txBody)
    wvkKey (WitVKey k _) = k

    -- Check regular key witnesses
    failed =
      wvkKey
        <$> filter
          (not . verifyWitVKey txBodyHash)  -- Verify signature
          (Set.toList $ tx ^. witsTxL . addrTxWitsL)

    -- Check bootstrap witnesses (Byron addresses)
    failedBootstrap =
      bwKey
        <$> filter
          (not . verifyBootstrapWit txBodyHash)
          (Set.toList $ tx ^. witsTxL . bootAddrTxWitsL)
```

**Formal Specification**:
```
∀ (vk ↦ σ) ∈ (txwitsVKey txw), V_vk⟦ txbodyHash ⟧_σ
```

**What It Does**:
1. Extracts transaction body hash (what gets signed)
2. Verifies each VKey witness:
   - Checks signature σ is valid for public key vk
   - Uses Ed25519 signature verification
3. Separately verifies Byron bootstrap witnesses
4. Fails if ANY signature is invalid

**Cryptographic Verification** (`Keys.hs`):
```haskell
verifyWitVKey :: Hash Blake2b_256 EraIndependentTxBody -> WitVKey Witness -> Bool
verifyWitVKey txbodyHash (WitVKey vk signature) =
  verifySignedDSIGN vk txbodyHash signature
```

**Error**: `InvalidWitnessesUTXOW [VKey Witness]` - List of keys with invalid signatures

---

### Step 4: Validate Required Witnesses Present
**Reference**: `Utxow.hs:421-436`

```haskell
validateNeededWitnesses ::
  EraUTxO era =>
  Set (KeyHash Witness) ->  -- Provided witness key hashes
  CertState era ->
  UTxO era ->
  TxBody t era ->
  Test (ShelleyUtxowPredFailure era)
validateNeededWitnesses witsKeyHashes certState utxo txBody =
  let needed = getWitsVKeyNeeded certState utxo txBody
      missingWitnesses = Set.difference needed witsKeyHashes
   in failureUnless (Set.null missingWitnesses) $
        MissingVKeyWitnessesUTXOW missingWitnesses
```

**Formal Specification**:
```
witsVKeyNeeded utxo tx genDelegs ⊆ witsKeyHashes
```

**What It Does**:
1. Computes `witsVKeyNeeded` - all key hashes required to authorize transaction
2. Compares with `witsKeyHashes` - hashes of provided VKey witnesses
3. Fails if required witnesses are missing

**Witnesses Are Needed For** (`Shelley/UTxO.hs:201-259`):

1. **Inputs locked by key hashes** (payment credentials):
   ```haskell
   -- For each input, extract key hash from address
   inputSet = txBody ^. inputsTxBodyL
   utxo' = txInsFilter utxo inputSet
   -- Extract key hashes from addresses
   keyHashes = [kh | (_, txOut) <- Map.toList utxo',
                     Addr _ (KeyHashObj kh) _ <- [txOut ^. addrTxOutL]]
   ```

2. **Certificates requiring authorization**:
   - Stake registration/deregistration (stake key hash)
   - Pool registration/retirement (pool owner key hashes + pool cold key)
   - Delegation certificates (stake key hash)

3. **Withdrawals** (stake key hashes):
   ```haskell
   withdrawals = Map.keys (unWithdrawals $ txBody ^. withdrawalsTxBodyL)
   -- Extract key hashes from reward accounts
   keyHashes = [kh | RewardAccount _ (KeyHashObj kh) <- withdrawals]
   ```

4. **Required signers** (Alonzo+):
   ```haskell
   reqSigners = txBody ^. reqSignerHashesTxBodyL
   ```

5. **Protocol parameter update proposals**:
   ```haskell
   -- Extract genesis delegate keys proposing updates
   [genDelegKeyHash | (genesisKeyHash, _) <- updateProposers,
                      GenDelegPair _ genDelegKeyHash <- lookupGenDeleg genesisKeyHash]
   ```

**Error**: `MissingVKeyWitnessesUTXOW (Set (KeyHash Witness))`

---

### Step 5: Validate Metadata Integrity
**Reference**: `Utxow.hs:438-457`

```haskell
validateMetadata :: EraTx era => PParams era -> Tx l era -> Test (ShelleyUtxowPredFailure era)
validateMetadata pp tx =
  let txBody = tx ^. bodyTxL
      pv = pp ^. ppProtocolVersionL
   in case (txBody ^. auxDataHashTxBodyL, tx ^. auxDataTxL) of
        (SNothing, SNothing) -> pure ()  -- No metadata - OK

        (SJust mdh, SNothing) ->
          failure $ MissingTxMetadata mdh  -- Hash but no metadata

        (SNothing, SJust md') ->
          failure $ MissingTxBodyMetadataHash (hashTxAuxData md')  -- Metadata but no hash

        (SJust mdh, SJust md') ->
          sequenceA_
            [ -- Check hash matches
              failureUnless (hashTxAuxData md' == mdh) $
                ConflictingMetadataHash $
                  Mismatch {mismatchSupplied = mdh, mismatchExpected = hashTxAuxData md'}
            , -- Check metadata value sizes (protocol version dependent)
              when (SoftForks.validMetadata pv) $
                failureUnless (validateTxAuxData pv md') InvalidMetadata
            ]
```

**Formal Specification**:
```
((adh = ◇) ∧ (ad = ◇)) ∨ (adh = hashAD ad)
```

**What It Does**:
1. Checks transaction body metadata hash field
2. Checks actual metadata in transaction witnesses
3. Verifies:
   - If hash present, metadata must be present
   - If metadata present, hash must be present
   - Hash must match actual metadata
   - Metadata strings not too long (protocol version dependent)

**Metadata Validation** (Soft Fork Feature):
- Introduced in protocol version 5
- Limits metadata string length to prevent abuse
- Validates CBOR structure

**Errors**:
- `MissingTxMetadata TxAuxDataHash` - Hash in body but no metadata
- `MissingTxBodyMetadataHash TxAuxDataHash` - Metadata present but no hash in body
- `ConflictingMetadataHash (Mismatch TxAuxDataHash)` - Hash mismatch
- `InvalidMetadata` - Metadata string too long

---

### Step 6: Validate MIR Certificate Genesis Signatures
**Reference**: `Utxow.hs:459-485`

```haskell
validateMIRInsufficientGenesisSigs ::
  (EraTx era, ShelleyEraTxBody era) =>
  GenDelegs ->              -- Genesis delegates
  Word64 ->                 -- Quorum size
  Set (KeyHash Witness) ->  -- Provided witnesses
  Tx TopTx era ->
  Test (ShelleyUtxowPredFailure era)
validateMIRInsufficientGenesisSigs (GenDelegs genMapping) coreNodeQuorum witsKeyHashes tx =
  let genDelegates =
        Set.fromList $ asWitness . genDelegKeyHash <$> Map.elems genMapping
      khAsSet = witsKeyHashes
      genSig = eval (genDelegates ∩ khAsSet)  -- Genesis sigs present
      txBody = tx ^. bodyTxL
      mirCerts =
        StrictSeq.forceToStrict
          . Seq.filter isInstantaneousRewards
          . StrictSeq.fromStrict
          $ txBody ^. certsTxBodyL
   in failureUnless
        (not (null mirCerts) ==> Set.size genSig >= fromIntegral coreNodeQuorum)
        $ MIRInsufficientGenesisSigsUTXOW genSig
```

**Formal Specification**:
```
genSig := { hashKey gkey | gkey ∈ dom(genDelegs)} ∩ witsKeyHashes
{ c ∈ txcerts txb ∩ TxCert_mir} ≠ ∅  ⇒ (|genSig| ≥ Quorum) ∧ (d pp > 0)
```

**What It Does**:
1. Checks if transaction contains MIR (Move Instantaneous Rewards) certificates
2. MIR certificates can move funds from reserves or treasury
3. Requires threshold of genesis delegate signatures (quorum)
4. Typical quorum: 5 of 7 genesis delegates

**MIR Certificates** (`Shelley/TxCert.hs`):
```haskell
data ShelleyTxCert era
  = ...
  | MIRCert (MIRCert era)  -- Move Instantaneous Rewards

data MIRCert era = MIRCert
  { mirPot :: MIRPot          -- Reserves or Treasury
  , mirRewards :: StakeCredentials  -- Target stake credentials
  , mirAmount :: Coin         -- Amount to distribute
  }

data MIRPot = Reserves | Treasury
```

**Use Cases**:
- Emergency fund movements
- Protocol governance actions
- Requires high threshold of genesis authority

**Error**: `MIRInsufficientGenesisSigsUTXOW (Set (KeyHash Witness))`

---

### Step 7: Call UTXO Rule
**Reference**: `Utxow.hs:333`

```haskell
trans @(EraRule "UTXO" era) $ TRC (utxoEnv, u, tx)
```

After all witnessing checks pass, UTXOW calls the UTXO rule for structural validation (inputs exist, fees correct, value conserved, etc.).

## Complete Validation Order

**Reference**: `Shelley/Rules/Utxow.hs:296-333`

```haskell
transitionRulesUTXOW = do
  (TRC (utxoEnv@(UtxoEnv _ pp certState), u, tx)) <- judgmentContext

  let utxo = utxosUtxo u
      witsKeyHashes = keyHashWitnessesTxWits (tx ^. witsTxL)
      scriptsProvided = getScriptsProvided utxo tx

  -- 1. Check native scripts validate
  runTestOnSignal $ validateFailedNativeScripts scriptsProvided tx

  -- 2. Check all needed scripts are provided (no missing, no extra)
  let scriptsNeeded = getScriptsNeeded utxo (tx ^. bodyTxL)
  runTest $ validateMissingScripts scriptsNeeded scriptsProvided

  -- 3. Check all VKey witnesses have valid signatures
  runTestOnSignal $ validateVerifiedWits tx

  -- 4. Check all required witnesses are present
  runTest $ validateNeededWitnesses witsKeyHashes certState utxo (tx ^. bodyTxL)

  -- 5. Check metadata hash integrity
  runTestOnSignal $ validateMetadata pp tx

  -- 6. Check MIR certificate genesis signatures
  let genDelegs = dsGenDelegs (certState ^. certDStateL)
  coreNodeQuorum <- liftSTS $ asks quorum
  runTest $
    validateMIRInsufficientGenesisSigs genDelegs coreNodeQuorum witsKeyHashes tx

  -- 7. Call UTXO rule for structural validation
  trans @(EraRule "UTXO" era) $ TRC (utxoEnv, u, tx)
```

## Complete Error Types

**Reference**: `Shelley/Rules/Utxow.hs:112-134`

```haskell
data ShelleyUtxowPredFailure era
  = InvalidWitnessesUTXOW
      [VKey Witness]  -- Witnesses with invalid signatures

  | MissingVKeyWitnessesUTXOW
      (Set (KeyHash Witness))  -- Required witnesses not provided

  | MissingScriptWitnessesUTXOW
      (Set ScriptHash)  -- Required scripts not provided

  | ScriptWitnessNotValidatingUTXOW
      (Set ScriptHash)  -- Native scripts that failed validation

  | UtxoFailure (PredicateFailure (EraRule "UTXO" era))  -- UTXO rule failures

  | MIRInsufficientGenesisSigsUTXOW
      (Set (KeyHash Witness))  -- Insufficient genesis signatures for MIR

  | MissingTxBodyMetadataHash
      TxAuxDataHash  -- Metadata present but no hash in body

  | MissingTxMetadata
      TxAuxDataHash  -- Hash present but no metadata

  | ConflictingMetadataHash
      (Mismatch RelEQ TxAuxDataHash)  -- Metadata hash mismatch

  | InvalidMetadata  -- Metadata strings too long

  | ExtraneousScriptWitnessesUTXOW
      (Set ScriptHash)  -- Scripts provided but not needed
```

## Understanding `UtxoFailure` Error

### What is `UtxoFailure`?

**Reference**: `Shelley/Rules/Utxow.hs:122`

```haskell
| UtxoFailure (PredicateFailure (EraRule "UTXO" era))
```

`UtxoFailure` is a **wrapper error** that embeds UTXO rule failures inside UTXOW failures. This represents the failure propagation from the embedded UTXO rule back to UTXOW.

### Rule Embedding Architecture

**Reference**: `Shelley/Rules/Utxow.hs:335-344`

```haskell
instance
  ( Era era
  , STS (ShelleyUTXO era)
  , PredicateFailure (EraRule "UTXO" era) ~ ShelleyUtxoPredFailure era
  , Event (EraRule "UTXO" era) ~ UtxoEvent era
  ) =>
  Embed (ShelleyUTXO era) (ShelleyUTXOW era)  -- UTXO embedded in UTXOW
  where
  wrapFailed = UtxoFailure  -- Wrap UTXO errors in UtxoFailure
  wrapEvent = UtxoEvent
```

**Key Concept**: UTXOW **embeds** the UTXO rule. When UTXOW calls UTXO (step 7), any UTXO errors are automatically wrapped in `UtxoFailure` and propagated back.

### How UTXO is Called from UTXOW

**Reference**: `Shelley/Rules/Utxow.hs:333`

```haskell
transitionRulesUTXOW = do
  (TRC (utxoEnv@(UtxoEnv _ pp certState), u, tx)) <- judgmentContext

  -- ... UTXOW validations (steps 1-6) ...

  -- Step 7: Call embedded UTXO rule
  trans @(EraRule "UTXO" era) $ TRC (utxoEnv, u, tx)
```

The `trans` function transitions to the embedded UTXO rule. If UTXO fails, the error is automatically wrapped via `wrapFailed = UtxoFailure`.

### Error Injection Mechanism

**Reference**: `Shelley/Rules/Utxow.hs:140-144`

```haskell
-- Allow UTXO errors to be injected into UTXOW
instance InjectRuleFailure "UTXOW" ShelleyUtxoPredFailure ShelleyEra where
  injectFailure = UtxoFailure

-- Allow PPUP errors to be injected into UTXOW (via UTXO)
instance InjectRuleFailure "UTXOW" ShelleyPpupPredFailure ShelleyEra where
  injectFailure = UtxoFailure . injectFailure
```

This type class instance allows UTXO errors to be automatically injected into the UTXOW error type through the `UtxoFailure` constructor.

### What UTXO Errors Can Occur?

**Reference**: `Shelley/Rules/Utxo.hs:168-191`

```haskell
data ShelleyUtxoPredFailure era
  = BadInputsUTxO (Set TxIn)
      -- Transaction inputs don't exist in UTxO

  | ExpiredUTxO (Mismatch RelLTEQ SlotNo)
      -- Transaction expired (slot > TTL)

  | MaxTxSizeUTxO (Mismatch RelLTEQ Word32)
      -- Transaction too large

  | InputSetEmptyUTxO
      -- No inputs provided

  | FeeTooSmallUTxO (Mismatch RelGTEQ Coin)
      -- Fee less than minimum required

  | ValueNotConservedUTxO (Mismatch RelEQ (Value era))
      -- Value in ≠ value out (fundamental invariant!)

  | WrongNetwork Network (Set Addr)
      -- Output addresses for wrong network

  | WrongNetworkWithdrawal Network (Set RewardAccount)
      -- Withdrawal addresses for wrong network

  | OutputTooSmallUTxO [TxOut era]
      -- Outputs below minimum UTxO value

  | UpdateFailure (EraRuleFailure "PPUP" era)
      -- Protocol parameter update failure (nested)

  | OutputBootAddrAttrsTooBig [TxOut era]
      -- Bootstrap address attributes exceed 64 bytes
```

See `tx-validation/utxo.md` for complete details on each UTXO error.

### Example: Complete Error Path

```
User submits transaction
    ↓
UTXOW validation starts
    ↓
1. validateFailedNativeScripts → ✓ Pass
2. validateMissingScripts → ✓ Pass
3. validateVerifiedWits → ✓ Pass
4. validateNeededWitnesses → ✓ Pass
5. validateMetadata → ✓ Pass
6. validateMIRInsufficientGenesisSigs → ✓ Pass
    ↓
7. trans @UTXO (call UTXO rule)
    ↓
    UTXO: validateBadInputsUTxO
        → Input TxIn{hash=abc..., ix=0} not in UTxO
        → Returns: BadInputsUTxO {TxIn{hash=abc..., ix=0}}
    ↓
    wrapFailed wraps error
    ↓
UTXOW returns: UtxoFailure (BadInputsUTxO {TxIn{hash=abc..., ix=0}})
    ↓
Transaction rejected
```

### Haskell Code Walkthrough

**Step 1: UTXOW defines error type with UtxoFailure constructor**

`Shelley/Rules/Utxow.hs:112-122`:
```haskell
data ShelleyUtxowPredFailure era
  = InvalidWitnessesUTXOW [VKey Witness]
  | MissingVKeyWitnessesUTXOW (Set (KeyHash Witness))
  | MissingScriptWitnessesUTXOW (Set ScriptHash)
  | ScriptWitnessNotValidatingUTXOW (Set ScriptHash)
  | UtxoFailure (PredicateFailure (EraRule "UTXO" era))  -- ← Wrapper
  | MIRInsufficientGenesisSigsUTXOW (Set (KeyHash Witness))
  | MissingTxBodyMetadataHash TxAuxDataHash
  | MissingTxMetadata TxAuxDataHash
  | ConflictingMetadataHash (Mismatch RelEQ TxAuxDataHash)
  | InvalidMetadata
  | ExtraneousScriptWitnessesUTXOW (Set ScriptHash)
```

**Step 2: Embed instance connects UTXO to UTXOW**

`Shelley/Rules/Utxow.hs:335-344`:
```haskell
instance Embed (ShelleyUTXO era) (ShelleyUTXOW era) where
  wrapFailed = UtxoFailure  -- When UTXO fails, wrap error
  wrapEvent = UtxoEvent
```

**Step 3: UTXOW calls UTXO via trans**

`Shelley/Rules/Utxow.hs:333`:
```haskell
trans @(EraRule "UTXO" era) $ TRC (utxoEnv, u, tx)
```

**Step 4: UTXO validates and may fail**

`Shelley/Rules/Utxo.hs:468-476` (example):
```haskell
validateBadInputsUTxO ::
  UTxO era -> Set TxIn -> Test (ShelleyUtxoPredFailure era)
validateBadInputsUTxO utxo inputs =
  failureUnless (Set.null badInputs) $ BadInputsUTxO badInputs
  where
    badInputs = Set.filter (`Map.notMember` unUTxO utxo) inputs
```

If `badInputs` is not empty, returns `BadInputsUTxO badInputs`.

**Step 5: Error wrapped automatically**

The STS (State Transition System) framework automatically applies `wrapFailed`:
```haskell
UtxoFailure (BadInputsUTxO badInputs)
```

**Step 6: Error propagates to caller**

The wrapped error is returned to whoever called UTXOW (typically LEDGER rule).

### Why This Design?

**Hierarchical Rule Structure**:

**Reference**: `Ledger.hs:322-343` (LEDGER calls UTXOW) and `Utxow.hs:333` (UTXOW calls UTXO)

```
LEDGER (Ledger.hs:328-342)
  ├─ DELEGS (certificate processing - line 329)
  └─ UTXOW (witnessing - line 337)
      └─ UTXO (structural - Utxow.hs:333)
          └─ PPUP (protocol params)
```

**Execution Order**:
1. LEDGER calls DELEGS to process certificates (`Ledger.hs:329-334`)
2. LEDGER calls UTXOW to process witnesses (`Ledger.hs:336-342`)
3. UTXOW calls UTXO to process structural validation (`Utxow.hs:333`)
4. UTXO calls PPUP to process protocol parameter updates

Each rule can fail with its own error type. The embedding mechanism allows:
1. **Error Preservation**: Original error information maintained
2. **Error Context**: Clear which rule failed (via wrapper constructors)
3. **Type Safety**: Compile-time guarantee of error propagation
4. **Composability**: Rules can be nested arbitrarily

### Common UtxoFailure Scenarios

#### Scenario 1: Input Doesn't Exist
```haskell
UtxoFailure (BadInputsUTxO {TxIn (TxId "abc...") 0})
```
**Cause**: Transaction references input that's not in current UTxO set (already spent or never existed)

#### Scenario 2: Insufficient Fee
```haskell
UtxoFailure (FeeTooSmallUTxO (Mismatch {supplied = 150000, expected = 200000}))
```
**Cause**: Transaction fee (150k lovelace) less than minimum (200k lovelace)

#### Scenario 3: Value Not Conserved
```haskell
UtxoFailure (ValueNotConservedUTxO (Mismatch {supplied = 1000 ADA, expected = 900 ADA}))
```
**Cause**: Input value (1000 ADA) ≠ output value + fee (900 ADA) - creating 100 ADA from nothing!

#### Scenario 4: Transaction Expired
```haskell
UtxoFailure (ExpiredUTxO (Mismatch {supplied = SlotNo 1000, expected = SlotNo 999}))
```
**Cause**: Current slot (1000) >= transaction TTL (1000)

### Debugging UtxoFailure

When you see `UtxoFailure` in an error:

1. **Look at the wrapped error** - it tells you what actually went wrong
2. **This means UTXOW passed** - witnessing was fine, structural validation failed
3. **Check the specific UTXO error** - see `tx-validation/utxo.md` for details
4. **Common fixes**:
   - `BadInputsUTxO`: Use unspent inputs
   - `FeeTooSmallUTxO`: Increase fee
   - `ValueNotConservedUTxO`: Fix input/output balance
   - `ExpiredUTxO`: Update transaction TTL

## Native Script Validation Deep Dive

### Getting Scripts Needed

**Reference**: `Shelley/UTxO.hs:104-120`

```haskell
getShelleyScriptsNeeded ::
  EraTxBody era =>
  UTxO era ->
  TxBody l era ->
  ShelleyScriptsNeeded era
getShelleyScriptsNeeded u txBody =
  ShelleyScriptsNeeded
    ( scriptHashes
        `Set.union` Set.fromList
          [sh | w <- withdrawals, Just sh <- [credScriptHash (raCredential w)]]
        `Set.union` Set.fromList
          [sh | c <- certificates, Just sh <- [getScriptWitnessTxCert c]]
    )
  where
    withdrawals = Map.keys (unWithdrawals (txBody ^. withdrawalsTxBodyL))
    scriptHashes = txinsScriptHashes (txBody ^. inputsTxBodyL) u
    certificates = toList (txBody ^. certsTxBodyL)
```

**Process**:
1. **For each input**: Look up output in UTxO, check if address is script hash
2. **For each withdrawal**: Extract stake credential, check if script hash
3. **For each certificate**: Check if certificate credential is script hash

### Getting Scripts Provided

**Reference**: `Shelley/UTxO.hs:190`

```haskell
getScriptsProvided _ tx = ScriptsProvided (tx ^. witsTxL . scriptTxWitsL)
```

Simply extracts all scripts from transaction witnesses.

### Validating Native Scripts

**Reference**: `Shelley/Scripts.hs:252-260`

```haskell
validateMultiSig ::
  (ShelleyEraScript era, EraTx era, NativeScript era ~ MultiSig era) =>
  Tx t era ->
  NativeScript era ->
  Bool
validateMultiSig tx =
  evalMultiSig $ Set.map witVKeyHash (tx ^. witsTxL . addrTxWitsL)
```

**Process**:
1. Extract all VKey witness key hashes from transaction
2. Call `evalMultiSig` with those key hashes and the script
3. Script evaluates to `True` or `False`

**For Timelock (Allegra+)** (`Allegra/Tx.hs:74`):
```haskell
validateNativeScript = validateTimelock

validateTimelock ::
  (AllegraEraScript era, EraTx era, NativeScript era ~ Timelock era) =>
  Tx t era ->
  NativeScript era ->
  Bool
validateTimelock tx script =
  evalTimelock vhks slot script
  where
    vhks = Set.map witVKeyHash (tx ^. witsTxL . addrTxWitsL)
    slot = tx ^. bodyTxL . vldtTxBodyL . invalidHereafterL
```

Additionally passes current slot for time-based validation.

## Key Differences Across Eras

### Shelley
- Only MultiSig native scripts
- Simple signature-based validation
- No time constraints

### Allegra & Mary
- Adds Timelock scripts with time bounds
- `RequireTimeStart` / `RequireTimeExpire`
- Mary adds native tokens but same UTXOW rules

### Alonzo
- Introduces Plutus scripts (Phase 2)
- **Native scripts still validated in UTXOW (Phase 1)**
- Plutus scripts validated in UTXOS (Phase 2)
- UTXOW unchanged - still validates native scripts only

### Babbage & Conway
- Reference inputs feature
- **Native scripts still in UTXOW (Phase 1)**
- Enhanced Plutus in UTXOS (Phase 2)

**Key Insight**: Native script validation in UTXOW has remained fundamentally the same across all eras. Plutus scripts are a separate Phase 2 concern.

## Formal Specification Summary

From the Shelley formal specification:

```
UTXOW Rule Judgement:
Γ ⊢ utxo →UTXOW[tx] utxo'

Prerequisites:
1. ∀ s ∈ range(txscripts txw) ∩ Scriptnative, runNativeScript s tx
   (All native scripts validate)

2. { s | (_,s) ∈ scriptsNeeded utxo tx } = dom(txscripts txw)
   (Scripts needed = scripts provided)

3. ∀ (vk ↦ σ) ∈ (txwitsVKey txw), V_vk⟦ txbodyHash ⟧_σ
   (All signatures valid)

4. witsVKeyNeeded utxo tx genDelegs ⊆ witsKeyHashes
   (All needed witnesses present)

5. ((adh = ◇) ∧ (ad = ◇)) ∨ (adh = hashAD ad)
   (Metadata hash consistent)

6. { c ∈ txcerts txb ∩ TxCert_mir} ≠ ∅  ⇒ |genSig| ≥ Quorum
   (MIR certificates have genesis quorum)

Then call UTXO rule: Γ ⊢ utxo →UTXO[tx] utxo'
```

## Important Architectural Points

### 1. UTXOW Before UTXO
UTXOW is evaluated **before** UTXO. This means:
- Signatures checked before inputs existence
- Scripts validated before fee calculation
- Witnesses verified before value conservation

**Rationale**: Fail fast on cryptographic failures before expensive structural checks.

### 2. Native vs Plutus Scripts
```
┌─────────────────────────────────────────┐
│ Native Scripts (MultiSig/Timelock)      │
│ - Validated in UTXOW (Phase 1)          │
│ - Lightweight, deterministic            │
│ - No redeemers or datums                │
│ - Success/failure immediate             │
├─────────────────────────────────────────┤
│ Plutus Scripts (Phase 2, Alonzo+)       │
│ - Executed in UTXOS (Phase 2)           │
│ - Expensive, resource-limited           │
│ - Requires redeemers and datums         │
│ - Failure collects collateral           │
└─────────────────────────────────────────┘
```

### 3. Script Hashes vs Script Content
- Transaction inputs/certificates reference scripts by **hash**
- Transaction witnesses provide **actual script content**
- Validation checks hash(script) matches referenced hash
- This allows scripts to be large without bloating references

### 4. VKey Witnesses vs Script Witnesses
```
Transaction Witnesses:
├─ addrTxWitsL      (VKey witnesses - Ed25519 signatures)
├─ bootAddrTxWitsL  (Bootstrap witnesses - Byron era)
├─ scriptTxWitsL    (Native scripts - MultiSig/Timelock)
└─ dataTxWitsL      (Datums for Plutus - Alonzo+)
```

## Performance Characteristics

### Native Script Validation
- **Time Complexity**: O(n) for n signatures in MultiSig tree
- **Space Complexity**: O(d) for depth d of script tree
- **Deterministic**: Always same result for same inputs
- **Fast**: Milliseconds for typical scripts

### Comparison to Plutus (Phase 2)
- Native: ~1ms for 10-signature MultiSig
- Plutus: Can consume millions of CPU steps
- Native: No ExUnits budget needed
- Plutus: Must fit within maxTxExUnits limit

## Common Validation Failures

### 1. `ScriptWitnessNotValidatingUTXOW`
**Cause**: Native script evaluated to `False`
**Common Reasons**:
- Missing required signature in MultiSig
- Transaction outside timelock validity window
- M-of-N requirement not met (e.g., only 1 of required 2 signatures)

**Example**:
```
Script: RequireMOf 2 [sig1, sig2, sig3]
Witnesses: [sig1]
Result: FAIL - need 2 signatures, only have 1
```

### 2. `MissingScriptWitnessesUTXOW`
**Cause**: Input/certificate requires script but script not provided
**Common Reasons**:
- Forgot to include script in transaction witnesses
- Script hash in address doesn't match provided script
- Using wrong script for input

### 3. `MissingVKeyWitnessesUTXOW`
**Cause**: Transaction needs signature but it's not provided
**Common Reasons**:
- Forgot to sign with required key
- Input locked by key hash not in witnesses
- Certificate requires authorization not provided
- Withdrawal from stake address without signing

### 4. `InvalidWitnessesUTXOW`
**Cause**: Signature cryptographically invalid
**Common Reasons**:
- Wrong private key used for signing
- Transaction body modified after signing
- Signature corruption during transmission
- Key mismatch (signed with different key than claimed)

## Complete Haskell Code Flow with Line References

This section traces the complete execution path through the Haskell codebase.

### 1. Entry Point: UTXOW Transition Rule

**File**: `eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxow.hs`

**Lines 278-333**: Main `transitionRulesUTXOW` function
```haskell
transitionRulesUTXOW ::
  forall era.
  ( EraUTxO era
  , ShelleyEraTxBody era
  , ScriptsNeeded era ~ ShelleyScriptsNeeded era
  , BaseM (EraRule "UTXOW" era) ~ ShelleyBase
  , Embed (EraRule "UTXO" era) (EraRule "UTXOW" era)
  , Environment (EraRule "UTXO" era) ~ UtxoEnv era
  , State (EraRule "UTXO" era) ~ UTxOState era
  , Signal (EraRule "UTXO" era) ~ Tx TopTx era
  , Environment (EraRule "UTXOW" era) ~ UtxoEnv era
  , State (EraRule "UTXOW" era) ~ UTxOState era
  , Signal (EraRule "UTXOW" era) ~ Tx TopTx era
  , InjectRuleFailure "UTXOW" ShelleyUtxowPredFailure era
  , STS (EraRule "UTXOW" era)
  , EraCertState era
  ) =>
  TransitionRule (EraRule "UTXOW" era)
```

**Line 297**: Extract environment, state, and transaction
```haskell
(TRC (utxoEnv@(UtxoEnv _ pp certState), u, tx)) <- judgmentContext
```

**Lines 301-303**: Prepare validation data
```haskell
let utxo = utxosUtxo u
    witsKeyHashes = keyHashWitnessesTxWits (tx ^. witsTxL)
    scriptsProvided = getScriptsProvided utxo tx
```

### 2. Step 1: Validate Native Scripts (Line 308)

```haskell
runTestOnSignal $ validateFailedNativeScripts scriptsProvided tx
```

**Function defined at lines 373-381**:
```haskell
validateFailedNativeScripts ::
  EraTx era => ScriptsProvided era -> Tx l era -> Test (ShelleyUtxowPredFailure era)
validateFailedNativeScripts (ScriptsProvided scriptsProvided) tx = do
  let failedScripts =
        Map.filter
          (maybe False (not . validateNativeScript tx) . getNativeScript)
          scriptsProvided
  failureUnless (Map.null failedScripts) $
    ScriptWitnessNotValidatingUTXOW (Map.keysSet failedScripts)
```

**Calls** → `validateNativeScript` (era-dependent):
- **Shelley**: `eras/shelley/impl/src/Cardano/Ledger/Shelley/Tx.hs:217-218`
  ```haskell
  validateNativeScript = validateMultiSig
  ```

- **Allegra/Mary/Alonzo/Babbage/Conway**: Uses `validateTimelock`
  - `eras/allegra/impl/src/Cardano/Ledger/Allegra/Tx.hs:74-75`

**Calls** → `validateMultiSig` or `validateTimelock`:
- **MultiSig**: `eras/shelley/impl/src/Cardano/Ledger/Shelley/Scripts.hs:253-260`
  ```haskell
  validateMultiSig tx =
    evalMultiSig $ Set.map witVKeyHash (tx ^. witsTxL . addrTxWitsL)
  ```

**Calls** → `evalMultiSig`:
- **File**: `eras/shelley/impl/src/Cardano/Ledger/Shelley/Scripts.hs:233-249`
  ```haskell
  evalMultiSig ::
    (ShelleyEraScript era, NativeScript era ~ MultiSig era) =>
    Set.Set (KeyHash Witness) -> NativeScript era -> Bool
  evalMultiSig vhks = go
    where
      go = \case
        RequireSignature hk -> Set.member hk vhks
        RequireAllOf msigs -> all go msigs
        RequireAnyOf msigs -> any go msigs
        RequireMOf m msigs -> isValidMOf m msigs
  ```

### 3. Step 2: Validate Script Presence (Lines 311-312)

```haskell
let scriptsNeeded = getScriptsNeeded utxo (tx ^. bodyTxL)
runTest $ validateMissingScripts scriptsNeeded scriptsProvided
```

**`getScriptsNeeded`** → `eras/shelley/impl/src/Cardano/Ledger/Shelley/UTxO.hs:104-120`
```haskell
getShelleyScriptsNeeded ::
  EraTxBody era => UTxO era -> TxBody l era -> ShelleyScriptsNeeded era
getShelleyScriptsNeeded u txBody =
  ShelleyScriptsNeeded
    ( scriptHashes
        `Set.union` Set.fromList
          [sh | w <- withdrawals, Just sh <- [credScriptHash (raCredential w)]]
        `Set.union` Set.fromList
          [sh | c <- certificates, Just sh <- [getScriptWitnessTxCert c]]
    )
  where
    withdrawals = Map.keys (unWithdrawals (txBody ^. withdrawalsTxBodyL))
    scriptHashes = txinsScriptHashes (txBody ^. inputsTxBodyL) u
    certificates = toList (txBody ^. certsTxBodyL)
```

**`validateMissingScripts`** → `eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxow.hs:383-398`

### 4. Step 3: Validate VKey Signatures (Line 316)

```haskell
runTestOnSignal $ validateVerifiedWits tx
```

**Function at lines 400-419**:
```haskell
validateVerifiedWits :: EraTx era => Tx l era -> Test (ShelleyUtxowPredFailure era)
validateVerifiedWits tx =
  case failed <> failedBootstrap of
    [] -> pure ()
    nonEmpty -> failure $ InvalidWitnessesUTXOW nonEmpty
  where
    txBody = tx ^. bodyTxL
    txBodyHash = extractHash (hashAnnotated txBody)
    wvkKey (WitVKey k _) = k
    failed =
      wvkKey
        <$> filter
          (not . verifyWitVKey txBodyHash)
          (Set.toList $ tx ^. witsTxL . addrTxWitsL)
    failedBootstrap =
      bwKey
        <$> filter
          (not . verifyBootstrapWit txBodyHash)
          (Set.toList $ tx ^. witsTxL . bootAddrTxWitsL)
```

**Calls** → `verifyWitVKey` (cryptographic verification in `Cardano.Ledger.Keys`)

### 5. Step 4: Validate Required Witnesses (Line 319)

```haskell
runTest $ validateNeededWitnesses witsKeyHashes certState utxo (tx ^. bodyTxL)
```

**Function at lines 421-436**:
```haskell
validateNeededWitnesses ::
  EraUTxO era =>
  Set (KeyHash Witness) -> CertState era -> UTxO era -> TxBody t era ->
  Test (ShelleyUtxowPredFailure era)
validateNeededWitnesses witsKeyHashes certState utxo txBody =
  let needed = getWitsVKeyNeeded certState utxo txBody
      missingWitnesses = Set.difference needed witsKeyHashes
   in failureUnless (Set.null missingWitnesses) $
        MissingVKeyWitnessesUTXOW missingWitnesses
```

**Calls** → `getWitsVKeyNeeded` → `eras/shelley/impl/src/Cardano/Ledger/Shelley/UTxO.hs:201-259`

### 6. Step 5: Validate Metadata (Line 323)

```haskell
runTestOnSignal $ validateMetadata pp tx
```

**Function at lines 438-457**:
```haskell
validateMetadata :: EraTx era => PParams era -> Tx l era -> Test (ShelleyUtxowPredFailure era)
validateMetadata pp tx =
  let txBody = tx ^. bodyTxL
      pv = pp ^. ppProtocolVersionL
   in case (txBody ^. auxDataHashTxBodyL, tx ^. auxDataTxL) of
        (SNothing, SNothing) -> pure ()
        (SJust mdh, SNothing) -> failure $ MissingTxMetadata mdh
        (SNothing, SJust md') ->
          failure $ MissingTxBodyMetadataHash (hashTxAuxData md')
        (SJust mdh, SJust md') ->
          sequenceA_
            [ failureUnless (hashTxAuxData md' == mdh) $
                ConflictingMetadataHash $
                  Mismatch {mismatchSupplied = mdh, mismatchExpected = hashTxAuxData md'}
            , when (SoftForks.validMetadata pv) $
                failureUnless (validateTxAuxData pv md') InvalidMetadata
            ]
```

### 7. Step 6: Validate MIR Signatures (Lines 328-331)

```haskell
let genDelegs = dsGenDelegs (certState ^. certDStateL)
coreNodeQuorum <- liftSTS $ asks quorum
runTest $
  validateMIRInsufficientGenesisSigs genDelegs coreNodeQuorum witsKeyHashes tx
```

**Function at lines 459-485**:
```haskell
validateMIRInsufficientGenesisSigs ::
  ( EraTx era, ShelleyEraTxBody era ) =>
  GenDelegs -> Word64 -> Set (KeyHash Witness) -> Tx TopTx era ->
  Test (ShelleyUtxowPredFailure era)
validateMIRInsufficientGenesisSigs (GenDelegs genMapping) coreNodeQuorum witsKeyHashes tx =
  let genDelegates =
        Set.fromList $ asWitness . genDelegKeyHash <$> Map.elems genMapping
      khAsSet = witsKeyHashes
      genSig = eval (genDelegates ∩ khAsSet)
      txBody = tx ^. bodyTxL
      mirCerts =
        StrictSeq.forceToStrict
          . Seq.filter isInstantaneousRewards
          . StrictSeq.fromStrict
          $ txBody ^. certsTxBodyL
   in failureUnless
        (not (null mirCerts) ==> Set.size genSig >= fromIntegral coreNodeQuorum)
        $ MIRInsufficientGenesisSigsUTXOW genSig
```

### 8. Step 7: Call UTXO Rule (Line 333)

```haskell
trans @(EraRule "UTXO" era) $ TRC (utxoEnv, u, tx)
```

**This calls the UTXO rule**: `eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxo.hs`

**Error wrapping** (lines 335-344):
```haskell
instance Embed (ShelleyUTXO era) (ShelleyUTXOW era) where
  wrapFailed = UtxoFailure  -- Wraps UTXO errors
  wrapEvent = UtxoEvent
```

### Complete Call Graph

```
UTXOW (Utxow.hs:296-333)
├─ validateFailedNativeScripts (Utxow.hs:373-381)
│  ├─ getNativeScript (Core)
│  ├─ validateNativeScript (era-dependent)
│  │  └─ Shelley: validateMultiSig (Scripts.hs:253-260)
│  │     └─ evalMultiSig (Scripts.hs:233-249)
│  │  └─ Allegra+: validateTimelock (Allegra/Tx.hs:74-75)
│  │     └─ evalTimelock (Allegra/Scripts.hs:428-451)
├─ validateMissingScripts (Utxow.hs:383-398)
│  ├─ getScriptsNeeded (UTxO.hs:104-120)
│  │  └─ txinsScriptHashes (UTxO.hs:89-102)
│  └─ getScriptsProvided (UTxO.hs:190)
├─ validateVerifiedWits (Utxow.hs:400-419)
│  ├─ verifyWitVKey (Keys - cryptographic Ed25519)
│  └─ verifyBootstrapWit (Keys - Byron bootstrap)
├─ validateNeededWitnesses (Utxow.hs:421-436)
│  └─ getWitsVKeyNeeded (UTxO.hs:201-259)
├─ validateMetadata (Utxow.hs:438-457)
│  └─ validateTxAuxData (protocol version dependent)
├─ validateMIRInsufficientGenesisSigs (Utxow.hs:459-485)
│  └─ isInstantaneousRewards (TxCert)
└─ trans @UTXO (Utxow.hs:333)
   └─ UTXO rule (Utxo.hs)
      └─ Errors wrapped via wrapFailed = UtxoFailure (Utxow.hs:343)
```

### Key Type Class Instances

**Error Injection** (lines 140-144):
```haskell
instance InjectRuleFailure "UTXOW" ShelleyUtxoPredFailure ShelleyEra where
  injectFailure = UtxoFailure

instance InjectRuleFailure "UTXOW" ShelleyPpupPredFailure ShelleyEra where
  injectFailure = UtxoFailure . injectFailure
```

**STS Instance** (lines 346-370):
```haskell
instance
  ( EraTx era
  , EraUTxO era
  , ShelleyEraTxBody era
  , ScriptsNeeded era ~ ShelleyScriptsNeeded era
  , Embed (EraRule "UTXO" era) (ShelleyUTXOW era)
  , Environment (EraRule "UTXO" era) ~ UtxoEnv era
  , State (EraRule "UTXO" era) ~ UTxOState era
  , Signal (EraRule "UTXO" era) ~ Tx TopTx era
  , EraRule "UTXOW" era ~ ShelleyUTXOW era
  , InjectRuleFailure "UTXOW" ShelleyUtxowPredFailure era
  , EraGov era
  , EraCertState era
  ) =>
  STS (ShelleyUTXOW era)
  where
  type State (ShelleyUTXOW era) = UTxOState era
  type Signal (ShelleyUTXOW era) = Tx TopTx era
  type Environment (ShelleyUTXOW era) = UtxoEnv era
  type BaseM (ShelleyUTXOW era) = ShelleyBase
  type PredicateFailure (ShelleyUTXOW era) = ShelleyUtxowPredFailure era
  type Event (ShelleyUTXOW era) = ShelleyUtxowEvent era
  transitionRules = [transitionRulesUTXOW]
  initialRules = [initialLedgerStateUTXOW]
```

This establishes UTXOW as a State Transition System with its transition rules.

## UTXOW Evolution Across Eras

### Shelley Era: Base UTXOW Implementation

**File**: `eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxow.hs`

Shelley defines the **complete UTXOW rule** with all core validation logic:
- Native script validation (MultiSig)
- Script presence checking
- VKey witness verification
- Required witnesses validation
- Metadata integrity
- MIR certificate genesis signatures

**Key Point**: All subsequent eras **reuse** Shelley's validation functions.

---

### Allegra Era: Zero Changes to UTXOW

**File**: `eras/allegra/impl/src/Cardano/Ledger/Allegra/Rules/Utxow.hs` (102 lines)

**Key Insight**: Allegra makes **ZERO changes** to the UTXOW rule itself!

**Line 75**:
```haskell
transitionRules = [transitionRulesUTXOW]  -- Uses Shelley's function directly!
```

**What Allegra Changed**:
1. **Script Type**: Adds Timelock scripts (but validated the same way as MultiSig)
2. **UTXO Rule**: Adds validity intervals (in UTXO, not UTXOW)
3. **Error Type**: Uses `ShelleyUtxowPredFailure` unchanged (line 34)

**Architecture** (`Allegra/Rules/Utxow.hs:51-79`):
```haskell
instance STS (AllegraUTXOW era) where
  type State (AllegraUTXOW era) = UTxOState era
  type Signal (AllegraUTXOW era) = Tx TopTx era
  type Environment (AllegraUTXOW era) = UtxoEnv era
  type BaseM (AllegraUTXOW era) = ShelleyBase
  type PredicateFailure (AllegraUTXOW era) = ShelleyUtxowPredFailure era  -- ← Same!
  type Event (AllegraUTXOW era) = ShelleyUtxowEvent era

  transitionRules = [transitionRulesUTXOW]  -- ← Shelley's function!
  initialRules = []
```

**Why No Changes Needed**:
- Timelock scripts extend MultiSig
- `validateNativeScript` is era-polymorphic:
  - Shelley: calls `validateMultiSig`
  - Allegra: calls `validateTimelock`
- Both return `Bool`, same validation interface
- UTXOW doesn't care about script internals, just success/failure

**Error Injection** (`Allegra/Rules/Utxow.hs:36-45`):
```haskell
instance InjectRuleFailure "UTXOW" ShelleyUtxowPredFailure AllegraEra

instance InjectRuleFailure "UTXOW" AllegraUtxoPredFailure AllegraEra where
  injectFailure = UtxoFailure  -- Wrap Allegra UTXO errors

instance InjectRuleFailure "UTXOW" ShelleyUtxoPredFailure AllegraEra where
  injectFailure = UtxoFailure . injectFailure  -- Wrap Shelley UTXO errors

instance InjectRuleFailure "UTXOW" ShelleyPpupPredFailure AllegraEra where
  injectFailure = UtxoFailure . injectFailure  -- Wrap PPUP errors
```

**Complete Allegra UTXOW Module**:
```haskell
module Cardano.Ledger.Allegra.Rules.Utxow (AllegraUTXOW) where

-- Imports...

-- Error type: reuse Shelley's
type instance EraRuleFailure "UTXOW" AllegraEra = ShelleyUtxowPredFailure AllegraEra

-- Error injection instances (allow UTXO errors to propagate)
instance InjectRuleFailure "UTXOW" ShelleyUtxowPredFailure AllegraEra
instance InjectRuleFailure "UTXOW" AllegraUtxoPredFailure AllegraEra where
  injectFailure = UtxoFailure
-- ... (more error injection)

-- STS instance: uses Shelley's transitionRulesUTXOW directly
instance STS (AllegraUTXOW era) where
  type PredicateFailure (AllegraUTXOW era) = ShelleyUtxowPredFailure era
  transitionRules = [transitionRulesUTXOW]  -- ← Shelley's function!
  initialRules = []

-- Embed instance: wrap UTXO errors
instance Embed (AllegraUTXO era) (AllegraUTXOW era) where
  wrapFailed = UtxoFailure
  wrapEvent = UtxoEvent
```

That's it! Only 102 lines, mostly boilerplate.

---

### Mary Era: Even Less Changes

**File**: `eras/mary/impl/src/Cardano/Ledger/Mary/Rules/Utxow.hs` (30 lines!)

Mary adds native tokens (multi-asset), but **ZERO UTXOW changes**.

**Complete Mary UTXOW Module**:
```haskell
module Cardano.Ledger.Mary.Rules.Utxow () where

-- Error type: reuse Shelley's
type instance EraRuleFailure "UTXOW" MaryEra = ShelleyUtxowPredFailure MaryEra

-- Error injection instances
instance InjectRuleFailure "UTXOW" ShelleyUtxowPredFailure MaryEra
instance InjectRuleFailure "UTXOW" AllegraUtxoPredFailure MaryEra where
  injectFailure = UtxoFailure
instance InjectRuleFailure "UTXOW" ShelleyUtxoPredFailure MaryEra where
  injectFailure = UtxoFailure . injectFailure
instance InjectRuleFailure "UTXOW" ShelleyPpupPredFailure MaryEra where
  injectFailure = UtxoFailure . injectFailure
```

**That's the entire file!** No STS instance needed - inherits from Allegra.

**Why Mary Needs No UTXOW Changes**:
- Multi-asset is a `Value` type change
- Value validation happens in UTXO (value conservation, output sizes)
- UTXOW only checks witnesses - doesn't care about value structure
- Native scripts still validated the same way

---

### Alonzo Era: First UTXOW Changes (Plutus Support)

**File**: `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Rules/Utxow.hs` (470 lines)

Alonzo introduces **Plutus scripts** (Phase 2), requiring new UTXOW checks for Phase 1 setup.

**New Error Type** (`Alonzo/Rules/Utxow.hs:97-129`):
```haskell
data AlonzoUtxowPredFailure era
  = ShelleyInAlonzoUtxowPredFailure (ShelleyUtxowPredFailure era)  -- ← Wraps Shelley errors
  | MissingRedeemers [(PlutusPurpose AsItem era, ScriptHash)]      -- ← NEW
  | MissingRequiredDatums (Set DataHash) (Set DataHash)            -- ← NEW
  | NotAllowedSupplementalDatums (Set DataHash) (Set DataHash)     -- ← NEW
  | PPViewHashesDontMatch (Mismatch RelEQ (StrictMaybe ScriptIntegrityHash))  -- ← NEW
  | UnspendableUTxONoDatumHash (Set TxIn)                          -- ← NEW
  | ExtraRedeemers [PlutusPurpose AsIx era]                        -- ← NEW
  | ScriptIntegrityHashMismatch                                    -- ← NEW
      (Mismatch RelEQ (StrictMaybe ScriptIntegrityHash))
      (StrictMaybe ByteString)
```

**Key Architectural Change**: Alonzo **wraps** Shelley errors instead of reusing the type directly.

**New Validation Function: `alonzoStyleWitness`** (`Alonzo/Rules/Utxow.hs:318-397`):

Alonzo defines its own transition rule instead of using Shelley's `transitionRulesUTXOW`:

```haskell
alonzoStyleWitness :: TransitionRule (EraRule "UTXOW" era)
alonzoStyleWitness = do
  TRC (utxoEnv@(UtxoEnv _ pp certState), u, tx) <- judgmentContext

  let utxo = utxosUtxo u
      txBody = tx ^. bodyTxL
      witsKeyHashes = keyHashWitnessesTxWits (tx ^. witsTxL)
      scriptsProvided = getScriptsProvided utxo tx

  -- REUSED: Shelley native script validation (line 349)
  runTestOnSignal $ Shelley.validateFailedNativeScripts scriptsProvided tx

  -- REUSED: Shelley script presence check (line 355)
  let scriptsNeeded = getScriptsNeeded utxo txBody
      scriptsHashesNeeded = getScriptsHashesNeeded scriptsNeeded
      shelleyScriptsNeeded = ShelleyScriptsNeeded scriptsHashesNeeded
  runTest $ Shelley.validateMissingScripts shelleyScriptsNeeded scriptsProvided

  -- NEW: Check required datums for Plutus scripts (line 359)
  runTest $ missingRequiredDatums utxo tx

  -- NEW: Check redeemers match Plutus scripts (line 367)
  runTest $ hasExactSetOfRedeemers tx scriptsProvided scriptsNeeded

  -- REUSED: Shelley VKey verification (line 371)
  runTestOnSignal $ Shelley.validateVerifiedWits tx

  -- REUSED: Shelley required witnesses (line 374)
  runTest $ validateNeededWitnesses witsKeyHashes certState utxo txBody

  -- REUSED: Shelley MIR signatures (line 381-382)
  let genDelegs = certState ^. certDStateL . dsGenDelegsL
  coreNodeQuorum <- liftSTS $ asks quorum
  runTest $ Shelley.validateMIRInsufficientGenesisSigs genDelegs coreNodeQuorum witsKeyHashes tx

  -- REUSED: Shelley metadata (line 387)
  runTestOnSignal $ Shelley.validateMetadata pp tx

  -- NEW: Check script integrity hash (line 393-395)
  let scriptIntegrity = mkScriptIntegrity pp tx scriptsProvided scriptsHashesNeeded
  runTest $ checkScriptIntegrityHash tx pp scriptIntegrity

  -- Call UTXO rule (line 397)
  trans @(EraRule "UTXO" era) $ TRC (utxoEnv, u, tx)
```

**Alonzo's Three New Validations**:

#### 1. Missing Required Datums (`Alonzo/Rules/Utxow.hs:230-258`)

```haskell
missingRequiredDatums ::
  (AlonzoEraTx era, AlonzoEraUTxO era) =>
  UTxO era -> Tx l era -> Test (AlonzoUtxowPredFailure era)
missingRequiredDatums utxo tx = do
  let txBody = tx ^. bodyTxL
      scriptsProvided = getScriptsProvided utxo tx
      (inputHashes, txInsNoDataHash) = getInputDataHashesTxBody utxo txBody scriptsProvided
      txHashes = domain (tx ^. witsTxL . datsTxWitsL . unTxDatsL)
      unmatchedDatumHashes = eval (inputHashes ➖ txHashes)
      allowedSupplementalDataHashes = getSupplementalDataHashes utxo txBody
      supplimentalDatumHashes = eval (txHashes ➖ inputHashes)
      (okSupplimentalDHs, notOkSupplimentalDHs) =
        Set.partition (`Set.member` allowedSupplementalDataHashes) supplimentalDatumHashes
  sequenceA_
    [ failureUnless (Set.null txInsNoDataHash)
        (UnspendableUTxONoDatumHash txInsNoDataHash)
    , failureUnless (Set.null unmatchedDatumHashes)
        (MissingRequiredDatums unmatchedDatumHashes txHashes)
    , failureUnless (Set.null notOkSupplimentalDHs)
        (NotAllowedSupplementalDatums notOkSupplimentalDHs okSupplimentalDHs)
    ]
```

**Checks**:
- **Inputs with two-phase scripts must have datums**
- **Provided datums must match required datum hashes**
- **Supplemental datums** (not required by inputs) must be for outputs or reference inputs

**Formal Spec**:
```
{ h | (_ → (a,_,h)) ∈ txins tx ◁ utxo, isTwoPhaseScriptAddress tx a} ⊆ dom(txdats txw)
dom(txdats txw) ⊆ inputHashes ∪ {h | ( , , h, ) ∈ txouts tx ∪ utxo (refInputs tx) }
```

#### 2. Exact Set of Redeemers (`Alonzo/Rules/Utxow.hs:263-286`)

```haskell
hasExactSetOfRedeemers ::
  AlonzoEraTx era =>
  Tx l era ->
  ScriptsProvided era ->
  AlonzoScriptsNeeded era ->
  Test (AlonzoUtxowPredFailure era)
hasExactSetOfRedeemers tx (ScriptsProvided scriptsProvided) (AlonzoScriptsNeeded scriptsNeeded) = do
  let redeemersNeeded =
        [ (hoistPlutusPurpose toAsIx sp, (hoistPlutusPurpose toAsItem sp, sh))
        | (sp, sh) <- scriptsNeeded
        , Just script <- [Map.lookup sh scriptsProvided]
        , not (isNativeScript script)  -- ← Only Plutus scripts need redeemers!
        ]
      (extraRdmrs, missingRdmrs) =
        extSymmetricDifference
          (Map.keys $ tx ^. witsTxL . rdmrsTxWitsL . unRedeemersL)
          id
          redeemersNeeded
          fst
  sequenceA_
    [ failureUnless (null extraRdmrs) (ExtraRedeemers extraRdmrs)
    , failureUnless (null missingRdmrs) (MissingRedeemers (map snd missingRdmrs))
    ]
```

**Checks**:
- **Every Plutus script needs exactly one redeemer**
- **No extra redeemers** (would waste space/fees)
- **No missing redeemers** (Plutus scripts can't execute without them)
- **Native scripts don't need redeemers** (`isNativeScript` filters them out)

**Formal Spec**:
```
dom (txrdmrs tx) = { rdptr txb sp | (sp, h) ∈ scriptsNeeded utxo tx,
                                     h ↦ s ∈ txscripts txw, s ∈ Scriptph2}
```

#### 3. Script Integrity Hash (`Alonzo/Rules/Utxow.hs:290-310`)

```haskell
checkScriptIntegrityHash ::
  AlonzoEraTx era =>
  Tx l era -> PParams era -> StrictMaybe (ScriptIntegrity era) ->
  Test (AlonzoUtxowPredFailure era)
checkScriptIntegrityHash tx pp scriptIntegrity = do
  let computedScriptIntegrityHash = hashScriptIntegrity <$> scriptIntegrity
      suppliedScriptIntegrityHash = tx ^. bodyTxL . scriptIntegrityHashTxBodyL
      mismatch = Mismatch
        { mismatchSupplied = suppliedScriptIntegrityHash
        , mismatchExpected = computedScriptIntegrityHash
        }
  failureUnless (suppliedScriptIntegrityHash == computedScriptIntegrityHash)
    $ if pvMajor (pp ^. ppProtocolVersionL) < natVersion @11
      then PPViewHashesDontMatch mismatch
      else ScriptIntegrityHashMismatch mismatch expectedScriptIntegrity
```

**Checks**:
- **Transaction body contains script integrity hash**
- **Hash commits to**: redeemers, datums, protocol parameters relevant to Plutus
- **Prevents tampering** with Plutus execution context after signing
- **Protocol version dependent** error message (pre/post protocol 11)

**Formal Spec**:
```
scriptIntegrityHash txb = hashScriptIntegrity pp (languages txw) (txrdmrs txw)
```

**What Gets Hashed**:
```haskell
mkScriptIntegrity ::
  AlonzoEraTx era =>
  PParams era ->
  Tx l era ->
  ScriptsProvided era ->
  Set ScriptHash ->
  StrictMaybe (ScriptIntegrity era)
```
- Redeemers (all of them, with execution units)
- Languages used (Plutus V1, V2, V3, etc.)
- Cost models from protocol parameters
- Datums (indirectly, via redeemer validation)

---

### Validation Order Comparison

#### Shelley/Allegra/Mary UTXOW Steps:
1. Validate native scripts
2. Check script presence
3. Verify VKey signatures
4. Check required witnesses
5. Validate metadata
6. Check MIR genesis signatures
7. Call UTXO rule

#### Alonzo UTXOW Steps:
1. Validate native scripts ← **Reused from Shelley**
2. Check script presence ← **Reused from Shelley**
3. **Check required datums** ← **NEW for Plutus**
4. **Check exact redeemers** ← **NEW for Plutus**
5. Verify VKey signatures ← **Reused from Shelley**
6. Check required witnesses ← **Reused from Shelley**
7. Check MIR genesis signatures ← **Reused from Shelley**
8. Validate metadata ← **Reused from Shelley**
9. **Check script integrity hash** ← **NEW for Plutus**
10. Call UTXO rule

**Key Insight**: Alonzo adds 3 new checks but **reuses 6 of 7** Shelley checks!

---

### Summary Table

| Era | UTXOW Changes | Error Type | Lines of Code | Reuses Shelley? |
|-----|---------------|------------|---------------|-----------------|
| **Shelley** | Base implementation | `ShelleyUtxowPredFailure` | ~380 | N/A (original) |
| **Allegra** | None | `ShelleyUtxowPredFailure` | 102 | ✓ 100% |
| **Mary** | None | `ShelleyUtxowPredFailure` | 30 | ✓ 100% |
| **Alonzo** | +3 Plutus checks | `AlonzoUtxowPredFailure` (wraps Shelley) | 470 | ✓ 85% |
| **Babbage** | None (inherits Alonzo) | `AlonzoUtxowPredFailure` | ~100 | ✓ 100% |
| **Conway** | Minor governance adjustments | `ConwayUtxowPredFailure` | ~200 | ✓ 95% |

### Key Architectural Principles

1. **Separation of Concerns**:
   - **Native scripts**: Always Phase 1 (UTXOW)
   - **Plutus scripts**: Phase 1 setup (UTXOW), Phase 2 execution (UTXOS)

2. **Code Reuse**:
   - Shelley's validation functions are **era-polymorphic**
   - Later eras call Shelley functions directly
   - Changes only where absolutely necessary

3. **Error Composition**:
   - **Shelley/Allegra/Mary**: Same error type
   - **Alonzo+**: Wrap previous errors, add new ones
   - Allows error propagation through era boundaries

4. **Backwards Compatibility**:
   - Allegra Timelock extends Shelley MultiSig
   - Alonzo Plutus adds alongside native scripts
   - Old transactions still validate the same way

5. **Type-Level Era Tracking**:
   ```haskell
   type family EraRule (rule :: Symbol) era = (r :: Type) | r -> rule era
   type instance EraRule "UTXOW" ShelleyEra = ShelleyUTXOW
   type instance EraRule "UTXOW" AllegraEra = AllegraUTXOW
   type instance EraRule "UTXOW" AlonzoEra = AlonzoUTXOW
   ```
   - Compile-time guarantee of correct rule usage
   - Type-safe era transitions

---

## Summary

**UTXOW (Unspent Transaction Output Witnessing)** is Cardano's Phase 1 transaction validation rule that:

1. **Validates Native Scripts**: Checks MultiSig and Timelock scripts evaluate to true
2. **Checks Script Presence**: Ensures all needed scripts provided, no extras
3. **Verifies Signatures**: Cryptographically validates Ed25519 signatures
4. **Checks Required Witnesses**: Ensures all needed authorization present
5. **Validates Metadata**: Checks metadata hash integrity
6. **Verifies MIR Signatures**: Ensures genesis quorum for special certificates
7. **Calls UTXO Rule**: Proceeds to structural validation

**Key Takeaways**:
- UTXOW = Phase 1 = Native scripts + Signatures + Witnesses
- Native scripts are lightweight and deterministic
- Executed before UTXO structural checks (fail fast)
- Separate from Plutus (Phase 2) which happens in UTXOS
- Has remained fundamentally unchanged since Shelley
- Critical security boundary - prevents unauthorized transactions

**Next Step**: After UTXOW passes, transaction proceeds to UTXO rule for structural validation (inputs exist, fees correct, value conserved), then UTXOS for Plutus script execution in Alonzo+.
