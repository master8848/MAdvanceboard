# Vocab SDK — data-only packs (the default)

A vocab pack is **static data**: a word list plus a manifest. No code runs.
If your feature is "more words in a category", stop here — do not use the
Plugin SDK (`docs/PLUGIN_SDK.md`). Rationale is a security decision, see §7.

Two formats exist:

| Format | Status | Loads in |
|---|---|---|
| **MVP pack JSON** (`packs/*.json`) | current — what `core-rust/src/pack.rs` loads today | `pack::load_pack` + `stack.add_pack(&pack, prio)` |
| **SPEC `.kbpack`** (zip: `kbpack.json` + `dict.jsonl.gz`) | target open standard (SPEC §3) | manual wrap (no zip loader in core yet) |

Word rows are otherwise identical; the migration path is mechanical (§6).

## 1. Manifest fields (`kbpack.json`)

Schema: `kbpack.schema.json` (required: `id`, `version`, `kind`, `name`).

| Field | Required | Meaning |
|---|---|---|
| `id` | yes | reverse-dns or short id, e.g. `com.ex.ne-math` |
| `version` | yes | semver, e.g. `1.2.0` |
| `kind` | yes | `vocab` (MVP only ships vocab) |
| `name` | yes | display name, e.g. `Math/LaTeX` |
| `license` | no | SPDX, e.g. `MIT` |
| `paid` | no (default `false`) | **informational only** — never gates loading |
| `langs` | no | e.g. `["en","ne","latex"]` |
| `categories` | no | e.g. `["math"]` |
| `priority` | no | integer `10–90` (base `0`, personal `100`; user-reorderable) |
| `dict` | no | payload files, e.g. `["dict.jsonl.gz"]` |
| `tokenizer` | no | `default \| latex \| custom` (WASM `tokenize(word)->seq`) |
| `minCore` | no | e.g. `0.3.0` |

MVP JSON equivalent: `{id, title, version, words[]}` (`title` = SPEC `name`).

## 2. Row format (`dict.jsonl.gz`: one JSON object per line, gzipped)

Full row (SPEC §5 / `docs/SYNC.md` §1):

```json
{"w":"tabby","lang":"en","cat":"pets","seq":"82299","freq":9000,"acc":0,"rej":0,"ts":1726000000,"del":false,"id":"en:tabby"}
```

| Field | Meaning | Static pack value |
|---|---|---|
| `w` | surface form | the word |
| `lang` | language tag | e.g. `en` |
| `cat` | category tab id | e.g. `pets` |
| `seq` | T9 digit string | explicit override, else derived by encoder |
| `tr` | Roman transliteration (Nepali packs) | seq source: `seq` > `encode(tr)` > `encode(w)` |
| `key` | latin keyword (emoji/LaTeX) | loader-opaque; builder materializes `seq` from it |
| `alt` | extra spelling variants | additional match-any seqs |
| `freq` | Zipf base count `1..1e6` | required in practice (loader defaults `100`) |
| `acc`/`rej` | monotonic accept/reject counters | `0` in static packs (personal/sync layer only) |
| `ts` | HLC timestamp | `0`/omit in static packs |
| `del` | tombstone (blocked) | `false`/omit in static packs |
| `id` | `"<lang>:<normalized-word>"` dedupe key | derived |

MVP JSON rows carry the subset `{w, freq, cat, lang, seq, key, tr, alt}`.
`seq` is required only when the display word is not T9-encodable
(emoji, `\latex`); otherwise the encoder derives it.

## 3. Tokenizer modes

- `default`: Latin T9 + Devanagari collapse (SPEC §1 table).
- `latex`: strips leading `\`, encodes letters (`alp→257` = α `\alpha`).
- `custom`: ship WASM exposing `tokenize(word)->seq`, sandboxed.
  No arbitrary code runs on Android — WASM tokenizer only.

## 4. Worked example: 10 words in a new category

Goal: a `pets` category with 10 breed names. Every command below was run;
outputs are verbatim.

**Step 1 — wordlist** (`/tmp/vocabdemo/pets.txt`, most-important-first;
rows without `freq` get a Zipf value):

```text
tabby 9000
tortoiseshell 8000
calico 7000
persian 6000
siamese 5000
mainecoon 4000
bengal 3000
sphynx 2000
ragdoll 1500
britishshorthair 1200
```

**Step 2 — build:**

```sh
python3 scripts/build_pack.py build --id pets --title "Pet Breeds" \
  --in /tmp/vocabdemo/pets.txt --out /tmp/vocabdemo/pets.json \
  --lang en --cat pets --force
```

Expected output:

```text
wrote /tmp/vocabdemo/pets.json: n=10 freq=[1200..9000] explicit_seq=0 dropped=0
```

(`--force` is needed here: `pets` is not a canonical id — see failure
mode F1 below. To ship a pack in-tree instead, pick a fresh lowercase id
and register it in `CANONICAL_IDS` in `scripts/build_pack.py` plus the ID
map in `packs/README.md`, then `--force` is not required.)

**Step 3 — validate + stats:**

```sh
python3 scripts/build_pack.py validate --pack /tmp/vocabdemo/pets.json
python3 scripts/build_pack.py stats --pack /tmp/vocabdemo/pets.json
```

Expected output:

```text
pets.json              pets          10 [1200..9000]     0
```

(`validate` reports `FAIL (1 errors, 0 empty-seq)` plus
`pets.json: non-canonical id 'pets'` for this demo id — F1. A canonical
in-tree id prints `…: OK id=… n=…`.)

**Step 4 — wrap as SPEC `.kbpack`:**

```sh
python3 -c "
import json, gzip
pack = json.load(open('/tmp/vocabdemo/pets.json', encoding='utf-8'))
rows = [{'w':e['w'],'lang':e.get('lang','en'),'cat':e.get('cat','pets'),
         'freq':e.get('freq',100)} for e in pack['words']]
with gzip.open('/tmp/vocabdemo/dict.jsonl.gz','wt',encoding='utf-8') as f:
    for r in rows: f.write(json.dumps(r, ensure_ascii=False)+'\n')
print('rows written:', len(rows))
"
```

Expected output:

```text
rows written: 10
```

Write `kbpack.json` (all `kind: vocab` fields from §1), zip, hash:

```sh
(cd /tmp/vocabdemo && zip -q pets.kbpack kbpack.json dict.jsonl.gz \
  && unzip -l pets.kbpack)
sha256sum /tmp/vocabdemo/pets.kbpack
```

Expected output:

```text
Archive:  pets.kbpack
  Length      Date    Time    Name
---------  ---------- -----   ----
      242  09-12-2026 09:22   kbpack.json
      193  09-12-2026 09:22   dict.jsonl.gz
---------                     -------
      435                     2 files
53c971fc4bd55f95f23ef200b38c707c998c3e8ab15f00d92c385d820a333502  /tmp/vocabdemo/pets.kbpack
```

**Step 5 — schema + repo validation:**

```sh
python3 -c "
import json, re
m = json.load(open('/tmp/vocabdemo/kbpack.json'))
schema = json.load(open('kbpack.schema.json'))
missing = [k for k in schema['required'] if k not in m]
assert not missing, missing
assert 10 <= m['priority'] <= 90
print('manifest OK:', m['id'], m['version'], m['kind'])
"
just validate-packs
```

Expected output:

```text
manifest OK: com.ex.pets 1.0.0 vocab
-> validating packs/*.json + kbpack.schema.json
ok: packs/code_html.json, packs/code_js.json, packs/code_rust.json, packs/emoji.json, packs/math.json, packs/medical.json, packs/nepali.json, packs/numbers.json, packs/words_en.json, kbpack.schema.json
VALID 10 files
```

Note: `just validate-packs` JSON-parses every `packs/*.json` plus the
schema file. It does **not** run the loader-compat checks (`seq`
encodability, freq range, duplicates) — run
`scripts/build_pack.py validate [--all | --pack …]` for those.

## 5. Install, priority, privacy, paid

- **Install:** Settings → Packs → `+ .kbpack / URL` → pick file/URL →
  confirm the shown **SHA-256** (compare with the `sha256sum` output from
  §4) → Enable + set priority. (Device UI steps; the hash command above
  is the verifiable half.)
- **Signature:** SHA-256 is shown at install. **Unsigned packs install
  with a warning.** Verify out-of-band before enabling.
- **Priority:** hint `10–90`; the user reorders within range (Settings →
  Packs drag-drop = stack order). Display keeps the highest-priority
  pack's `cat`; frequencies sum; personal (`100`) always outranks packs.
  Base (`0`) stays bottom. See `plan/08-dict-stack-ordering.md`.
- **privacy.learn:** per-category opt-out of personalization, nested as
  `"privacy": {"learn": false}` in `assets/categories/<id>.json`
  (flat `"learn"` also accepted — `CategoryRegistry.kt`). Example:
  `math` ships `"privacy": {"learn": false}` (its LaTeX tokens must not
  pollute personal learning); `numbers` likewise. Default is `true`.
- **paid:** informational only. The loader never checks it; any pack
  installs via file/URL; the store is optional, never exclusive.

## 6. Failure modes (exact messages, all reproduced)

- **F1 — non-canonical id.** `build` without `--force` exits 1:
  `validation errors (use --force to write anyway):` +
  `<out>: non-canonical id 'pets'`. `validate --pack` on such a file
  prints `<file>: FAIL (1 errors, 0 empty-seq)` + the same line, exit 1.
  Fix: `--force` (out-of-tree demo) or register the id in
  `CANONICAL_IDS` + `packs/README.md` (in-tree ship).
- **F2 — duplicate word.** Row `{"w":"hello"}` twice in one pack:
  `demo word[1]: duplicate ('hello', 'en')`. Fix: dedupe on
  `(word, lang)` (`expand-pack` does this automatically, reporting
  `+N new, M skipped/dup`).
- **F3 — freq out of range.** `{"w":"x","freq":0}`:
  `demo word[0]: freq 0 out of range 1..1e6`. Fix: keep `1..1000000`.
- **F4 — non-digit seq.** `{"w":"y","seq":"AB12"}`:
  `demo word[1]: seq 'AB12' not digits`. Fix: digits `0-9` only.
- **F5 — unencodable word.** A word with empty effective seq
  (`seq_for` empty, e.g. emoji without `key`/`seq`) is dropped at build
  (`dropped=N`) and flagged by validate as
  `<loc>: '…' encodes to empty seq`. Fix: add latin `key` or explicit
  `seq`.
- **F6 — `add-word` prints but does not write.** On a pack that fails
  validation (e.g. demo id `pets`) it prints `updated 'tabby' in …`
  then the error line and exits 1 **without writing** (verified: freq
  stayed `9000`). Fix: resolve the validation error first.
- **F7 — no `.kbpack` zip loader in core yet.** Only `packs/*.json`
  loads via `pack::load_pack`. A `.kbpack` zip is the distributable
  envelope (install + hash review); do not expect core to open it.

## 7. Separation rationale (summary)

A keyboard sees personal data: `personal.jsonl`, bigrams, session log,
typed text. Code execution next to that surface gets ambient access to
it, so **if only vocab is required, ship only vocab**: a data-only pack
gets ranking weight + display `cat` + personal-learn overlay and
**nothing else** — no I/O, no network, no personal-store read.
Full capability matrix and plugin rules: `docs/PLUGIN_SDK.md` §4.
