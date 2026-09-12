# 13 — Temporal / Periodic Smart Suggestions (Time-of-Day, Weekly, Monthly)

Owner want: keyboard is integral, sees more than any app — morning "good morning
jaan" to wife, month-end "salary aayo?" ask, Saturday "go out?" — many with
**zero keypress**. Entire app is private-local, so smarter suggestions are
desirable, but keyboard demands absolute minimum CPU/RAM/battery. SQLite
sprinkle is preferred over new engines. Plan only, no code.

## 1. What Gboard / SwiftKey / iOS actually do (no myth)

- **Gboard**: base = on-device LSTM/CIFG NWP + Smart Compose + OTF rescore
  (~2-6M params, 10-30k vocab). Personalization = tiny on-device delta
  (~4KB) from accept/ignore feedback, federated + DP so raw text never leaves.
  Temporal signals exist but narrow: **hour-of-day + day-of-week one-hots as
  features in the query-suggestion triggering model** (show/hide a candidate),
  plus per-app vocab split (WhatsApp slang vs Gmail formal). Training only
  when idle + charging + unmetered (JobScheduler, night). Training cache is
  **SQLite with TTL**. No "Saturday go-out" strip with empty input — zero-prefix
  surfaces are clipboard suggestion row / Smart Reply in notification, not the
  prediction strip.
- **SwiftKey**: per-language packs (2-20MB) downloaded on demand, on-device
  NWP + emoji/context prediction, clipboard history + per-app learning. Same
  pattern: context re-ranks *after* ≥1 char or after a commit, never a
  routine-push on empty strip.
- **iOS**: on-device personalization (contacts, style), Smart Reply / inline
  prediction triggered by committed context, not by clock alone.
- Lesson for kb9: nobody pushes routine phrases on **empty `seq`** in the main
  strip. False-positive cost is too high (strip is small, user is mid-thought).
  They do (a) **re-rank after prefix**, (b) **separate surface for zero-input**
  (clipboard row, smart-reply chip, assistant routine). Copy that split.

## 2. Why zero-keypress is a different problem

- `suggest(ctx, digits, tab)` needs `digits` to cut 5-15k vocab to ~2-16
  candidates at 4 digits (`00:12`). With `digits=""` every word matches —
  ranking is pure prior, noise dominates.
- Periodic examples need different clocks: daily-morning (24h), Saturday
  (7d), month-end (~30d). A single `recency=exp(-dt/7d)` (`rank.rs:52`) cannot
  express all three; adding three clocks triples tuning surface (`09#1`).
- Complexity tax: background clock checks = wakeups = battery (`06`, `android`
  24h budget). Must be lazy (check on keyboard-open, never a timer).

## 3. Design: two surfaces, three phases (cheapest first)

- **Surface A — prefix re-rank (in-strip, after ≥1 key).** Cheapest, always on.
  Adds `+w_temporal * temporal_term` to `rank.rs` score, computed only for the
  ≤200 already-scored candidates (`05#8` pattern). No extra FST scan.
- **Surface B — routine row (zero-keypress, separate UI, owner decision).**
  Mostly **active by default, user-disableable** (Settings → Suggestions →
  `Routines on/off`). When `seq` empty, global suggestion knows little, so it
  shows only bucket-matched routines, never generic predictions. Once user
  starts typing, current-sentence candidates always win: routine is strictly
  **low priority, never steals a top-3 slot**. If in-strip next-prediction
  misses (routine not in top-30 scored), a small `⟳/clock open-button` appears
  at strip end — tap to expand routine row and check if useful. Explicit tap
  to expand, explicit dismiss. Own metric (tap-through, not top-3@4).

Phases:

- **P1 — time-bucketed boost (Surface A, SQLite sprinkle, no new engine).**
  Schema: reuse `session_events.ts` already stored (`session.rs:121`) +
  one new tiny table `context_counts(word, bucket, count)` where
  `bucket = tod(4: night/morning/afternoon/evening) × dow(2: weekday/weekend)`
  = 8 buckets max. Cap: top 500 words only, `COUNT` as u16, total <50KB.
  Update: on accept, `INSERT … ON CONFLICT(word,bucket) DO UPDATE`
  inside existing 2s coalesced flush (`06:9`) — zero extra wakeups.
  Query: single `SELECT count WHERE word IN (≤200 candidates) AND bucket=?`,
  one round-trip per keystroke, integer adds only. Deterministic: bucket from
  1h-quantized `now` (`personal.rs:68`).
  Covers: "morning message wife" (morning bucket boosts `good morning jaan`
  after `4…`), Saturday wording drift. Falsifiable: +top-3@4 on morning subset
  vs no-temporal, no regression on night subset.
- **P2 — routine row (Surface B, zero-keypress, gated, on-by-default).**
  Default ON, one toggle OFF (no per-routine config for MVP). Trigger: keyboard
  opened, `seq` empty, `bucket` matches learned routine (≥5 accepts same bucket
  /14d, precision ≥60% or suppress). Show max 3 chips in routine row only, with
  clock icon + bucket label ("Saturday"). Typing path: routine never reorders
  top-3; if routine word not in current top-30, show `open-button` (clock) at
  strip end so user can test "is this useful when prediction misses." Tap chip
  = commit, long-press = Block ("don't suggest here"), toggle OFF = row + button
  both hidden. No auto-commit ever. Respects incognito/password (hidden),
  respects `numbers` (never). Logged as
  `action=routine_shown/routine_accepted/routine_opened` for `09#7` KSR.
- **P3 — long-period (monthly payday etc., DEFERRED).** Month-end needs ≥2-3
  months of log to separate signal from noise; 7-day clipboard TTL (`11:3`)
  and 5000-event session cap cannot see it. Requires longitudinal export +
  `09#7` week-over-week KSR first. Do not build until P1 ships + P2 precision
  ≥60%. Documented no-build.

## 4. Resource budget (hard caps)

- RAM: `context_counts` fully in RAM as `HashMap<(word,bucket),u16>` (~500×8
  worst-case, real sparse ~1-2k entries, <100KB). SQLite is durability only,
  never read-path (`06:3` rule).
- CPU: temporal term = 1 hash lookup + 1 integer add per candidate, only for
  ≤200 scored. No float unless candidate in top-30. p95 impact target <1ms.
- Battery: no timers/alarms. Bucket computed lazily on `onStartInputView`.
  Flush piggybacks existing 2s dirty-set + hourly WAL checkpoint (`06:9`).
  No extra `WorkManager` job beyond existing daily sweep (`11:3`).
- Privacy: all buckets local, quantized (4×2, never exact minute), never
  exported in sync files unless user opts in (`docs/SYNC.md`1938 = personal +
  bigrams + session only — add `context_counts` to same opt-in bundle, never
  separate upload). Incognito suppresses both read + write.

## 5. Acceptance / falsification

- P1: morning-subset top-3@4 lift ≥3pts, no regression >1pt on other buckets,
  p95 still <50ms, DB growth <50KB at 30 days.
- P2: routine precision ≥60% (accepted/shown), dismiss-rate <20%, zero
  password/incognito leaks in audit. Open-button usefulness: `routine_opened →`
  accepted rate tracked separately — if opened often but rarely accepted, row
  ranking is wrong, not the button. Toggle OFF respected on next open, no residual row.
- Kill rule: if P1 lift <3pts after 14d real use → drop temporal term, keep
  table dormant (no migration, just stop reading). If P2 precision <40% →
  remove routine row, keep Surface A only.

## 6. Relation to other plans

- Needs `14-db-indexing.md` `(word,bucket)` PK + `session_events(ts)` index
  for the 14d routine-mining query — otherwise P2 scan is full-table.
- Needs `12-snippet-vs-learn-gates.md`: `temporalNow = learnNow`
  (same gate; incognito/password never read/write temporal).
- Metric split (`00:5`): tuning stays `top-3@4`, longitudinal stays `KSR`,
  routine row gets its own precision metric — do not mix.
