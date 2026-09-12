# Adding / expanding a dictionary pack (5 minutes)

Tool: `scripts/build_pack.py` (stdlib only). Pack format: `{id, title,
version, words[]}`, each word `{w, freq?, cat?, lang?, seq?, key?, tr?, alt?}`.
The Rust loader (`core-rust/src/pack.rs`) defaults `freq→100`,
`cat→pack id`, `lang→"en"` and ignores `key`. `seq` is required only
when the display word itself is not T9-encodable (emoji, `\latex`).
`tr` (Nepali romanization) is first-class: the loader prefers
`encode(tr)` over `encode(w)` (plan/03); `alt` holds extra Roman
spelling variants (`;`-separated in csv, list in JSON), each an
additional match-any seq.

Canonical ids: `words, ne, js, rust, html, emoji, numbers, math, medical`.
New packs: pick a fresh lowercase id and add it to `CANONICAL_IDS` in
`build_pack.py` + the ID map in `packs/README.md`.

## Add a brand-new dictionary

1. Make a wordlist — `txt` (one `word [freq]` per line, `#` comments) or
   `csv` with a `w` header plus optional `freq,cat,lang,seq,key,tr,alt`:
   ```text
   # packs/sources/mywords.txt
   hello 9000
   world 8000
   foobar
   ```
   ```csv
   w,key
   ❤️,hea
   ```
   Words without `freq` get a Zipf value (`f0 / rank^alpha`, see `--f0`,
   `--alpha`, `--fmin`); order the file most-important-first.
   For emoji/LaTeX give a latin `key` (or explicit `seq`); the builder
   materializes `seq` from `key` when `w` itself is unencodable.
2. Build it:
   ```sh
   python3 scripts/build_pack.py build --id myid --title "My Words" \
     --in packs/sources/mywords.txt --out packs/mine.json \
     --lang en --cat myid
   ```
3. Check: `python3 scripts/build_pack.py validate --pack packs/mine.json`.

## Expand an existing pack

```sh
# append new words (dedupe on (word, lang), resort by freq desc):
python3 scripts/build_pack.py expand-pack --pack packs/words_en.json \
  --in packs/sources/extra.txt

# fix / insert a single word:
python3 scripts/build_pack.py add-word --pack packs/medical.json \
  --word bronchoscopy --freq 1200
```

`expand-pack` keeps existing `freq`s and assigns the Zipf tail (`--f0`,
`--alpha`, `--fmin`) to newcomers. Re-run `validate --all` + `stats --all`
afterwards and paste the new row into the counts table in `packs/README.md`.

## How the current packs were built

`scripts/seed_expansion.py` is the reproducible recipe: it writes
`packs/sources/*` (curated lists; English tail filtered from the offline
system wordlist `/usr/share/dict/web2`) and then drives `build` /
`expand-pack` exactly as above. Re-run it to regenerate everything
**except Nepali** (see below).

**Nepali (`ne`, v2.0.0, 8000 words) is pipeline-owned, not seed-owned:**
built by `scripts/build_ne_pack.py` from the HF
`Saugatkafley/Nepali-Roman-Transliteration` train parquet (MIT, 2.4M
`native↔english` rows) + Nepali-Wikipedia token counts for `freq`
(Leipzig `nep_news_2019` was specified but is bot-walled; see the
script header) + the 253 curated headwords pinned from the previous
`nepali.json`. Do NOT re-run `seed_expansion.py`'s Nepali section
against it (it would rewrite `packs/sources/nepali.csv` back to the
223-word curated list); the script now skips Nepali itself.

## SDK docs (ship without reading source)

- Vocab packs (data-only, the default): `docs/VOCAB_SDK.md` — manifest
  fields, row format, tokenizer modes, worked 10-word example, install +
  SHA-256, priority, `privacy.learn`, failure modes F1–F7.
- Code plugins (privileged): `docs/PLUGIN_SDK.md` — `candidates()`
  contract, registry/manifest, sandbox rules, worked example, capability
  matrix + separation rationale.
- SPEC `.kbpack` envelope: `docs/EXTENSIONS.md` (target format) +
  `kbpack.schema.json`. Wrap an MVP pack JSON into `kbpack.json` +
  `dict.jsonl.gz` + zip exactly as `docs/VOCAB_SDK.md` §4 step 4 shows;
  core loads `packs/*.json` only (no zip loader yet — VOCAB_SDK F7).

## Memory / leanness rules

- `freq` stays in `1..1e6` (Zipf, enforced by validation).
- Prefer single tokens; phrases with spaces encode `0` and still load, but
  keep them rare.
- Keep `packs/` total under ~1 MiB (now ~385 KiB / 6613 words). Large
  corpora (full medical ontologies, full npm/crates keyword dumps) are
  deferred — add them as separate extension packs, not by bloating base.
