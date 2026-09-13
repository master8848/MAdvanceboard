# 04 — Deterministic Engine (Slow OK, Never Different)

User rule: feel a little slow is fine; suggestions must not change based on how long engine ran. No wall-clock cutoff, ever.

## Invariants

- Same `(dict set, ctx, digits, tab, layout, personal snapshot, now-hour)` → byte-identical `Vec<Suggestion>`. Slower devices do same work, just slower.
- Snippet 2-min buffer (`12`) is part of the visible strip, so it is part of the key: `expires_at` uses the same 1h-quantized `now` (`12` lifecycle), keeping the tuple invariant intact.
- Budget = fixed counts only: `MATCH_CAP=200 → HEAP_TOP=30 → return limit` (`SPEC.md:55`, `core-rust/src/stack.rs:364`). Never `Instant::now()`-gated early exit. Verified: no deadline exists today — keep it that way.
- Scoring path single-threaded, no rayon. `partial_cmp(...).unwrap_or(Equal)` must never see NaN: inputs can't NaN today (`log10(≥1)`, `exp` bounded) — add `debug_assert!(!score.is_nan())` + golden-vector test. Keep tie-break `score → len → lexicographic → priority` (`core-rust/src/stack.rs:409`) — already deterministic, must be total.

## P0 fixes (correctness before speed)

1. **Nondeterministic 200-truncate:** `core-rust/src/stack.rs:364-366` collects `HashMap::into_values()` (RandomState order), stable-sorts by `freq_base` only, truncates. Equal-freq ties → different 200-set per run. Fix: total-order before truncate `(freq desc, word asc, lang asc)` or `BTreeMap`. Same for final heap (already total — keep).
2. **Clock quantization:** `suggest_inner` calls `now_ts()` once (good) but `recency=exp(-dt/7d)` (`core-rust/src/rank.rs:52`) flaps per-second. Quantize `now` to 1h buckets at `Predictor` level, pass in. Stable within hour, still decays across days (intended), never runtime-sensitive.
3. **Bigram cold-start:** `core-rust/src/rank.rs:69` returns `log10(1/V)` when `count_prev==0` (e.g. −4 for V=10k), penalizing all candidates and drifting as `V=entries.len()` grows (`core-rust/src/personal.rs:172`). Return `0.0` backoff when `count_prev==0` (SPEC "backoff 0"). One-line, big new-user win.
4. **Personal-OOV invisible:** comment claims personal-only words participate; loop `core-rust/src/stack.rs:309` iterates only `base+extensions`, personal only overlays `merged`. Learned QWERTY/OOV words never suggest. Add personal seq index to union (see `05-suggest-optimization.md` #7).
5. **Harness:** golden vectors per layout (`digits,ctx,tab → top-5 words+scores`) + seeded fuzz asserting byte-identical repeats. Catches future HashMap/time regressions.

## Non-goals

- No per-keystroke SQLite I/O, no Room on IME process, no background thread faster than 2s. No neural LM — n-gram + recency dominates at 10k vocab + 50ms budget.
