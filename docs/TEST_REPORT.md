# Docs Test Report — docs/ only (2026-09-12)

Scope: `docs/USER.md, PRIVACY.md, EXTENSIONS.md, SYNC.md, TUNING.md`.
Read-only cross-checks: `SPEC.md` §§1–5, `android/README.md`, `core-rust/README.md`, `packs/*.json`.
No edits outside `docs/`. No builds.

## 1. Existence + internal links — PASS (all 5 exist, all refs resolve)

| File | Exists | Outgoing `.md` refs | Resolve? |
|---|---|---|---|
| `docs/USER.md` | yes | `TUNING.md` (L8, L34, L80), `PRIVACY.md`/`SYNC.md`/`EXTENSIONS.md` (L79–80) | all in `docs/` — PASS |
| `docs/PRIVACY.md` | yes | `android/README.md` (L5, cross-dir, exists), `SYNC.md` (L8, L54), `USER.md` (L54) | PASS |
| `docs/EXTENSIONS.md` | yes | `TUNING.md` (L97, L99), `USER.md`/`SYNC.md` (L99–100) | PASS |
| `docs/SYNC.md` | yes | `PRIVACY.md`/`USER.md`/`TUNING.md` (L70–71) | PASS |
| `docs/TUNING.md` | yes | `USER.md`/`SYNC.md`/`EXTENSIONS.md` (L59–60) | PASS |

No broken in-docs links. Code-path refs (`core-rust/rank.rs`, `core-rust/session.rs`, `core-rust/pack.rs`, `core/config.rs`) are informational; see §4 note on stale `core/` prefix.

## 2. Code-fence JSON validity — PASS (with note)

Parsed with Python `json.loads` (whole-fence for objects, per-line for JSONL):

| File | Fences | Result |
|---|---|---|
| `USER.md` | 0 json fences | PASS (nothing to parse) |
| `PRIVACY.md` | 0 json fences | PASS |
| `EXTENSIONS.md` L30–33 (2-line JSONL: `\alpha` + `function`) | 1 fence, 2 lines | PASS per-line; whole-fence parse fails by design (JSONL, as labelled L28 "one JSON object per line") |
| `EXTENSIONS.md` L49–54 (MVP pack object) | 1 fence | PASS whole-object |
| `EXTENSIONS.md` L73–75 (`fun` row), L79–81 (`\alpha` row) | 2 fences | PASS each |
| `SYNC.md` L11–13 (row schema) | 1 fence | PASS |
| `TUNING.md` | 0 json fences (`text` formula only) | PASS |

No broken fences. No fix needed.

## 3. Category IDs — PASS with flags

Canonical tabs per SPEC §0: `numbers | EN | NE | js | rust | html | emoji | math | ★personal`.
Actual pack ids/cats on disk (`packs/*.json`): `numbers, en, ne/NE, js, rust, html, emoji, math`.

| Check | Result |
|---|---|
| `USER.md` L31 tab list matches SPEC §0 verbatim | PASS |
| `EXTENSIONS.md` cats `math` (L31, L80), `js` (L32, L51, L74), `emoji` (L52) | PASS — all in canonical set |
| `SYNC.md` L12 `"cat":"NE"` | PASS (matches SPEC `NE`), but note case drift: `words_en.json` uses `"cat":"en"` (lowercase) vs SPEC `EN` (uppercase); `nepali.json` has `id: "ne"` with rows `"cat":"NE"`. Consistent within each pack, inconsistent case across EN/NE. Flag only. |
| `EXTENSIONS.md` L86–87 showcase `en-base, ne-base` vs disk ids `en` (`words_en.json`), `ne` (`nepali.json`) | FLAG — name drift (trivial, `en-base`→`en`, `ne-base`→`ne`). |
| Task canonical set lists `words` | FLAG — no pack with id `words`; closest is `words_en.json` with `"id":"en"`. If `words` is intended as canonical, `packs/words_en.json` id should be renamed by pack owner (out of docs scope — not changed here). |

## 4. Claims vs SPEC §§1–5 / android + core-rust READMEs

| # | Claim | Docs source | Reference | Verdict |
|---|---|---|---|---|
| 1 | T9 map (key 1 punct-commit, 2–9 Latin+Devanagari, long-press digit, `hello`→`43556`, matra collapse `कि`→`29`, neighbor cost 0.4) | `USER.md` L5–27 | SPEC §1; core-rust README mapping + tests | PASS |
| 2 | Ranking weights 1.0/1.2/0.8/0.5/0.3/0.2/−1.5 + KeyFit exact 1.0/neighbor 0.6 + prefix 0.9 as core-rust addition | `TUNING.md` L8–31 | SPEC §2; core-rust README L48–56 | PASS (divergence explicitly documented L18) |
| 3 | Stack prio base 0 < ext 10–90 < personal 100; dedupe `(norm, lang)`; cap 200/cap top-30; tie-break short→lex→prio | `TUNING.md` L33–35; `EXTENSIONS.md` L23, L95 | SPEC §4; core-rust README | PASS |
| 4 | JSONL schema w/lang/cat/seq/freq/acc/rej/ts/del/id; LWW + max-merge; WAL + hourly compact; HLC ts+deviceId | `SYNC.md` L6–68 | SPEC §5 | PASS |
| 5 | Thresholds: hide `S < -0.5`; OOV ≥2 accepts/7d → personal `freq_personal=accepts`; reject = delete-within-5s or pick-other → `rej+=1`, decay 30d | `USER.md` L44, L53–55, L76–77; `TUNING.md` L45–49 | SPEC §2 (threshold, 5s, thds), §4 (OOV) | PASS |
| 6 | `privacy.learn: false` for `numbers`, `math`; no learning in password fields; no INTERNET; opt-in sync | `PRIVACY.md` L3–28; `USER.md` L59–61 | SPEC (implicit); `android/README.md` L33–40 verbatim match | PASS |
| 7 | Cap: SPEC 20k LRU vs core-rust LFU 10k redb | `TUNING.md` L55–56 | SPEC §4 L98; core-rust README L18 (`LFU 10k, redb`) | PASS — explicitly labelled "SPEC as target, code as current", not a silent contradiction |
| 8 | Tombstone field name | docs use `{del: true}` / `del` (`USER.md` L72, `PRIVACY.md` L32, `SYNC.md` L24) | SPEC §4 L96 says `{deleted:true}` but SPEC §5 L104 example says `"del":false`; `android/README.md` L40 says (`deleted: true`) | **CONTRADICTION (inherited)** — SPEC is self-inconsistent (§4 `deleted` vs §5 `del`); docs consistently follow §5 `del`; android README follows §4 `deleted`. Needs owner decision; docs not changed. |
| 9 | UniFFI version | docs make no version claim | core-rust README: uniffi 0.29 | PASS (nothing to contradict) |
| 10 | Stale code path `core/config.rs` | `TUNING.md` L3; SPEC §2 `core/config.rs`, SPEC §3 `core/src/pack.rs` | disk layout is `core-rust/src/*.rs` per core-rust README | **FLAG (inherited)** — docs repeat SPEC's stale `core/` prefix. Trivial doc-side fix available (see §5). |

## 5. Fixes applied (trivial typos/broken fences only, docs/-scoped)

1. `docs/EXTENSIONS.md` L52 — non-schema stray field in MVP emoji example:
   `{"w":"❤️","seq":"432","key":"hea","freq":9000,"cat":"emoji"}`
   → removed stray `"key":"hea"` (schema rows are `{w, freq?, cat?, lang?, seq?}` per core-rust README L70–74; keyword encoding is already carried by `seq`). Rationale: `key` appears nowhere else in docs/SPEC/pack loader description.
2. `docs/TUNING.md` L3 — stale path `core/config.rs` → `core-rust/src/rank.rs` (weights live in `rank::RankWeights::default()` per core-rust README L58–60; `core/` does not exist on disk).

Not changed (needs owner, out of docs scope or non-trivial):
- Tombstone `del` vs `deleted` (§4 row 8) — SPEC + android README must converge first.
- `en-base`/`ne-base` showcase names vs disk ids `en`/`ne`; EN/NE case drift; `words` vs `en` — pack-manifest ownership.
- No rewrites performed.
