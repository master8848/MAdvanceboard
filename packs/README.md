# Showcase packs (`packs/`)

Wordlist sources for the 8 MVP showcase packs (SPEC section 3, milestone M3).
Format per file: `{id, title, version, words[]}` where each word is
`{w, freq?, cat?, lang?, seq?}`. Load with `pack::load_pack(path)` in
`core-rust`, then `stack.add_pack(&pack, priority)`. `seq` is an explicit
digit-sequence override for display words that are not encodable Latin text
(emoji, LaTeX); all other words encode via the 9-key map
(`mapping::encode_word`).

## Pack table

| File | Pack `id` | Title | Words | Notes |
|---|---|---|---|---|
| `words_en.json` | `words` | English Base | 50 | Base EN vocabulary; default `cat`/`lang` = `words`/`en` |
| `nepali.json` | `ne` | Nepali Base (Romanized) | 30 | `cat: NE`, `lang: ne`; `tr` romanization is first-class (loader prefers `encode(tr)`, plan/03) |
| `numbers.json` | `numbers` | Numbers | 10 | Digits `0-9`; digit-commit mode, no prediction |
| `code_js.json` | `js` | JavaScript Keywords | 30 | Keyword prefixes, e.g. `fun` |
| `code_rust.json` | `rust` | Rust Keywords | 32 | |
| `code_html.json` | `html` | HTML Tags | 33 | |
| `emoji.json` | `emoji` | Emoji | 20 | Keyword `seq` overrides, e.g. `hea` |
| `math.json` | `math` | Math/LaTeX | 20 | LaTeX-name `seq` overrides, e.g. `alp` |

Total: 225 words. All 8 files load via `pack::load_pack` and every word
encodes to a non-empty `seq` (verified: entries == words for each pack).

## Expansion (2026-09-12, v1.1.0 + new `medical` pack)

Built with `scripts/build_pack.py` (see `packs/ADDING.md`); sources in
`packs/sources/`; recipe in `scripts/seed_expansion.py`. English tail is
50 curated + ~1600 common/tech words + `/usr/share/dict/web2` filtered to
`[a-z]{3,12}` (offline, no downloads). New-word freqs are Zipf tails per
pack (`--f0/--alpha/--fmin`); existing freqs untouched. Canonical IDs gain
**`medical`** (now 9 total). Verified two ways: `build_pack.py validate
--all` (python T9 port, parity-checked vs `mapping.rs` vectors
`hello→43556`, `कमल→267`) **and** the real Rust loader
(`pack::load_pack` + `to_entries`, entries == words for every pack).

| File | Pack `id` | Words | `freq` range | Notes |
|---|---|---|---|---|
| `words_en.json` | `words` | 5150 | 120..1000000 | EN core + common/tech + web2 tail |
| `nepali.json` | `ne` | 8000 (v2.0.0) | 200..9000 | pipeline `w+tr` (HF 2.4M + wiki freq + 253 curated head); see Nepali rebuild below |
| `numbers.json` | `numbers` | 10 | 50000 | unchanged (builtin digit-commit) |
| `code_js.json` | `js` | 183 | 500..9500 | +153 keywords, DOM/Node/builtins |
| `code_rust.json` | `rust` | 150 | 500..9500 | +118 keywords, std types/macros/attrs |
| `code_html.json` | `html` | 142 | 500..9500 | +109 tags + attributes |
| `emoji.json` | `emoji` | 111 | 800..9000 | +91, `seq` materialized from `key` |
| `math.json` | `math` | 69 | 800..6000 | +49, short key-derived `seq` (e.g. `\to→86`) |
| `medical.json` | `medical` | 545 | 150..6000 | NEW: symptoms, diseases, anatomy, drugs, procedures |

Total: **6613 words, ~385 KiB** (was 225). Lean by design: Zipf freqs,
single tokens, no bloat.

## Nepali rebuild (plan/03, v2.0.0, 8000 words)

`scripts/build_ne_pack.py` replaces the 253-word toy with a downloaded
`w+tr` pack (never hand-made):

- Pairs: HF `Saugatkafley/Nepali-Roman-Transliteration` train parquet
  (MIT, 2,397,414 rows → 2,392,263 unique natives, zero drops: all
  single-token Devanagari + Roman). Top roman per word = `tr`, next two
  distinct romans = `alt` (304 rows carry variants).
- Freq: Leipzig `nep_news_2019` specified but UNAVAILABLE to automation
  (wortschatz.uni-leipzig.de Anubis bot-wall, no working direct tarball
  URL, no desktop browser) — substituted with Nepali-Wikipedia
  pages-articles token counts (488,569 unique tokens / 11.2M occurrences;
  log-scaled to `freq`, floor 200). The `w+tr` pairs are unaffected.
- Head: the 253 curated headwords pin the ranking with their freqs
  (curated `tr` wins conflicts; HF top roman becomes `alt`).
- Result: 253 curated + 7747 tail = **8000 rows**, `freq` 200..9000,
  every row has `tr`, zero all-`9` (matra-only) seqs in the index.
  Source CSV regenerated at `packs/sources/nepali.csv` (pipeline-owned;
  `seed_expansion.py` skips Nepali since).

Gate (`core-rust/tests/gate.rs`, frozen seed, Roman `encode(tr)` digits):

| Corpus | NE top-3@4 ON | NE top-3@4 OFF | Scope top-3@4 ON | KSPW | Verdict |
|---|---|---|---|---|---|
| 253-word toy, Devanagari digits (old harness) | 0.75 | — | — | — | below bar, untypeable seqs |
| 253-word toy, Roman `tr` digits | 0.94 | 0.98 | 0.92 | 0.686 | PASS (headwords work) |
| 8000-word v2.0.0, Roman `tr` digits | 0.26 | 0.39 | 0.34 | 0.852 | **NO-GO on scope** |

Diagnosis: 4-prefix buckets average **5.12** words (max 109) vs EN
2.42 (max 13) — within-tab crowding, not cross-tab pollution (NE-only
stack: 0.218 ON / 0.356 OFF unweighted). Neighbor-ON scores *below*
neighbor-OFF at this density (extra 1-edit candidates outrank the tail).
Cheapest-first per `plan/00-gates.md`: neighbor retune (alt 1), category
boost 0.3→0.6 / hard tab filter (alt 2), per-category 16-key last
(alt 5). Engine behavior unchanged by this task — numbers reported,
alternatives left to the gate owner.

## Sequence examples

Key map: `2:abc 3:def 4:ghi 5:jkl 6:mno 7:pqrs 8:tuv 9:wxyz`.

| Input | `seq` | Pack | Match |
|---|---|---|---|
| `hello` | `43556` | words | exact dictionary word |
| `fun` | `386` | js | keyword prefix (`fun` -> `function…`) |
| `hea` (`❤️`) | `432` | emoji | explicit `seq` override on `{"w":"❤️","seq":"432","key":"hea"}` |
| `alp` (`\alpha`) | `257` | math | explicit `seq` override on `{"w":"\\alpha","seq":"257","key":"alp"}` |

## Canonical IDs

Canonical pack/category IDs (aligned 2026-09-12 across `packs/*.json`,
`android/ime/.../assets/categories/*.json`, and the Kotlin tab/mapping
defaults):

`words, ne, js, rust, html, emoji, numbers, math`

Reconciled renames (done, not pending):

- **English base:** `packs/words_en.json` `"id"` was `"en"` → now **`"words"`**
  (matches `android/.../assets/categories/words.json`, display `EN`).
  `pack.rs` defaults missing `cat` to the pack id, so base rows now carry
  `cat "words"`.
- **Nepali:** `android/.../assets/categories/nepali.json` (`"id": "nepali"`)
  → renamed file `ne.json` with **`"id": "ne"`** (matches
  `packs/nepali.json` `"id": "ne"`), keeping `displayName: NE`.
  `KbInputMethodService.onCategoryChanged` still accepts legacy labels
  `EN`/`NE` and the old `nepali` id (compat mapping).
- The remaining six ids (`js`, `rust`, `html`, `emoji`, `numbers`, `math`)
  already agreed — untouched.

Display vs id note: tab display labels stay `EN`/`NE`/`123` (SPEC §0);
engine/manifest ids are lowercase (`words`/`ne`/`numbers`). Pack row `cat`
values in `nepali.json` still use the display form `"NE"`; normalize to
`"ne"` only together with the `stack.rs`/`predictor.rs` `"EN"`-default
cleanup (deferred — `cat_boost` compares exact strings).

## SPEC deltas (code vs SPEC, informational)

- **Personal dict cap:** code (`personal::PERSONAL_CAP`) is **10k** entries
  (LFU-evicted, block tombstones exempt); SPEC section 4 says **20k** LRU.
  Behavior otherwise matches (tombstones survive, exempt from eviction).
  Decision needed: bump code to 20k or amend SPEC to 10k.
- **Hide threshold:** code `RankWeights::hide_threshold` is **-0.5**, matching
  SPEC section 2 (`S < -0.5` hidden unless expand-all). No action.
