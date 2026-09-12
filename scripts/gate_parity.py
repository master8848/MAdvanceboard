#!/usr/bin/env python3
"""T9 parity check for the Python port (scripts/build_pack.py encoder).

Asserts the frozen anchors `hello->43556`, `कमल->267` from
`core-rust/src/mapping.rs` through the data-driven `layouts/*.json` map.
Prints explicit PASS/FAIL per vector; exits non-zero on any mismatch or on
a missing layouts/corpus file (never a silent pass).

Usage: python3 scripts/gate_parity.py
"""

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))

try:
    from build_pack import encode_word
except Exception as ex:
    print(f"FAIL: cannot import Python T9 port (scripts/build_pack.py): {ex}")
    sys.exit(2)

VECTORS = [
    ("hello", "43556"),
    ("कमल", "267"),
]

failures = 0
for word, want in VECTORS:
    try:
        got = encode_word(word)
    except Exception as ex:
        print(f"FAIL: encode_word({word!r}) raised {ex}")
        failures += 1
        continue
    status = "PASS" if got == want else "FAIL"
    if got != want:
        failures += 1
    print(f"{status}: encode_word({word!r}) = {got!r} (want {want!r})")

if failures:
    print(f"FAIL: {failures}/{len(VECTORS)} parity vectors mismatched")
    sys.exit(1)
print(f"PASS: all {len(VECTORS)} parity vectors match")
