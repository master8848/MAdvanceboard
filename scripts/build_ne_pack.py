#!/usr/bin/env python3
"""Nepali w+tr pack pipeline (plan/03-nepali-transliteration.md).

Stages:
  1. HF `Saugatkafley/Nepali-Roman-Transliteration` train parquet
     (MIT, 2.4M native<->english rows) -> cleaned, NFC-normalized,
     single-token Devanagari pairs grouped by native word
     (top roman = tr, next distinct romans = alt variants).
  2. Frequency weights: Leipzig `nep_news_2019` was specified but is
     UNAVAILABLE to automation (Anubis bot-wall on wortschatz.uni-
     leipzig.de, no working direct tarball URL, no desktop browser in
     this session). Substituted with token counts from the Nepali
     Wikipedia pages-articles dump (CC BY-SA text; counts are facts).
     Pass --wiki-counts <tsv> (word<TAB>count) or --wiki-xml <file>
     to count inline. Rows absent from wiki get the floor frequency.
  3. Curated in-repo headwords (existing packs/nepali.json: 253 words
     with hand-set frequencies incl. the top-30) pin the head of the
     ranking; HF/wiki fill the tail. Curated `tr` always wins on
     conflict; the HF top roman becomes an `alt` variant.
  4. Emits packs/sources/nepali.csv (w,freq,cat,lang,tr,alt); the
     caller then runs `build_pack.py build` to regenerate
     packs/nepali.json (see --rebuild flag).

Any failure is explicit (non-zero exit + message). Nothing is sampled
or hand-invented: every row traces to the HF pair or the curated pack.

Examples:
  python3 scripts/build_ne_pack.py --parquet /tmp/ne-data/train.parquet \\
      --wiki-xml /tmp/ne-data/newiki.xml.bz2 --top-n 8000 --rebuild
  python3 scripts/build_ne_pack.py --parquet train.parquet \\
      --wiki-counts wiki_counts.tsv --top-n 8000 --rebuild
"""

import argparse
import bz2
import csv
import re
import sys
import unicodedata
import xml.etree.ElementTree as ET
from collections import Counter
from pathlib import Path
from typing import NoReturn

ROOT = Path(__file__).resolve().parent.parent
PACKS = ROOT / "packs"
CURATED_PACK = PACKS / "nepali.json"

DEVA_RE = re.compile(r"[\u0900-\u097f]")
TOKEN_RE = re.compile(r"[\u0900-\u097f]+")
ROMAN_RE = re.compile(r"[A-Za-z'’\- ]+")
FLOOR_FREQ = 120


def fail(msg: str) -> NoReturn:
    print(f"build_ne_pack: ERROR: {msg}", file=sys.stderr)
    sys.exit(1)


def load_hf(parquet: Path):
    """Return {native: Counter(roman -> rows)} plus drop statistics."""
    try:
        import pyarrow.parquet as pq
    except ImportError:
        fail("pyarrow is required to read the HF parquet (pip install pyarrow)")
    if not parquet.is_file():
        fail(f"HF parquet not found: {parquet} "
             f"(download https://huggingface.co/datasets/Saugatkafley/Nepali-Roman-Transliteration "
             f"file data/train-00000-of-00001.parquet)")
    stats = Counter()
    pairs: dict[str, Counter] = {}
    t = pq.read_table(str(parquet), columns=["native word", "english word"]).to_pandas()
    for nat, eng in zip(t["native word"], t["english word"]):
        stats["rows"] += 1
        w = unicodedata.normalize("NFC", str(nat)).strip()
        r = str(eng).strip()
        if not w or not r:
            stats["drop_empty"] += 1
            continue
        if re.search(r"\s", w) or re.search(r"\s", r):
            stats["drop_multitoken"] += 1
            continue
        if not DEVA_RE.search(w):
            stats["drop_non_devanagari"] += 1
            continue
        if not ROMAN_RE.fullmatch(r):
            stats["drop_non_roman"] += 1
            continue
        c = pairs.get(w)
        if c is None:
            pairs[w] = c = Counter()
        c[r] += 1
    return pairs, stats


def count_wiki_xml(path: Path) -> Counter:
    """Stream a pages-articles .xml(.bz2) dump, count Devanagari tokens."""
    if not path.is_file():
        fail(f"wiki dump not found: {path}")
    opener = bz2.open if path.suffix == ".bz2" else open
    counts: Counter[str] = Counter()
    with opener(path, "rt", encoding="utf-8", errors="ignore") as f:
        ctx = ET.iterparse(f, events=("end",))
        for _, el in ctx:
            if el.tag.endswith("}text") or el.tag == "text":
                for tok in TOKEN_RE.findall(el.text or ""):
                    counts[unicodedata.normalize("NFC", tok)] += 1
                el.clear()
    if not counts:
        fail(f"wiki dump yielded zero Devanagari tokens: {path}")
    return counts


def load_wiki_counts(path: Path) -> Counter:
    counts: Counter[str] = Counter()
    if not path.is_file():
        fail(f"wiki counts file not found: {path}")
    with open(path, encoding="utf-8") as f:
        for i, line in enumerate(f, 1):
            line = line.rstrip("\n")
            if not line:
                continue
            try:
                w, c = line.rsplit("\t", 1)
                counts[w] += int(c)
            except ValueError:
                fail(f"{path}:{i}: want 'word<TAB>count', got {line!r}")
    if not counts:
        fail(f"wiki counts file is empty: {path}")
    return counts


def load_curated() -> list[dict]:
    """Existing packs/nepali.json words (headword pinning)."""
    import json

    if not CURATED_PACK.is_file():
        fail(f"curated pack missing: {CURATED_PACK}")
    try:
        pack = json.loads(CURATED_PACK.read_text(encoding="utf-8"))
    except Exception as ex:
        fail(f"curated pack unreadable: {ex}")
    words = pack.get("words")
    if not words:
        fail("curated pack has zero words; refusing to build a tail-only pack")
    out = []
    for e in words:
        if not e.get("w") or not e.get("tr"):
            fail(f"curated row missing w/tr: {e!r}")
        out.append(e)
    return out


def main() -> None:
    a = argparse.ArgumentParser(description="Nepali w+tr pack pipeline")
    a.add_argument("--parquet", required=True)
    a.add_argument("--wiki-xml", default=None)
    a.add_argument("--wiki-counts", default=None)
    a.add_argument("--top-n", type=int, default=8000)
    a.add_argument("--out-csv", default=None)
    a.add_argument("--rebuild", action="store_true",
                   help="run build_pack.py build into packs/nepali.json")
    args = a.parse_args()

    if bool(args.wiki_xml) == bool(args.wiki_counts):
        fail("pass exactly one of --wiki-xml / --wiki-counts")

    print("ERROR: Leipzig nep_news_2019 frequency source unavailable "
          "(wortschatz.uni-leipzig.de is Anubis bot-walled; no working "
          "direct tarball URL; no desktop browser in this session). "
          "Substituting Nepali-Wikipedia token counts for freq weights; "
          "the w+tr pairs themselves are unaffected (HF 2.4M).",
          file=sys.stderr)

    pairs, stats = load_hf(Path(args.parquet))
    print(f"HF: {stats['rows']} rows -> {len(pairs)} unique natives "
          f"(drop: {dict(stats)})")

    counts = (count_wiki_xml(Path(args.wiki_xml)) if args.wiki_xml
              else load_wiki_counts(Path(args.wiki_counts)))
    print(f"wiki: {len(counts)} unique tokens, "
          f"{sum(counts.values())} total occurrences")

    curated = load_curated()
    have = {e["w"] for e in curated}
    print(f"curated headwords: {len(curated)}")

    # Tail: HF natives not already curated, ranked by wiki count desc
    # (absent = floor, stable order by (-count, word)).
    tail = []
    hit = 0
    for w, romans in pairs.items():
        if w in have:
            continue
        c = counts.get(w, 0)
        hit += int(c > 0)
        ordered = [r for r, _ in romans.most_common()]
        tail.append((w, c, ordered))
    print(f"tail candidates: {len(tail)} ({hit} with wiki counts)")
    tail.sort(key=lambda t: (-t[1], t[0]))
    tail = tail[: max(0, args.top_n - len(curated))]

    # Freq: wiki count scaled into pack range; floor for unseen.
    # Scale: top wiki count -> 4000 (below curated head 9000 max),
    # linear in log1p space so the long tail separates.
    import math

    cmax = max([c for _, c, _ in tail] + [1])
    rows = []
    for e in curated:
        rows.append({
            "w": e["w"], "freq": e["freq"],
            "cat": e.get("cat", "NE"), "lang": e.get("lang", "ne"),
            "tr": e["tr"],
            "alt": ";".join(
                [r for r, _ in pairs.get(e["w"], Counter()).most_common()
                 if r != e["tr"]][:2]),
        })
    for w, c, ordered in tail:
        f = FLOOR_FREQ if c <= 0 else max(
            FLOOR_FREQ,
            int(4000 * math.log1p(c) / math.log1p(cmax)))
        rows.append({
            "w": w, "freq": f, "cat": "NE", "lang": "ne",
            "tr": ordered[0],
            "alt": ";".join(ordered[1:3]),
        })
    # NFC sanity + drop rows whose tr cannot encode (loud, not silent).
    sys.path.insert(0, str(ROOT / "scripts"))
    from build_pack import encode_word, cmd_build
    dropped = [r for r in rows if not encode_word(r["tr"])]
    if dropped:
        fail(f"{len(dropped)} rows with unencodable tr, e.g. {dropped[:5]}")
    out_csv = Path(args.out_csv) if args.out_csv else PACKS / "sources" / "nepali.csv"
    with open(out_csv, "w", encoding="utf-8", newline="") as f:
        w = csv.DictWriter(f, fieldnames=["w", "freq", "cat", "lang", "tr", "alt"])
        w.writeheader()
        for r in rows:
            w.writerow(r)
    n_alt = sum(1 for r in rows if r["alt"])
    print(f"wrote {out_csv}: {len(rows)} rows "
          f"({len(curated)} curated + {len(tail)} HF/wiki tail), {n_alt} with alt")

    if args.rebuild:
        ns = argparse.Namespace(
            cmd="build", id="ne", title="Nepali Base (Romanized)",
            in_file=str(out_csv), out=str(PACKS / "nepali.json"),
            version="2.0.0", lang="ne", cat="NE", layout="t9-9",
            f0=60000.0, alpha=0.9, fmin=100, force=False)
        cmd_build(ns)


if __name__ == "__main__":
    main()
