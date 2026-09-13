# 24 — Friend-Ready: Blockers, Checklist, Alternatives

> NEW 2026-09-13. End goal: ask any non-technical friend to install and daily-drive for messaging/everything. This file ranks what blocks that and defines done.

## Top 10 blockers (severity order)

1. **9-key T9 default; friends expect QWERTY+autocorrect.** Whole product is `43556→hello`; QWERTY is FAB fallback only. Fix: `22` parity + QWERTY default option (`21` step 2).
2. **Install friction: sideload + 2-step enable + scary warning.** No Play track. Fix: Play internal/closed-test track; `21` step 1 pre-frames warning + auto-advance + switch-back instructions.
3. **Trust gap.** BIND-only / no-`INTERNET` claim has no verifiable artifact. Fix: Play Data Safety listing + in-app audit link (`PRIVACY.md`); reproducible/F-Droid build later.
4. **No voice typing.** `15` unbuilt; mic is the fallback when typing fails. Fix: ship system-engine path first (`15` §1: `isRecognitionAvailable()`, `EXTRA_LANGUAGE` from tab, `PREFER_OFFLINE`, zero audio retention).
5. **No visual emoji picker.** `emoji.json` ~200 keyword rows (`hea→❤️`) is undiscoverable. Fix: scrollable grid + recents + search (WhatsApp parity) before asking friends.
6. **Missing message basics + weak undo.** No autocapitalize, no spell underline, OOV needs ≥2/7d (too slow for names/slang), single-level fling undo only. Fix: `22` (caps, double-space, backspace-restore) + general strip undo.
7. **Nepali + code-switching vs WhatsApp reality.** Manual `E↔ने` per word; Roman-Nepali Latin output missing; cryptic badges `E/ने/JS/Rs…`. Fix: `23` V1 + friend-default visible tabs `EN/ने/😀/123` only.
8. **Dev tabs pollute default strip.** `DEFAULT_CATEGORIES` shows `JS/Rs/HTML/∑` to messengers. Fix: hide code tabs by default (`19`).
9. **Clipboard defaults scare.** 7-day default, 100-item ring reads as "stores everything incl. OTP". Fix: friend-default OFF or 24h + visible incognito indicator; 7d stays power-user opt-in (`11` amended).
10. **Settings maze + stability unknowns.** 8 pages, raw tuning sliders, per-page Refresh, JSON category flow; 13MB unminified debug APK; no Tier-2 power/latency trace. Fix: `19` IA + minify/shrink on + WhatsApp/Viber/FB-Messenger field test + power trace before invites.

## Minimal friend-ready checklist

1. Signed distributable (Play internal track at minimum) + EN/NE listing + screenshots.
2. Enable→switch <2 min (`21`).
3. QWERTY default; 9-key opt-in (`22`, `21`).
4. EN+NE messaging without tab dance (`23` steps 1+4), visible tabs only.
5. Emoji grid + recents.
6. System voice, mic hidden in password/numbers (`15`).
7. Autocap, double-space period, backspace-restore, general undo (`22`).
8. Zero crashes across WhatsApp/Viber/Messenger field test + no battery regression (Tier-2 trace done, IME-process slim).
9. Clipboard OFF/24h for friends; incognito visibly indicated; one-sentence retention story.
10. One-tap "report problem / switch back" path.

## What in current plan hurts this (fixed by this reorg)

- `10` ordered voice after Tier-2 perf and docs last → reordered: voice-system + emoji + docs ride with QWERTY, not after (`28`).
- Code tabs default-on; manual Refresh everywhere; JSON custom-category flow; exposed tuning sliders; 7-day clipboard default → friend-defaults in `19`/`20`/`11`-amendment.
- `21` 5 linear steps risk dropout → skippable wizard + progressive in-keyboard coach split (trust fear needs the wizard; tips don't).

## Alternatives decided

- **Onboarding:** blocking wizard (enable→switch→trust→layout→done, ≤4 taps) AND progressive coach (emoji after 10 commits, voice after 20). Wizard alone gets skipped; progressive alone leaves install fear unaddressed.
- **Voice:** system `SpeechRecognizer` only for V1 (zero audio retention story). Custom-API engine (`AudioRecord` + `INTERNET` + key storage + spill lifecycle) deferred — adds permission, battery, Play-review, and privacy questions.
- **Play listing (before invites):** signed AAB, icon + feature graphic + screenshots (EN+NE+emoji), EN (+NE) descriptions, content rating, Data Safety form (no data collected — justify with BIND-only manifest + `PRIVACY.md`), privacy-policy URL, target-SDK/64-bit compliance, closed-test track, crash-report opt-in.
