# Security Policy

## Supported versions

| Version | Supported |
|---|---|
| main (latest) | ✅ |
| older tags | ❌ (please upgrade / open an issue) |

## Reporting a vulnerability

- **Do not** open a public issue for sensitive security bugs.
- Use GitHub private vulnerability reporting:
  https://github.com/master8848/MAdvanceboard/security/advisories/new
- Include: affected version/commit, repro steps, impact assessment.

We aim to acknowledge within 72 hours and will coordinate a fix + disclosure.

## Scope notes

- The IME is intentionally offline (no `INTERNET` permission). Any network
  egress from the keyboard process is treated as a security bug.
- Sideloaded `.kbpack` files are untrusted input — the loader validates
  against `kbpack.schema.json` and shows SHA-256. Parser panics / path
  traversal via packs are in scope.
