// Byron Era UTxO Validation Implementation
// Reference: eras/byron/ledger/impl/src/Cardano/Chain/UTxO/Validation.hs
//
// Byron is the original Cardano era with simple key-based authentication.
// No scripts, no datums, no redeemers - just signatures.

use std::collections::{HashMap, HashSet};

// ============================================================================
// Core Types
// Reference: UTxO/Tx.hs
// ============================================================================

pub type Hash = [u8; 32];
pub type TxId = Hash;
pub type KeyHash = Hash;
pub type Lovelace = u64;

/// Network magic identifies mainnet vs testnet
/// Reference: Common/NetworkMagic.hs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkMagic {
    NetworkMainnet,
    NetworkTestnet(u32),
}

/// Protocol magic ID (full network identifier)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolMagicId(pub u32);

// ============================================================================
// Transaction Types
// Reference: UTxO/Tx.hs:55-62, 140-144, 187-190
// ============================================================================

/// Byron Transaction Input
/// Reference: UTxO/Tx.hs:140-144
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TxIn {
    /// Hash of the transaction whose output is being spent
    pub tx_id: TxId,
    /// Index of the output within that transaction (Word16 in Haskell)
    pub index: u16,
}

/// Byron Transaction Output
/// Reference: UTxO/Tx.hs:187-190
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxOut {
    pub address: Address,
    pub value: Lovelace,
}

/// Byron Transaction (without witnesses)
/// Reference: UTxO/Tx.hs:55-62
#[derive(Debug, Clone)]
pub struct Tx {
    /// At least one input required (NonEmpty in Haskell)
    pub inputs: Vec<TxIn>,
    /// At least one output required (NonEmpty in Haskell)
    pub outputs: Vec<TxOut>,
    /// Extensible attributes (for soft forks)
    pub attributes: TxAttributes,
}

/// Transaction attributes (for extensibility)
/// Reference: UTxO/Tx.hs:130-133
#[derive(Debug, Clone, Default)]
pub struct TxAttributes {
    /// Unknown/future attributes bytes
    pub unknown_attributes: Vec<u8>,
}

impl TxAttributes {
    pub fn unknown_length(&self) -> usize {
        self.unknown_attributes.len()
    }
}

// ============================================================================
// Address Types
// Reference: Common/Address.hs
// ============================================================================

/// Byron Address
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    /// Address root (hash)
    pub root: KeyHash,
    /// Address attributes
    pub attributes: AddressAttributes,
    /// Address type
    pub addr_type: AddressType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressAttributes {
    pub network_magic: NetworkMagic,
    pub unknown_attributes: Vec<u8>,
}

impl AddressAttributes {
    pub fn unknown_length(&self) -> usize {
        self.unknown_attributes.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddressType {
    /// Regular public key address
    PubKey,
    /// Redeem address (bootstrap era)
    Redeem,
}

// ============================================================================
// Witness Types
// Reference: UTxO/TxWitness.hs:60-68
// ============================================================================

/// Witnesses for a transaction (one per input)
/// Reference: UTxO/TxWitness.hs:60
pub type TxWitness = Vec<TxInWitness>;

/// A witness for a single input
/// Reference: UTxO/TxWitness.hs:63-68
#[derive(Debug, Clone)]
pub enum TxInWitness {
    /// Regular verification key witness
    /// VKWitness VerificationKey TxSig
    VKWitness {
        vkey: VerificationKey,
        signature: Signature,
    },
    /// Redeem address witness (bootstrap era)
    /// RedeemWitness RedeemVerificationKey (RedeemSignature TxSigData)
    RedeemWitness {
        redeem_vkey: RedeemVerificationKey,
        signature: RedeemSignature,
    },
}

/// Verification key (public key)
#[derive(Debug, Clone)]
pub struct VerificationKey(pub [u8; 32]);

impl VerificationKey {
    /// Get the hash of this verification key
    pub fn hash(&self) -> KeyHash {
        // Simplified - real implementation uses Blake2b
        self.0
    }
}

/// Redeem verification key
#[derive(Debug, Clone)]
pub struct RedeemVerificationKey(pub [u8; 32]);

impl RedeemVerificationKey {
    pub fn hash(&self) -> KeyHash {
        self.0
    }
}

/// Ed25519 signature
#[derive(Debug, Clone)]
pub struct Signature(pub [u8; 64]);

/// Redeem signature
#[derive(Debug, Clone)]
pub struct RedeemSignature(pub [u8; 64]);

/// Data that is signed in a transaction
/// Reference: UTxO/TxWitness.hs:131-133
#[derive(Debug, Clone)]
pub struct TxSigData {
    pub tx_hash: TxId,
}

// ============================================================================
// Transaction with Witnesses
// Reference: UTxO/TxAux.hs
// ============================================================================

/// Annotated transaction with witnesses
/// Reference: UTxO/TxAux.hs
#[derive(Debug, Clone)]
pub struct TxAux {
    pub tx: Tx,
    pub witnesses: TxWitness,
    /// Serialized size of the full transaction
    pub serialized_size: usize,
}

// ============================================================================
// UTxO
// ============================================================================

/// Unspent Transaction Outputs
pub type UTxO = HashMap<TxIn, TxOut>;

/// Check if UTxO only contains redeem addresses
/// Reference: UTxO/UTxO.hs (isRedeemUTxO)
pub fn is_redeem_utxo(utxo: &UTxO) -> bool {
    utxo.values().all(|out| out.address.addr_type == AddressType::Redeem)
}

// ============================================================================
// Environment
// Reference: Validation.hs:325-329
// ============================================================================

/// Validation environment
/// Reference: Validation.hs:325-329
#[derive(Debug, Clone)]
pub struct Environment {
    pub protocol_magic: ProtocolMagicId,
    pub protocol_parameters: ProtocolParameters,
    pub utxo_configuration: UTxOConfiguration,
}

/// Protocol parameters
/// Reference: Update/ProtocolParameters.hs
#[derive(Debug, Clone)]
pub struct ProtocolParameters {
    /// Maximum transaction size in bytes
    pub max_tx_size: u32,
    /// Transaction fee policy
    pub tx_fee_policy: TxFeePolicy,
}

/// Fee policy (linear on transaction size)
/// Reference: Common/TxFeePolicy.hs
#[derive(Debug, Clone)]
pub enum TxFeePolicy {
    /// fee = a + b * size
    TxSizeLinear { a: Lovelace, b: f64 },
}

/// UTxO configuration
/// Reference: UTxO/UTxOConfiguration.hs
#[derive(Debug, Clone, Default)]
pub struct UTxOConfiguration {
    /// Addresses that are "asset locked" - spending from them is forbidden
    pub asset_locked_addresses: HashSet<KeyHash>,
}

// ============================================================================
// Validation Errors
// Reference: Validation.hs:92-103
// ============================================================================

/// Transaction validation errors
/// Reference: Validation.hs:92-103
#[derive(Debug, Clone, PartialEq)]
pub enum TxValidationError {
    /// Arithmetic error in Lovelace calculations
    /// TxValidationLovelaceError Text LovelaceError
    LovelaceError(String),

    /// Fee is less than minimum required
    /// TxValidationFeeTooSmall Tx Lovelace Lovelace
    FeeTooSmall {
        min_fee: Lovelace,
        actual_fee: Lovelace,
    },

    /// Signature does not verify
    /// TxValidationWitnessWrongSignature TxInWitness ProtocolMagicId TxSigData
    WitnessWrongSignature { witness_index: usize },

    /// Key in witness doesn't match address being spent
    /// TxValidationWitnessWrongKey TxInWitness Address
    WitnessWrongKey {
        witness_index: usize,
        expected_key_hash: KeyHash,
    },

    /// Input not found in UTxO
    /// TxValidationMissingInput TxIn
    MissingInput(TxIn),

    /// Output address has wrong network magic
    /// TxValidationNetworkMagicMismatch NetworkMagic NetworkMagic
    NetworkMagicMismatch {
        expected: NetworkMagic,
        actual: NetworkMagic,
    },

    /// Transaction exceeds maximum size
    /// TxValidationTxTooLarge Natural Natural
    TxTooLarge { tx_size: usize, max_size: usize },

    /// Output address has unknown attributes > 128 bytes
    /// TxValidationUnknownAddressAttributes
    UnknownAddressAttributes { output_index: usize },

    /// Transaction has unknown attributes > 128 bytes
    /// TxValidationUnknownAttributes
    UnknownAttributes,

    /// Transaction must have at least one input
    NoInputs,

    /// Transaction must have at least one output
    NoOutputs,

    /// Input from asset-locked address
    AssetLockedInput(TxIn),

    /// Insufficient input balance to cover outputs + fee
    InsufficientBalance {
        input_balance: Lovelace,
        output_balance: Lovelace,
    },
}

/// Higher-level UTxO validation error
/// Reference: Validation.hs:332-335
#[derive(Debug, Clone, PartialEq)]
pub enum UTxOValidationError {
    TxValidationError(TxValidationError),
    UTxOError(String),
}

impl From<TxValidationError> for UTxOValidationError {
    fn from(e: TxValidationError) -> Self {
        UTxOValidationError::TxValidationError(e)
    }
}

// ============================================================================
// Validation Functions
// Reference: Validation.hs
// ============================================================================

/// Calculate the minimum fee for a transaction
/// Reference: Validation.hs:229-234
fn calculate_minimum_fee(
    policy: &TxFeePolicy,
    tx_size: usize,
    is_redeem: bool,
) -> Result<Lovelace, TxValidationError> {
    // Redeem transactions have zero fee
    if is_redeem {
        return Ok(0);
    }

    match policy {
        TxFeePolicy::TxSizeLinear { a, b } => {
            // fee = a + b * size
            let size_component = (*b * tx_size as f64) as u64;
            Ok(a.saturating_add(size_component))
        }
    }
}

/// Validate transaction size
/// Reference: Validation.hs:189-193
fn validate_tx_size(
    tx_size: usize,
    max_size: u32,
) -> Result<(), TxValidationError> {
    if tx_size > max_size as usize {
        Err(TxValidationError::TxTooLarge {
            tx_size,
            max_size: max_size as usize,
        })
    } else {
        Ok(())
    }
}

/// Validate transaction attributes
/// Reference: Validation.hs:248-251
fn validate_tx_attributes(tx: &Tx) -> Result<(), TxValidationError> {
    if tx.attributes.unknown_length() >= 128 {
        Err(TxValidationError::UnknownAttributes)
    } else {
        Ok(())
    }
}

/// Validate output network magic and attributes
/// Reference: Validation.hs:281-293
fn validate_tx_out(
    expected_nm: NetworkMagic,
    output_index: usize,
    output: &TxOut,
) -> Result<(), TxValidationError> {
    // Check address attributes size
    if output.address.attributes.unknown_length() >= 128 {
        return Err(TxValidationError::UnknownAddressAttributes { output_index });
    }

    // Check network magic matches
    let actual_nm = output.address.attributes.network_magic;
    if expected_nm != actual_nm {
        return Err(TxValidationError::NetworkMagicMismatch {
            expected: expected_nm,
            actual: actual_nm,
        });
    }

    Ok(())
}

/// Validate that input exists in UTxO and is not asset-locked
/// Reference: Validation.hs:263-278
fn validate_tx_in<'a>(
    config: &UTxOConfiguration,
    utxo: &'a UTxO,
    txin: &TxIn,
) -> Result<&'a TxOut, TxValidationError> {
    match utxo.get(txin) {
        None => Err(TxValidationError::MissingInput(txin.clone())),
        Some(txout) => {
            // Check if address is asset-locked
            if config.asset_locked_addresses.contains(&txout.address.root) {
                Err(TxValidationError::AssetLockedInput(txin.clone()))
            } else {
                Ok(txout)
            }
        }
    }
}

/// Verify a witness signature
/// Reference: Validation.hs:297-323
fn validate_witness(
    _protocol_magic: ProtocolMagicId,
    _sig_data: &TxSigData,
    address: &Address,
    witness_index: usize,
    witness: &TxInWitness,
) -> Result<(), TxValidationError> {
    match witness {
        TxInWitness::VKWitness { vkey, signature: _ } => {
            // Verify signature (simplified - real impl uses crypto library)
            // verifySignatureDecoded pmi SignTx vk sigData sig
            
            // Verify key matches address
            // checkVerKeyAddress vk addr
            let key_hash = vkey.hash();
            if key_hash != address.root {
                return Err(TxValidationError::WitnessWrongKey {
                    witness_index,
                    expected_key_hash: address.root,
                });
            }
            
            // Check address type
            if address.addr_type != AddressType::PubKey {
                return Err(TxValidationError::WitnessWrongKey {
                    witness_index,
                    expected_key_hash: address.root,
                });
            }
            
            Ok(())
        }
        TxInWitness::RedeemWitness { redeem_vkey, signature: _ } => {
            // Verify redeem signature (simplified)
            // verifyRedeemSigDecoded pmi SignRedeemTx vk sigData sig
            
            // Verify key matches redeem address
            // checkRedeemAddress vk addr
            let key_hash = redeem_vkey.hash();
            if key_hash != address.root {
                return Err(TxValidationError::WitnessWrongKey {
                    witness_index,
                    expected_key_hash: address.root,
                });
            }
            
            // Check address type
            if address.addr_type != AddressType::Redeem {
                return Err(TxValidationError::WitnessWrongKey {
                    witness_index,
                    expected_key_hash: address.root,
                });
            }
            
            Ok(())
        }
    }
}

/// Validate transaction structure (without witnesses)
/// Reference: Validation.hs:241-258
pub fn validate_tx(
    env: &Environment,
    utxo: &UTxO,
    tx: &Tx,
) -> Result<(), TxValidationError> {
    // Check non-empty inputs
    if tx.inputs.is_empty() {
        return Err(TxValidationError::NoInputs);
    }

    // Check non-empty outputs
    if tx.outputs.is_empty() {
        return Err(TxValidationError::NoOutputs);
    }

    // Check transaction attributes size
    // Reference: Validation.hs:248-251
    validate_tx_attributes(tx)?;

    // Get network magic from protocol magic
    let network_magic = match env.protocol_magic.0 {
        764824073 => NetworkMagic::NetworkMainnet,
        n => NetworkMagic::NetworkTestnet(n),
    };

    // Validate each output
    // Reference: Validation.hs:254-255
    for (idx, output) in tx.outputs.iter().enumerate() {
        validate_tx_out(network_magic, idx, output)?;
    }

    // Validate each input exists
    // Reference: Validation.hs:258
    for txin in &tx.inputs {
        validate_tx_in(&env.utxo_configuration, utxo, txin)?;
    }

    Ok(())
}

/// Validate transaction with witnesses (full validation including size and fee)
/// Reference: Validation.hs:183-234
pub fn validate_tx_aux(
    env: &Environment,
    utxo: &UTxO,
    tx_aux: &TxAux,
) -> Result<(), TxValidationError> {
    let tx = &tx_aux.tx;
    let params = &env.protocol_parameters;

    // Step 1: Check transaction size
    // Reference: Validation.hs:189-193
    validate_tx_size(tx_aux.serialized_size, params.max_tx_size)?;

    // Step 2: Collect input UTxO
    let input_set: HashSet<TxIn> = tx.inputs.iter().cloned().collect();
    let input_utxo: HashMap<&TxIn, &TxOut> = utxo
        .iter()
        .filter(|(k, _)| input_set.contains(*k))
        .collect();

    // Step 3: Check if this is a redeem-only UTxO
    // Reference: Validation.hs:196-199
    let is_redeem = input_utxo
        .values()
        .all(|out| out.address.addr_type == AddressType::Redeem);

    // Step 4: Calculate minimum fee
    // Reference: Validation.hs:196-199
    let min_fee = calculate_minimum_fee(
        &params.tx_fee_policy,
        tx_aux.serialized_size,
        is_redeem,
    )?;

    // Step 5: Calculate output balance
    // Reference: Validation.hs:202-204
    let balance_out: Lovelace = tx.outputs.iter().map(|o| o.value).sum();

    // Step 6: Calculate input balance
    // Reference: Validation.hs:206-209
    let balance_in: Lovelace = input_utxo.values().map(|o| o.value).sum();

    // Step 7: Calculate fee (input - output)
    // Reference: Validation.hs:211-214
    if balance_in < balance_out {
        return Err(TxValidationError::InsufficientBalance {
            input_balance: balance_in,
            output_balance: balance_out,
        });
    }
    let fee = balance_in - balance_out;

    // Step 8: Check fee is sufficient
    // Reference: Validation.hs:217
    if fee < min_fee {
        return Err(TxValidationError::FeeTooSmall {
            min_fee,
            actual_fee: fee,
        });
    }

    Ok(())
}

/// Full UTxO update with witness validation
/// Reference: Validation.hs:373-404
pub fn update_utxo_tx_witness(
    env: &Environment,
    utxo: &UTxO,
    tx_aux: &TxAux,
) -> Result<UTxO, UTxOValidationError> {
    let tx = &tx_aux.tx;

    // Step 1: Get addresses for each input
    // Reference: Validation.hs:381-384
    let mut input_addresses = Vec::new();
    for txin in &tx.inputs {
        match utxo.get(txin) {
            Some(txout) => input_addresses.push(&txout.address),
            None => {
                return Err(TxValidationError::MissingInput(txin.clone()).into());
            }
        }
    }

    // Step 2: Check witness count matches input count
    if tx_aux.witnesses.len() != tx.inputs.len() {
        return Err(UTxOValidationError::UTxOError(format!(
            "Witness count {} does not match input count {}",
            tx_aux.witnesses.len(),
            tx.inputs.len()
        )));
    }

    // Step 3: Validate each witness
    // Reference: Validation.hs:386-390
    let sig_data = TxSigData {
        tx_hash: compute_tx_hash(tx),
    };

    for (idx, (address, witness)) in input_addresses.iter().zip(&tx_aux.witnesses).enumerate() {
        validate_witness(env.protocol_magic, &sig_data, address, idx, witness)?;
    }

    // Step 4: Validate transaction structure
    // Reference: Validation.hs:393-394
    validate_tx(env, utxo, tx)?;
    validate_tx_aux(env, utxo, tx_aux)?;

    // Step 5: Update UTxO
    // Reference: Validation.hs:396-397
    update_utxo_tx(env, utxo, tx)
}

/// Update UTxO (remove spent inputs, add new outputs)
/// Reference: Validation.hs:359-370
pub fn update_utxo_tx(
    env: &Environment,
    utxo: &UTxO,
    tx: &Tx,
) -> Result<UTxO, UTxOValidationError> {
    // Validate transaction
    validate_tx(env, utxo, tx)?;

    // Create new UTxO
    let mut new_utxo = utxo.clone();

    // Remove spent inputs
    // Reference: S.fromList (NE.toList (txInputs tx)) </| utxo
    for txin in &tx.inputs {
        new_utxo.remove(txin);
    }

    // Add new outputs
    // Reference: txOutputUTxO tx
    let tx_hash = compute_tx_hash(tx);
    for (idx, output) in tx.outputs.iter().enumerate() {
        let txin = TxIn {
            tx_id: tx_hash,
            index: idx as u16,
        };
        new_utxo.insert(txin, output.clone());
    }

    Ok(new_utxo)
}

/// Compute transaction hash (simplified)
fn compute_tx_hash(_tx: &Tx) -> TxId {
    // Real implementation would serialize and hash
    [0u8; 32]
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_address(id: u8, addr_type: AddressType, nm: NetworkMagic) -> Address {
        let mut root = [0u8; 32];
        root[0] = id;
        Address {
            root,
            attributes: AddressAttributes {
                network_magic: nm,
                unknown_attributes: vec![],
            },
            addr_type,
        }
    }

    fn make_txin(id: u8, idx: u16) -> TxIn {
        let mut tx_id = [0u8; 32];
        tx_id[0] = id;
        TxIn { tx_id, index: idx }
    }

    fn make_env() -> Environment {
        Environment {
            protocol_magic: ProtocolMagicId(764824073), // mainnet
            protocol_parameters: ProtocolParameters {
                max_tx_size: 16384,
                tx_fee_policy: TxFeePolicy::TxSizeLinear {
                    a: 155381,      // Base fee
                    b: 43.946,      // Per-byte fee
                },
            },
            utxo_configuration: UTxOConfiguration::default(),
        }
    }

    #[test]
    fn test_validate_tx_no_inputs() {
        let env = make_env();
        let utxo = UTxO::new();
        let tx = Tx {
            inputs: vec![],
            outputs: vec![TxOut {
                address: make_address(1, AddressType::PubKey, NetworkMagic::NetworkMainnet),
                value: 1000,
            }],
            attributes: TxAttributes::default(),
        };

        let result = validate_tx(&env, &utxo, &tx);
        assert!(matches!(result, Err(TxValidationError::NoInputs)));
    }

    #[test]
    fn test_validate_tx_missing_input() {
        let env = make_env();
        let utxo = UTxO::new();
        let tx = Tx {
            inputs: vec![make_txin(1, 0)],
            outputs: vec![TxOut {
                address: make_address(1, AddressType::PubKey, NetworkMagic::NetworkMainnet),
                value: 1000,
            }],
            attributes: TxAttributes::default(),
        };

        let result = validate_tx(&env, &utxo, &tx);
        assert!(matches!(result, Err(TxValidationError::MissingInput(_))));
    }

    #[test]
    fn test_validate_tx_unknown_attributes() {
        let env = make_env();
        let mut utxo = UTxO::new();
        let txin = make_txin(1, 0);
        utxo.insert(
            txin.clone(),
            TxOut {
                address: make_address(1, AddressType::PubKey, NetworkMagic::NetworkMainnet),
                value: 1000,
            },
        );

        let tx = Tx {
            inputs: vec![txin],
            outputs: vec![TxOut {
                address: make_address(2, AddressType::PubKey, NetworkMagic::NetworkMainnet),
                value: 500,
            }],
            attributes: TxAttributes {
                unknown_attributes: vec![0u8; 200], // Too large!
            },
        };

        let result = validate_tx(&env, &utxo, &tx);
        assert!(matches!(result, Err(TxValidationError::UnknownAttributes)));
    }

    #[test]
    fn test_validate_tx_network_magic_mismatch() {
        let env = make_env();
        let mut utxo = UTxO::new();
        let txin = make_txin(1, 0);
        utxo.insert(
            txin.clone(),
            TxOut {
                address: make_address(1, AddressType::PubKey, NetworkMagic::NetworkMainnet),
                value: 1000,
            },
        );

        let tx = Tx {
            inputs: vec![txin],
            outputs: vec![TxOut {
                // Output has testnet magic but we're on mainnet!
                address: make_address(2, AddressType::PubKey, NetworkMagic::NetworkTestnet(1)),
                value: 500,
            }],
            attributes: TxAttributes::default(),
        };

        let result = validate_tx(&env, &utxo, &tx);
        assert!(matches!(
            result,
            Err(TxValidationError::NetworkMagicMismatch { .. })
        ));
    }

    #[test]
    fn test_validate_tx_aux_fee_too_small() {
        let env = make_env();
        let mut utxo = UTxO::new();
        let txin = make_txin(1, 0);
        utxo.insert(
            txin.clone(),
            TxOut {
                address: make_address(1, AddressType::PubKey, NetworkMagic::NetworkMainnet),
                value: 1000,
            },
        );

        let tx = Tx {
            inputs: vec![txin],
            outputs: vec![TxOut {
                address: make_address(2, AddressType::PubKey, NetworkMagic::NetworkMainnet),
                value: 999, // Only 1 lovelace fee - way too small!
            }],
            attributes: TxAttributes::default(),
        };

        let tx_aux = TxAux {
            tx,
            witnesses: vec![],
            serialized_size: 200,
        };

        let result = validate_tx_aux(&env, &utxo, &tx_aux);
        assert!(matches!(result, Err(TxValidationError::FeeTooSmall { .. })));
    }

    #[test]
    fn test_validate_tx_aux_tx_too_large() {
        let env = make_env();
        let utxo = UTxO::new();

        let tx = Tx {
            inputs: vec![make_txin(1, 0)],
            outputs: vec![TxOut {
                address: make_address(2, AddressType::PubKey, NetworkMagic::NetworkMainnet),
                value: 500,
            }],
            attributes: TxAttributes::default(),
        };

        let tx_aux = TxAux {
            tx,
            witnesses: vec![],
            serialized_size: 100000, // Way too large!
        };

        let result = validate_tx_aux(&env, &utxo, &tx_aux);
        assert!(matches!(result, Err(TxValidationError::TxTooLarge { .. })));
    }

    #[test]
    fn test_redeem_utxo_zero_fee() {
        // Redeem transactions have zero minimum fee
        let min_fee = calculate_minimum_fee(
            &TxFeePolicy::TxSizeLinear { a: 155381, b: 43.946 },
            200,
            true, // is_redeem = true
        )
        .unwrap();

        assert_eq!(min_fee, 0);
    }

    #[test]
    fn test_regular_tx_fee() {
        // Regular transactions have fee = a + b * size
        let min_fee = calculate_minimum_fee(
            &TxFeePolicy::TxSizeLinear { a: 155381, b: 43.946 },
            200,
            false, // is_redeem = false
        )
        .unwrap();

        // 155381 + 43.946 * 200 = 155381 + 8789 = 164170
        assert!(min_fee > 155381);
    }
}
