#!/usr/bin/env python3
"""
Script integrity hash for Alonzo (and compatible) transactions.

Formula (Tx.hs SafeToHash ScriptIntegrity):
  originalBytes = originalBytes(redeemers) <> dBytes <> lBytes
  script_integrity_hash = BLAKE2b-256(originalBytes)

When tx has:
  - no redeemers -> redeemers bytes = A0 (empty CBOR map)
  - no Plutus scripts -> lang_views bytes = A0 (empty CBOR map)
  - one datum (TxDats) -> dBytes = exact bytes stored in MemoBytes for TxDats
    (i.e. the value for key 4 in the witness set as it appears on the wire)

Tx ID: 137f32a8c6e55a5b85472ba13e9908160623a18877e9d0fa4f7a8c393df0560e
Expected script integrity hash: 882F2862C3EC2B692FC22AEA0CBE3A88D94E32B63A83E23F5B64E02C11DCEFDF

Optional: set env RAW_TX_HEX to the full transaction CBOR hex to use the exact
key-4 (TxDats) bytes extracted from the tx; then only that candidate is tried.
"""

import hashlib
import os
import sys


def blake2b_256(data: bytes) -> bytes:
    return hashlib.blake2b(data, digest_size=32).digest()


def script_integrity_hash_hex(redeemers_hex: str, txdats_hex: str, lang_views_hex: str) -> str:
    """Compute script integrity hash from the three parts (hex strings)."""
    redeemers = bytes.fromhex(redeemers_hex)
    txdats = bytes.fromhex(txdats_hex)
    lang_views = bytes.fromhex(lang_views_hex)
    original_bytes = redeemers + txdats + lang_views
    return blake2b_256(original_bytes).hex().upper()


# For tx with only one datum, no redeemers, no Plutus scripts:
# Redeemers: empty map = A0 (TxWits encodes Redeemers as map for version >= 9)
EMPTY_REDEEMERS_MAP = "a0"
# Empty array 80 in case redeemers were encoded as list in some version
EMPTY_REDEEMERS_LIST = "80"
EMPTY_LANG_VIEWS = "a0"

# TxDats: key 4 value = set tag (optional) + array of plutus_data.
# Datum = tag 121 + indefinite array; last integer 229 encoded as 19 01 e5 (uint16).
# Base datum with 19 01 e5 then ff (one break):
DATUM_SUFFIX_U16 = "1901e5"  # 229 as uint16
DATUM_SUFFIX_U8 = "18e5"     # 229 as uint8
DATUM_SUFFIX_U32 = "1a000000e5"  # 229 as uint32
DATUM_SUFFIX_NEG = "38e5"    # -230 in CBOR (value -1-229) - unlikely
# Prefix common to all datums (tag 121 + indefinite list start + content up to the 229)
DATUM_PREFIX = (
    "d8799f428001d8799fd8799fd8799fd8799f581c7dbc2c8db0fa0a26a7d7c3d51bd830b3230e026b92f5bc88cf7dd901ff"
    "d8799fd8799fd8799fd8799f581c8a1caf5302ec18643186cacca790a10bb2daa981102878933836ff4fffffffffd87a80ff"
    "d87a80ff1a002625a0d8799fd879801a00baba7dd8799f"
)
# Datum endings: suffix + ff (close indefinite list)
DATUM_E5FF = DATUM_PREFIX + DATUM_SUFFIX_U16 + "ff"
DATUM_E5FFFFFF = DATUM_PREFIX + DATUM_SUFFIX_U16 + "ffffff"  # extra ff bytes
DATUM_U8_FF = DATUM_PREFIX + DATUM_SUFFIX_U8 + "ff"
DATUM_U32_FF = DATUM_PREFIX + DATUM_SUFFIX_U32 + "ff"
DATUM_NEG_FF = DATUM_PREFIX + DATUM_SUFFIX_NEG + "ff"

SET_TAG_258 = "d90102"

EXPECTED = "882F2862C3EC2B692FC22AEA0CBE3A88D94E32B63A83E23F5B64E02C11DCEFDF"


def build_candidates():
    """All combinations: redeemers, TxDats variants, lang_views."""
    redeemers_opts = [
        ("redeemers=A0", EMPTY_REDEEMERS_MAP),
        ("redeemers=80", EMPTY_REDEEMERS_LIST),
        ("redeemers=empty", ""),
    ]
    lang_views_opts = [
        ("lang=A0", EMPTY_LANG_VIEWS),
        ("lang=80", "80"),
        ("lang=empty", ""),
    ]
    # TxDats: no tag / tag 258; definite array 81 / indefinite 9f...ff; datum variants
    txdats_opts = [
        ("81+datum(u16,ff)", "81" + DATUM_E5FF),
        ("81+datum(u16,ffffff)", "81" + DATUM_E5FFFFFF),
        ("81+datum(u8,ff)", "81" + DATUM_U8_FF),
        ("81+datum(u32,ff)", "81" + DATUM_U32_FF),
        ("81+datum(neg,ff)", "81" + DATUM_NEG_FF),
        ("D90102+81+datum(u16,ff)", SET_TAG_258 + "81" + DATUM_E5FF),
        ("D90102+81+datum(u16,ffffff)", SET_TAG_258 + "81" + DATUM_E5FFFFFF),
        ("D90102+81+datum(u8,ff)", SET_TAG_258 + "81" + DATUM_U8_FF),
        ("D90102+81+datum(u32,ff)", SET_TAG_258 + "81" + DATUM_U32_FF),
        ("D90102+81+datum(neg,ff)", SET_TAG_258 + "81" + DATUM_NEG_FF),
        # Indefinite-length array: 9f <datum> ff
        ("9f+datum(u16,ff)+ff", "9f" + DATUM_E5FF + "ff"),
        ("9f+datum(u16,ffffff)+ff", "9f" + DATUM_E5FFFFFF + "ff"),
        ("D90102+9f+datum(u16,ff)+ff", SET_TAG_258 + "9f" + DATUM_E5FF + "ff"),
        ("D90102+9f+datum(u16,ffffff)+ff", SET_TAG_258 + "9f" + DATUM_E5FFFFFF + "ff"),
        # Datum only (no array wrapper)
        ("datum_only(u16,ff)", DATUM_E5FF),
        ("D90102+datum_only(u16,ff)", SET_TAG_258 + DATUM_E5FF),
    ]
    for rname, rhex in redeemers_opts:
        for tname, thex in txdats_opts:
            for lname, lhex in lang_views_opts:
                yield (f"{rname}, TxDats={tname}, {lname}", rhex, thex, lhex)


def extract_txdats_from_raw_tx_hex(raw_hex: str) -> str | None:
    """Extract key-4 (TxDats) value bytes from raw tx CBOR hex. Returns hex string or None."""
    script_dir = os.path.dirname(os.path.abspath(__file__))
    if script_dir not in sys.path:
        sys.path.insert(0, script_dir)
    from extract_txdats_from_tx_cbor import extract_txdats_bytes
    raw = raw_hex.strip().replace(" ", "").replace("\n", "")
    data = bytes.fromhex(raw)
    out = extract_txdats_bytes(data)
    return out.hex() if out else None


def main():
    print("Script integrity hash: redeemers || txdats || lang_views -> BLAKE2b-256")
    print(f"Expected: {EXPECTED}\n")

    raw_tx_hex = os.environ.get("RAW_TX_HEX", "").strip()
    if raw_tx_hex:
        txdats_hex = extract_txdats_from_raw_tx_hex(raw_tx_hex)
        if txdats_hex is None:
            print("RAW_TX_HEX set but could not extract key-4 (TxDats) from tx CBOR.")
            return 1
        print(f"Using TxDats extracted from raw tx ({len(txdats_hex)//2} bytes)\n")
        candidates = [
            ("extracted_txdats + redeemers=A0", EMPTY_REDEEMERS_MAP, txdats_hex, EMPTY_LANG_VIEWS),
            ("extracted_txdats + redeemers=80", EMPTY_REDEEMERS_LIST, txdats_hex, EMPTY_LANG_VIEWS),
            ("extracted_txdats + redeemers=empty", "", txdats_hex, EMPTY_LANG_VIEWS),
        ]
    else:
        candidates = list(build_candidates())
        # (name, rhex, thex, lhex)
        candidates = [(n, r, t, l) for (n, r, t, l) in candidates]

    for item in candidates:
        if len(item) == 3:
            name, rhex, thex = item
            lhex = EMPTY_LANG_VIEWS
        else:
            name, rhex, thex, lhex = item
        h = script_integrity_hash_hex(rhex, thex, lhex)
        if h == EXPECTED:
            print(f"MATCH: {name}")
            print(f"  redeemers_hex={rhex}")
            print(f"  txdats_hex={thex[:80]}{'...' if len(thex) > 80 else ''}")
            print(f"  lang_views_hex={lhex}")
            print(f"  hash={h}")
            return 0
    print("No candidate matched.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
