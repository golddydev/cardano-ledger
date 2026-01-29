# Byron Era UTxO Validation

This document explains the Byron era transaction validation rules based on the Haskell
node implementation in `eras/byron/ledger/impl/src/Cardano/Chain/UTxO/Validation.hs`.

## Overview

Byron is the original Cardano era, predating smart contracts. Transaction validation in Byron
is simpler than later eras because:

1. **No scripts** - Only key-based authentication (VK witnesses and Redeem witnesses)
2. **No datums** - No Plutus data attached to outputs
3. **No redeemers** - No execution units or script execution
4. **Simple witness model** - One witness per input

## Transaction Structure

### Tx (Transaction)
```haskell
-- Reference: UTxO/Tx.hs:55-62
data Tx = UnsafeTx
  { txInputs :: !(NonEmpty TxIn)      -- At least one input required
  , txOutputs :: !(NonEmpty TxOut)    -- At least one output required
  , txAttributes :: !TxAttributes     -- Extensible attributes (for soft forks)
  }
```

### TxIn (Transaction Input)
```haskell
-- Reference: UTxO/Tx.hs:140-144
data TxIn = TxInUtxo TxId Word16
  -- TxId: Hash of the transaction being spent
  -- Word16: Index of the output within that transaction
```

### TxOut (Transaction Output)
```haskell
-- Reference: UTxO/Tx.hs:187-190
data TxOut = TxOut
  { txOutAddress :: !Address
  , txOutValue :: !Lovelace
  }
```

### TxInWitness (Witness)
```haskell
-- Reference: UTxO/TxWitness.hs:63-68
data TxInWitness
  = VKWitness !VerificationKey !TxSig          -- Regular key witness
  | RedeemWitness !RedeemVerificationKey !(RedeemSignature TxSigData)  -- Redeem address
```

## Validation Environment

```haskell
-- Reference: Validation.hs:325-329
data Environment = Environment
  { protocolMagic :: !(AProtocolMagic ByteString)   -- Network identifier
  , protocolParameters :: !ProtocolParameters       -- Fee policy, max tx size, etc.
  , utxoConfiguration :: !UTxOConfiguration         -- Asset-locked addresses
  }
```

## Validation Errors

```haskell
-- Reference: Validation.hs:92-103
data TxValidationError
  = TxValidationLovelaceError Text LovelaceError
    -- ^ Arithmetic error in Lovelace calculations
  
  | TxValidationFeeTooSmall Tx Lovelace Lovelace
    -- ^ Fee is less than minimum. Fields: tx, minFee, actualFee
  
  | TxValidationWitnessWrongSignature TxInWitness ProtocolMagicId TxSigData
    -- ^ Signature does not verify
  
  | TxValidationWitnessWrongKey TxInWitness Address
    -- ^ Key in witness doesn't match address being spent
  
  | TxValidationMissingInput TxIn
    -- ^ Input not found in UTxO
  
  | TxValidationNetworkMagicMismatch NetworkMagic NetworkMagic
    -- ^ Output address has wrong network magic (mainnet vs testnet)
  
  | TxValidationTxTooLarge Natural Natural
    -- ^ Transaction exceeds max size. Fields: txSize, maxSize
  
  | TxValidationUnknownAddressAttributes
    -- ^ Output address has unknown attributes (> 128 bytes)
  
  | TxValidationUnknownAttributes
    -- ^ Transaction has unknown attributes (> 128 bytes)
```

## Validation Rules

### Rule 1: Transaction Size Check
```haskell
-- Reference: Validation.hs:189-193
txSize <= maxTxSize
  `orThrowError` TxValidationTxTooLarge txSize maxTxSize
```

**Plain English**: The serialized transaction (including witnesses) must not exceed
the maximum transaction size specified in protocol parameters.

### Rule 2: Fee Calculation and Validation
```haskell
-- Reference: Validation.hs:196-217
-- Calculate minimum fee based on fee policy
minFee <- if isRedeemUTxO inputUTxO
            then pure $ mkKnownLovelace @0   -- Redeem UTxOs have zero fee
            else calculateMinimumFee feePolicy

-- Calculate balances
balanceOut <- balance (txOutputUTxO tx)
balanceIn <- balance inputUTxO
fee <- subLovelace balanceIn balanceOut

-- Check fee is sufficient
(minFee <= fee) `orThrowError` TxValidationFeeTooSmall tx minFee fee
```

**Plain English**: 
- The fee = total input value - total output value
- The fee must be at least the minimum fee calculated from the fee policy
- **Exception**: Redeem transactions (spending from redeem addresses) have zero minimum fee

### Rule 3: Transaction Attributes Size
```haskell
-- Reference: Validation.hs:248-251
unknownAttributesLength (txAttributes tx) < 128
  `orThrowError` TxValidationUnknownAttributes
```

**Plain English**: Unknown attributes in the transaction must be less than 128 bytes.
This prevents bloating transactions with arbitrary data.

### Rule 4: Output Network Magic
```haskell
-- Reference: Validation.hs:281-293
validateTxOutNM nm txOut = do
  -- Check address attributes size
  unknownAttributesLength (addrAttributes (txOutAddress txOut)) < 128
    `orThrowError` TxValidationUnknownAddressAttributes
  
  -- Check network magic matches
  (nm == addrNm) `orThrowError` TxValidationNetworkMagicMismatch nm addrNm
```

**Plain English**:
- Each output's address must have unknown attributes < 128 bytes
- Each output's address network magic must match the expected network (mainnet/testnet)

### Rule 5: Input Existence
```haskell
-- Reference: Validation.hs:263-278
validateTxIn utxoConfiguration utxo txIn
  | txIn `UTxO.member` utxo = pure ()
  | Just txOut <- UTxO.lookupCompact txIn utxo
  , txOutAddr `S.notMember` tcAssetLockedSrcAddrs = pure ()
  | otherwise = throwError $ TxValidationMissingInput txIn
```

**Plain English**:
- Every input must exist in the UTxO set
- Inputs from "asset-locked" addresses are forbidden (used for token locking)

### Rule 6: Witness Signature Verification
```haskell
-- Reference: Validation.hs:297-323
validateWitness pmi sigData addr witness = case witness of
  VKWitness vk sig -> do
    -- Verify signature
    verifySignatureDecoded pmi SignTx vk sigData sig
      `orThrowError` TxValidationWitnessWrongSignature witness pmi sigData
    -- Verify key matches address
    checkVerKeyAddress vk addr
      `orThrowError` TxValidationWitnessWrongKey witness addr
  
  RedeemWitness vk sig -> do
    -- Verify redeem signature
    verifyRedeemSigDecoded pmi SignRedeemTx vk sigData sig
      `orThrowError` TxValidationWitnessWrongSignature witness pmi sigData
    -- Verify key matches redeem address
    checkRedeemAddress vk addr
      `orThrowError` TxValidationWitnessWrongKey witness addr
```

**Plain English**:
- For each input, there must be a corresponding witness
- **VK Witness**: The signature must verify against the verification key, and the 
  key hash must match the address being spent
- **Redeem Witness**: Same rules but for redeem addresses (bootstrap era addresses)

## Full Update Flow

```haskell
-- Reference: Validation.hs:373-404
updateUTxOTxWitness env utxo ta = do
  whenTxValidation $ do
    -- Get addresses for each input
    addresses <- mapM (`UTxO.lookupAddress` utxo) (txInputs tx)
    
    -- Validate witnesses match addresses
    mapM_ (uncurry $ validateWitness pmi sigData) (zip addresses witnesses)
    
    -- Validate transaction structure and fees
    validateTxAux env utxo ta
  
  -- Update UTxO: remove spent inputs, add new outputs
  updateUTxOTx env utxo aTx
```

**Plain English**:
1. Look up the address for each input from the UTxO
2. Validate each witness against its corresponding address
3. Validate transaction size and fees
4. Remove spent inputs from UTxO
5. Add new outputs to UTxO

## Key Differences from Later Eras

| Feature | Byron | Shelley+ |
|---------|-------|----------|
| Scripts | None | Native scripts, Plutus |
| Datums | None | Hash, Inline |
| Witnesses | VK/Redeem only | VK, Bootstrap, Script |
| Redeemers | None | Required for Plutus |
| Fee Model | Linear on size | More complex |
| Certificates | None (separate) | In transaction |

## Summary of Checks

1. **Structural checks**:
   - Transaction size ≤ max size
   - Transaction attributes ≤ 128 bytes unknown
   - At least one input and one output

2. **Input checks**:
   - All inputs exist in UTxO
   - Not from asset-locked addresses

3. **Output checks**:
   - Address attributes ≤ 128 bytes unknown
   - Network magic matches

4. **Witness checks**:
   - Signature verifies
   - Key matches address

5. **Value checks**:
   - Fee ≥ minimum fee
   - Input balance = output balance + fee
