# Field Test Checklist — WhatsApp / Viber / Messenger (plan/24)

Status: **NOT-RUN** — no device run claimed. Run on a mid-range
minSdk-26-class device, release (minified) build, before inviting anyone. Zero crashes is the bar; everything else is
notes for the next Tier-2 pass.

## Per app (WhatsApp, Viber, Messenger)

- [ ] Install → onboarding Enable→switch <2 min (stopwatch), warning
  pre-frame read, switch-back instructions seen.
- [ ] Send 3 EN messages (QWERTY default): suggestions show, space
  commits, ⌫ behaves, no stuck composing text.
- [ ] Send 3 NE messages (ने tab): Roman typing surfaces Devanagari
  (or the current `23` behavior — record what happened, not what should).
- [ ] Emoji picker: 🙂 opens grid, recent appears after first pick,
  search filters, pick commits into the field.
- [ ] Tab strip: only EN/ने/😀/123 visible by default; "Show code tabs"
  reveals the rest after a field restart at most.
- [ ] Incognito/password: password field shows the dark strip + 🎭 (or
  current indicator); nothing learns (spot-check personal export empty
  of the typed secret).
- [ ] Clipboard: default OFF (panel entry says where to enable); enable
  → copy in-app → paste works → 24h preset marked ★.
- [ ] Rotate / background / kill-and-return mid-word: no crash, no
  leaked composing text in the next field.
- [ ] Voice (gated on friend-ready #6 — currently BLOCKED, service
  wiring not built): airplane-mode retry without re-speak (offline retry
  reuses kept text per `VoiceInputPolicyTest`); zero residue after close
  (`close()` wipe + `cacheDir` sweep on-device, none claimed yet).

## Stability + power

- [ ] Zero crashes across all three apps (logcat `AndroidRuntime`
  clean for the session).
- [ ] No battery regression impression: 1 h mixed-typing day feels
  normal (formal Tier-2 trace in `06` still owed — this checklist is the
  friend-gate, not the power verdict).
- [ ] IME-process slim: no ANR on field start; strip appears with the pad.

## Reporting

File failures as: app + step + expected + actual + logcat snippet.
"Report a problem" in onboarding graduation → Settings is the
friend-facing path; testers should use it once to prove the path works.
