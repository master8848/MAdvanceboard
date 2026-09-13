# Play Listing (plan/24, before invites)

## Distributable

- Signed AAB via the release keystore; Play **internal/closed-test**
  track minimum before any friend invite. Debug APKs (`assembleDebug`,
  ~13 MB unminified) are NOT the invite artifact — enable minify/shrink
  for the invite build (plan/24 §10).
- 64-bit compliant (arm64-v8a `libkbcore.so` via `just ndk-libs`);
  target-SDK per current policy (compileSdk/targetSdk 36, minSdk 26).

## Store assets

- Icon + feature graphic.
- Screenshots: EN board + NE board + emoji picker + suggestion strip
  (4 minimum, phone).
- Descriptions EN (+NE translation): one paragraph — configurable pad
  (QWERTY default, 9-key opt-in), offline prediction, EN/NE tabs, no network permission.
- Content rating questionnaire (everyone; no user-generated-content
  exchange, no network).
- Crash-report opt-in note in the listing ("anonymous crash reports only,
  off by default" — no SDK is bundled; Play vitals only).

## Data Safety form (no data collected)

Declare **no data collected, no data shared**. Justification bundle for
review:

- IME manifest declares **only `BIND_INPUT_METHOD`** — no `INTERNET`
  permission in the shell (`android/README.md` privacy notes).
- Sync is opt-in file-based JSONL only (`docs/SYNC.md`); clipboard never
  syncs; password/incognito sessions never learn (`docs/PRIVACY.md`).
- In-app audit link: Settings → Advanced → Privacy/Sync quotes the same
  one-sentence story; `docs/PRIVACY.md` is the canonical text.

## Privacy-policy URL

Host `docs/PRIVACY.md` (or its published rendering) at a stable URL and
link it from the listing + the in-app Privacy row. Reproducible/F-Droid
build is deferred (later trust milestone, not a friend-blocker).
