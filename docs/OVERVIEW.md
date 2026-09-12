# MAdvanceboard — Overview

**What:** open-source Android keyboard like Gboard: 9-key T9 pad +
prediction engine + QWERTY fallback.
**Why:** research keyboard exploring fast typing on few keys with
on-device prediction, fully offline.
**How:** Rust core engine (`core-rust/`, crate `kbcore`) + Kotlin IME
(`android/`) + vocab packs (`packs/`).

Build: `just test-core` / `just validate-packs` / `just android-assemble`.

See: `SPEC.md` (normative), `docs/ENGINEERING.md` (architecture),
`docs/BUILD.md` (prerequisites + troubleshooting).
