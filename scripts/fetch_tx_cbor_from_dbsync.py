#!/usr/bin/env python3
"""
Fetch raw transaction CBOR from Cardano DB Sync by transaction hash.

DB Sync schema (see cardano-db-sync doc/schema.md):
  - tx: id, hash (hash32type), ...
  - tx_cbor: id, tx_id (FK to tx.id), bytes (bytea) = raw CBOR transaction

Usage:
  python3 fetch_tx_cbor_from_dbsync.py <tx_hash_hex>
  TX_HASH=137f32a8c6e55a5b85472ba13e9908160623a18877e9d0fa4f7a8c393df0560e \\
    python3 fetch_tx_cbor_from_dbsync.py

Output: raw transaction CBOR as hex (one line), or nothing and exit 1 if not found.

Requires: psycopg2 (pip install psycopg2-binary). Connection string from env
DBSYNC_CONNECTION_STRING (defaults to a Demeter mainnet example; replace with yours).
"""

import os
import sys


def main() -> int:
    tx_hash_hex = (
        os.environ.get("TX_HASH") or
        (sys.argv[1] if len(sys.argv) > 1 else None)
    )
    if not tx_hash_hex or len(tx_hash_hex) != 64:
        sys.stderr.write("Usage: fetch_tx_cbor_from_dbsync.py <tx_hash_hex> (64 hex chars)\n")
        sys.stderr.write("   or: TX_HASH=<hex> python3 fetch_tx_cbor_from_dbsync.py\n")
        return 1

    conn_str = os.environ.get(
        "DBSYNC_CONNECTION_STRING",
        "postgresql://dbsync1sj9jf7ly8pl6cua0hg2:hwGC55BXnek@cardano-mainnet.dbsync-v3.demeter.run:5432/dbsync-mainnet",
    )

    try:
        import psycopg2
    except ImportError:
        sys.stderr.write("Requires psycopg2. Run: pip install psycopg2-binary\n")
        return 1

    try:
        conn = psycopg2.connect(conn_str)
    except Exception as e:
        sys.stderr.write(f"Connection failed: {e}\n")
        return 1

    # tx.hash is stored as bytea (32 bytes). Pass hex and decode in SQL.
    sql = """
    SELECT encode(c.bytes, 'hex')
    FROM tx_cbor c
    JOIN tx t ON c.tx_id = t.id
    WHERE t.hash = decode(%s, 'hex')
    """
    try:
        with conn.cursor() as cur:
            cur.execute(sql, (tx_hash_hex.strip().lower(),))
            row = cur.fetchone()
    finally:
        conn.close()

    if not row:
        sys.stderr.write("No tx_cbor row found for that tx hash.\n")
        return 1

    print(row[0])
    return 0


if __name__ == "__main__":
    sys.exit(main())
