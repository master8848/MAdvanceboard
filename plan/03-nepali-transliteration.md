# 03 — Nepali Transliteration (Download, Don't Hand-Make)

Breakage today: all vowels + matras `ा ि ी ु ू े ै ो ौ ं ः ँ ॉ ्` → key `9` (`core-rust/src/mapping.rs:49`). Matras can't stand alone, so every word gets spurious `9`s; Roman length ≠ Devanagari length (`namaste` 7 vs `नमस्ते` 6). `PackWord.tr` parsed by `scripts/build_pack.py` but ignored by `core-rust/src/pack.rs:42` (`seq=encode(w)`). Roman keystrokes and stored seqs live in disjoint spaces. Expectation `k gardai xau` → direct Devanagari candidates never works in this shape.

## Architecture: store `w` (Devanagari) + search `seq=encode(tr)` (Latin T9)

- `PackWord` gains `tr: Option<String>` first-class (not ignored); `to_entries`: `seq || encode(tr) || encode(w)`. Display stays Devanagari, matching is Roman T9.
- Multi-seq per word for variants (`paani/pani`, `shabda/sabda`, schwa-dropped `nepal` not `nepala`): extend `DictEntry` to `seqs: Vec<String>`, match `any`, or keep alias rows with distinct keys (today `suggest_inner` dedupes `(norm,lang)` keeping first `seq` — collapses aliases, must fix).
- Matras as non-emitting modifiers in any direct-Devanagari layout: remove `ा ि ी… ् ं ः ँ` from `layouts/t9-*.json` symbols (control/empty role) so `encode(Devanagari)` ≈ consonant skeleton. Fallback only; `tr`-seqs are primary.
- Reuse transliteration engine, don't invent: `realsanjeev/nepali_unicoder` (MIT, greedy longest-match `ka→क`/`k→क्`, matra reordering, `word_maps.json` overrides) or `shabdakosha` IAST→casual generator. Rules: consonant inherent `a`, `@` forces standalone matra, `^` forces virama, schwa-deletion + overrides (cf. pinyin `pyseg` lattice + Gboard pair-n-gram WFST `s:स a:ϵ n:ं` pattern).

## Wordlist pipeline (take dictionary, don't create)

1. `Saugatkafley/Nepali-Roman-Transliteration` (HF, MIT, 2.4M `native↔english` parquet, 475MB) → `w+tr` source. No freq.
2. Intersect with Leipzig `nep_news_2019` (115k sents/1.8M toks, frequency lists, research download — verify redistribution) for `freq` weights.
3. Quality headwords: `nepalibhasha/shabdakosha` (Brihat+Pragya, `roman_aliases` IAST→casual generator — check license).
4. Supplements: `SushilShrestha/NepaliTransliteralDataset`, `ritushishir/NepalAndNepali` (MIT place names). Avoid `scientiac/ne-roman-translit` GPL-2.0 code (rules ok, code not).
5. Pipeline: HF parquet → filter top-N by Leipzig freq, dedupe, NFC → CSV `w,freq,cat,lang,tr` (`w`=Devanagari, `tr`=roman) → `build_pack.py build --id ne --lang ne --cat NE` / `expand-pack`. One-line semantic change required: `seq_for/ensure_seq/normalize_entries/validate` prefer `encode(tr)` over `encode(w)` (currently `seq>key>w`, `tr` ignored).

## Acceptance

- 253-word toy → 5k+ `w+tr` pack. `k→2`-style Roman typing surfaces `काठमाडौं`-class words. No matra-only `seq` (e.g. bare `9`s) in index. Per-tab top-3@4 reported for NE separately in `00-gates.md`.
