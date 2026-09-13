# 10 — Execution Order (Multi-Tier Dependencies)

User compiles later. Plan only, in this sequence. Each step = its file. Gate blocks downstream.

> Rewritten 2026-09-13: covers all 24 files (was 00–09/15/16 only). Old `17/18` split into `17/18` (core/strip) + `19/20/21` (settings/dicts/onboarding); new `22` QWERTY parity, `23` multilingual, `24` friend-ready.

## Tier-0 — Gate (do first, 1-2 days)

1. `00-gates.md` — freeze corpus split + seed, run top-1/top-3 @3/@4/full, KSPW, neighbor ON/OFF, per-tab breakdown, learning arm. Fix `HashMap` tie-order first or numbers are meaningless. GO → continue; NO-GO → cheapest-first alternatives in file, re-run. Status 2026-09-13 row #1: NO-GO (EN+NE top-3@4 ON 0.375 < 0.85; EN GO 0.932/0.971, NE passes @full 0.953/0.981; KSPW + lift PASS) — see `GATE_RESULT.md`. Unblocks everything not assuming NE@4.

## Tier-1 — Correctness (parallel after GO)

2. `04-engine-determinism.md` — total-order truncate, 1h clock buckets, bigram backoff 0, personal-OOV index, golden + fuzz harness. Unblocks all measurement.
3. `03-nepali-transliteration.md` — `tr` first-class, `encode(tr)` pipeline change, matras non-emitting, download 2.4M-row HF + Leipzig intersect → 5k+ `w+tr` pack. Unblocks NE gate + `02`. (Tail-freq caveat + scale warning amended in file; see `23`.)
4. `02-layout-global-percat.md` — `resolve(cat)`, `set_default/set_cat_layout`, per-layout FST + precomputed seqs, global + per-tab settings. Depends on 03 for NE default (**now t9-9**, t9-16 opt-in — amended).
5. `01-gestures-action-only.md` — Pad `←` delete / `↑` space+commit / `→` accept / `↓` hide, Tabs scroll, Bar tap/fling, FAB toggle, 12-key override, AT fallback. UI-only, needs `02` layout ids for per-cat deltas. (QWERTY-fling amendment: flings call same `onQwertySpace/onDelete` as keys — see `22`.)
6. `22-qwerty-parity.md` — literal slot + corrections + confidence (additive engine), space/punct/double-space/autocaps state machine, backspace-restore, QWERTY letter-graph scorer, `Corrections & suggestions` setting. T9 behavior untouched. Needs `04` (determinism) + `02` (layout ids); parallel with 2-5.
7. `17-single-press-fallback.md` — synthetic `key_label` inject `len<=2` (scoped `!qwerty`, truncate-exempt, no-learn) + short-exact boost + cache-key version. Needs `04` + `02`.
8. `18-strip-muscle-memory.md` — chips as view of core union, space-commits-top, backspace cascade, candidate preview (no digit leak), leftmost-#1 ordering. Needs `17`.
9. `23-multilingual-v1.md` — NE→t9-9 default + gate re-run, user-wordlist `tr`/`alt`, learn-path `tr` lookup, Roman-commit toggle, tail-freq low-confidence. Needs `03` + `02`; parallel with 6-8.

## Tier-2 — Performance (after Tier-1)

10. `05-suggest-optimization.md` — FST posting + `StartsWith` automaton, generative neighbors, heap top-30, alloc kills, drop `suggest` from `learn`/`reject`, prefix cache (QWERTY key `(layout,raw)` — amended), quantized scores. Re-bench on 6613 + 10k personal.
11. `06-persistence-power.md` — single handle + WAL/NORMAL, dirty-set 2s flush, ring-cap session log, settle 10k LFU vs 20k LRU. Power trace after.
12. `14-db-indexing.md` — index set lands after `06` single-handle (before: wasted on DELETE+reinsert). `context_counts` DDL serves `13` P1.

## Tier-3 — Extension (after Tier-2)

13. `07-code-math-palettes.md` — keep Tier-0 vocab for all tabs; Tier-1 palette behind `CategoryProvider` for `js/rust/html/math` only. `words/ne/emoji/numbers/medical` stay pure.
14. `12-snippet-vs-learn-gates.md` — snippet exempt (`tab != numbers` frozen 2026-09-13), learn stays `privacy.learn`. Lifecycle owner for the 2-min buffer (`07` points here).
15. `08-dict-stack-ordering.md` — drag-drop 10-90 UI, enable/layout/learn badges, placement popup, order round-trips export. Depends on palette priority-vs-availability rule (`07`).
16. `11-incognito-clipboard.md` — learn always local; incognito is the off-switch; clipboard with pins + TTL (**friend-default OFF/24h** — amended, see `24`).
17. `13-temporal-contextual-suggestions.md` — P1 8-bucket re-rank after prefix + P2 routine row (never empty-strip push); P3 deferred. Needs `14` index + `12` gate.
18. `15-voice-typing.md` — system engine FIRST (ship for friends: zero audio retention), optional API engine second. After Tier-2 (needs `06` lifecycle + `11` gates).
19. `19-settings-ia.md` + `20-dictionaries-sheet.md` — 4 sections + search + quick-sheet; `medical` canon (single owner: `20`).
20. `21-onboarding-friend-ready.md` — trust pre-frame + layout choice + trial + tabs + graduation; skippable + progressive coach.
21. `24-friend-ready.md` — emoji grid + Play listing + field test close the loop before inviting friends.
22. `16-docs-refresh.md` — living-docs wording task. Anytime after Tier-0, land before release.

## Tier-4 — Spikes (any time after Tier-0, time-boxed)

23. `09-research-open-questions.md` — weights grid-search, `V` stability, alias caps, ephemeral snippet helper, structural math tasks, longitudinal/power. Feed back into tiers; don't block Tier-1/2.

## What stays deferred

- `sync.rs` merge/recovery (design frozen `docs/SYNC.md`, unbuilt), `.kbpack` zip + WASM sandbox + SHA UI, `uniffi-bindgen 0.32` regen + `StubPredictor` replace, JDK assemble + on-device smoke. Platform (iOS companion split, TV/VR gaze) after core gate — keyboard layout is the test harness, not the product.
- Gesture-typing (glide): stays non-goal. Neural LM rescorer: revisit only if vocab/latency budget changes (`09` #6, `22`).
- Auto-detect / simultaneous multilingual union: gated until per-tab NE-8000 clears gate (`23` step 5).
