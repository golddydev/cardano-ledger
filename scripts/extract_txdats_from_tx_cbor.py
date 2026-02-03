#!/usr/bin/env python3
"""
Extract the raw bytes of TxDats (key 4 in transaction_witness_set) from raw
Alonzo transaction CBOR.

Alonzo tx = [transaction_body, transaction_witness_set, bool, auxiliary_data].
Witness set is a map with optional keys 0..5; key 4 = [* plutus_data] (TxDats).

Usage:
  python3 extract_txdats_from_tx_cbor.py < raw_tx.hex
  echo "$RAW_TX_HEX" | python3 extract_txdats_from_tx_cbor.py

Output: key-4 value bytes as hex (one line), or nothing if key 4 is absent.
"""

import sys


def cbor_skip_value(data: bytes, pos: int) -> int:
    """Advance pos past one CBOR value; return new position."""
    if pos >= len(data):
        return pos
    b = data[pos]
    mt = b >> 5
    ai = b & 0x1F
    pos += 1
    if mt == 0:  # unsigned int
        if ai < 24:
            return pos
        if ai == 24:  # 1-byte
            return pos + 1
        if ai == 25:  # 2-byte
            return pos + 2
        if ai == 26:  # 4-byte
            return pos + 4
        if ai == 27:  # 8-byte
            return pos + 8
        return pos
    if mt == 1:  # negative int
        if ai < 24:
            return pos
        if ai == 24:
            return pos + 1
        if ai == 25:
            return pos + 2
        if ai == 26:
            return pos + 4
        if ai == 27:
            return pos + 8
        return pos
    if mt == 2:  # byte string
        if ai < 24:
            n = ai
        elif ai == 24:
            n = data[pos]
            pos += 1
        elif ai == 25:
            n = int.from_bytes(data[pos : pos + 2], "big")
            pos += 2
        elif ai == 26:
            n = int.from_bytes(data[pos : pos + 4], "big")
            pos += 4
        else:
            n = int.from_bytes(data[pos : pos + 8], "big")
            pos += 8
        return pos + n
    if mt == 3:  # text string
        if ai < 24:
            n = ai
        elif ai == 24:
            n = data[pos]
            pos += 1
        elif ai == 25:
            n = int.from_bytes(data[pos : pos + 2], "big")
            pos += 2
        elif ai == 26:
            n = int.from_bytes(data[pos : pos + 4], "big")
            pos += 4
        else:
            n = int.from_bytes(data[pos : pos + 8], "big")
            pos += 8
        return pos + n
    if mt == 4:  # array
        if ai == 31:  # indefinite-length array
            while pos < len(data) and data[pos] != 0xFF:
                pos = cbor_skip_value(data, pos)
            return pos + 1 if pos < len(data) else pos
        if ai < 24:
            n = ai
        elif ai == 24:
            n = data[pos]
            pos += 1
        elif ai == 25:
            n = int.from_bytes(data[pos : pos + 2], "big")
            pos += 2
        elif ai == 26:
            n = int.from_bytes(data[pos : pos + 4], "big")
            pos += 4
        else:
            n = int.from_bytes(data[pos : pos + 8], "big")
            pos += 8
        for _ in range(n):
            pos = cbor_skip_value(data, pos)
        return pos
    if mt == 5:  # map
        if ai == 31:  # indefinite-length map
            while pos < len(data) and data[pos] != 0xFF:
                pos = cbor_skip_value(data, pos)
                pos = cbor_skip_value(data, pos)
            return pos + 1 if pos < len(data) else pos
        if ai < 24:
            n = ai
        elif ai == 24:
            n = data[pos]
            pos += 1
        elif ai == 25:
            n = int.from_bytes(data[pos : pos + 2], "big")
            pos += 2
        elif ai == 26:
            n = int.from_bytes(data[pos : pos + 4], "big")
            pos += 4
        else:
            n = int.from_bytes(data[pos : pos + 8], "big")
            pos += 8
        for _ in range(n):
            pos = cbor_skip_value(data, pos)  # key
            pos = cbor_skip_value(data, pos)  # value
        return pos
    if mt == 6:  # tag
        if ai < 24:
            return cbor_skip_value(data, pos)
        if ai == 24:
            return cbor_skip_value(data, pos + 1)
        if ai == 25:
            return cbor_skip_value(data, pos + 2)
        if ai == 26:
            return cbor_skip_value(data, pos + 4)
        return cbor_skip_value(data, pos + 8)
    if mt == 7:  # simple/float
        if ai < 24:
            return pos
        if ai == 24:
            return pos + 1
        if ai == 25:
            return pos + 2
        if ai == 26:
            return pos + 4
        if ai == 27:
            return pos + 8
        return pos
    return pos


def cbor_value_slice(data: bytes, pos: int) -> tuple[int, int]:
    """Return (start, end) byte range for the CBOR value at pos (inclusive start, exclusive end)."""
    start = pos
    end = cbor_skip_value(data, pos)
    return (start, end)


def cbor_read_unsigned(data: bytes, pos: int) -> tuple[int, int]:
    """Read CBOR unsigned int at pos; return (value, new_pos)."""
    b = data[pos]
    if (b >> 5) != 0:
        return (0, pos)
    ai = b & 0x1F
    pos += 1
    if ai < 24:
        return (ai, pos)
    if ai == 24:
        return (data[pos], pos + 1)
    if ai == 25:
        return (int.from_bytes(data[pos : pos + 2], "big"), pos + 2)
    if ai == 26:
        return (int.from_bytes(data[pos : pos + 4], "big"), pos + 4)
    if ai == 27:
        return (int.from_bytes(data[pos : pos + 8], "big"), pos + 8)
    return (0, pos)


def extract_txdats_bytes(data: bytes) -> bytes | None:
    """
    Parse Alonzo tx = [body, witness_set, bool, aux], find key 4 in witness_set,
    return the raw bytes of its value (TxDats), or None if key 4 absent.
    """
    pos = 0
    if pos >= len(data):
        return None
    b = data[pos]
    if (b >> 5) != 4:  # not array
        return None
    ai = b & 0x1F
    pos += 1
    if ai == 31:  # indefinite array
        # skip first element (body)
        pos = cbor_skip_value(data, pos)
        # second element = witness set (map)
        if pos >= len(data) or (data[pos] >> 5) != 5:
            return None
        return _map_find_value(data, pos, 4)
    # definite array
    if ai < 24:
        n = ai
    elif ai == 24:
        n = data[pos]
        pos += 1
    elif ai == 25:
        n = int.from_bytes(data[pos : pos + 2], "big")
        pos += 2
    else:
        n = int.from_bytes(data[pos : pos + 4], "big")
        pos += 4
    if n < 2:
        return None
    # skip first element (body)
    pos = cbor_skip_value(data, pos)
    # second element = witness set
    if pos >= len(data) or (data[pos] >> 5) != 5:
        return None
    return _map_find_value(data, pos, 4)


def _map_find_value(data: bytes, map_pos: int, want_key: int) -> bytes | None:
    """Given position at start of a CBOR map, return raw bytes of value for want_key, or None."""
    pos = map_pos
    b = data[pos]
    if (b >> 5) != 5:
        return None
    ai = b & 0x1F
    pos += 1
    if ai == 31:  # indefinite map
        while pos < len(data) and data[pos] != 0xFF:
            key_start = pos
            pos = cbor_skip_value(data, pos)
            key_end = pos
            # key is usually small int
            k, _ = cbor_read_unsigned(data, key_start)
            if k == want_key:
                start, end = cbor_value_slice(data, pos)
                return data[start:end]
            pos = cbor_skip_value(data, pos)  # skip value
        return None
    if ai < 24:
        n = ai
    elif ai == 24:
        n = data[pos]
        pos += 1
    elif ai == 25:
        n = int.from_bytes(data[pos : pos + 2], "big")
        pos += 2
    else:
        n = int.from_bytes(data[pos : pos + 4], "big")
        pos += 4
    for _ in range(n):
        key_start = pos
        pos = cbor_skip_value(data, pos)
        k, _ = cbor_read_unsigned(data, key_start)
        if k == want_key:
            start, end = cbor_value_slice(data, pos)
            return data[start:end]
        pos = cbor_skip_value(data, pos)
    return None


def main() -> int:
    raw = sys.stdin.read().strip().replace(" ", "").replace("\n", "")
    if not raw:
        sys.stderr.write("No hex input\n")
        return 1
    try:
        data = bytes.fromhex(raw)
    except ValueError:
        sys.stderr.write("Invalid hex\n")
        return 1
    out = extract_txdats_bytes(data)
    if out is None:
        return 1
    print(out.hex())
    return 0


if __name__ == "__main__":
    sys.exit(main())
