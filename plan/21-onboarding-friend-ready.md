# 21 — Onboarding: 5 Steps, Friend-Ready

> Split 2026-09-13 from old `18-settings-ia-reorg.md`, expanded with trust + layout-choice + graduation steps (research: friend-adoption blockers).

## Problem

Current onboarding (`OnboardingActivity.kt:53-90`) is enable-only, 3 static steps, no auto-advance, no warning pre-frame, forces T9 trial. Friends drop at sideload + scary system warning ("can collect all you type").

## Flow — 5 steps, each deep-links to its section

### Step 1 — Enable (deep-link `ACTION_INPUT_METHOD_SETTINGS`, auto-advance via `enabledInputMethodList`)

Pre-frame the system warning first: "Android says this for *all* keyboards; ours has no network permission — verify in Settings→Apps." Link to App page (Android docs requirement).

### Step 2 — Switch (`showInputMethodPicker`) + layout choice

Ask "Familiar QWERTY or 9-key?" upfront and default friends to QWERTY (see `22`). 9-key is opt-in research vehicle, not the greeting.

### Step 3 — Trial (`43556→hello`, live `EditText` trial row)

"Tap once per letter" interactive. Max 3 tutorial slides; keep keyboard View (not Compose) for latency. Lottie deferred — telemetry on step completion, not animation views.

### Step 4 — Tabs = dictionaries (`E`=English, `ने`=Nepali, swipe/long-press)

Friend-defaults: only `EN/ने/😀/123` visible; code tabs hidden (see `19`, `24`).

### Step 5 — Pin/block + graduation

Personal stays on-device (one sentence) → "open a chat, send 3 messages" WhatsApp/Viber graduation → explicit "how to switch back" escape hatch so stuck reports don't become support calls.

## User journeys (design for these)

1. First-run → trial → tabs intro → pin/block.
2. Daily 9-key: tap-per-letter → center best prefix → `...` overflow → `*` cycles (`227 car>cap`) → tap commits → hold `7` for OOV spelling → auto-learns. (`*`-cycle + hold-picker are P1 — need control remap, not in this file.)
3. Short-seq: `7→s` chip, `2→a`, `4→i` first (`17`+`18`).
4. Tab switch: swipe `‹/›`, `∨` expand-all 30, long-press info (`pack•freq•accepts`) → Dictionaries row (`20`).

## Tests

* Enable→switch in <2 min with warning pre-explain + auto-advance + switch-back instructions.
* Skippable; contextual in-keyboard coach after (emoji tab after 10 commits, voice after 20 — progressive, not blocking).

## Order

`17` before `18`-strip-polish. Both before `26` spikes / routine row / voice.
