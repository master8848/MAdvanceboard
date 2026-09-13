# kb9 Master Build Plan — Start Here

> Single entry point. Detail lives in `01–24` files; this doc tells you what to build, in what order, and what "done" means. Read this, then work top to bottom.

## 1. Goal

A keyboard any non-technical friend can install and daily-drive for everything (messaging, WhatsApp/Viber, English + Nepali + others). Two boards, one engine:

- **QWERTY** (default for friends): Gboard-grade suggestions + autocorrect (`22`).
- **9-key T9** (research vehicle, opt-in): tap-once-per-letter with a strip that is never empty (`17`, `18`).

## 2. Global rules (apply to every step)

1. No swipe-typing. Gestures = actions only.
2. Deterministic: same `(dict, ctx, digits, tab, layout, personal-hour, snippet-hour)` → byte-identical results. Fixed-count budgets only, never wall-clock cutoffs.
3. One `layout_id` per keystroke: global default + per-category override. NE defaults to t9-9.
4. Nepali = downloaded `w+tr` wordlists, Roman `tr` for `seq`, Devanagari display. No matra-only seqs. Tail freqs are low-confidence — don't tune against them.
5. QWERTY and T9 ranking never blend: T9 = digit-prefix + short-exact boost; QWERTY = literal + letter-graph corrections + confidence threshold.
6. Learn is always local; incognito is the off-switch. Clipboard friend-default OFF/24h.

## 3. Build order

### Phase 0 — Gate (1–2 days) — GO/NO-GO for everything

| # | File | Do | Done |
|---|------|----|------|
| 1 | `00-gates.md` | Freeze corpus split + seed. Run top-1/top-3 @3/@4/full, KSPW, neighbor ON/OFF, per-tab EN/NE/code, learning arm. Fix `HashMap` tie-order first. | KSPW≤1.5, top-3@4≥85%, lift≥10pts → GO. Else cheapest-first fixes in file, re-run. |

### Phase 1 — Correctness (parallel after GO)

| # | File | Do | Done |
|---|------|----|------|
| 2 | `04-engine-determinism.md` | Total-order truncate, 1h clock buckets, bigram backoff 0, personal-OOV index, golden + fuzz harness. | Same inputs → byte-identical outputs, fuzz green. |
| 3 | `03-nepali-transliteration.md` | `tr` first-class, `encode(tr)` pipeline, matras non-emitting, 5k+ `w+tr` pack. | Roman typing surfaces Devanagari; NE per-tab top-3@4 reported. |
| 4 | `02-layout-global-percat.md` | `resolve(cat)`, `set_default/set_cat_layout`, per-layout FST + precomputed seqs. | Layout switch changes labels + results deterministically; NE→t9-9. |
| 5 | `01-gestures-action-only.md` | Pad flings (←del / ↑space / →accept / ↓hide), Tabs/Bar, FAB toggle, 12-key override, AT fallback. QWERTY flings call same path as keys. | Gesture map works per file; no dead-ends for TalkBack/D-pad. |
| 6 | `22-qwerty-parity.md` | Literal slot + corrections + confidence (additive); space/punct/double-space/autocaps; backspace-restore via `lastAuto`; letter-graph scorer; `Aggressive/Conservative/Suggest-only/Off`. | `teh`→`the`, space commits, `⌫` restores; password never autocorrects; 1 FFI/keystroke; T9 metrics unchanged. |
| 7 | `17-single-press-fallback.md` | Synthetic `key_label` inject for `len<=2` (truncate-exempt, no-learn) + short-exact boost + cache-key version. | `suggest("7")` contains `s` first; len≥3 unchanged; QWERTY unaffected. |
| 8 | `18-strip-muscle-memory.md` | Chips as core-union view; space-commits-top; backspace cascade; candidate preview (no digit leak); leftmost-#1. | Single tap → chips; space commits chip; backspace cascades per digit. |
| 9 | `23-multilingual-v1.md` | NE→t9-9 + re-run gate; user-wordlist `tr`/`alt`; learn-path `tr` lookup; Roman-commit toggle. | Roman→Devanagari + Roman→Roman both work; Hindi sideload matches. |

### Phase 2 — Performance (after Phase 1)

| # | File | Do | Done |
|---|------|----|------|
| 10 | `05-suggest-optimization.md` | FST posting + StartsWith, generative neighbors, top-30 heap, alloc kills, `suggest` out of `learn`/`reject`, prefix cache, quantized scores. | p95 <50ms on minSdk26 at 6613 + 10k personal. |
| 11 | `06-persistence-power.md` | Single handle + WAL/NORMAL, dirty-set 2s flush, ring-cap session log, settle LFU-vs-LRU. | No per-accept full rewrite; power trace clean. |
| 12 | `14-db-indexing.md` | 6 indexes + `context_counts` DDL, after `06` only. | Analytics queries `USING INDEX`; hot path pure-RAM; DB <5MB at 30d. |

### Phase 3 — Extension (after Phase 2; ship friends after row 18)

| # | File | Do | Done |
|---|------|----|------|
| 13 | `07-code-math-palettes.md` | Tier-0 token packs all tabs; Tier-1 palette for js/rust/html/math only. | Keyword Tier-0 + one snippet + one palette action per code tab, no engine fork. |
| 14 | `12-snippet-vs-learn-gates.md` | Freeze `learnNow`/`snippetNow` truth table; 2-min buffer lifecycle owner. | OTP→nothing; `if`→`if (` for 2min; incognito/password→neither. |
| 15 | `08-dict-stack-ordering.md` | Drag-drop 10-90 UI, badges, placement popup, export round-trip. | Reorder changes rank deterministically; personal still outranks. |
| 16 | `11-incognito-clipboard.md` | `shouldLearnNow()` single gate; incognito triggers + visuals; clipboard pins + TTL. | Password/incognito capture nothing; pin survives 8-day sweep. |
| 17 | `13-temporal-contextual-suggestions.md` | P1 bucketed re-rank; P2 routine row (gated, toggleable). | Morning lift ≥3pts; routine precision ≥60% or kill. |
| 18 | `15-voice-typing.md` | **System engine first** (zero audio retention), API engine second. | Airplane-mode retry without re-speak; zero residue after close. |
| 19 | `19-settings-ia.md` + `20-dictionaries-sheet.md` | 4 sections + search + quick-sheet; unified tab manager; `medical` canon. | 8→4 routes resolve; sheet edits current tab only. |
| 20 | `21-onboarding-friend-ready.md` | Trust pre-frame + QWERTY choice + trial + tabs + graduation + switch-back. | Enable→switch <2 min; skippable. |
| 21 | `24-friend-ready.md` | Emoji grid + recents; Play listing; WhatsApp/Viber/Messenger field test. | Checklist all green before inviting anyone. |
| 22 | `16-docs-refresh.md` | Configurable-board wording across living docs. | Zero stale "9-button" claims; `just docs-check` passes. |

### Phase 4 — Spikes (any time after Gate, time-boxed, never blocking)

| # | File | Do | Done |
|---|------|----|------|
| 23 | `09-research-open-questions.md` | 7 spikes: weights, bigram-V, alias caps, snippet recall, math nav, novel input, longitudinal/power. | Each ends falsifiable; feeds back, blocks nothing. |

## 4. Friend-ready definition of done (check before any invite)

1. Signed distributable (Play internal track min) + EN/NE listing + screenshots.
2. Enable→switch <2 min with warning pre-frame + switch-back instructions.
3. QWERTY default; 9-key opt-in.
4. EN+NE messaging without tab dance; visible tabs `EN/ने/😀/123` only.
5. Emoji grid + recents. 6. System voice (mic hidden in password/numbers).
7. Autocap, double-space period, backspace-restore, general undo.
8. Zero crashes across WhatsApp/Viber/Messenger + no battery regression.
9. Clipboard OFF/24h; incognito visibly indicated.
10. One-tap "report problem / switch back".

## 5. How to work

1. Pick the next unfinished row above. Open its file — it has Problem / Steps / Tests.
2. Implement, run its Tests, tick it off.
3. If a step's premise breaks (e.g. gate fails, NE crowding unsolved), stop and re-run Phase 0 before continuing downstream — downstream assumes upstream numbers.
4. Deferred, do not build: sync merge/recovery, `.kbpack` zip/WASM/SHA UI, gesture-typing, neural rescorer, auto-detect multilingual union (gated until NE per-tab clears gate).

## 6. Map: old → new (for continuity)

Old `17` → new `17` + `18`. Old `18` → new `19` + `20` + `21`. New `22/23/24` have no predecessor. `00–16` kept filenames; targeted improvements recorded inside each file (`01` QWERTY-fling rule, `02` NE→t9-9, `03` freq honesty, `04` snippet key, `05` cache keys, `07` fallback decision, `11` clipboard friend-default, `12` frozen exclusion). Archive (`archive-v1/`) is history — not audited, not referenced; all improvements live in this set.
