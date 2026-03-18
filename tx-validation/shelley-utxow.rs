// Shelley Era UTXOW Rule Implementation
// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Rules/Utxow.hs
//
// This is a simplified educational implementation demonstrating the
// UTXOW (Unspent Transaction Output Witnessing) validation logic.

use std::collections::{HashMap, HashSet};

// ============================================================================
// Core Types
// ============================================================================

/// 32-byte hash (simplified)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hash([u8; 32]);

pub type KeyHash = Hash;
pub type ScriptHash = Hash;
pub type TxBodyHash = Hash;
pub type MetadataHash = Hash;

/// Ed25519 verification key (32 bytes)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VKey(pub [u8; 32]);

impl VKey {
    /// Hash to KeyHash (BLAKE2b-224 in real implementation)
    pub fn hash(&self) -> KeyHash {
        // Simplified - real impl uses BLAKE2b-224
        Hash([0u8; 32])
    }
}

/// Ed25519 signature (64 bytes)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature(pub [u8; 64]);

/// Slot number for timelock validation
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SlotNo(pub u64);

// ============================================================================
// Native Scripts (Shelley MultiSig)
// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/Scripts.hs
// ============================================================================

/// Native script types
/// Reference: Scripts.hs:57-62
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeScript {
    /// Require signature from specific key
    RequireSignature(KeyHash),
    /// Require ALL sub-scripts to validate
    RequireAllOf(Vec<NativeScript>),
    /// Require ANY sub-script to validate
    RequireAnyOf(Vec<NativeScript>),
    /// Require at least M of N sub-scripts to validate
    RequireMOf { required: usize, scripts: Vec<NativeScript> },
}

impl NativeScript {
    /// Evaluate native script
    /// Reference: Scripts.hs:233-249 (evalMultiSig)
    ///
    /// # Arguments
    /// * `vkey_hashes` - Set of key hashes that have provided signatures
    ///
    /// # Returns
    /// * `true` if the script validates, `false` otherwise
    pub fn validate(&self, vkey_hashes: &HashSet<KeyHash>) -> bool {
        match self {
            // RequireSignature hk -> Set.member hk vhks
            NativeScript::RequireSignature(key_hash) => vkey_hashes.contains(key_hash),

            // RequireAllOf msigs -> all go msigs
            NativeScript::RequireAllOf(scripts) => {
                scripts.iter().all(|s| s.validate(vkey_hashes))
            }

            // RequireAnyOf msigs -> any go msigs
            NativeScript::RequireAnyOf(scripts) => {
                scripts.iter().any(|s| s.validate(vkey_hashes))
            }

            // RequireMOf m msigs -> m <= sum [if go msig then 1 else 0 | msig <- msigs]
            NativeScript::RequireMOf { required, scripts } => {
                let valid_count = scripts.iter().filter(|s| s.validate(vkey_hashes)).count();
                valid_count >= *required
            }
        }
    }

    /// Compute script hash (BLAKE2b-256 of CBOR in real implementation)
    pub fn hash(&self) -> ScriptHash {
        Hash([0u8; 32])
    }
}

// ============================================================================
// Transaction Types
// ============================================================================

/// Transaction input reference
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TxIn {
    pub tx_id: Hash,
    pub index: u32,
}

/// Transaction output
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxOut {
    pub address: Address,
    pub value: u64, // Simplified - real impl uses multi-asset Value
}

/// Address with payment credential
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    pub payment: PaymentCredential,
    pub staking: Option<StakingCredential>,
}

/// Payment credential (key or script)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaymentCredential {
    KeyHash(KeyHash),
    ScriptHash(ScriptHash),
}

/// Staking credential
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StakingCredential {
    KeyHash(KeyHash),
    ScriptHash(ScriptHash),
}

/// Reward account for withdrawals
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RewardAccount {
    pub credential: StakingCredential,
}

/// Shelley certificate types (simplified)
#[derive(Debug, Clone)]
pub enum ShelleyTxCert {
    /// Register staking credential
    RegKey(StakingCredential),
    /// Deregister staking credential
    DeRegKey(StakingCredential),
    /// Delegate to pool
    Delegate { credential: StakingCredential, pool: KeyHash },
    /// Register stake pool
    RegPool(PoolParams),
    /// Retire stake pool
    RetirePool { pool_id: KeyHash, epoch: u64 },
    /// Move Instantaneous Rewards (requires genesis quorum)
    MIR { pot: MIRPot, amount: u64, target: KeyHash },
}

#[derive(Debug, Clone)]
pub struct PoolParams {
    pub pool_id: KeyHash,
    pub owners: HashSet<KeyHash>,
}

#[derive(Debug, Clone)]
pub enum MIRPot {
    Treasury,
    Reserves,
}

/// VKey witness (public key + signature)
/// Reference: Cardano.Ledger.Keys
#[derive(Debug, Clone)]
pub struct VKeyWitness {
    pub vkey: VKey,
    pub signature: Signature,
}

impl VKeyWitness {
    pub fn key_hash(&self) -> KeyHash {
        self.vkey.hash()
    }

    /// Verify signature against transaction body hash
    /// Reference: Cardano.Ledger.Keys.verifyWitVKey
    pub fn verify(&self, _tx_body_hash: TxBodyHash) -> bool {
        // Real implementation uses Ed25519 verification
        true
    }
}

/// Transaction body
#[derive(Debug, Clone)]
pub struct TxBody {
    pub inputs: HashSet<TxIn>,
    pub outputs: Vec<TxOut>,
    pub fee: u64,
    pub ttl: Option<SlotNo>,
    pub certificates: Vec<ShelleyTxCert>,
    pub withdrawals: HashMap<RewardAccount, u64>,
    pub update: Option<Update>,
    pub auxiliary_data_hash: Option<MetadataHash>,
}

/// Protocol parameter update
#[derive(Debug, Clone)]
pub struct Update {
    pub proposed_by: HashSet<KeyHash>, // Genesis delegates proposing
}

/// Transaction witnesses
#[derive(Debug, Clone, Default)]
pub struct TxWits {
    pub vkey_wits: Vec<VKeyWitness>,
    pub script_wits: HashMap<ScriptHash, NativeScript>,
}

// ============================================================================
// Metadatum and metadata validation
// Reference: libs/cardano-ledger-core/src/Cardano/Ledger/Metadata.hs (validMetadatum);
// eras/shelley/impl/src/Cardano/Ledger/Shelley/SoftForks.hs (validMetadata: pv > (2,0))
// ============================================================================

/// Metadatum: recursive metadata value (CBOR structure).
/// Reference: Metadata.hs:49-56
#[derive(Debug, Clone)]
pub enum Metadatum {
    Map(Vec<(Metadatum, Metadatum)>),
    List(Vec<Metadatum>),
    I(i128),
    B(Vec<u8>),
    S(String),
}

/// Max length for bytestring and text metadatum values (bytes).
/// Reference: Metadata.hs:78-79
const METADATUM_MAX_BYTES: usize = 64;

/// validMetadatum: I ok; B len <= 64; S UTF-8 len <= 64; List/Map recursive.
/// Reference: Metadata.hs:75-87
pub fn valid_metadatum(m: &Metadatum) -> bool {
    match m {
        Metadatum::I(_) => true,
        Metadatum::B(b) => b.len() <= METADATUM_MAX_BYTES,
        Metadatum::S(s) => s.as_bytes().len() <= METADATUM_MAX_BYTES,
        Metadatum::List(xs) => xs.iter().all(valid_metadatum),
        Metadatum::Map(kvs) => kvs
            .iter()
            .all(|(k, v)| valid_metadatum(k) && valid_metadatum(v)),
    }
}

/// Decode a single CBOR byte string chunk (major 2, definite length only).
/// Used for indefinite-length byte string content.
#[allow(dead_code)]
fn decode_cbor_byte_string_chunk(data: &[u8]) -> Option<(Vec<u8>, usize)> {
    if data.is_empty() {
        return None;
    }
    let b0 = data[0];
    if (b0 >> 5) != 2 {
        return None; // Must be byte string
    }
    let ai = b0 & 0x1F;
    if ai == 31 {
        return None; // Indefinite not allowed as chunk
    }
    let (len_u64, pos) = decode_cbor_length(data, ai, 1)?;
    let len = len_u64 as usize;
    if data.len() < pos + len {
        return None;
    }
    let bytes = data[pos..pos + len].to_vec();
    Some((bytes, pos + len))
}

/// Decode a single CBOR text string chunk (major 3, definite length only).
/// Used for indefinite-length text string content.
#[allow(dead_code)]
fn decode_cbor_text_chunk(data: &[u8]) -> Option<(String, usize)> {
    if data.is_empty() {
        return None;
    }
    let b0 = data[0];
    if (b0 >> 5) != 3 {
        return None;
    }
    let ai = b0 & 0x1F;
    if ai == 31 {
        return None;
    }
    let (len_u64, pos) = decode_cbor_length(data, ai, 1)?;
    let len = len_u64 as usize;
    if data.len() < pos + len {
        return None;
    }
    let raw = &data[pos..pos + len];
    let s = String::from_utf8(raw.to_vec()).ok()?;
    Some((s, pos + len))
}

/// Decode CBOR length field (ai + optional extra bytes). Returns (length, byte_offset_after_header).
#[allow(dead_code)]
fn decode_cbor_length(data: &[u8], ai: u8, start: usize) -> Option<(u64, usize)> {
    let pos = start;
    let (len_u64, bytes_read) = if ai <= 23 {
        (ai as u64, 0)
    } else if ai == 24 {
        if data.len() < pos + 1 {
            return None;
        }
        (data[pos] as u64, 1)
    } else if ai == 25 {
        if data.len() < pos + 2 {
            return None;
        }
        (
            u64::from_be_bytes([data[pos], data[pos + 1], 0, 0, 0, 0, 0, 0]) >> 48,
            2,
        )
    } else if ai == 26 {
        if data.len() < pos + 4 {
            return None;
        }
        (
            u64::from_be_bytes([0, 0, 0, 0, data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]),
            4,
        )
    } else if ai == 27 {
        if data.len() < pos + 8 {
            return None;
        }
        (
            u64::from_be_bytes([
                data[pos], data[pos + 1], data[pos + 2], data[pos + 3],
                data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7],
            ]),
            8,
        )
    } else {
        return None;
    };
    Some((len_u64, pos + bytes_read))
}

/// Minimal CBOR decoder for Metadatum (deserialization utility).
/// Reference: Metadata.hs:124-240 (decodeMetadatum)
///
/// In Haskell, CBOR decoding happens at deserialization time (when a transaction
/// is received from the network). Each metadatum value is decoded into the
/// Metadatum ADT. The original CBOR bytes are preserved in MemoBytes for hashing.
///
/// This decoder supports both definite and indefinite length encodings:
/// CBOR major types: 0=unsigned int, 1=negative int, 2=bytes, 3=text, 4=array, 5=map.
/// ai=31 means indefinite length for bytes/text/array/map.
///
/// Note: Size constraints (bytes/text <= 64) are NOT enforced during decoding.
/// They are checked later by validMetadatum during validation.
/// Reference: Metadata.hs:131-134 "Note that we do not enforce byte and string
/// lengths here in the decoder. We enforce that in the tx validation rules."
#[allow(dead_code)]
fn decode_metadatum_cbor(data: &[u8]) -> Option<(Metadatum, usize)> {
    if data.is_empty() {
        return None;
    }
    let b0 = data[0];
    let major = b0 >> 5;
    let ai = b0 & 0x1F;
    let mut pos = 1u32;
    let mut len_u64 = 0u64;

    match major {
        0 => {
            // Unsigned integer
            let n = if ai <= 23 {
                ai as u64
            } else if ai == 24 {
                if data.len() < 2 {
                    return None;
                }
                pos += 1;
                data[1] as u64
            } else if ai == 25 {
                if data.len() < 3 {
                    return None;
                }
                pos += 2;
                u64::from_be_bytes([data[1], data[2], 0, 0, 0, 0, 0, 0]) >> 48
            } else if ai == 26 {
                if data.len() < 5 {
                    return None;
                }
                pos += 4;
                u64::from_be_bytes([
                    0, 0, 0, 0, data[1], data[2], data[3], data[4],
                ])
            } else if ai == 27 {
                if data.len() < 9 {
                    return None;
                }
                pos += 8;
                u64::from_be_bytes([
                    data[1], data[2], data[3], data[4], data[5], data[6], data[7], data[8],
                ])
            } else {
                return None;
            };
            return Some((Metadatum::I(n as i128), pos as usize));
        }
        1 => {
            // Negative integer: -1 - n
            let len_u64: u64 = if ai <= 23 {
                ai as u64
            } else if ai == 24 {
                if data.len() < 2 {
                    return None;
                }
                pos += 1;
                data[1] as u64
            } else if ai == 25 {
                if data.len() < 3 {
                    return None;
                }
                pos += 2;
                u64::from_be_bytes([data[1], data[2], 0, 0, 0, 0, 0, 0]) >> 48
            } else if ai == 26 {
                if data.len() < 5 {
                    return None;
                }
                pos += 4;
                u64::from_be_bytes([0, 0, 0, 0, data[1], data[2], data[3], data[4]])
            } else if ai == 27 {
                if data.len() < 9 {
                    return None;
                }
                pos += 8;
                u64::from_be_bytes([
                    data[1], data[2], data[3], data[4], data[5], data[6], data[7], data[8],
                ])
            } else {
                return None;
            };
            return Some((Metadatum::I(-1i128 - (len_u64 as i128)), pos as usize));
        }
        2 => {
            // Byte string (definite or indefinite)
            if ai == 31 {
                // Indefinite-length bytes: 0x5F, byte-string chunks until 0xFF. Reference: Metadata.hs decodeBytesIndef
                let mut chunks: Vec<Vec<u8>> = Vec::new();
                let mut p = 1usize;
                loop {
                    if p >= data.len() {
                        return None;
                    }
                    if data[p] == 0xFF {
                        p += 1;
                        break;
                    }
                    let (chunk, n) = decode_cbor_byte_string_chunk(&data[p..])?;
                    chunks.push(chunk);
                    p += n;
                }
                let bytes: Vec<u8> = chunks.into_iter().flatten().collect();
                return Some((Metadatum::B(bytes), p));
            }
            let len_u64: u64 = if ai <= 23 {
                ai as u64
            } else if ai == 24 {
                if data.len() < 2 {
                    return None;
                }
                pos += 1;
                data[1] as u64
            } else if ai == 25 {
                if data.len() < 3 {
                    return None;
                }
                pos += 2;
                u64::from_be_bytes([data[1], data[2], 0, 0, 0, 0, 0, 0]) >> 48
            } else if ai == 26 {
                if data.len() < 5 {
                    return None;
                }
                pos += 4;
                u64::from_be_bytes([0, 0, 0, 0, data[1], data[2], data[3], data[4]])
            } else if ai == 27 {
                if data.len() < 9 {
                    return None;
                }
                pos += 8;
                u64::from_be_bytes([
                    data[1], data[2], data[3], data[4], data[5], data[6], data[7], data[8],
                ])
            } else {
                return None;
            };
            let len = len_u64 as usize;
            if data.len() < pos as usize + len {
                return None;
            }
            let bytes = data[pos as usize..pos as usize + len].to_vec();
            return Some((Metadatum::B(bytes), pos as usize + len));
        }
        3 => {
            // Text string (definite or indefinite)
            if ai == 31 {
                // Indefinite-length text: 0x7F, text chunks until 0xFF. Reference: Metadata.hs decodeStringIndef
                let mut parts: Vec<String> = Vec::new();
                let mut p = 1usize;
                loop {
                    if p >= data.len() {
                        return None;
                    }
                    if data[p] == 0xFF {
                        p += 1;
                        break;
                    }
                    let (chunk, n) = decode_cbor_text_chunk(&data[p..])?;
                    parts.push(chunk);
                    p += n;
                }
                let s = parts.join("");
                return Some((Metadatum::S(s), p));
            }
            if ai <= 23 {
                len_u64 = ai as u64;
            } else if ai == 24 {
                if data.len() < 2 {
                    return None;
                }
                pos += 1;
                len_u64 = data[1] as u64;
            } else if ai == 25 {
                if data.len() < 3 {
                    return None;
                }
                pos += 2;
                len_u64 = u64::from_be_bytes([data[1], data[2], 0, 0, 0, 0, 0, 0]) >> 48;
            } else if ai == 26 {
                if data.len() < 5 {
                    return None;
                }
                pos += 4;
                len_u64 = u64::from_be_bytes([0, 0, 0, 0, data[1], data[2], data[3], data[4]]);
            } else if ai == 27 {
                if data.len() < 9 {
                    return None;
                }
                pos += 8;
                len_u64 = u64::from_be_bytes([
                    data[1], data[2], data[3], data[4], data[5], data[6], data[7], data[8],
                ]);
            } else {
                return None;
            };
            let len = len_u64 as usize;
            if data.len() < pos as usize + len {
                return None;
            }
            let raw = &data[pos as usize..pos as usize + len];
            let s = String::from_utf8(raw.to_vec()).ok()?;
            return Some((Metadatum::S(s), pos as usize + len));
        }
        4 => {
            // Array (definite or indefinite)
            if ai == 31 {
                // Indefinite-length array: 0x9F, items until 0xFF. Reference: Metadata.hs decodeListIndef
                let mut arr = Vec::new();
                let mut p = 1usize;
                loop {
                    if p >= data.len() {
                        return None;
                    }
                    if data[p] == 0xFF {
                        p += 1;
                        break;
                    }
                    let (elem, n) = decode_metadatum_cbor(&data[p..])?;
                    arr.push(elem);
                    p += n;
                }
                return Some((Metadatum::List(arr), p));
            }
            if ai <= 23 {
                len_u64 = ai as u64;
            } else if ai == 24 {
                if data.len() < 2 {
                    return None;
                }
                pos += 1;
                len_u64 = data[1] as u64;
            } else if ai == 25 {
                if data.len() < 3 {
                    return None;
                }
                pos += 2;
                len_u64 = u64::from_be_bytes([data[1], data[2], 0, 0, 0, 0, 0, 0]) >> 48;
            } else if ai == 26 {
                if data.len() < 5 {
                    return None;
                }
                pos += 4;
                len_u64 = u64::from_be_bytes([0, 0, 0, 0, data[1], data[2], data[3], data[4]]);
            } else if ai == 27 {
                if data.len() < 9 {
                    return None;
                }
                pos += 8;
                len_u64 = u64::from_be_bytes([
                    data[1], data[2], data[3], data[4], data[5], data[6], data[7], data[8],
                ]);
            } else {
                return None;
            };
            let mut arr = Vec::with_capacity(len_u64 as usize);
            let mut p = pos as usize;
            for _ in 0..len_u64 {
                let (elem, n) = decode_metadatum_cbor(&data[p..])?;
                arr.push(elem);
                p += n;
            }
            return Some((Metadatum::List(arr), p));
        }
        5 => {
            // Map (definite or indefinite)
            if ai == 31 {
                // Indefinite-length map: 0xBF, key-value pairs until 0xFF. Reference: Metadata.hs decodeMapIndef
                let mut kvs = Vec::new();
                let mut p = 1usize;
                loop {
                    if p >= data.len() {
                        return None;
                    }
                    if data[p] == 0xFF {
                        p += 1;
                        break;
                    }
                    let (k, nk) = decode_metadatum_cbor(&data[p..])?;
                    p += nk;
                    let (v, nv) = decode_metadatum_cbor(&data[p..])?;
                    p += nv;
                    kvs.push((k, v));
                }
                return Some((Metadatum::Map(kvs), p));
            }
            let len_u64: u64 = if ai <= 23 {
                ai as u64
            } else if ai == 24 {
                if data.len() < 2 {
                    return None;
                }
                pos += 1;
                data[1] as u64
            } else if ai == 25 {
                if data.len() < 3 {
                    return None;
                }
                pos += 2;
                u64::from_be_bytes([data[1], data[2], 0, 0, 0, 0, 0, 0]) >> 48
            } else if ai == 26 {
                if data.len() < 5 {
                    return None;
                }
                pos += 4;
                u64::from_be_bytes([0, 0, 0, 0, data[1], data[2], data[3], data[4]])
            } else if ai == 27 {
                if data.len() < 9 {
                    return None;
                }
                pos += 8;
                u64::from_be_bytes([
                    data[1], data[2], data[3], data[4], data[5], data[6], data[7], data[8],
                ])
            } else {
                return None;
            };
            let mut kvs = Vec::with_capacity(len_u64 as usize);
            let mut p = pos as usize;
            for _ in 0..len_u64 {
                let (k, nk) = decode_metadatum_cbor(&data[p..])?;
                p += nk;
                let (v, nv) = decode_metadatum_cbor(&data[p..])?;
                p += nv;
                kvs.push((k, v));
            }
            return Some((Metadatum::Map(kvs), p));
        }
        _ => {}
    }
    None
}

/// validateTxAuxData (metadata part): all metadatums pass validMetadatum.
/// Reference: TxAuxData.hs:98 (Shelley): `validateTxAuxData _ (ShelleyTxAuxData m) = all validMetadatum m`
///
/// In Haskell, metadata values are decoded from CBOR into `Metadatum` at
/// deserialization time (when the transaction is received from the network).
/// The decoded `Map Word64 Metadatum` is stored alongside the original CBOR
/// bytes (via `MemoBytes`). Validation operates on the decoded values directly.
///
/// This function validates that every metadatum value respects size limits:
/// - I (integer): always valid
/// - B (bytestring): raw byte length <= 64
/// - S (text): UTF-8 encoded byte length <= 64
/// - List/Map: recursive check on all children
fn validate_tx_aux_data_metadata(metadata: &HashMap<u64, Metadatum>) -> bool {
    metadata.values().all(valid_metadatum)
}

/// Protocol version (major, minor). Used for soft fork validMetadata (pv > (2,0)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolVersion {
    pub major: u32,
    pub minor: u32,
}

/// validMetadata: metadata value-size check is enabled only when pv > (2,0).
/// Reference: SoftForks.hs:12-15
pub fn valid_metadata_soft_fork(pv: ProtocolVersion) -> bool {
    pv.major > 2 || (pv.major == 2 && pv.minor > 0)
}

// ============================================================================
// Auxiliary Data Types (era-specific)
// ============================================================================
//
// Auxiliary data evolves across eras. Each era adds new fields:
//
// Era         | Type               | Fields                            | CDDL format
// ------------|--------------------| ----------------------------------|---------------------------
// Shelley     | ShelleyTxAuxData   | metadata only                     | { uint => metadatum }
// Allegra     | AllegraTxAuxData   | metadata + native scripts         | [metadata, [native_script*]]
// Mary        | AllegraTxAuxData   | (reuses Allegra)                  | (same as Allegra)
// Alonzo      | AlonzoTxAuxData    | metadata + native + plutus        | #6.259({0:metadata, 1:[native*], 2:[plutusV1*], ...})
// Babbage     | AlonzoTxAuxData    | (reuses Alonzo)                   | (same as Alonzo)
// Conway      | AlonzoTxAuxData    | (reuses Alonzo, + PlutusV3/V4)   | (same as Alonzo, keys 4/5 for V3/V4)
//
// All eras use MemoBytes in Haskell: the decoded value is stored alongside
// the original CBOR bytes. The hash is BLAKE2b-256 of the original bytes.
//
// Haskell references:
//   Shelley:  eras/shelley/impl/src/Cardano/Ledger/Shelley/TxAuxData.hs
//   Allegra:  eras/allegra/impl/src/Cardano/Ledger/Allegra/TxAuxData.hs
//   Alonzo:   eras/alonzo/impl/src/Cardano/Ledger/Alonzo/TxAuxData.hs
// ============================================================================

/// Shelley auxiliary data: metadata only.
/// Reference: Shelley/TxAuxData.hs:53-55
///
/// ```haskell
/// newtype ShelleyTxAuxDataRaw era = ShelleyTxAuxDataRaw
///   { stadrMetadata :: Map Word64 Metadatum }
/// ```
///
/// In Haskell, the full type is wrapped in MemoBytes:
///   `newtype ShelleyTxAuxData era = MkShelleyTxAuxData (MemoBytes (ShelleyTxAuxDataRaw era))`
///
/// On the wire (CDDL): `metadata = { * metadatum_label => metadatum }`
/// where `metadatum_label = uint .size 8`.
///
/// The hash is BLAKE2b-256 of the entire CBOR encoding of this map,
/// NOT of individual values. Haskell preserves original bytes via MemoBytes.
#[derive(Debug, Clone)]
pub struct AuxiliaryData {
    /// Decoded metadata map: Map Word64 Metadatum.
    /// In Haskell, values are fully decoded from CBOR at deserialization time.
    /// Each Metadatum::B holds raw bytes (CBOR framing stripped),
    /// each Metadatum::S holds decoded text, etc.
    pub metadata: HashMap<u64, Metadatum>,
    /// Original CBOR bytes of the entire auxiliary data (for hashing).
    /// In Haskell, MemoBytes preserves these alongside the decoded value.
    /// The hash is BLAKE2b-256(original_bytes), matching the tx body's auxDataHash.
    pub original_bytes: Vec<u8>,
}

impl AuxiliaryData {
    /// Hash the auxiliary data: BLAKE2b-256 of original CBOR bytes.
    /// Reference: Core.hs:467-468
    ///   `hashTxAuxData = TxAuxDataHash . hashAnnotated`
    /// where hashAnnotated for ShelleyTxAuxData uses getMemoSafeHash,
    /// which returns the pre-computed hash of the memoized CBOR bytes.
    pub fn hash(&self) -> MetadataHash {
        // Real implementation: BLAKE2b-256 of self.original_bytes
        // Simplified - returns dummy hash
        Hash([0u8; 32])
    }

    /// Validate metadata for Shelley era.
    /// Reference: TxAuxData.hs:98
    ///   `validateTxAuxData _ (ShelleyTxAuxData m) = all validMetadatum m`
    pub fn validate_shelley(&self, _pv: ProtocolVersion) -> bool {
        validate_tx_aux_data_metadata(&self.metadata)
    }
}

/// Allegra/Mary auxiliary data: metadata + native scripts.
/// Reference: Allegra/TxAuxData.hs:77-85
///
/// ```haskell
/// data AllegraTxAuxDataRaw era = AllegraTxAuxDataRaw
///   { atadrMetadata      :: !(Map Word64 Metadatum)
///   , atadrNativeScripts  :: !(StrictSeq (NativeScript era))
///   }
/// ```
///
/// On the wire (CDDL): `auxiliary_data = metadata / [metadata, [native_script*]]`
/// Allegra supports both the Shelley raw-map format AND a new array format.
///
/// Validation (TxAuxData.hs:100):
///   `validateTxAuxData _ (AllegraTxAuxData md as) = as `deepseq` all validMetadatum md`
/// Note: native scripts in aux data are NOT validated by validateTxAuxData in Allegra/Mary.
/// They are just forced with deepseq (no bottom values). Script validation only
/// happens for scripts referenced in the UTXOW rule (needed scripts).
#[derive(Debug, Clone)]
pub struct AllegraAuxiliaryData {
    pub metadata: HashMap<u64, Metadatum>,
    pub native_scripts: Vec<NativeScript>,
    pub original_bytes: Vec<u8>,
}

impl AllegraAuxiliaryData {
    pub fn hash(&self) -> MetadataHash {
        Hash([0u8; 32])
    }

    /// Allegra/Mary validateTxAuxData: only validates metadata, not scripts.
    /// Reference: Allegra/TxAuxData.hs:100
    pub fn validate_allegra(&self, _pv: ProtocolVersion) -> bool {
        validate_tx_aux_data_metadata(&self.metadata)
    }
}

/// Plutus script language version.
/// Reference: Alonzo/Scripts.hs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    PlutusV1,
    PlutusV2,
    PlutusV3,
    PlutusV4,
}

/// Plutus script binary (opaque bytes, validated by deserializability).
/// Reference: Alonzo/Scripts.hs - `PlutusBinary`
#[derive(Debug, Clone)]
pub struct PlutusBinary(pub Vec<u8>);

/// Alonzo auxiliary data: metadata + native scripts + Plutus scripts.
/// Reference: Alonzo/TxAuxData.hs:106-110
///
/// ```haskell
/// data AlonzoTxAuxDataRaw era = AlonzoTxAuxDataRaw
///   { atadrMetadata      :: !(Map Word64 Metadatum)
///   , atadrNativeScripts  :: !(StrictSeq (NativeScript era))
///   , atadrPlutusScripts  :: !(Map Language (NE.NonEmpty PlutusBinary))
///   }
/// ```
///
/// On the wire (CDDL): `#6.259({ ?0: metadata, ?1: [native_script*], ?2: [plutusV1*], ?3: [plutusV2*], ... })`
/// Uses CBOR tag 259. Empty fields are omitted.
///
/// Backward-compatible decoding (Alonzo/TxAuxData.hs:199-235):
///   - If CBOR map (no tag): decode as Shelley metadata only
///   - If CBOR array: decode as Allegra [metadata, scripts]
///   - If Tag 259: decode as full Alonzo structure
///
/// Validation (Alonzo/TxAuxData.hs:295-302):
///   `validateAlonzoTxAuxData pv auxData =
///      all validMetadatum metadata && all (validScript pv) scripts`
/// Unlike Allegra, Alonzo DOES validate scripts in aux data using `validScript`:
///   - Native scripts: forced with deepseq (structure is valid)
///   - Plutus scripts: deserialization check via `isValidPlutusScript`
#[derive(Debug, Clone)]
pub struct AlonzoAuxiliaryData {
    pub metadata: HashMap<u64, Metadatum>,
    pub native_scripts: Vec<NativeScript>,
    pub plutus_scripts: HashMap<Language, Vec<PlutusBinary>>,
    pub original_bytes: Vec<u8>,
}

impl AlonzoAuxiliaryData {
    pub fn hash(&self) -> MetadataHash {
        Hash([0u8; 32])
    }

    /// Alonzo validateTxAuxData: validates metadata AND scripts.
    /// Reference: Alonzo/TxAuxData.hs:295-302
    ///
    /// ```haskell
    /// validateAlonzoTxAuxData pv auxData =
    ///   all validMetadatum metadata
    ///     && all (validScript pv) (getAlonzoTxAuxDataScripts auxData)
    /// ```
    ///
    /// validScript (Alonzo/Scripts.hs:634-641):
    ///   - Plutus: attempts deserialization; returns true if it succeeds
    ///   - Native: deepseq forces full evaluation (validates structure)
    pub fn validate_alonzo(&self, pv: ProtocolVersion) -> bool {
        // 1. Validate metadata (same as Shelley)
        if !validate_tx_aux_data_metadata(&self.metadata) {
            return false;
        }
        // 2. Validate scripts (Alonzo addition)
        //    Reference: Alonzo/Scripts.hs:634-641 (validScript)
        //    Native scripts: structure validity (deepseq in Haskell)
        //    Plutus scripts: deserialization check (isValidPlutusScript)
        for script in &self.native_scripts {
            if !valid_native_script(script) {
                return false;
            }
        }
        for (_lang, scripts) in &self.plutus_scripts {
            for plutus in scripts {
                if !valid_plutus_script(pv, plutus) {
                    return false;
                }
            }
        }
        true
    }
}

/// Validate a native script's structure.
/// Reference: Alonzo/Scripts.hs:639 - `deepseq timelockScript True`
/// In Haskell, deepseq forces the value to normal form. If the structure
/// contains a bottom (error/undefined), this will raise an exception.
/// Here we just check that the structure is well-formed (always true for
/// a constructed NativeScript value in Rust).
fn valid_native_script(_script: &NativeScript) -> bool {
    true // Structure validity guaranteed by Rust type system
}

/// Validate a Plutus script binary.
/// Reference: Alonzo/Scripts.hs:635 - `isValidPlutusScript (pvMajor pv) plutusScript`
/// Attempts to deserialize the Plutus script binary. Returns true if
/// deserialization succeeds. Real implementation would check script
/// structure and version compatibility.
fn valid_plutus_script(_pv: ProtocolVersion, _script: &PlutusBinary) -> bool {
    // Simplified: real impl attempts deserialization
    true
}

/// Complete transaction
#[derive(Debug, Clone)]
pub struct Tx {
    pub body: TxBody,
    pub wits: TxWits,
    pub auxiliary_data: Option<AuxiliaryData>,
}

impl Tx {
    pub fn body_hash(&self) -> TxBodyHash {
        // Real implementation: BLAKE2b-256 of CBOR-encoded body
        Hash([0u8; 32])
    }
}

// ============================================================================
// UTxO State
// ============================================================================

/// Unspent Transaction Output set
pub type UTxO = HashMap<TxIn, TxOut>;

/// Genesis delegates configuration
#[derive(Debug, Clone, Default)]
pub struct GenDelegs {
    /// Maps genesis key hash -> delegate key hash
    pub delegates: HashMap<KeyHash, KeyHash>,
}

/// Certificate state (delegation state)
#[derive(Debug, Clone, Default)]
pub struct CertState {
    pub gen_delegs: GenDelegs,
}

// NOTE: Alonzo/Babbage collateral validation and failed-transaction handling
// are in their respective files: alonzo-utxo.rs and babbage-utxo.rs



// ============================================================================
// Predicate Failures (Errors)
// Reference: Utxow.hs:85-112
// ============================================================================

/// Shelley UTXOW predicate failures
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShelleyUtxowPredFailure {
    /// VKey signature verification failed
    /// Reference: Utxow.hs:87
    InvalidWitnessesUTXOW(Vec<KeyHash>),

    /// Required VKey witnesses not provided
    /// Reference: Utxow.hs:90
    MissingVKeyWitnessesUTXOW(HashSet<KeyHash>),

    /// Required scripts not provided
    /// Reference: Utxow.hs:93
    MissingScriptWitnessesUTXOW(HashSet<ScriptHash>),

    /// Native script evaluated to false
    /// Reference: Utxow.hs:96
    ScriptWitnessNotValidatingUTXOW(HashSet<ScriptHash>),

    /// Error from embedded UTXO rule
    /// Reference: Utxow.hs:99
    UtxoFailure(String), // Simplified - real impl uses nested error type

    /// Auxiliary data provided but no hash in body
    /// Reference: Utxow.hs:100
    MissingTxBodyMetadataHash(MetadataHash),

    /// Hash in body but no auxiliary data
    /// Reference: Utxow.hs:101
    MissingTxMetadata(MetadataHash),

    /// Auxiliary data hash doesn't match
    /// Reference: Utxow.hs:102
    ConflictingMetadataHash { expected: MetadataHash, actual: MetadataHash },

    /// Invalid auxiliary data: metadatum value-size validation failed.
    /// Raised only when (1) body hash and auxiliary data are both present, (2) hash matches,
    /// (3) protocol version > (2,0) (soft fork), and (4) some metadatum violates size limits
    /// (bytestring or text length > 64 bytes, or nested structure containing such).
    /// In Shelley era protocol version is (2,0), so this check is never enabled there.
    /// Reference: Utxow.hs:131-132 (constructor), Utxow.hs:451-452 (when raised);
    /// validateTxAuxData in TxAuxData.hs:98; validMetadatum in Metadata.hs:75-87;
    /// SoftForks.validMetadata in SoftForks.hs:12-15 (pv > (2,0)).
    InvalidMetadata,

    /// Scripts provided but not needed
    /// Reference: Utxow.hs:104
    ExtraneousScriptWitnessesUTXOW(HashSet<ScriptHash>),

    /// MIR certificate without enough genesis signatures
    /// Reference: Utxow.hs:105
    MIRInsufficientGenesisSigsUTXOW(HashSet<KeyHash>),
}

// ============================================================================
// Scripts Needed Computation
// Reference: eras/shelley/impl/src/Cardano/Ledger/Shelley/UTxO.hs:103-119
// ============================================================================

/// Compute which script hashes are needed for this transaction
/// Reference: UTxO.hs:103-119 (getShelleyScriptsNeeded)
///
/// Collects scripts from:
/// - Inputs locked by script addresses
/// - Withdrawals from script-locked reward accounts
/// - Certificates authorized by scripts
pub fn get_shelley_scripts_needed(utxo: &UTxO, tx_body: &TxBody) -> HashSet<ScriptHash> {
    let mut needed = HashSet::new();

    // 1. Scripts from inputs
    // scriptHashes = txinsScriptHashes (txBody ^. inputsTxBodyL) u
    for txin in &tx_body.inputs {
        if let Some(txout) = utxo.get(txin) {
            if let PaymentCredential::ScriptHash(sh) = &txout.address.payment {
                needed.insert(*sh);
            }
        }
    }

    // 2. Scripts from withdrawals
    // [sh | w <- withdrawals, Just sh <- [credScriptHash (raCredential w)]]
    for reward_account in tx_body.withdrawals.keys() {
        if let StakingCredential::ScriptHash(sh) = &reward_account.credential {
            needed.insert(*sh);
        }
    }

    // 3. Scripts from certificates
    // [sh | c <- certificates, Just sh <- [getScriptWitnessTxCert c]]
    for cert in &tx_body.certificates {
        if let Some(sh) = get_script_witness_tx_cert(cert) {
            needed.insert(sh);
        }
    }

    needed
}

/// Get script hash from certificate if it requires script authorization
fn get_script_witness_tx_cert(cert: &ShelleyTxCert) -> Option<ScriptHash> {
    match cert {
        ShelleyTxCert::DeRegKey(StakingCredential::ScriptHash(sh)) => Some(*sh),
        ShelleyTxCert::Delegate { credential: StakingCredential::ScriptHash(sh), .. } => Some(*sh),
        _ => None,
    }
}

// ============================================================================
// VKey Witnesses Needed Computation
// Reference: UTxO.hs:223-280
// ============================================================================

/// Compute which key hashes must sign this transaction
/// Reference: UTxO.hs:270-280 (getShelleyWitsVKeyNeeded)
pub fn get_shelley_wits_vkey_needed(
    cert_state: &CertState,
    utxo: &UTxO,
    tx_body: &TxBody,
) -> HashSet<KeyHash> {
    let mut needed = get_shelley_wits_vkey_needed_no_gov(utxo, tx_body);

    // Add genesis delegate witnesses for protocol updates
    // witsVKeyNeededGenDelegs txBody (dsGenDelegs (certState ^. certDStateL))
    needed.extend(wits_vkey_needed_gen_delegs(tx_body, &cert_state.gen_delegs));

    needed
}

/// VKey witnesses needed (excluding governance)
/// Reference: UTxO.hs:223-268 (getShelleyWitsVKeyNeededNoGov)
pub fn get_shelley_wits_vkey_needed_no_gov(utxo: &UTxO, tx_body: &TxBody) -> HashSet<KeyHash> {
    let mut needed = HashSet::new();

    // 1. Input authors - keys owning UTxO inputs
    for txin in &tx_body.inputs {
        if let Some(txout) = utxo.get(txin) {
            if let PaymentCredential::KeyHash(kh) = &txout.address.payment {
                needed.insert(*kh);
            }
        }
    }

    // 2. Withdrawal authors - keys authorizing withdrawals
    for reward_account in tx_body.withdrawals.keys() {
        if let StakingCredential::KeyHash(kh) = &reward_account.credential {
            needed.insert(*kh);
        }
    }

    // 3. Certificate authors - keys authorizing certificates
    for cert in &tx_body.certificates {
        if let Some(kh) = get_vkey_witness_tx_cert(cert) {
            needed.insert(kh);
        }
    }

    // 4. Pool owners - for pool registration
    for cert in &tx_body.certificates {
        if let ShelleyTxCert::RegPool(params) = cert {
            needed.extend(params.owners.iter().copied());
        }
    }

    needed
}

/// Get VKey witness from certificate
fn get_vkey_witness_tx_cert(cert: &ShelleyTxCert) -> Option<KeyHash> {
    match cert {
        ShelleyTxCert::DeRegKey(StakingCredential::KeyHash(kh)) => Some(*kh),
        ShelleyTxCert::Delegate { credential: StakingCredential::KeyHash(kh), .. } => Some(*kh),
        ShelleyTxCert::RetirePool { pool_id, .. } => Some(*pool_id),
        _ => None,
    }
}

/// Genesis delegate witnesses needed for protocol updates
/// Reference: UTxO.hs:206-219 (witsVKeyNeededGenDelegs)
fn wits_vkey_needed_gen_delegs(tx_body: &TxBody, gen_delegs: &GenDelegs) -> HashSet<KeyHash> {
    if let Some(update) = &tx_body.update {
        // Proposed updates require genesis delegate signatures
        update
            .proposed_by
            .iter()
            .filter_map(|genesis_key| gen_delegs.delegates.get(genesis_key).copied())
            .collect()
    } else {
        HashSet::new()
    }
}

// ============================================================================
// Validation Functions
// Reference: Utxow.hs:184-289
// ============================================================================

/// Validate failed native scripts
/// Reference: Utxow.hs:184-197 (validateFailedNativeScripts)
///
/// Evaluates all native scripts. Fails if any returns false.
pub fn validate_failed_native_scripts(
    scripts_provided: &HashMap<ScriptHash, NativeScript>,
    tx: &Tx,
) -> Result<(), ShelleyUtxowPredFailure> {
    // Get key hashes from VKey witnesses
    let vkey_hashes: HashSet<KeyHash> = tx.wits.vkey_wits.iter().map(|w| w.key_hash()).collect();

    // Find scripts that fail validation
    let failed_scripts: HashSet<ScriptHash> = scripts_provided
        .iter()
        .filter(|(_, script)| !script.validate(&vkey_hashes))
        .map(|(hash, _)| *hash)
        .collect();

    if failed_scripts.is_empty() {
        Ok(())
    } else {
        Err(ShelleyUtxowPredFailure::ScriptWitnessNotValidatingUTXOW(
            failed_scripts,
        ))
    }
}

/// Validate missing scripts
/// Reference: Utxow.hs:382-389 (validateMissingScripts)
///
/// Checks that exactly the needed scripts are provided.
pub fn validate_missing_scripts(
    scripts_needed: &HashSet<ScriptHash>,
    scripts_provided: &HashMap<ScriptHash, NativeScript>,
) -> Result<(), ShelleyUtxowPredFailure> {
    let scripts_received: HashSet<ScriptHash> = scripts_provided.keys().copied().collect();

    // Missing = needed - received
    let missing: HashSet<ScriptHash> = scripts_needed
        .difference(&scripts_received)
        .copied()
        .collect();

    // Extra = received - needed
    let extra: HashSet<ScriptHash> = scripts_received
        .difference(scripts_needed)
        .copied()
        .collect();

    if !missing.is_empty() {
        Err(ShelleyUtxowPredFailure::MissingScriptWitnessesUTXOW(missing))
    } else if !extra.is_empty() {
        Err(ShelleyUtxowPredFailure::ExtraneousScriptWitnessesUTXOW(extra))
    } else {
        Ok(())
    }
}

/// Validate verified witnesses
/// Reference: Utxow.hs:210-226 (validateVerifiedWits)
///
/// Cryptographically verifies all VKey signatures.
pub fn validate_verified_wits(tx: &Tx) -> Result<(), ShelleyUtxowPredFailure> {
    let tx_body_hash = tx.body_hash();

    // Find witnesses where verification fails
    let failed_wits: Vec<KeyHash> = tx
        .wits
        .vkey_wits
        .iter()
        .filter(|wit| !wit.verify(tx_body_hash))
        .map(|wit| wit.key_hash())
        .collect();

    if failed_wits.is_empty() {
        Ok(())
    } else {
        Err(ShelleyUtxowPredFailure::InvalidWitnessesUTXOW(failed_wits))
    }
}

/// Validate needed witnesses
/// Reference: Utxow.hs:422-434 (validateNeededWitnesses)
///
/// Checks that all required VKey witnesses are present.
pub fn validate_needed_witnesses(
    wits_key_hashes: &HashSet<KeyHash>,
    cert_state: &CertState,
    utxo: &UTxO,
    tx_body: &TxBody,
) -> Result<(), ShelleyUtxowPredFailure> {
    let needed = get_shelley_wits_vkey_needed(cert_state, utxo, tx_body);
    let missing: HashSet<KeyHash> = needed.difference(wits_key_hashes).copied().collect();

    if missing.is_empty() {
        Ok(())
    } else {
        Err(ShelleyUtxowPredFailure::MissingVKeyWitnessesUTXOW(missing))
    }
}

/// Validate metadata (full: hash consistency + metadatum value-size when pv > (2,0)).
/// Reference: Utxow.hs:436-452 (validateMetadata)
///
/// ```haskell
/// validateMetadata pp tx =
///   let txBody = tx ^. bodyTxL
///       pv = pp ^. ppProtocolVersionL
///    in case (txBody ^. auxDataHashTxBodyL, tx ^. auxDataTxL) of
///         (SNothing, SNothing) -> pure ()
///         (SJust mdh, SNothing) -> failure $ MissingTxMetadata mdh
///         (SNothing, SJust md') -> failure $ MissingTxBodyMetadataHash (hashTxAuxData md')
///         (SJust mdh, SJust md') ->
///           sequenceA_
///             [ failureUnless (hashTxAuxData md' == mdh) $ ConflictingMetadataHash ...
///             , when (SoftForks.validMetadata pv) $
///                 failureUnless (validateTxAuxData pv md') InvalidMetadata
///             ]
/// ```
///
/// Steps:
/// 1. No hash, no data: OK.
/// 2. Hash but no data: MissingTxMetadata.
/// 3. Data but no hash: MissingTxBodyMetadataHash.
/// 4. Both present:
///    a. Hash of auxiliary data CBOR bytes must match body's auxDataHash.
///    b. When protocol_version > (2,0) (SoftForks.validMetadata), call
///       era-specific validateTxAuxData:
///       - Shelley:      `all validMetadatum metadata`
///       - Allegra/Mary: `all validMetadatum metadata` (scripts not validated)
///       - Alonzo+:      `all validMetadatum metadata && all (validScript pv) scripts`
pub fn validate_metadata(
    tx: &Tx,
    protocol_version: ProtocolVersion,
) -> Result<(), ShelleyUtxowPredFailure> {
    match (&tx.body.auxiliary_data_hash, &tx.auxiliary_data) {
        // (SNothing, SNothing) -> pure ()
        (None, None) => Ok(()),

        // (SJust mdh, SNothing) -> failure $ MissingTxMetadata mdh
        (Some(hash), None) => Err(ShelleyUtxowPredFailure::MissingTxMetadata(*hash)),

        // (SNothing, SJust md') -> failure $ MissingTxBodyMetadataHash (hashTxAuxData md')
        (None, Some(aux_data)) => Err(ShelleyUtxowPredFailure::MissingTxBodyMetadataHash(
            aux_data.hash(),
        )),

        // (SJust mdh, SJust md') -> hash check + validateTxAuxData
        (Some(body_hash), Some(aux_data)) => {
            // hashTxAuxData md' == mdh
            // Hash is BLAKE2b-256 of original CBOR bytes (via MemoBytes/getMemoSafeHash)
            let computed_hash = aux_data.hash();
            if *body_hash != computed_hash {
                return Err(ShelleyUtxowPredFailure::ConflictingMetadataHash {
                    expected: *body_hash,
                    actual: computed_hash,
                });
            }
            // when (SoftForks.validMetadata pv) $ failureUnless (validateTxAuxData pv md') InvalidMetadata
            // Shelley era: validateTxAuxData _ (ShelleyTxAuxData m) = all validMetadatum m
            if valid_metadata_soft_fork(protocol_version)
                && !aux_data.validate_shelley(protocol_version)
            {
                return Err(ShelleyUtxowPredFailure::InvalidMetadata);
            }
            Ok(())
        }
    }
}

/// Validate MIR insufficient genesis signatures
/// Reference: Utxow.hs:267-288 (validateMIRInsufficientGenesisSigs)
///
/// Checks that MIR certificates have enough genesis signatures.
pub fn validate_mir_insufficient_genesis_sigs(
    gen_delegs: &GenDelegs,
    quorum: u64,
    wits_key_hashes: &HashSet<KeyHash>,
    tx: &Tx,
) -> Result<(), ShelleyUtxowPredFailure> {
    // Check if tx contains MIR certificates
    let has_mir = tx.body.certificates.iter().any(|cert| matches!(cert, ShelleyTxCert::MIR { .. }));

    if !has_mir {
        return Ok(());
    }

    // Count genesis delegate signatures
    let gen_sigs: HashSet<KeyHash> = gen_delegs
        .delegates
        .values()
        .filter(|delegate| wits_key_hashes.contains(*delegate))
        .copied()
        .collect();

    if gen_sigs.len() as u64 >= quorum {
        Ok(())
    } else {
        Err(ShelleyUtxowPredFailure::MIRInsufficientGenesisSigsUTXOW(
            gen_sigs,
        ))
    }
}

// ============================================================================
// Main UTXOW Transition Function
// Reference: Utxow.hs:296-333 (transitionRulesUTXOW)
// ============================================================================

/// UTXOW environment
/// Reference: Utxow.hs (UtxoEnv: slot, pp, certState); pp includes protocol version
pub struct UtxoEnv {
    pub slot: SlotNo,
    pub quorum: u64,
    /// Protocol version (major, minor). Used for metadata soft fork: pv > (2,0) enables metadatum value-size check.
    pub protocol_version: ProtocolVersion,
}

/// Shelley UTXOW validation
/// Reference: Utxow.hs:296-333 (transitionRulesUTXOW)
///
/// This is the main entry point for Phase 1 witness validation.
/// After this passes, the transaction proceeds to UTXO structural validation.
pub fn shelley_utxow_transition(
    env: &UtxoEnv,
    cert_state: &CertState,
    utxo: &UTxO,
    tx: &Tx,
) -> Result<(), ShelleyUtxowPredFailure> {
    // Extract witness key hashes
    // witsKeyHashes := { hashKey vk | vk ∈ dom(txwitsVKey txw) }
    let wits_key_hashes: HashSet<KeyHash> =
        tx.wits.vkey_wits.iter().map(|w| w.key_hash()).collect();

    // Get scripts provided
    let scripts_provided = &tx.wits.script_wits;

    // Step 1: Validate native scripts (line 308)
    // ∀ s ∈ range(txscripts txw) ∩ Scriptnative, runNativeScript s tx
    validate_failed_native_scripts(scripts_provided, tx)?;

    // Step 2: Check script presence (line 311)
    // { s | (_,s) ∈ scriptsNeeded utxo tx} = dom(txscripts txw)
    let scripts_needed = get_shelley_scripts_needed(utxo, &tx.body);
    validate_missing_scripts(&scripts_needed, scripts_provided)?;

    // Step 3: Verify VKey signatures (line 316)
    // ∀ (vk ↦ σ) ∈ (txwitsVKey txw), V_vk⟦ txBodyHash ⟧_σ
    validate_verified_wits(tx)?;

    // Step 4: Check required witnesses (line 319)
    // witsVKeyNeeded utxo tx genDelegs ⊆ witsKeyHashes
    validate_needed_witnesses(&wits_key_hashes, cert_state, utxo, &tx.body)?;

    // Step 5: Validate metadata (line 323)
    // (adh = ◇ ∧ ad = ◇) ∨ (adh = hashAD ad); when pv > (2,0) also validateTxAuxData (validMetadatum)
    validate_metadata(tx, env.protocol_version)?;

    // Step 6: Check MIR genesis signatures (line 328)
    // { c ∈ txcerts txb ∩ TxCert_mir } ≠ ∅ ⇒ |genSig| ≥ Quorum
    validate_mir_insufficient_genesis_sigs(
        &cert_state.gen_delegs,
        env.quorum,
        &wits_key_hashes,
        tx,
    )?;

    // Step 7: Call UTXO rule (line 333)
    // In real implementation, this would call the UTXO transition function
    // trans @(EraRule "UTXO" era) $ TRC (utxoEnv, u, tx)

    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_key_hash(id: u8) -> KeyHash {
        let mut bytes = [0u8; 32];
        bytes[0] = id;
        Hash(bytes)
    }

    fn make_script_hash(id: u8) -> ScriptHash {
        let mut bytes = [0u8; 32];
        bytes[0] = id;
        Hash(bytes)
    }

    #[test]
    fn test_native_script_require_signature() {
        let key1 = make_key_hash(1);
        let key2 = make_key_hash(2);

        let script = NativeScript::RequireSignature(key1);

        // With key1 present: validates
        let mut keys = HashSet::new();
        keys.insert(key1);
        assert!(script.validate(&keys));

        // With different key: fails
        let mut keys = HashSet::new();
        keys.insert(key2);
        assert!(!script.validate(&keys));
    }

    #[test]
    fn test_native_script_require_all_of() {
        let key1 = make_key_hash(1);
        let key2 = make_key_hash(2);

        let script = NativeScript::RequireAllOf(vec![
            NativeScript::RequireSignature(key1),
            NativeScript::RequireSignature(key2),
        ]);

        // With both keys: validates
        let mut keys = HashSet::new();
        keys.insert(key1);
        keys.insert(key2);
        assert!(script.validate(&keys));

        // With only key1: fails
        let mut keys = HashSet::new();
        keys.insert(key1);
        assert!(!script.validate(&keys));
    }

    #[test]
    fn test_native_script_require_m_of_n() {
        let key1 = make_key_hash(1);
        let key2 = make_key_hash(2);
        let key3 = make_key_hash(3);

        // 2-of-3 multisig
        let script = NativeScript::RequireMOf {
            required: 2,
            scripts: vec![
                NativeScript::RequireSignature(key1),
                NativeScript::RequireSignature(key2),
                NativeScript::RequireSignature(key3),
            ],
        };

        // With 2 keys: validates
        let mut keys = HashSet::new();
        keys.insert(key1);
        keys.insert(key3);
        assert!(script.validate(&keys));

        // With 1 key: fails
        let mut keys = HashSet::new();
        keys.insert(key1);
        assert!(!script.validate(&keys));
    }

    #[test]
    fn test_validate_missing_scripts() {
        let sh1 = make_script_hash(1);
        let sh2 = make_script_hash(2);

        let mut needed = HashSet::new();
        needed.insert(sh1);

        // Script provided: OK
        let mut provided = HashMap::new();
        provided.insert(sh1, NativeScript::RequireSignature(make_key_hash(1)));
        assert!(validate_missing_scripts(&needed, &provided).is_ok());

        // Script missing: Error
        let provided = HashMap::new();
        let result = validate_missing_scripts(&needed, &provided);
        assert!(matches!(
            result,
            Err(ShelleyUtxowPredFailure::MissingScriptWitnessesUTXOW(_))
        ));

        // Extra script: Error
        let mut provided = HashMap::new();
        provided.insert(sh1, NativeScript::RequireSignature(make_key_hash(1)));
        provided.insert(sh2, NativeScript::RequireSignature(make_key_hash(2)));
        let result = validate_missing_scripts(&needed, &provided);
        assert!(matches!(
            result,
            Err(ShelleyUtxowPredFailure::ExtraneousScriptWitnessesUTXOW(_))
        ));
    }

    // ========================================================================
    // Metadata validation tests
    // ========================================================================
    //
    // These tests validate against decoded Metadatum values (matching Haskell).
    // In Haskell, metadata values are decoded from CBOR at deserialization time
    // and stored as `Map Word64 Metadatum`. The CBOR decoder produces:
    //   - I(n) for integers
    //   - B(bytes) for byte strings (raw bytes, CBOR framing stripped)
    //   - S(text) for text strings (decoded UTF-8)
    //   - List/Map for arrays/maps (recursive)
    //
    // The old tests used raw CBOR bytes; now they use decoded Metadatum.
    // ========================================================================

    /// Helper: construct AuxiliaryData from decoded metadata.
    fn make_aux_data(metadata: HashMap<u64, Metadatum>) -> AuxiliaryData {
        // In a real implementation, original_bytes would be the CBOR
        // encoding of the metadata map. The hash is computed from these bytes.
        AuxiliaryData {
            metadata,
            original_bytes: vec![], // Simplified for tests
        }
    }

    /// Helper: construct a Tx with auxiliary data for metadata validation tests.
    fn make_tx_with_aux_data(aux_data: AuxiliaryData) -> Tx {
        let hash = aux_data.hash();
        Tx {
            body: TxBody {
                inputs: HashSet::new(),
                outputs: vec![],
                fee: 0,
                ttl: None,
                certificates: vec![],
                withdrawals: HashMap::new(),
                update: None,
                auxiliary_data_hash: Some(hash),
            },
            wits: TxWits {
                vkey_wits: vec![],
                script_wits: HashMap::new(),
            },
            auxiliary_data: Some(aux_data),
        }
    }

    #[test]
    fn test_validate_metadata_invalid_metadatum_when_pv_gt_2_0() {
        // When pv > (2,0), metadatum with bytestring > 64 bytes must fail with InvalidMetadata.
        // Metadatum::B holds raw bytes (CBOR framing already stripped by decoder).
        // Reference: Metadata.hs:80 - `validMetadatum (B b) = BS.length b <= 64`
        let long_bytes = vec![0u8; 65]; // 65 raw bytes (exceeds 64 limit)
        let metadata = HashMap::from([(1u64, Metadatum::B(long_bytes))]);
        let aux_data = make_aux_data(metadata);
        let tx = make_tx_with_aux_data(aux_data);
        let pv_3_0 = ProtocolVersion { major: 3, minor: 0 };
        let result = validate_metadata(&tx, pv_3_0);
        assert!(
            matches!(result, Err(ShelleyUtxowPredFailure::InvalidMetadata)),
            "expected InvalidMetadata when pv > (2,0) and metadatum bytestring len > 64, got {:?}",
            result
        );
    }

    #[test]
    fn test_validate_metadata_valid_metadatum_when_pv_gt_2_0() {
        // When pv > (2,0), metadatum with bytestring <= 64 bytes must pass.
        // Reference: Metadata.hs:80 - `validMetadatum (B b) = BS.length b <= 64`
        let metadata = HashMap::from([(1u64, Metadatum::B(vec![0x01, 0x02]))]);
        let aux_data = make_aux_data(metadata);
        let tx = make_tx_with_aux_data(aux_data);
        let pv_3_0 = ProtocolVersion { major: 3, minor: 0 };
        let result = validate_metadata(&tx, pv_3_0);
        assert!(result.is_ok(), "expected OK for valid metadatum, got {:?}", result);
    }

    #[test]
    fn test_validate_metadata_text_too_long() {
        // When pv > (2,0), metadatum text with UTF-8 byte length > 64 must fail.
        // Reference: Metadata.hs:81 - `validMetadatum (S s) = BS.length (T.encodeUtf8 s) <= 64`
        let long_text = "a".repeat(65); // 65 ASCII bytes
        let metadata = HashMap::from([(1u64, Metadatum::S(long_text))]);
        let aux_data = make_aux_data(metadata);
        let tx = make_tx_with_aux_data(aux_data);
        let pv_3_0 = ProtocolVersion { major: 3, minor: 0 };
        let result = validate_metadata(&tx, pv_3_0);
        assert!(
            matches!(result, Err(ShelleyUtxowPredFailure::InvalidMetadata)),
            "expected InvalidMetadata for text > 64 UTF-8 bytes, got {:?}",
            result
        );
    }

    #[test]
    fn test_validate_metadata_integer_always_valid() {
        // Integers are always valid (no size limit on the metadatum value).
        // Reference: Metadata.hs:79 - `validMetadatum (I _) = True`
        // Note: Integer *encoding* range (-(2^64-1) .. 2^64-1) is enforced
        // by the CBOR decoder, not by validMetadatum.
        let metadata = HashMap::from([(1u64, Metadatum::I(i128::MAX))]);
        let aux_data = make_aux_data(metadata);
        let tx = make_tx_with_aux_data(aux_data);
        let pv_3_0 = ProtocolVersion { major: 3, minor: 0 };
        let result = validate_metadata(&tx, pv_3_0);
        assert!(result.is_ok(), "expected OK for integer metadatum, got {:?}", result);
    }

    #[test]
    fn test_validate_metadata_nested_invalid() {
        // Nested structure with invalid leaf must fail.
        // Reference: Metadata.hs:82-87 - recursive check on List/Map
        let invalid_leaf = Metadatum::B(vec![0u8; 65]); // too long
        let nested = Metadatum::List(vec![
            Metadatum::I(42),
            Metadatum::List(vec![invalid_leaf]), // deeply nested invalid
        ]);
        let metadata = HashMap::from([(1u64, nested)]);
        let aux_data = make_aux_data(metadata);
        let tx = make_tx_with_aux_data(aux_data);
        let pv_3_0 = ProtocolVersion { major: 3, minor: 0 };
        let result = validate_metadata(&tx, pv_3_0);
        assert!(
            matches!(result, Err(ShelleyUtxowPredFailure::InvalidMetadata)),
            "expected InvalidMetadata for nested invalid metadatum, got {:?}",
            result
        );
    }

    #[test]
    fn test_validate_metadata_map_keys_validated() {
        // Map keys must also satisfy validMetadatum.
        // Reference: Metadata.hs:83-87 - checks both k and v
        let invalid_key = Metadatum::B(vec![0u8; 65]);
        let valid_value = Metadatum::I(1);
        let map_metadatum = Metadatum::Map(vec![(invalid_key, valid_value)]);
        let metadata = HashMap::from([(1u64, map_metadatum)]);
        let aux_data = make_aux_data(metadata);
        let tx = make_tx_with_aux_data(aux_data);
        let pv_3_0 = ProtocolVersion { major: 3, minor: 0 };
        let result = validate_metadata(&tx, pv_3_0);
        assert!(
            matches!(result, Err(ShelleyUtxowPredFailure::InvalidMetadata)),
            "expected InvalidMetadata for invalid map key, got {:?}",
            result
        );
    }

    #[test]
    fn test_validate_metadata_skipped_when_pv_le_2_0() {
        // When pv <= (2,0), metadatum value-size check is disabled.
        // Reference: SoftForks.hs:12-15 - `validMetadata pv = pv > ProtVer (natVersion @2) 0`
        // Invalid metadatum should NOT cause failure at Shelley pv (2,0).
        let long_bytes = vec![0u8; 65]; // Would fail if checked
        let metadata = HashMap::from([(1u64, Metadatum::B(long_bytes))]);
        let aux_data = make_aux_data(metadata);
        let tx = make_tx_with_aux_data(aux_data);
        let pv_2_0 = ProtocolVersion { major: 2, minor: 0 };
        let result = validate_metadata(&tx, pv_2_0);
        assert!(
            result.is_ok(),
            "expected OK at pv (2,0) even with invalid metadatum, got {:?}",
            result
        );
    }

    #[test]
    fn test_validate_metadata_boundary_64_bytes() {
        // Exactly 64 bytes: valid. 
        // Reference: Metadata.hs:80 - `BS.length b <= 64`
        let metadata_ok = HashMap::from([(1u64, Metadatum::B(vec![0u8; 64]))]);
        let aux_data = make_aux_data(metadata_ok);
        let tx = make_tx_with_aux_data(aux_data);
        let pv_3_0 = ProtocolVersion { major: 3, minor: 0 };
        assert!(validate_metadata(&tx, pv_3_0).is_ok());

        // Exactly 64 UTF-8 bytes text: valid.
        let metadata_text = HashMap::from([(1u64, Metadatum::S("a".repeat(64)))]);
        let aux_data = make_aux_data(metadata_text);
        let tx = make_tx_with_aux_data(aux_data);
        assert!(validate_metadata(&tx, pv_3_0).is_ok());
    }

    #[test]
    fn test_validate_metadata_complex_valid_structure() {
        // Complex nested structure that is valid.
        let metadata = HashMap::from([
            (0u64, Metadatum::I(42)),
            (1u64, Metadatum::B(vec![1, 2, 3])),
            (2u64, Metadatum::S("hello".to_string())),
            (3u64, Metadatum::List(vec![
                Metadatum::I(-100),
                Metadatum::B(vec![0u8; 64]),
                Metadatum::Map(vec![
                    (Metadatum::S("key".to_string()), Metadatum::I(999)),
                ]),
            ])),
        ]);
        let aux_data = make_aux_data(metadata);
        let tx = make_tx_with_aux_data(aux_data);
        let pv_3_0 = ProtocolVersion { major: 3, minor: 0 };
        assert!(
            validate_metadata(&tx, pv_3_0).is_ok(),
            "expected OK for complex valid metadata structure"
        );
    }
}
