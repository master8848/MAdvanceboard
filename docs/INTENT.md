# Research Intent — 9-Key Predictive Keyboard (`kb9`)

> Research-app framing. Normative spec remains `SPEC.md`; behavior guides are `docs/USER.md`, `docs/PRIVACY.md`, `docs/EXTENSIONS.md`, `docs/SYNC.md`, `docs/TUNING.md`.

## 1. Problem statement

Full QWERTY is slow and error-prone on small, distant, or attention-constrained surfaces (phone one-hand, TV remote, VR/AR, watch), and monolingual autocomplete fails users who mix natural languages (EN/NE), code (`js`/`rust`/`html`), emoji, numbers, and math/LaTeX in one session. The research bet: a 9-key pad (3×3) plus a shared prediction engine can beat QWERTY on keystrokes-per-word without adding layout complexity, if ranking, categories, and personal learning do the heavy lifting.

Grounded in repo:

- 9-key default + 12-key toggle + gestures defined in `SPEC.md` §0; T9 map with Devanagari collapse in `SPEC.md` §1 and `docs/USER.md` §1.
- Category tabs as dict filter (`numbers | EN | NE | js | rust | html | emoji | math | ★personal`) in `SPEC.md` §0 and `docs/USER.md` §2.
- Suggestion bar (3 inline + expand-all 30-item sheet) in `SPEC.md` §0 and `docs/USER.md` §3.
- QWERTY fallback sharing the engine in `SPEC.md` §0 and `docs/USER.md` §4.
- Code/emoji/math reuse of digit sequences (e.g. `fun→386`, `hea→432`, `alp→257`) in `SPEC.md` §1, `packs/README.md`, `docs/EXTENSIONS.md` §§1–3.
- Hawking-style / Chinese-phonetic analogy: few digits → full-word candidates via shared `seq` lookup across scripts and packs (`SPEC.md` §1: one `seq` matches `english_trie` and `nepali_trie`; ranking picks script by tab + history).

## 2. Target users and contexts

1. **Small-screen typists** (phone one-hand, TV, wearables): 9 large keys reduce mis-press vs 26-key QWERTY; fuzzy 1-edit neighbor matching absorbs fat-finger error (`SPEC.md` §2; `docs/USER.md` §1; `docs/TUNING.md` §1).
2. **Bilingual EN/NE users**: phonetic Devanagari collapse + per-tab ranking, matra normalization (`SPEC.md` §1; `packs/README.md` Nepali rows `cat: NE`).
3. **Mobile coders / technical writers**: `js`/`rust`/`html`/`math` packs map keyword/laTeX-name prefixes to the same 9 keys (`SPEC.md` §§1,3; `docs/EXTENSIONS.md` §3).
4. **Emoji/number-heavy chat**: `emoji` keyword `seq` overrides, `numbers` digit-commit category (`SPEC.md` §§1,3; `docs/USER.md` §5).
5. **VR/AR/TV and accessibility**: few-target pad suits gaze/d-pad/switch input; the same motive as Hawking-style few-letter prediction. No special engine branch — only the key→sequence encoder changes (`SPEC.md` §0; `docs/USER.md` §4).
6. **Domain vocabularies (e.g. doctor/medical)**: treated as ordinary category packs with priority 10–90, not a fork — same loader, same stack (`SPEC.md` §§3–4; `docs/EXTENSIONS.md` §§1,5).

## 3. Core hypotheses

- **H1 — 3–4 letters suffice.** A 3–4 digit prefix narrows candidates enough that top-3 contains the target ≥90% of sessions. Mechanism: trie lookup capped at 200 matches → top-30 heap (`SPEC.md` §2), threshold `S < −0.5` hidden unless expanded (`SPEC.md` §2; `docs/TUNING.md` §4).
- **H2 — Categories beat one big dict.** Active-tab boost (`CategoryBoost`, `docs/TUNING.md` §1) plus stack priorities (base 0 < extensions 10–90 < personal 100, `SPEC.md` §4; `docs/TUNING.md` §2) resolves cross-domain collisions (e.g. `fun` as word vs keyword) better than frequency alone.
- **H3 — QWERTY-learn bridges adoption.** Users tolerate 9-key only if QWERTY fallback teaches the personal dict: OOV committed ≥2 times in 7 days → promoted with `freq_personal=accepts` (`SPEC.md` §4; `docs/USER.md` §4; `docs/TUNING.md` §4).
- **H4 — Open packs prevent enclosure.** If any `.kbpack` installs via file/URL with `paid` informational-only and unsigned-allowed-with-warning, domain vocabularies stay open and the store stays optional (`SPEC.md` §3; `kbpack.schema.json` `paid` description; `docs/EXTENSIONS.md` §5).

## 4. What this is NOT

- **Not a commercial lock-in keyboard.** No exclusive store, no gated loader, no server account (`SPEC.md` §§3,5; `docs/EXTENSIONS.md` §5; `docs/PRIVACY.md`; `docs/SYNC.md` §1).
- **Not a cloud-AI keyboard.** Bigrams are personal-only with backoff 0, no server model (`SPEC.md` §2; `docs/PRIVACY.md` incognito section).
- **Not gesture/voice typing.** Explicit MVP non-goals: cloud account sync, gesture typing, voice (`SPEC.md` §7).
- **Not a password learner.** Password fields and `privacy.learn: false` packs (`numbers`, `math`) never touch personal/bigram/log (`docs/PRIVACY.md`; `android/README.md` privacy notes; `docs/USER.md` §5).

## 5. Research questions + falsifiable success criteria

Questions: (a) keystroke saving vs QWERTY? (b) mis-press recovery via neighbor matching? (c) does per-tab + personal ranking lift top-3 hit rate over time? (d) do open packs get installed over bundled defaults? (e) does the engine stay within lean/power/memory budgets?

Proposed criteria (targets to confirm or refute — no thresholds exist in repo today):

| Metric | Pass threshold (proposal) | Source instrument |
|---|---|---|
| Keystrokes-per-word (KSPW, incl. selection taps) | ≤1.5 on EN/NE test set vs ~3–4 on QWERTY baseline | Session `log.jsonl` accept events (`docs/SYNC.md` §2; `core-rust/README.md` session) |
| Top-3 hit rate | ≥85% exact-seq, ≥75% with 1-edit neighbor on | `suggest(seq,ctx,tab,limit)` 3 vs 30 (`SPEC.md` §2; `docs/TUNING.md`) |
| Mis-press rate | Neighbor-on reduces failed commits ≥30% vs neighbor-off | Neighbor cost 0.4 toggle (`docs/USER.md` §1; `docs/TUNING.md` §3) |
| Learning lift | Personal + bigram + recency raises top-3 hit ≥10 pts after 7 d vs day 0 | Accept/reject logging, 5 s reject window, 30 d reject decay (`SPEC.md` §2; `docs/TUNING.md` §§1,4–5) |
| Snappy / lean | p95 `suggest` <50 ms on minSdk 26 device; core RSS <50 MB; personal ≤10–20k entries | Trie O(\|seq\|+matches) + heap top-30 (`SPEC.md` §2); cap dispute 20k LRU vs 10k LFU (`docs/TUNING.md` §5; `packs/README.md` SPEC deltas) |
| Power | No measurable drain delta in 1 h typing trace; background work ≤24 h periodic only | `WorkManager` 24 h job, batch WAL writes every 2 s / compact hourly (`android/README.md`; `SPEC.md` §§2,5; `docs/SYNC.md` §3) |

Falsification: if KSPW >2.0, neighbor-on shows no mis-press gain, or 7-day learning lift <5 pts, H1–H3 are rejected as framed.

## 6. Ethics and risks + mitigations

- **Paid-pack enclosure.** Risk: store-only vocabularies lock medical/code users in. Mitigation: open `.kbpack` (manifest + `dict.jsonl.gz` + optional sandboxed WASM tokenizer), `paid` never gates loading, SHA-256 shown, unsigned allowed with warning (`SPEC.md` §3; `kbpack.schema.json`; `docs/EXTENSIONS.md` §§1,5).
- **Sync conflicts / neologism evolution.** Risk: new words lost or duplicated across devices. Mitigation: per-word-id last-write-wins + element-wise max-merge of monotonic counters, tombstone-wins-on-newest, HLC `ts` + `deviceId`, pack updates never overwrite `personal.jsonl`, session log replay rebuilds counters deterministically (`SPEC.md` §5; `docs/SYNC.md` §§2–4).
- **Layout-shift discouragement.** Risk: churning suggestions teach distrust. Mitigation: stable ranking weights, tie-break (shorter → lexicographic → priority), hide threshold `−0.5`, expand-all pages instead of reordering the bar, user-reorderable tabs, per-user tuning sliders with reset (`SPEC.md` §§0,2; `docs/TUNING.md` §§2–4; `docs/USER.md` §§2–3).
- **Spelling atrophy / over-reliance.** Risk: users accept wrong words. Mitigation: placement popup (`packId • freq • accepts`) answers "why suggested", Pin/Block/Info actions, reject penalty (−1.5 weight), block tombstones exempt from eviction and replicated, QWERTY path keeps full spelling visible (`SPEC.md` §§2,4; `docs/USER.md` §§6–7; `docs/TUNING.md` §1).
- **Privacy.** Risk: keystroke leakage. Mitigation: `BIND_INPUT_METHOD`-only, no `INTERNET` in shell, opt-in file sync only, no password learning, on-device `redb` + WAL, nothing transmitted (`docs/PRIVACY.md`; `android/README.md`).

## 7. Scope: MVP vs v2

**MVP (SPEC M0–M4, `SPEC.md` §7):** monorepo + Rust `trie`/encoder/CLI; Android IME shell (9/12-key, bar + expand-all, QWERTY toggle, EN/numbers tabs); ranking + bigram + personal + reject logging + placement popup; `.kbpack` loader + 8 showcase packs (`en`, `ne`, `numbers`, `js`, `rust`, `html`, `emoji`, `math` per `packs/README.md`); JSONL session export/import + file-sync + auto-recovery + tuning settings + privacy doc + Play-internal release. Current state: 33 unit tests green, 8/8 packs load (225 words), stub `Predictor` + missing snapshot/WAL/merge/recovery still owed (`INTEGRATION_REPORT.md` §§1–4).

**V2 (deferred):** background sync service beyond the 24 h `WorkManager` file-sync stub (Drive/WebDAV folder polling, conflict UI), cloud account sync explicitly out of MVP (`SPEC.md` §7; `android/README.md` sync stub); medical/domain pack showcase; longitudinal KSPW/learning-lift study using exported session logs (`docs/SYNC.md` §2); gesture/voice remain non-goals until H1–H3 are confirmed.
