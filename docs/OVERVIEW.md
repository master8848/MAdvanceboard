# MAdvanceboard — Overview

**What:** open-source Android keyboard like Gboard: configurable pad
(9-key T9 default, 12/16/QWERTY per category) + prediction engine +
category tabs + QWERTY fallback.
**Why:** research keyboard exploring fast typing on few keys with
on-device prediction, fully offline. 9-key is the default research
vehicle; symbols/numbers/emoji live in category tabs + action gestures,
not stacked QWERTY mode-switches.
**How:** Rust core engine (`core-rust/`, crate `kbcore`) + Kotlin IME
(`android/`) + vocab packs (`packs/`).

Build: `just test-core` / `just validate-packs` / `just android-assemble`.

See: `SPEC.md` (normative), `docs/ENGINEERING.md` (architecture),
`docs/BUILD.md` (prerequisites + troubleshooting).
