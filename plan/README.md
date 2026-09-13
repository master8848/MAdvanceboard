# kb9 Plan Ground — Multi-Tier Research + Build Plan

> Start here: **`MASTER.md`** (build order, definition of done, how to work). Detail files below.

> 2026-09-13: v1 plan (`00`–`16`, see `plan/archive-v1/`, 18 files) archived. Active plan below.
> 2026-09-13 (2nd pass): split overloaded files — old `17` → `17` core + `18` strip; old `18` → `19` settings + `20` dictionaries + `21` onboarding; new `22` QWERTY parity, `23` multilingual V1, `24` friend-ready. Steps inside new files numbered 1,2,3… per file. `10-execution-order.md` rewritten to cover all files.

Source: 5 research tracks (encoding gate, gesture-as-action, Nepali transliteration, code/math palettes, deterministic optimization) + repo audit (`SPEC.md`, `core-rust/src/stack.rs`, `mapping.rs`, `rank.rs`, `personal.rs`, `predictor.rs`, `layout.rs`) + 5 parallel research passes 2026-09-13 (Gboard QWERTY parity, T9 strip muscle-memory, friend-adoption blockers, multilingual, plan-structure audit).

## Tiers

- **Tier-0 Gate:** `00-gates.md` — KSPW≤1.5, top-3@4≥85%, lift≥10pts. GO/NO-GO before all else. (Row #1 2026-09-13: NO-GO — scope 0.375; EN GO, NE passes @full; see `GATE_RESULT.md`.)
- **Tier-1 Correctness:** `01-gestures-action-only.md`, `02-layout-global-percat.md`, `03-nepali-transliteration.md`, `04-engine-determinism.md`, `22-qwerty-parity.md`, `17-single-press-fallback.md`, `18-strip-muscle-memory.md`, `23-multilingual-v1.md` — must be right, else everything downstream reshapes.
- **Tier-2 Performance:** `05-suggest-optimization.md`, `06-persistence-power.md` — lean + snappy, deterministic caps, no time-cutoff.
- **Tier-3 Extension:** `07-code-math-palettes.md`, `08-dict-stack-ordering.md`, `12-snippet-vs-learn-gates.md`, `11-incognito-clipboard.md`, `13-temporal-contextual-suggestions.md`, `14-db-indexing.md`, `15-voice-typing.md` (system engine first), `19-settings-ia.md`, `20-dictionaries-sheet.md`, `21-onboarding-friend-ready.md`, `24-friend-ready.md` — friend-ready loop closes here.
- **Tier-4 Open:** `09-research-open-questions.md` — novel/unsure, needs spikes.
- **Order:** `10-execution-order.md` — dependency graph. User compiles later; plan only.
- **Privacy switch:** `11-incognito-clipboard.md` — learn always local (no off-toggle); incognito is the off-switch; clipboard history with pins (friend-default OFF/24h, 7d power-user opt-in).
- **Gates split:** `12-snippet-vs-learn-gates.md` — snippet exempt (`tab != numbers` frozen), lifecycle owner; learn stays `privacy.learn`.
- **Temporal:** `13-temporal-contextual-suggestions.md` — Gboard/SwiftKey pattern (re-rank after prefix + separate routine row, never empty-strip push); P1 8-bucket SQLite sprinkle, P2 routine row, P3 monthly deferred.
- **DB indexes:** `14-db-indexing.md` — today only PKs, zero secondary; index set lands after `06` single-handle.
- **Voice:** `15-voice-typing.md` — system engine FIRST (ship for friends), optional API engine second, no re-speak via bounded RAM buffer cleared on close.
- **Docs:** `16-docs-refresh.md` — living-docs wording task: configurable board (9 default, 12/16/QWERTY per category), categories + gestures replace QWERTY mode mazes.
- **Single-press core:** `17-single-press-fallback.md` — `len<=2` never empty (synthetic `key_label` inject scoped `!qwerty`, short-exact boost, truncate-exempt, no-learn).
- **Strip muscle-memory:** `18-strip-muscle-memory.md` — chips as core-union view, space-commits-top never bare, backspace cascade, candidate preview, leftmost-#1.
- **Settings IA:** `19-settings-ia.md` — 4 sections + search + quick-sheet; friend-defaults hide code tabs; Advanced buried.
- **Dictionaries:** `20-dictionaries-sheet.md` — single editor, drag 10-90, `medical` canon (sole owner), quick-sheet current-tab-only.
- **Onboarding:** `21-onboarding-friend-ready.md` — trust pre-frame + QWERTY choice + trial + tabs + graduation + switch-back; skippable + progressive coach.
- **QWERTY parity:** `22-qwerty-parity.md` — literal slot + correction confidence, space/punct/double-space/autocaps, backspace-restore, letter-graph scorer. The file that makes friends stay.
- **Multilingual V1:** `23-multilingual-v1.md` — NE→t9-9, user-wordlist `tr`/`alt`, learn-`tr` lookup, Roman-commit toggle, tail-freq honesty; auto-detect deferred with gate.
- **Friend-ready:** `24-friend-ready.md` — top-10 blockers ranked, minimal checklist, emoji grid + Play listing + field test, alternatives decided.

## Rules across all tiers

1. No swipe-typing. Gestures = actions only (switch category, accept suggestion, QWERTY toggle, delete/space).
2. Deterministic: same `(dict, ctx, digits, tab, layout)` → same results. Allow slightly slow, never change results based on runtime. No wall-clock cutoff. (Snippet buffer joins the key via 1h-quantized expiry — see `04` amendment, `12`.)
3. Global layout + per-category override. 9 default for simple, QWERTY full-fallback (`22`), 16 opt-in for complex. Single `layout_id` resolved per `cat`. NE defaults to t9-9 (see `02` amendment, `23`).
4. Nepali via downloaded `w+tr` wordlists (Devanagari display + Roman `tr` for `seq`), not hand-made 253 words. Matra-only seqs are banned. Tail freqs marked low-confidence until a real source lands.
5. Each file = one step. Implement in `10-execution-order.md` sequence.
6. QWERTY and T9 never share ranking behavior: T9 uses digit-prefix + short-exact boost (`17`); QWERTY uses literal + letter-graph corrections + confidence (`22`). Shared code paths must be gated, never blended.
