# 23 — Multilingual V1: English + Nepali + Others (No Rewrite)

> NEW 2026-09-13. Answers "friends might use English or others": what ships for bilingual/code-switched reality without an architecture rewrite.

## Reality check

- `w+tr` architecture landed and is sound for findability (`pack.rs:29,67` `seq>encode(tr)>encode(w)`; matra fix `mapping.rs:55-65`; 8005-row `nepali.json` v2.0.1). But ranking at scale is NOT solved: 8000-word NE gate scores **0.26 ON / 0.39 OFF** (toy 253-word was 0.94) — 4-prefix buckets avg **5.12 words (max 109)** vs EN 2.42. Crowding, not encoding, is the blocker.
- Tail freqs are wiki-token counts log-scaled with floor 200 (Leipzig automation hit Anubis wall) — ~7700 rows near floor can't break 5-109-way ties. Treat as low-confidence.
- Only 304/8005 rows carry `alt` variants; uncapped IAST→casual generation worsens buckets (`26` #3 cap curve unmeasured).
- Learned words diverge: `learn_with_shown` encodes `mapping.encode_word(&word)` directly (`predictor.rs:333`) — a learned `नमस्ते` gets skeleton `5685`, never pack `tr` seq.
- Today: hard tab isolation per word (`policy_for_tab` neighbor-OFF + `hard_tab_filter`, `stack.rs:600-606`); manual `E↔ने` switch per word. No simultaneous dictionaries, no auto-detect, no Roman-output mode. No Hindi/other pack; user wordlists take `word [freq]` only (`pack.rs:213-276`, `tr:None`) — Latin-only in effect.
- Layout split (NE t9-16 vs EN t9-9) forces two muscle memories; the `tr`-model condition for moving NE back to t9-9 (`layout.rs:351-358`) is now met.

## V1 (5 small moves, no rewrite)

### Step 1 — NE → t9-9 default (keep t9-16 opt-in)

One-line cat default + gate re-run. Kills layout-switch friction for the two biggest tabs. Measured t9-16 win (38→33% collisions) was pre-`tr`; re-measure before keeping divergence.

### Step 2 — `tr`/`alt` columns in user wordlists

Extend `parse_user_wordlist`/`build_user_pack` (e.g. `word [freq] [tr] [alt,…]`), reusing proven `ne` machinery with loud validation. Unblocks Hindi/other friends via sideload *with* transliteration. Cheapest "other language" path.

### Step 3 — Fix learn-path `tr` gap (lookup, no inference)

When learning a Devanagari word already present in any pack, store its pack `tr`/`alt` too. Stops learned/pack divergence for known words; true OOV transliteration stays future work.

### Step 4 — Roman-display commit choice on NE tab

Offer the `tr` string as an alternate candidate (commit-form choice, not a second engine): enables Nepglish/Hinglish-Latin output (`maile khana khaye` in Roman) with zero index change. If cut for scope, explicitly document "NE commits Devanagari only" so friends self-select.

### Step 5 — Mark tail freqs low-confidence

Until a real freq source lands: uniform + curated-head boost for the ~7700 floor-200 tail rows; don't tune weights against them (`26` #1 holdout warning). Do not scale `alt` variants without the `26` #3 cap curve.

## Deferred (gated, not denied)

Auto-detect / simultaneous union (`with_tab_filter(false)` exists as measurement arm, `stack.rs:621`) waits until per-tab NE-8000 clears gate — mixed union on 5.12-avg buckets scores worse, not better. Per-tab gates first (`01`), then a measured union experiment with freq rebalancing. Bad idea to avoid: selling `08` drag-drop priority as code-switch handling (under hard filter it only moves within-tab tie-breaks).

## Tests

* NE t9-9 gate re-run recorded; Roman typing surfaces Devanagari; Roman-commit toggle outputs Latin.
* Hindi sideload list with `tr` matches via Roman digits; block/round-trip holds.
