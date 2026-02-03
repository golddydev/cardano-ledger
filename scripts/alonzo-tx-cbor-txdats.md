# Decoding raw Alonzo transaction CBOR and locating TxDats bytes

## Alonzo CDDL transaction structure

From `eras/alonzo/impl/cddl/data/alonzo.cddl`:

```cddl
transaction = 
  [transaction_body, transaction_witness_set, bool, auxiliary_data/ nil]

transaction_witness_set = 
  { ? 0 : [* vkeywitness]      
  , ? 1 : [* native_script]    
  , ? 2 : [* bootstrap_witness]
  , ? 3 : [* plutus_v1_script]
  , ? 4 : [* plutus_data]       ; TxDats (datums)
  , ? 5 : redeemers
  }
```

So a raw Alonzo transaction is a **CBOR array of 4 elements**:
1. **transaction_body** – CBOR map (keys 0, 1, 2, …)
2. **transaction_witness_set** – CBOR map with optional keys 0–5
3. **bool** – validity flag
4. **auxiliary_data** or nil

## Where TxDats lives

- **TxDats** = the value of key **4** in `transaction_witness_set`.
- In CDDL, key 4 is `[* plutus_data]` (array of Plutus data / datums).
- For the **script integrity hash**, the spec (and `Tx.hs` `originalBytes`) use the **exact bytes** of that value as stored on the wire: *“the datums are exactly the data present in the transaction witness set”* (CDDL comment). So **TxDats bytes** = the raw CBOR bytes that encode the value at key 4 in the witness set (the same bytes that would be stored in MemoBytes for TxDats when decoding).

## How to find the raw bytes of TxDats from raw tx CBOR

1. **Parse the outer array**
   - Decode the first byte: if it is `0x83` or `0x84` (or similar array), you have `[body, wits, …]`.
   - Skip the first array element (transaction_body) by decoding it and noting its byte range, or by parsing into the witness set.

2. **Parse the witness set (second element)**
   - The witness set is a **CBOR map** (major type 5).
   - Map encoding: one byte for type+size (e.g. `0xa4` = map of 4 entries), then alternating **key** and **value** for each entry.
   - Keys are small integers 0–5; values vary. You need the **value** associated with key **4**.

3. **Locate the value for key 4**
   - Walk the map entries in order. Each entry is: `encode(key)` then `encode(value)`.
   - When you see key **4** (CBOR: `0x04`), the **following** bytes form the value until the next key or end of map. That value is the **TxDats** payload.

4. **What those bytes are**
   - The Haskell encoder for TxDats is `encodeWithSetTag . Map.elems . unTxDatsRaw` (see `TxWits.hs`).
   - For encoding version ≥ 9, `encodeWithSetTag` prepends **CBOR tag 258** (set tag), then the array of datums.
   - Tag 258 is encoded as **3 bytes**: `D9 01 02` (RFC 8949: major type 6, additional info 25 = 2-byte tag, then 258 in big-endian).
   - So the TxDats value on the wire is typically:
     - **D9 01 02** + CBOR array of plutus_data (e.g. **81** + one datum for a single datum).
   - For older encoding (version &lt; 9), the value is just the CBOR array of plutus_data (no tag 258).

So the **raw bytes of TxDats** to use for the script integrity hash are exactly the bytes of that value (the full encoding of key 4, including the optional `D9 01 02` prefix if present).

## Script integrity hash: original bytes (Haskell)

The **original bytes** that get BLAKE2b-256-hashed are exactly:

**`redeemers_bytes || txdats_bytes || lang_views_bytes`**

Defined in `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Tx.hs`:

```haskell
-- SafeToHash (ScriptIntegrity era)
originalBytes (ScriptIntegrity m d l) =
  let dBytes = if null (d ^. unTxDatsL) then mempty else originalBytes d
      lBytes = serialize' (eraProtVerLow @era) (encodeLangViews l)
   in originalBytes m <> dBytes <> lBytes
```

- **m** = redeemers (Redeemers era) → `originalBytes m` = MemoBytes of redeemers (exact bytes from tx CBOR key 5).
- **d** = TxDats → `originalBytes d` = MemoBytes of TxDats, i.e. the **exact bytes** that encoded key 4 when the tx was decoded (no re-encoding).
- **l** = lang views → `lBytes` = encoded language views from protocol params.

So the hash input is always: **redeemers (as stored) || datums (as stored) || language views (re-encoded)**.

## Why is the datums array sometimes 81 (definite) and sometimes 9f (indefinite)?

In the **tx CBOR on chain**, the value at key 4 (TxDats) is a CBOR array of Plutus data. That array can be encoded in two ways:

- **Definite-length**: e.g. `81` (one byte) = array of 1 element, then one datum.
- **Indefinite-length**: `9f` (start indefinite array) … `ff` (break).

Both are valid CBOR. Which one appears depends on **how the transaction was encoded when it was created** (wallet/node/version):

- **cardano-ledger-binary** encodes lists with `encodeList` (`Encoder.hs`):
  - **Encoding version ≥ 2**: if the list has **≤ 23 elements**, use **definite** length (e.g. `81` for 1 element). If **> 23 elements**, use **indefinite** (`9f` … `ff`).
  - **Encoding version < 2**: **always indefinite** (`9f` … `ff`).

So:

- Tx `137f32a8...` has key-4 value starting with `81` → created with encoding version ≥ 2 and ≤ 23 datums (e.g. 1 datum).
- Tx `94a4e709...` has key-4 value starting with `9f` → either created with encoding version < 2, or with a client that always uses indefinite lists for this field.

Once the tx is on chain, the **stored** bytes for key 4 are whatever the creator produced. The ledger does **not** re-encode: when decoding, TxDats are stored in **MemoBytes** (`TxWits.hs`: “we must preserve the original bytes for ScriptIntegrity”), so the script integrity hash uses those exact bytes.

## Extracting datums CBOR from tx CBOR “as is”

You want the **exact** bytes of the value at key 4, with no re-encoding (so definite `81...` or indefinite `9f...ff` is preserved).

That is exactly what **`scripts/extract_txdats_from_tx_cbor.py`** does: it parses the tx CBOR only far enough to find the witness map and the **value** associated with key 4, then returns that value as a **raw byte substring**. It does not decode the array or the datums inside it; it just returns the slice of the tx bytes that form the key-4 value. So you get the datums CBOR in the same form as in the tx (definite or indefinite).

Usage:

```bash
# Raw tx CBOR hex on stdin → key-4 value hex on stdout (exact bytes, 81 or 9f...ff)
python3 scripts/extract_txdats_from_tx_cbor.py < raw_tx.hex
# Or with DB Sync:
python3 scripts/fetch_tx_cbor_from_dbsync.py <tx_hash_hex> | python3 scripts/extract_txdats_from_tx_cbor.py
```

The resulting hex is the **txdats_bytes** to use in:

`BLAKE2b-256(redeemers_bytes || txdats_bytes || lang_views_bytes)`.

## Getting the exact TxDats bytes

The only way to get the **exact** bytes used for the script integrity hash is to parse the stored transaction CBOR and take the **value** of key 4 in the witness set as a raw byte substring. Re-encoding the datum (e.g. with or without set tag 258, or with different integer encodings) can change the hash.

**Scripts in this repo:**

- **`scripts/fetch_tx_cbor_from_dbsync.py`** – Fetches raw transaction CBOR from Cardano DB Sync by tx hash. Requires `psycopg2-binary` and a DB Sync connection string (env `DBSYNC_CONNECTION_STRING`). Outputs tx CBOR as hex.
- **`scripts/extract_txdats_from_tx_cbor.py`** – Reads raw tx CBOR hex from stdin and prints the key-4 (TxDats) value as hex. No external deps.
- **`scripts/script_integrity_hash.py`** – Tries many redeemers/txdats/lang_views variants; set env **`RAW_TX_HEX`** to use TxDats extracted from that tx (then only those candidates are tried).

**Full workflow (fetch from DB Sync → verify script integrity hash):**

```bash
# 1) Fetch raw tx CBOR from DB Sync (requires psycopg2-binary)
export DBSYNC_CONNECTION_STRING="postgresql://user:pass@host:5432/dbsync-mainnet"
RAW_TX_HEX=$(python3 scripts/fetch_tx_cbor_from_dbsync.py 137f32a8c6e55a5b85472ba13e9908160623a18877e9d0fa4f7a8c393df0560e)
# 2) Run script integrity hash using extracted TxDats from that tx
python3 scripts/script_integrity_hash.py
```

Or without env: pipe fetched CBOR into the extractor, then use the hash formula manually:

```bash
TXDATS=$(python3 scripts/fetch_tx_cbor_from_dbsync.py <tx_hash_hex> | python3 scripts/extract_txdats_from_tx_cbor.py)
# BLAKE2b-256(A0 || $TXDATS || A0)
```

## Fetching raw tx CBOR from Cardano DB Sync

[Cardano DB Sync](https://github.com/IntersectMBO/cardano-db-sync) stores chain data in PostgreSQL. Raw transaction CBOR is in **`tx_cbor`**:

| Table     | Key columns        | Description                    |
|----------|---------------------|--------------------------------|
| `tx`     | `id`, `hash` (hash32) | One row per transaction      |
| `tx_cbor`| `tx_id`, `bytes` (bytea) | Raw CBOR per tx (join on `tx_id = tx.id`) |

**SQL to get raw tx CBOR by transaction hash (64 hex chars):**

```sql
SELECT encode(c.bytes, 'hex')
FROM tx_cbor c
JOIN tx t ON c.tx_id = t.id
WHERE t.hash = decode('<tx_hash_hex>', 'hex');
```

Example with `psql` (connection string in env or as args):

```bash
psql "$DBSYNC_CONNECTION_STRING" -t -A -c "SELECT encode(c.bytes, 'hex') FROM tx_cbor c JOIN tx t ON c.tx_id = t.id WHERE t.hash = decode('137f32a8c6e55a5b85472ba13e9908160623a18877e9d0fa4f7a8c393df0560e', 'hex');"
```

The script **`scripts/fetch_tx_cbor_from_dbsync.py`** does this and prints the hex; it uses env **`DBSYNC_CONNECTION_STRING`** (or a default Demeter mainnet URL) and takes the tx hash as argument or **`TX_HASH`**.

## Reference

- **Script integrity original bytes**: `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/Tx.hs` (`SafeToHash ScriptIntegrity`: `originalBytes m <> dBytes <> lBytes`; `dBytes` = `originalBytes d` when non-empty).
- **TxDats encoding**: `eras/alonzo/impl/src/Cardano/Ledger/Alonzo/TxWits.hs` – `TxDatsRaw` `encCBOR` = `encodeWithSetTag . Map.elems . unTxDatsRaw`; `encodeWithSetTag` at line 746. TxDats are MemoBytes so decoded bytes are preserved.
- **List encoding (81 vs 9f)**: `libs/cardano-ledger-binary/src/Cardano/Ledger/Binary/Encoding/Encoder.hs` – `encodeList` (lines 492–501): encoding version ≥ 2 uses definite length for lists with ≤ 23 elements (`lengthThreshold = 23`), indefinite otherwise; version < 2 always indefinite.
- **CDDL**: `eras/alonzo/impl/cddl/data/alonzo.cddl` (transaction, transaction_witness_set, script_data_hash comment).
