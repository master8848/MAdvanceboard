# User Guide — 9-Key Predictive Keyboard

## 1. How 9-key typing works

Each key emits one digit (`1`–`9`). The encoder maps a keypress run to a
sequence, e.g. `hello` → `43556`. The engine looks the sequence up in every
enabled dictionary (exact match + 1-edit neighbor for fat-finger, cost 0.4)
and ranks candidates (see `TUNING.md`).

Key map (SPEC §1):

| Key | Latin | Devanagari initial | Long-press |
|---|---|---|---|
| 1 | `.,?!'` | `। ॥` | `1 + - × / ( )` |
| 2–9 | ABC…WXYZ | k/c/retroflex/dental/p/y-s/vowel series | digit insert |
| 12-key row only | — | — | `Sym \| Space/0 \| ⌫/⏎` |

Rules:

- Key `1` is punctuation-commit: tap cycles `. , ? !`, breaks the word, does
  not enter the prediction sequence.
- Long-press any alpha key inserts its digit (e.g. hold `2` → `2`).
- Key `9` vowels: Nepali matras normalize to the base vowel for lookup
  (`कि` → `29`).
- In 9-key mode there is no bottom row: swipe-left = delete, swipe-up =
  space, long-press pad = symbols. The 12-key toggle adds the bottom row
  explicitly.

## 2. Category tabs

Tabs filter dictionary + key labels (SPEC §0): `numbers | EN | NE | js |
rust | html | emoji | math | ★personal`.

- Active tab boosts its words (`CategoryBoost`, see `TUNING.md`).
- Tabs are user-reorderable (Android `TabRow`).
- `numbers` is a category, not a mode: selecting it remaps the pad to
  `1–9,0` digit-commit with no prediction.
- `★personal` shows only learned words.

## 3. Suggestion bar + expand-all

- The bar shows 3 inline candidates + `∨` (expand-all).
- Expand-all opens a bottom sheet with up to 30 items, paged.
- Candidates with score `S < -0.5` are hidden unless expand-all is open.
- Long-press a candidate → popup with **placement**: `packId • freq •
  accepts` (which dict the word came from), plus actions **Pin / Block /
  Info**.

## 4. QWERTY toggle

- Floating button bottom-right switches Pad to a full QWERTY view.
- The prediction engine is shared; only the key→sequence encoder changes.
- Use QWERTY to type out-of-vocabulary (OOV) words. Committing an OOV word
  ≥2 times in 7 days promotes it to the personal dictionary
  (`freq_personal = accepts`).

## 5. Numbers category

Select the `numbers` tab: keys commit digits directly, no prediction, no
learning (`privacy.learn: false`). Same digits are reachable in alpha mode
via long-press.

## 6. Placement popup (long-press)

Long-press any candidate to see where it comes from and what the engine
knows: source pack id, base frequency, personal accepts. This answers "why
is this word suggested?".

## 7. Block / forget word

- **Block:** long-press candidate → Block. Writes a tombstone
  `{del: true}`: hidden everywhere, exempt from LRU eviction and cap,
  replicated on sync, never resurrected except by explicit unblock.
- **Forget (unblock):** Settings → Personal dictionary → search word →
  Remove block.
- Reject signal is automatic too: deleting a committed word within 5 s or
  picking another candidate increments `rej` (decayed over 30 d).

Related: `PRIVACY.md` (what is never learned), `SYNC.md` (how blocks
sync), `TUNING.md` (ranking weights), `EXTENSIONS.md` (adding packs).
