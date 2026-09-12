# 9-Key Predictive Keyboard — Architecture Spec

## 0. Layout (top-to-bottom)

1. `CategoryTabs`: horizontal scrollable tabs. Categories = `numbers | EN | NE | js | rust | html | emoji | math | ★personal`. Active tab sets dictionary filter + key labels. Tabs are user-reorderable; `numbers` is a category, not a mode.
2. `SuggestionBar`: 3 inline candidates + `expand-all (∨)` button opening bottom-sheet grid (30 items, paged). Long-press candidate = pin / block / info (placement: which dict + freq).
3. `Pad`: `9-key` (3×3) default; `12-key` toggle adds bottom row `[Sym|Shift | Space/0 | ⌫/⏎]`. In 9-key mode bottom actions become gestures: swipe-left on pad = delete, swipe-up = space, long-press = symbols.
4. `QWERTY toggle`: floating button bottom-right; switches Pad to full QWERTY view. Prediction engine shared; only key→sequence encoder changes.

## 1. T9 9-Key Mapping Proposal

Standard Latin T9 preserved; Devanagari collapsed phonetically onto same keys so one sequence matches both scripts. Ranking decides script by active category + history.

| Key | Latin | Devanagari (translit initial) | Punct/Alt (long-press) |
|---|---|---|---|
| 1 | `.,?!'` | `। ॥` | `1 + - × / ( )` |
| 2 | ABC `2` | क ख ग घ ङ (k-series) | `2` |
| 3 | DEF `3` | च छ ज झ ञ (c/j-series) | `3` |
| 4 | GHI `4` | ट ठ ड ढ ण (retroflex) | `4` |
| 5 | JKL `5` | त थ द ध न (dental) | `5` |
| 6 | MNO `6` | प फ ब भ म (p/b/m) | `6 @` |
| 7 | PQRS `7` | य र ल व श (y/r/l/w) | `7 $` |
| 8 | TUV `8` | ष स ह क्ष त्र ज्ञ (s/h/conj) | `8 %` |
| 9 | WXYZ `9` | vowels अ आ इ ई उ ऊ ए ऐ ओ औ अं अः + matras | `9 #` |
| */0/# (12-key only) | — | — | `Sym | Space/0 | ⌫/⏎` |

Rules:
- Each keypress emits digit `1-9`. Encoder produces `seq` e.g. `264`. Lookup matches both `english_trie.get(264)` and `nepali_trie.get(264)`.
- Key `1` = punctuation commit (tap cycles `. , ? !`), does not enter prediction seq; breaks word.
- Key `9` vowel handling: Nepali matras (ा ि ी ु ू े ै ो ौ ं) normalized to base vowel for lookup (e.g. `कि` → क+इ → `29`).
- Numbers: tapping via `numbers` category remaps pad to `1-9,0` digit-commit (no prediction). Long-press any key in alpha mode inserts its digit.
- Code/emoji/math categories reuse same seq: `js` pack maps `2-9` to keyword prefixes (`fun→386`); emoji maps to keyword (`hea→432`=❤️); math maps to latex name (`alp→257`=α `\alpha`).

## 2. Suggestion Ranking Formula

Per keystroke, engine unions candidates from dictionary stack matching `seq` (exact + 1-edit neighbor for fat-finger, edit cost 0.4), scores, takes top-N.

```
S(w) = 1.0*log10(freq_base + freq_personal + 1)
     + 1.2*P_personal(w)        # 0/1 if in personal, scaled by accepts
     + 0.8*Bigram(ctx_prev, w)  # log-prob from personal bigram, backoff 0
     + 0.5*Recency(w)           # exp(-dt/7d), dt since last accept
     + 0.3*CategoryBoost(w)     # 1.0 if w.cat == activeTab else 0, 0.5 if ★
     + 0.2*KeyFit(seq,w)        # 1.0 exact, 0.6 neighbor
     - 1.5*RejectPenalty(w)     # rejects/(accepts+rejects+1), decayed 30d
```

- `freq_base`: static count from pack (Zipf-normalized 1..1e6). `freq_personal`: learned counter.
- `Bigram`: `log10(count(prev,w)+1)-log10(count(prev)+V)`; personal only (privacy); no server.
- Weights are constants in `core/config.rs`, tunable per user in Settings → Tuning (advanced).
- Tie-break: shorter word, then lexicographic, then pack priority.
- Learning update on accept: `freq_personal+=1`, bigram+=1, recency=now. On reject (user deletes committed word within 5s or picks another candidate): `rejects+=1`. Background thread batches writes every 2s to WAL.
- Threshold: candidates with `S < -0.5` hidden unless expand-all.

Complexity: trie lookup O(|seq| + matches); scoring O(matches), matches capped at 200 before top-30 heap.

## 3. Extension API Spec

Open `.kbpack` format (mitigates paid-plugin concern: any pack installable via file/URL; store is optional, never exclusive).

Manifest `kbpack.json`:
```json
{
  "id": "com.ex.ne-math", "version": "1.2.0", "kind": "vocab",
  "name": "Math/LaTeX", "license": "MIT", "paid": false,
  "langs": ["en","ne","latex"], "categories": ["math"],
  "priority": 20, "dict": ["dict.jsonl.gz"],
  "tokenizer": "latex", "minCore": "0.3.0"
}
```

Rust trait (`core/src/pack.rs`):
```rust
trait VocabPack {
  fn meta(&self) -> &Manifest;
  fn categories(&self) -> &[String];
  fn candidates(&self, seq: &str, ctx: &str, limit: usize) -> Vec<Cand>;
  fn contains(&self, word: &str) -> Option<DictEntry>;
}
struct Cand { word: String, seq: String, freq: u32, cat: String, lang: String }
```

- Packs are static data + optional WASM tokenizer (`tokenize(word)->seq`); no arbitrary code on Android (sandbox). Dynamic plugins (paid) implement same trait but ship dict only; host app never locks loader.
- Install: Settings → Packs → `+ .kbpack / URL`. Signature optional (SHA-256 shown); unsigned allowed with warning.
- Showcase packs bundled at MVP: `en-base, ne-base, numbers (builtin), js, rust, html, emoji, math`.

## 4. Dictionary Stack + Personal Dict Rules

Stack (low→high): `base EN/NE (prio 0)` < `extension packs (10-90, user-ordered)` < `personal (100)`.

- Lookup = union across enabled packs + personal; dedupe key `(norm(word), lang)`. Display keeps highest-prio pack's `cat`; score sums frequencies but personal weighted per §2.
- `Search dictionary placement`: long-press candidate → popup shows `packId • freq • accepts` (answers "where is this word from").
- Personal dict rules:
  1. Never overwritten by pack updates; stored separately `personal.jsonl`.
  2. Auto-add: OOV word committed (typed via QWERTY/multitap or selected from expand-all raw) with ≥2 accepts in 7d → promoted to personal with `freq_personal=accepts`.
  3. Block: user action Block → tombstone `{deleted:true}`; hidden everywhere, survives local export/import (single-device scope, no cross-device merge).
  4. Decay: `freq_personal *= 0.98` monthly if unused; never deletes, only demotes.
  5. Cap: 20k entries LRU-evicted (blocked tombstones exempt).

## 5. Sync / Export Format (JSONL, local only — single-device scope)

Each dict row (base, pack, personal, session log) is JSONL:
```json
{"w":"काठमाडौं","lang":"ne","cat":"NE","seq":"524...","freq":120,"acc":5,"rej":1,"ts":1726000000,"del":false,"id":"ne:काठमाडौं"}
```

- Export session: `session-<date>.jsonl.gz` = personal delta + `log.jsonl` (accepted/rejected events `{ts, seq, ctx, chosen, shown, action}`). Import replays log through ranking to rebuild counters (deterministic).
- File layout (user picks folder/WebDAV/Drive): `personal.jsonl`, `bigrams.jsonl`, `snapshot-<ts>.gz`, `wal.log`. Import applies rows as-is (upsert by `id`).
- No multi-device merge: the earlier last-write-wins + counters max-merge rule was REMOVED (whole-row LWW silently lost counters; unbuilt per plan). Each device keeps its own personal table; `del` tombstones are local-only, never reconciled across devices.
- Auto-recovery: on startup, if `personal.jsonl` corrupt → restore newest `snapshot`, replay `wal.log`; if both corrupt → start empty personal, keep base functional, notify. Every write is append-to-WAL then batch-compact hourly. Vector clock: single-device HLC (`ts` + deviceId tiebreak) suffices; no server clock trust.

## 6. Repo Monorepo Layout

```
kb9/
  core/               # Rust, no Android deps, unit-tested
    src/{lib,trie,rank,stack,pack,sync,learn}.rs
    tests/ dict fixtures
  android/            # Kotlin IME service + Compose UI
    app/ (InputMethodService, CategoryTabs, SuggestionBar, PadView, QwertyView)
    packs/ loader + WASM tokenizer bridge
  packs/              # open-format vocab sources
    en-base/ ne-base/ js/ rust/ html/ emoji/ math/ numbers.md
    scripts/build_pack.py  # csv/txt -> dict.jsonl.gz + kbpack.json
  docs/ SPEC.md USER.md PRIVACY.md
  sessions/ (gitignored) exports
  Cargo.toml settings.gradle kbpack.schema.json
```

- `core` exposes C-ABI/UniFFI: `encode_keys(), suggest(seq,ctx,tab,limit), accept(word), reject(word), export(), import()`. Android calls via JNI; desktop demo via CLI.
- Background logging thread lives in `core`; Android only schedules flush (WorkManager).

## 7. MVP Milestones

- **M0 skeleton (wk1):** monorepo + `core` trie + T9 encoder + CLI `suggest`. EN mini-dict (5k).
- **M1 Android shell (wk2-3):** IME service, 9/12-key pad, suggestion bar + expand-all sheet, QWERTY toggle, category tabs (EN/numbers only).
- **M2 prediction (wk4-5):** ranking formula + bigram learning + personal dict + rejected logging + placement popup.
- **M3 packs (wk6):** `.kbpack` loader, manifest+trait, NE-base + js/rust/html/emoji/math showcase packs, sideload UI.
- **M4 sync/polish (wk7-8):** JSONL export/import, file-sync + auto-recovery, tuning settings, privacy doc, Play-internal release.

Non-goals MVP: cloud account sync, gesture typing, voice.
