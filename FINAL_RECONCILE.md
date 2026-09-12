# Final Reconcile — cross-cut (2026-09-12)

Static-only pass (no JDK/Gradle builds). Prior crews done (android fix,
engine lean, docs intent); this pass reconciles IDs, bridge parity, and the
tombstone name across `packs/`, `core-rust/`, `android/`, `docs/`.

## 1. ID map (canonical: `words, ne, js, rust, html, emoji, numbers, math`)

| Canonical | `packs/<file>.json` (`id`) | `android/.../assets/categories/<file>.json` (`id`) | Change this pass |
|---|---|---|---|
| `words` | `words_en.json` (`words`, was `en`) | `words.json` (`words`, display `EN`) | pack id `en`→`words` |
| `ne` | `nepali.json` (`ne`) | `ne.json` (`ne`, was `nepali.json`/`nepali`, display `NE`) | manifest file+id renamed |
| `js` | `code_js.json` | `js.json` | — (agreed) |
| `rust` | `code_rust.json` | `rust.json` | — |
| `html` | `code_html.json` | `html.json` | — |
| `emoji` | `emoji.json` | `emoji.json` | — |
| `numbers` | `numbers.json` | `numbers.json` (display `123`) | — |
| `math` | `math.json` | `math.json` | — |

Pack-id set == manifest-id set (8 == 8, verified via `python json.load`).
Kotlin: `CategoryTabs.DEFAULT_CATEGORIES` now uses canonical ids
(`words`, `ne`, …); `onCategoryChanged` keeps compat mapping for legacy
labels `EN`/`NE` and the old `nepali` id. `pack.rs` needs no code change
(default `cat` = pack id); its test id updated `en`→`words`.
Docs: `packs/README.md` (table + Canonical IDs section) and
`docs/EXTENSIONS.md` (showcase IDs + display-vs-id note) updated.

Deferred (noted, not changed): tab display labels stay `EN`/`NE`/`123`
(SPEC §0); `nepali.json` rows still carry display `cat "NE"`; `stack.rs` /
`predictor.rs` defaults still `"EN"` — normalize row cats + engine defaults
to `words`/`ne` together later (`cat_boost` is exact-match, so do it in one
move).

## 2. Bridge delta

Rust UniFFI `Predictor` (0.32.1): `new` / `suggest` / `learn` / `forget` /
`export_session`. Android `core-bridge` `Predictor` was `suggest` / `learn` /
`close` — now also declares `forget` / `reject` / `exportSession`, with
in-memory `StubPredictor` implementations (`core-bridge/.../Predictor.kt`).
`reject` has no Rust counterpart yet (`record_reject` is core-internal);
`forget`/`exportSession` stubs mirror the existing Rust methods. No JNI
regen attempted here — the real binding swap needs a JDK machine (below).

## 3. Tombstone decision: `del` (compat reader for `deleted`)

SPEC is self-inconsistent (§4 `{deleted:true}` vs §5 `"del":false` + merge
rule naming `del`). Decision: **`del` is canonical** (SPEC §5, all of
`docs/`). Compat readers accept legacy `deleted`:
core `PersonalEntry` via `#[serde(rename = "del", alias = "deleted")]`
(Rust field `deleted` and redb blobs untouched — old blobs still load, new
writes emit `del`); Android `SyncMerge.fromJsonl` reads `del` else `deleted`,
`toJsonl` emits `del`. Verified: transient test (new-write `del` only +
legacy-`deleted` load, then removed) green; full `cargo test` **33/33**.
`android/README.md`, `docs/SYNC.md`, `android/FIX_REPORT.md` note the rule.
`SPEC.md` itself left untouched (historical).

## 4. Verification (this pass)

- `cargo test` in `core-rust/`: **33 passed, 0 failed** (after all edits).
- Transient checks (run then deleted, nothing left in repo): tombstone
  `del`-write/`deleted`-read compat PASS; 8/8 packs load via
  `pack::load_pack`, entries == words, all `seq` non-empty PASS.
- `python json.load` all 8 packs + 8 category manifests PASS; id sets equal.
- `grep -rn INTERNET android/ime/ android/app/src` → no hits (BIND-only).
- Learn gating intact: `shouldLearnNow()` (password + incognito +
  input-class + email/URI + per-category `privacy.learn`) gates
  `predictor.learn`; `method.xml` subtypes `en`/`ne` present.

## 5. Remaining JDK tasks (on-device machine)

1. `export JAVA_HOME=<jdk17> && ./gradlew :app:assembleDebug
   :ime:assembleDebug` + lint (esp. `NewApi` on the API-28+ IME-switch call).
2. Add core `reject(word)` UniFFI export, then regen Kotlin bindings with
   **`uniffi-bindgen 0.32`** (must match crate `uniffi 0.32.1`) and replace
   `StubPredictor` with the generated `Predictor`.
3. Wire delete-within-5s → `reject`, explicit block → `forget`, active-tab
   string (canonical ids) + limit 3/30 into `suggest`, base wordlist JSON
   into `Predictor.new`; on-device smoke test.
