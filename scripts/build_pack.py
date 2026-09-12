#!/usr/bin/env python3
"""Build / expand / validate kb9 showcase packs (packs/*.json).

MVP pack format (what core-rust/pack.rs loads):
  {"id": ..., "title": ..., "version": ..., "words": [{"w":..., "freq":...,
   "cat":..., "lang":..., "seq":..., "key":..., "tr":...}]}
Defaults in the Rust loader: freq->100, cat->pack id, lang->"en".
`seq` is an explicit digit override for non-encodable display words
(emoji, LaTeX); otherwise the T9 encoder derives it. `key` (emoji/math
keyword) is loader-opaque; `tr` (Nepali romanization) is first-class:
the loader prefers `encode(tr)` over `encode(w)` (plan/03).

Subcommands:
  build        txt/csv/wordlist -> pack JSON (freq assignment + T9 seq + validate)
  add-word     insert/update a single word in a pack
  expand-pack  merge a txt/csv wordlist into a pack (dedupe, resort)
  validate     loader-compat check of one pack file
  stats        counts + size summary of one pack file (or all packs)

Examples:
  python3 scripts/build_pack.py build --id words --title "English Base" \\
      --in packs/sources/words_en.txt --out packs/words_en.json --lang en --cat words
  python3 scripts/build_pack.py add-word --pack packs/words_en.json --word serendipity
  python3 scripts/build_pack.py expand-pack --pack packs/medical.json --in packs/sources/medical.txt
  python3 scripts/build_pack.py validate --pack packs/words_en.json
  python3 scripts/build_pack.py stats --all
"""

import argparse
import csv
import json
import math
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PACKS = ROOT / "packs"
LAYOUTS_DIR = ROOT / "layouts"

# Canonical pack ids (packs/README.md ID map). `medical` is the new addition.
CANONICAL_IDS = {"words", "ne", "js", "rust", "html", "emoji", "numbers", "math", "medical"}

# Minimum production sizes per pack id. `validate` fails loudly below
# these and names the exact missing source file — a pack that cannot be
# completed from available sources must never ship near-empty silently.
MIN_WORDS = {
    "words": 5000, "ne": 5000,
    "js": 200, "rust": 200, "html": 200,
    "emoji": 150, "math": 100,
    "medical": 500, "numbers": 10,
}
MIN_SOURCE = {
    "words": "packs/sources/words_en.txt",
    "ne": "packs/sources/nepali.csv (pipeline-owned: scripts/build_ne_pack.py)",
    "js": "packs/sources/code_js.txt",
    "rust": "packs/sources/code_rust.txt",
    "html": "packs/sources/code_html.txt",
    "emoji": "packs/sources/emoji.csv",
    "math": "packs/sources/math.csv",
    "medical": "packs/sources/medical.txt",
    "numbers": "builtin digits (no source file)",
}

# --- T9 encoder (loaded from layouts/*.json, single source of truth) --------

DEFAULT_LAYOUT = "t9-9"


def load_layout_map(layout_id: str = DEFAULT_LAYOUT) -> dict:
    """Char -> code map from layouts/<id>.json (mirrors LayoutSpec::assemble).

    ASCII digits pass through verbatim (no key may claim them); otherwise the
    first key (in file order) whose `symbols` contain the char wins.
    """
    doc = json.loads((LAYOUTS_DIR / f"{layout_id}.json").read_text(encoding="utf-8"))
    m = {}
    for key in doc["keys"]:
        for ch in key.get("symbols", ""):
            m.setdefault(ch, key["code"])
    for d in "0123456789":
        m[d] = d
    return m


_T9_MAP = load_layout_map()


def set_layout(layout_id: str):
    """Switch the active encoder map (CLI --layout)."""
    global _T9_MAP
    _T9_MAP = load_layout_map(layout_id)


def encode_word(word: str) -> str:
    """Port of LayoutSpec::encode_word (t9-9 default): latin case-insensitive,
    digits passthrough, unmapped skipped."""
    out = []
    for ch in word:
        d = _T9_MAP.get(ch, _T9_MAP.get(ch.lower()))
        if d is not None:
            out.append(d)
    return "".join(out)


def seq_for(entry: dict) -> str:
    """Effective seq: explicit override wins, else `encode(tr)` (Nepali
    Roman transliteration, plan/03), else keyword `key`, else `encode(w)`."""
    if entry.get("seq"):
        return entry["seq"]
    if entry.get("tr"):
        return encode_word(entry["tr"])
    if entry.get("key"):
        return encode_word(entry["key"])
    return encode_word(entry.get("w", ""))


def ensure_seq(entry: dict) -> dict:
    """Materialize an explicit `seq` when the display word is not encodable
    (emoji, LaTeX) but a latin `key` is present. The Rust loader only honors
    `seq` overrides, so the built JSON must carry it literally.
    `tr`-derived seqs are deliberately NOT materialized: the JSON keeps
    `tr` so the loader (and per-layout re-encoding) derives them."""
    entry = dict(entry)
    if not entry.get("seq") and not entry.get("tr") \
            and not encode_word(entry.get("w", "")) and entry.get("key"):
        entry["seq"] = encode_word(entry["key"])
    return entry


# --- freq assignment ----------------------------------------------------------

def zipf_freq(rank0: int, f0: float = 60000.0, alpha: float = 0.9,
               fmin: int = 100, fmax: int = 1000000) -> int:
    """Zipf-ish freq for 0-based rank: f0 / (rank+1)^alpha, clamped."""
    return max(fmin, min(fmax, int(f0 / ((rank0 + 1) ** alpha))))


# --- input parsing ------------------------------------------------------------

def parse_wordlist(path: Path):
    """Parse txt (one `word [freq]` per line, `#` comments) or csv (w,freq,...).

    Returns list of dicts {w, freq?, cat?, lang?, seq?, key?, tr?, alt?}.
    The `alt` column holds `;`-separated Roman spelling variants.
    """
    rows = []
    text = path.read_text(encoding="utf-8")
    if path.suffix.lower() == ".csv":
        reader = csv.DictReader(text.splitlines())
        if reader.fieldnames and "w" not in reader.fieldnames:
            raise ValueError(f"{path}: csv needs a 'w' header, got {reader.fieldnames}")
        for r in reader:
            w = (r.get("w") or "").strip()
            if not w or w.startswith("#"):
                continue
            e = {"w": w}
            for k in ("freq", "cat", "lang", "seq", "key", "tr", "alt"):
                v = (r.get(k) or "").strip()
                if not v:
                    continue
                if k == "freq":
                    e[k] = int(v)
                elif k == "alt":
                    alts = [x.strip() for x in v.split(";") if x.strip()]
                    if alts:
                        e[k] = alts
                else:
                    e[k] = v
            rows.append(e)
    else:
        for line in text.splitlines():
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = line.split()
            e = {"w": parts[0]}
            if len(parts) > 1:
                try:
                    e["freq"] = int(parts[1])
                except ValueError:
                    pass
            rows.append(e)
    return rows


# --- pack IO ------------------------------------------------------------------

def load_pack(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def write_pack(path: Path, pack: dict):
    path.write_text(json.dumps(pack, ensure_ascii=False) + "\n", encoding="utf-8")


def normalize_entries(entries, lang: str, cat: str, f0=60000.0, alpha=0.9,
                       fmin=100, keep_freq=True):
    """Fill defaults, assign Zipf freqs to rows missing one, drop unencodable.

    Rows are assumed pre-sorted most-important-first; Zipf rank follows order.
    Returns (entries, dropped).
    """
    out, dropped = [], []
    rank = 0
    for e in entries:
        e = ensure_seq(dict(e))
        e.setdefault("lang", lang)
        e.setdefault("cat", cat)
        if "freq" not in e or not keep_freq or e["freq"] is None:
            e["freq"] = zipf_freq(rank, f0, alpha, fmin)
        if not seq_for(e):
            dropped.append(e["w"])
            continue
        out.append(e)
        rank += 1
    out.sort(key=lambda e: (-e["freq"], e["w"]))
    return out, dropped


def validate_pack(pack: dict, path_name="pack") -> list:
    """Loader-compat checks. Returns list of error strings (empty = ok)."""
    errs = []
    for k in ("id", "title", "version", "words"):
        if k not in pack:
            errs.append(f"{path_name}: missing key '{k}'")
    if errs:
        return errs
    if pack["id"] not in CANONICAL_IDS:
        errs.append(f"{path_name}: non-canonical id '{pack['id']}'")
    want = MIN_WORDS.get(pack["id"])
    have = len(pack["words"])
    if want is not None and have < want:
        errs.append(
            f"{path_name}: pack '{pack['id']}' has {have} words "
            f"(< production minimum {want}): missing source "
            f"{MIN_SOURCE.get(pack['id'], '?')} — refusing near-empty pack")
    seen = set()
    for i, e in enumerate(pack["words"]):
        loc = f"{path_name} word[{i}]"
        w = e.get("w")
        if not w:
            errs.append(f"{loc}: missing 'w'")
            continue
        key = (w, e.get("lang", "en"))
        if key in seen:
            errs.append(f"{loc}: duplicate {key}")
        seen.add(key)
        if not seq_for(e):
            errs.append(f"{loc}: {w!r} encodes to empty seq")
        # plan/03: `tr` is the primary seq source for Nepali rows; a `tr`
        # that encodes empty (with no explicit `seq`) is a loud error, as
        # is any empty `alt` variant.
        if e.get("tr") and not e.get("seq") and not encode_word(e["tr"]):
            errs.append(f"{loc}: {w!r} tr {e['tr']!r} encodes to empty seq")
        alt = e.get("alt", [])
        alts = alt.split(";") if isinstance(alt, str) else alt
        if isinstance(alts, list):
            for a in alts:
                if not encode_word(a):
                    errs.append(f"{loc}: {w!r} alt variant {a!r} encodes to empty seq")
        elif alts:
            errs.append(f"{loc}: {w!r} alt {alt!r} is not a list")
        f = e.get("freq", 100)
        if not isinstance(f, int) or not (1 <= f <= 1000000):
            errs.append(f"{loc}: freq {f!r} out of range 1..1e6")
        if e.get("seq") and not re.fullmatch(r"[0-9]+", e["seq"]):
            errs.append(f"{loc}: seq {e['seq']!r} not digits")
    return errs


def stats_of(pack: dict) -> dict:
    n = len(pack["words"])
    freqs = [e.get("freq", 100) for e in pack["words"]]
    explicit = sum(1 for e in pack["words"] if e.get("seq"))
    empties = sum(1 for e in pack["words"] if not seq_for(e))
    with_tr = sum(1 for e in pack["words"] if e.get("tr"))
    with_alt = sum(1 for e in pack["words"] if e.get("alt"))
    return {
        "id": pack.get("id"), "n": n,
        "freq_min": min(freqs) if freqs else 0,
        "freq_max": max(freqs) if freqs else 0,
        "explicit_seq": explicit, "empty_seq": empties,
        "with_tr": with_tr, "with_alt": with_alt,
    }


# --- subcommands --------------------------------------------------------------

def cmd_build(a):
    set_layout(a.layout)
    rows = parse_wordlist(Path(a.in_file))
    entries, dropped = normalize_entries(rows, a.lang, a.cat or a.id,
                                         f0=a.f0, alpha=a.alpha, fmin=a.fmin)
    pack = {"id": a.id, "title": a.title, "version": a.version, "words": entries}
    errs = validate_pack(pack, a.out or a.id)
    if errs and not a.force:
        print("validation errors (use --force to write anyway):", file=sys.stderr)
        for e in errs:
            print("  " + e, file=sys.stderr)
        sys.exit(1)
    out = Path(a.out) if a.out else PACKS / f"{a.id}.json"
    write_pack(out, pack)
    s = stats_of(pack)
    print(f"wrote {out}: n={s['n']} freq=[{s['freq_min']}..{s['freq_max']}] "
          f"explicit_seq={s['explicit_seq']} dropped={len(dropped)}")
    for w in dropped[:10]:
        print(f"  dropped (empty seq): {w}")


def cmd_add_word(a):
    set_layout(a.layout)
    path = Path(a.pack)
    pack = load_pack(path)
    e = {"w": a.word}
    if a.freq is not None:
        e["freq"] = a.freq
    else:
        e["freq"] = zipf_freq(len(pack["words"]))
    for k, v in (("cat", a.cat), ("lang", a.lang), ("seq", a.seq), ("key", a.key)):
        if v is not None:
            e[k] = v
    e.setdefault("cat", pack["id"] if pack["id"] != "ne" else "NE")
    e.setdefault("lang", "en")
    e = ensure_seq(e)
    if not seq_for(e):
        print(f"refusing: {a.word!r} encodes to empty seq", file=sys.stderr)
        sys.exit(1)
    for i, old in enumerate(pack["words"]):
        if old["w"] == a.word and old.get("lang", "en") == e["lang"]:
            pack["words"][i] = {**old, **e}
            print(f"updated {a.word!r} in {path}")
            break
    else:
        pack["words"].append(e)
        print(f"added {a.word!r} to {path}")
    pack["words"].sort(key=lambda x: (-x.get("freq", 100), x["w"]))
    errs = validate_pack(pack, str(path))
    if errs:
        for er in errs:
            print("  " + er, file=sys.stderr)
        sys.exit(1)
    write_pack(path, pack)
    print(f"now {len(pack['words'])} words")


def cmd_expand_pack(a):
    set_layout(a.layout)
    path = Path(a.pack)
    pack = load_pack(path)
    rows = parse_wordlist(Path(a.in_file))
    have = {(e["w"], e.get("lang", "en")) for e in pack["words"]}
    base_cat = pack["words"][0].get("cat", pack["id"]) if pack["words"] else pack["id"]
    base_lang = pack["words"][0].get("lang", "en") if pack["words"] else "en"
    added, skipped = 0, 0
    # Zipf tail continues after existing entries.
    for r in rows:
        key = (r["w"], r.get("lang", base_lang))
        if key in have:
            skipped += 1
            continue
        r = ensure_seq(r)
        r.setdefault("cat", a.cat or base_cat)
        r.setdefault("lang", a.lang or base_lang)
        if "freq" not in r:
            r["freq"] = zipf_freq(len(pack["words"]) + added,
                                  f0=a.f0, alpha=a.alpha, fmin=a.fmin)
        if a.key_from and not r.get("seq") and not r.get("key"):
            pass  # encodable words need no key
        if not seq_for(r):
            skipped += 1
            continue
        pack["words"].append(r)
        have.add(key)
        added += 1
    pack["words"].sort(key=lambda x: (-x.get("freq", 100), x["w"]))
    errs = validate_pack(pack, str(path))
    if errs and not a.force:
        for er in errs:
            print("  " + er, file=sys.stderr)
        sys.exit(1)
    write_pack(path, pack)
    print(f"{path}: +{added} new, {skipped} skipped/dup, total {len(pack['words'])}")


def cmd_validate(a):
    set_layout(a.layout)
    targets = sorted(PACKS.glob("*.json")) if a.all else [Path(a.pack)]
    # packs/README.md is the only non-pack file; glob only hits *.json so fine.
    rc = 0
    for t in targets:
        if t.name == "kbpack.json":
            continue
        try:
            pack = load_pack(t)
        except Exception as ex:
            print(f"{t}: UNREADABLE {ex}")
            rc = 1
            continue
        errs = validate_pack(pack, t.name)
        # loader-compat spot check: every word must yield a non-empty seq
        empties = [e["w"] for e in pack.get("words", []) if not seq_for(e)]
        if errs or empties:
            rc = 1
            print(f"{t}: FAIL ({len(errs)} errors, {len(empties)} empty-seq)")
            for er in errs[:20]:
                print("  " + er)
        else:
            print(f"{t}: OK id={pack['id']} n={len(pack['words'])}")
    sys.exit(rc)


def cmd_stats(a):
    targets = sorted(PACKS.glob("*.json")) if a.all else [Path(a.pack)]
    total = 0
    print(f"{'file':22} {'id':9} {'n':>6} {'freq range':>18} {'xseq':>5}")
    for t in targets:
        if t.name == "kbpack.json":
            continue
        pack = load_pack(t)
        s = stats_of(pack)
        total += s["n"]
        print(f"{t.name:22} {s['id']:9} {s['n']:>6} "
              f"[{s['freq_min']}..{s['freq_max']}] {s['explicit_seq']:>5}")
    if a.all:
        kb = sum(t.stat().st_size for t in targets if t.name != "kbpack.json") / 1024
        print(f"TOTAL words={total} size={kb:.0f} KiB")


def main():
    p = argparse.ArgumentParser(description="kb9 pack builder")
    sub = p.add_subparsers(dest="cmd", required=True)

    b = sub.add_parser("build", help="txt/csv -> pack JSON")
    b.add_argument("--id", required=True)
    b.add_argument("--title", required=True)
    b.add_argument("--in", dest="in_file", required=True)
    b.add_argument("--out", default=None)
    b.add_argument("--version", default="1.0.0")
    b.add_argument("--lang", default="en")
    b.add_argument("--cat", default=None)
    b.add_argument("--layout", default=DEFAULT_LAYOUT,
                   help="layouts/<id>.json encoder (default t9-9)")
    b.add_argument("--f0", type=float, default=60000.0)
    b.add_argument("--alpha", type=float, default=0.9)
    b.add_argument("--fmin", type=int, default=100)
    b.add_argument("--force", action="store_true")
    b.set_defaults(f=cmd_build)

    aw = sub.add_parser("add-word", help="insert/update one word")
    aw.add_argument("--pack", required=True)
    aw.add_argument("--word", required=True)
    aw.add_argument("--freq", type=int, default=None)
    aw.add_argument("--cat", default=None)
    aw.add_argument("--lang", default=None)
    aw.add_argument("--seq", default=None)
    aw.add_argument("--key", default=None)
    aw.add_argument("--layout", default=DEFAULT_LAYOUT)
    aw.set_defaults(f=cmd_add_word)

    ex = sub.add_parser("expand-pack", help="merge wordlist into pack")
    ex.add_argument("--pack", required=True)
    ex.add_argument("--in", dest="in_file", required=True)
    ex.add_argument("--cat", default=None)
    ex.add_argument("--lang", default=None)
    ex.add_argument("--key-from", action="store_true",
                    help="(reserved) derive seq from key column")
    ex.add_argument("--layout", default=DEFAULT_LAYOUT)
    ex.add_argument("--f0", type=float, default=60000.0)
    ex.add_argument("--alpha", type=float, default=0.9)
    ex.add_argument("--fmin", type=int, default=100)
    ex.add_argument("--force", action="store_true")
    ex.set_defaults(f=cmd_expand_pack)

    v = sub.add_parser("validate", help="loader-compat check")
    v.add_argument("--pack", default=None)
    v.add_argument("--all", action="store_true")
    v.add_argument("--layout", default=DEFAULT_LAYOUT)
    v.set_defaults(f=cmd_validate)

    st = sub.add_parser("stats", help="counts table")
    st.add_argument("--pack", default=None)
    st.add_argument("--all", action="store_true")
    st.set_defaults(f=cmd_stats)

    a = p.parse_args()
    if (a.cmd in ("validate", "stats")) and not a.all and not a.pack:
        p.error("validate/stats need --pack or --all")
    a.f(a)


if __name__ == "__main__":
    main()
