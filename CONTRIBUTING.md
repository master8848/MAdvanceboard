# Contributing to MAdvanceboard

Thanks for your interest in contributing!

## Quick start

```sh
mise install          # or: just bootstrap
just test-core        # cargo test -p kbcore
just validate-packs   # validate packs/*.json
```

See `docs/BUILD.md` for prerequisites and `docs/ENGINEERING.md` for architecture.

## Ground rules

- This project is licensed under **GPL-3.0-only**. By contributing you agree
  your contribution will be licensed under the same license.
- `SPEC.md` is normative / read-only history — don't edit it to match code.
  Propose spec changes via an issue first.
- Keep the IME offline: no `INTERNET` permission, no network calls from
  `android/ime/`. Sync stays opt-in file-based (see `docs/SYNC.md`).
- Run relevant checks before pushing:
  `just test-core`, `just validate-packs`, `just check-core`.

## Pull requests

1. Fork + feature branch (`feat/...`, `fix/...`).
2. Small, focused diffs with tests where it matters (`core-rust/tests/`).
3. Update docs (`docs/`, `packs/README.md`, `android/README.md`) when behavior changes.
4. CI must pass.

## Reporting bugs / requesting features

Use the GitHub issue templates (Bug report / Feature request). Include:
repro steps, expected vs actual, device/emulator + Android version for IME bugs,
or `cargo test` output for engine bugs.

## License header

New source files should carry:

```
// SPDX-License-Identifier: GPL-3.0-only
// MAdvanceboard — Copyright (C) 2026 Saurav Sanjel
```
