# Adding / expanding a dictionary pack (5 minutes)

Tool: `scripts/build_pack.py` (stdlib only). Pack format: `{id, title,
version, words[]}`, each word `{w, freq?, cat?, lang?, seq?, key?, tr?}`.
The Rust loader (`core-rust/src/pack.rs`) defaults `freq→100`,
`cat→pack id`, `lang→"en"` and ignores `key`/`tr`. `seq` is required only
when the display word itself is not T9-encodable (emoji, `\latex`).

Canonical ids: `words, ne, js, rust, html, emoji, numbers, math, medical`.
New packs: pick a fresh lowercase id and add it to `CANONICAL_IDS` in
`build_pack.py` + the ID map in `packs/README.md`.

## Add a brand-new dictionary

1. Make a wordlist — `txt` (one `word [freq]` per line, `#` comments) or
   `csv` with a `w` header plus optional `freq,cat,lang,seq,key,tr`:
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
`expand-pack` exactly as above. Re-run it to regenerate everything.

## Memory / leanness rules

- `freq` stays in `1..1e6` (Zipf, enforced by validation).
- Prefer single tokens; phrases with spaces encode `0` and still load, but
  keep them rare.
- Keep `packs/` total under ~1 MiB (now ~385 KiB / 6613 words). Large
  corpora (full medical ontologies, full npm/crates keyword dumps) are
  deferred — add them as separate extension packs, not by bloating base.
