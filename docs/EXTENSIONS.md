# Extensions — `.kbpack` Authoring Guide

> Start here: **vocab (data-only, default)** → `VOCAB_SDK.md`;
> **code plugins (privileged)** → `PLUGIN_SDK.md` (capability matrix +
> separation rationale, §4). This page is the SPEC-format reference.

Two formats exist. **SPEC format** (target, §3) is the open standard;
**current MVP format** (what `core-rust/pack.rs` loads today) is a subset.

## 1. SPEC `.kbpack` (target)

A `.kbpack` is a zip containing `kbpack.json` + `dict.jsonl.gz` (+ optional
WASM tokenizer).

Manifest `kbpack.json` fields:

| Field | Required | Meaning |
|---|---|---|
| `id` | yes | reverse-dns, e.g. `com.ex.ne-math` |
| `version` | yes | semver, e.g. `1.2.0` |
| `kind` | yes | `vocab` (MVP) |
| `name` | yes | display name |
| `license` | yes | e.g. `MIT` |
| `paid` | yes | `true/false` — **informational only**, never gates loading |
| `langs` | yes | e.g. `["en","ne","latex"]` |
| `categories` | yes | e.g. `["math"]` |
| `priority` | yes | integer `10–90` (base `0`, personal `100`; user-reorderable within range) |
| `dict` | yes | e.g. `["dict.jsonl.gz"]` |
| `tokenizer` | no | `default \| latex \| custom` (WASM `tokenize(word)->seq`) |
| `minCore` | yes | e.g. `0.3.0` |

`dict.jsonl.gz`: one JSON object per line, gzipped:

```json
{"w":"\\alpha","lang":"latex","cat":"math","seq":"257","freq":6000}
{"w":"function","lang":"en","cat":"js","seq":"386","freq":9000}
```

- `w`: surface form. `lang`: BCP-ish tag. `cat`: category tab id.
- `seq`: explicit digit string. Omit only if the default T9 encoder
  produces it (`tokenizer: default`).
- `freq`: Zipf-normalized base count `1..1e6`.

Tokenizer options: `default` (Latin T9 + Devanagari collapse), `latex`
(strips leading `\`, encodes letters), `custom` (ship WASM with the same
mapping). No arbitrary code runs on Android — WASM tokenizer only,
sandboxed.

## 2. MVP pack JSON (what loads today)

`packs/*.json` (e.g. `code_js.json`, `emoji.json`, `math.json`):

```json
{"id":"js","title":"JavaScript Keywords","version":"1.0.0","words":[
  {"w":"function","freq":9000,"cat":"js"},
  {"w":"❤️","seq":"432","freq":9000,"cat":"emoji"}
]}
```

Defaults: `cat` → pack id, `lang` → `"en"`, `freq` → `100`. `seq` is
required for non-encodable words (emoji, `\alpha`); otherwise the encoder
derives it. Load:

```rust
let pack = pack::load_pack("packs/code_js.json")?;
stack.add_pack(&pack, 20); // priority 10–90
```

Migration path: add `lang`, rename `title→name`, gzip the word list to
`dict.jsonl.gz`, and wrap with `kbpack.json` — word rows are otherwise
identical.

## 3. Minimal examples

JS vocab (`js`):

```json
{"w":"fun","lang":"en","cat":"js","seq":"386","freq":6000}
```

Math (`math`, `tokenizer: latex`):

```json
{"w":"\\alpha","lang":"latex","cat":"math","seq":"257","freq":6000}
```

## 4. Sideload steps

Settings → Packs → `+ .kbpack / URL` → pick file → confirm SHA-256 →
Enable + set priority. Showcase packs bundled at MVP: `words` (English base,
file `words_en.json`, 5150 words), `ne` (Nepali base, file `nepali.json`,
253 words), `medical` (new 2026-09-12, file `medical.json`, 545 terms),
numbers (builtin, 10), js (183), rust (150), html (142), emoji (111),
math (69) — 6613 total. Build/expand tooling: `scripts/build_pack.py`,
how-to: `packs/ADDING.md`.

Canonical IDs (2026-09-12 reconcile): pack/category ids are `words, ne,
js, rust, html, emoji, numbers, math`, plus **`medical`** (added in the
2026-09-12 expansion) — lowercase ids; tab display labels stay
`EN`/`NE`/`123` (SPEC §0). Android manifests live in
`android/ime/.../assets/categories/<id>.json` (`ne.json`, not `nepali.json`).
`cat` defaults to the pack id (`pack.rs`); `nepali.json` rows still carry
the display form `"NE"` (normalize to `"ne"` only with the engine
`"EN"`-default cleanup — deferred, `cat_boost` is exact-match).

## 5. Policies

- `paid` flag is **informational only**. The loader never locks on it; any
  pack installs via file/URL; the store is optional, never exclusive.
- Signature: SHA-256 is shown at install. **Unsigned packs are allowed
  with a warning.** Verify out-of-band before enabling.
- Priority is a hint (`10–90`); the user can reorder. Display keeps the
  highest-priority pack's `cat`; frequencies sum (personal weighted
  separately — see `TUNING.md`).

Related: `USER.md` (placement popup), `TUNING.md` (stack + scoring),
`SYNC.md` (pack updates never overwrite personal).
