# Tuning — Ranking Weights, Thresholds, Decay

Engine: `core-rust/src/rank.rs` (`RankWeights::default()`); constants per SPEC §2, tunable in Settings → Tuning (advanced).

## 1. Score formula

```text
S(w) = 1.0*log10(freq_base + freq_personal + 1)
     + 1.2*P_personal(w)        # 0/1 if in personal (scaled by accepts)
     + 0.8*Bigram(ctx_prev, w)  # log10(c+1)-log10(c_prev+V), personal only, else 0
     + 0.5*Recency(w)           # exp(-dt/7d) since last accept
     + 0.3*CategoryBoost(w)     # 1.0 if w.cat == activeTab, 0.5 if ★ tab, else 0
     + 0.2*KeyFit(seq, w)       # 1.0 exact, 0.9 prefix, 0.6 neighbor (core-rust)
     - 1.5*RejectPenalty(w)     # rej/(acc+rej+1), decayed 30d
```

Note: SPEC §2 lists `KeyFit` as `1.0 / 0.6`; `core-rust` adds `0.9` for
prefix matches. Neighbor edit cost is `0.4`.

## 2. Weights table

| Term | Weight | Input | Effect |
|---|---|---|---|
| log-freq | `1.0` | `freq_base + freq_personal` | Zipf base; learned counts stack |
| personal | `1.2` | in-personal 0/1 | learned words outrank base |
| bigram | `0.8` | `P(prev→w)` | context fit, personal only |
| recency | `0.5` | `exp(-dt/7d)` | recently used wins |
| category | `0.3` | tab match | active tab `1.0`, `★` `0.5` |
| key-fit | `0.2` | exact/prefix/neighbor | exact `1.0`, prefix `0.9`, neighbor `0.6` |
| reject | `-1.5` | reject ratio | typo/blocked demotion |

Tie-break: shorter word → lexicographic → pack priority. Lookup unions
the stack (base prio `0` < extensions `10–90` < personal `100`), dedupes
on `(norm(word), lang)`, caps at 200 matches, heaps top-30.

## 3. Tuning settings (what the UI actually exposes)

Settings → Advanced → Tuning exposes five gesture-detection sliders
(fling distance / velocity / axis ratio / word step / long-press) — the
ranking weights in the table above are engine reference values
(`rank::RankWeights::default()`), not per-user sliders. Weight changes
ride the grid-search spike (`docs/SPIKES.md` #1), not hand-tuning.
Settings → Advanced → Reset restores board/layout defaults (global
layout, per-tab overrides, QWERTY-first, code tabs hidden).

## 4. Thresholds

- `S < -0.5` → hidden unless expand-all is open.
- OOV promotion: word committed via QWERTY/raw with `≥2` accepts in 7 d →
  personal with `freq_personal = accepts`.
- Reject event: user deletes a committed word within 5 s or picks another
  candidate → `rej += 1`.

## 5. Decay / cap rules

- `freq_personal *= 0.98` monthly if unused — demotes, never deletes.
- Reject ratio decays over 30 d.
- Cap: 20 k entries, LRU-evicted (SPEC); `core-rust` personal overlay uses
  LFU 10 k via bundled SQLite (`rusqlite`) — decided in code
  (`core-rust/src/personal.rs`), SPEC stays the aspirational target.
- Blocked tombstones are exempt from eviction and decay.

Related: `USER.md` (suggestion bar, block), `SYNC.md` (local-only
export/import, no merge), `EXTENSIONS.md` (pack priority).
