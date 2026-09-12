# kb9 Plan Ground — Multi-Tier Research + Build Plan

Core engine is the research. Keyboard UI is the test harness. Platform later.

Source: 5 research tracks (encoding gate, gesture-as-action, Nepali transliteration, code/math palettes, deterministic optimization) + repo audit (`SPEC.md`, `core-rust/src/stack.rs`, `mapping.rs`, `rank.rs`, `personal.rs`, `predictor.rs`, `layout.rs`).

## Tiers

- **Tier-0 Gate:** `00-gates.md` — KSPW≤1.5, top-3@4≥85%, lift≥10pts. GO/NO-GO before all else.
- **Tier-1 Correctness:** `01-gestures-action-only.md`, `02-layout-global-percat.md`, `03-nepali-transliteration.md`, `04-engine-determinism.md` — must be right, else everything downstream reshapes.
- **Tier-2 Performance:** `05-suggest-optimization.md`, `06-persistence-power.md` — lean + snappy, deterministic caps, no time-cutoff.
- **Tier-3 Extension:** `07-code-math-palettes.md`, `08-dict-stack-ordering.md` — which tabs reuse `suggest()` vs palette, drag-drop priorities.
- **Tier-4 Open:** `09-research-open-questions.md` — novel/unsure, needs spikes.
- **Order:** `10-execution-order.md` — dependency graph. User compiles later; plan only.
- **Privacy switch:** `11-incognito-clipboard.md` — learn always local (no off-toggle); incognito is the off-switch; clipboard history with pins + 7-day TTL.
- **Gates split:** `12-snippet-vs-learn-gates.md` — snippet exempt (`tab != numbers`, 2-min RAM, delete-after-expand); learn stays `privacy.learn`.
- **Temporal:** `13-temporal-contextual-suggestions.md` — Gboard/SwiftKey pattern (re-rank after prefix + separate routine row, never empty-strip push); P1 8-bucket SQLite sprinkle, P2 routine row, P3 monthly deferred.
- **DB indexes:** `14-db-indexing.md` — today only PKs, zero secondary; index set lands after `06` single-handle.

## Rules across all tiers

1. No swipe-typing. Gestures = actions only (switch category, accept suggestion, QWERTY toggle, delete/space).
2. Deterministic: same `(dict, ctx, digits, tab, layout)` → same results. Allow slightly slow, never change results based on runtime. No wall-clock cutoff.
3. Global layout + per-category override. 9/12 for simple, 16 for Nepali/complex. Single `layout_id` resolved per `cat`.
4. Nepali via downloaded `w+tr` wordlists (Devanagari display + Roman `tr` for `seq`), not hand-made 253 words. Matra-only seqs are banned.
5. Each file = one step. Implement in `10-execution-order.md` sequence.
