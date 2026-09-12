# Custom categories — a names tab from your own wordlist

User packs (plan/08) turn a pasted/imported wordlist into a first-class
category tab: isolated ranking under the new tab, personal-learn overlay,
enable/disable toggle, priority within 10-90.

**Import-only by design: no built-in names starter list ships.**
There is no in-repo names source to derive one from, so instead of a
token starter file the import path is kept trivial (paste words, save)
and validated loudly.

## 1. Create a names category end-to-end

1. Open **Settings → Custom categories**.
2. Fill the form:
   - `id`: `names` (2-24 chars, `a-z 0-9 _`, must start with a letter;
     built-in ids like `words`, `medical`, `personal` are reserved).
   - `title`: `Names`.
   - `priority`: `50` (any integer 10-90; base `0` and personal `100`
     are fixed — higher wins the display `cat` on collisions).
   - `wordlist` (one `word [freq]` per line, `#` comments allowed):
     ```text
     # family + contacts, most-called first
     ava 9000
     bob 8000
     chrissie 7000
     ```
     Rows without `freq` default to `100`. `freq` must sit in `1..1000000`.
3. Tap **Save category**. The `names` tab appears in the keyboard tab
   strip (before `★personal`).
4. **Restart the keyboard** (re-select it in the system input menu) so
   the engine installs the new pack. Fresh enables/disables also refresh
   the strip live, but engine ranking for a newly added pack needs the
   restart.
5. Type the T9 digits for a name (e.g. `ava` → `282`): the `names` tab
   shows **its own words only** (custom-tab isolation). Accepting a name
   twice learns it into the personal overlay **under `names`**, so it
   promotes on that tab exactly like built-in tabs.

## 2. Import validation (loud, never silent)

Bad rows are rejected with exact per-row errors and **nothing is stored**
unless the whole pack validates. You will see messages of this shape:

| Input | Error |
|---|---|
| `ava 9000` twice | `line 2 "ava": duplicate word` |
| `bob 0` | `line 3 "bob": freq "0" out of range 1-1000000` |
| `*` | `line 4 "*": encodes to empty seq (latin letters required)` |
| `bob 100 extra` | `line 5 "bob": expected \`word [freq]\`, trailing text rejected` |
| empty form | `wordlist is empty: add one \`word [freq]\` per line` |
| id `words` | `category id "words" invalid: reserved by a built-in pack …` |
| priority `5` | `priority 5 out of range 10-90 (base 0 and personal 100 are fixed)` |

The engine install (`add_pack_json`) re-validates independently
(`PackFile::validate`): an unencodable row fails the whole install with
its word index instead of shipping a half-loaded category.

## 3. Enable / disable / delete

- Each saved pack has an enable **Switch**: off hides the tab
  (the pack is skipped at engine install and drops out of the strip).
- **Delete** removes the pack (learned personal words under that
  category stay in the personal dict; blocking still applies).

## 4. Export / import

- **Export** renders one JSON document carrying every pack
  (envelope + `priority` + `enabled`). Copy it somewhere safe.
- Paste it into the import field and tap **Import pasted JSON**:
  every pack validates first — any failure aborts the whole import
  with all per-pack causes, never a partial import.

## 5. Engine notes (for contributors)

- Runtime registration: `Predictor::try_add_user_pack(id, title,
  wordlist, lang, priority)` (plain Rust; mobile installs the same
  envelope via the existing `add_pack_json` FFI).
- Wordlist parsing/validation: `pack::parse_user_wordlist`,
  `pack::build_user_pack`, `pack::check_user_priority`,
  `pack::validate_user_id` (`core-rust/src/pack.rs`).
- Enable toggle: `Predictor::set_cat_enabled` / `is_cat_enabled`
  (UniFFI-exported; the IME tab popup and Settings toggle call them live,
  verified with an `is_cat_enabled` read-back) — disabled cats leave the
  suggest union entirely.
- Word provenance: `Predictor::placement(word)` (UniFFI-exported;
  `pack cat • freq • accepts`, `""` when no pack holds the word) — the
  tab long-press popup shows it live for the last committed word.
- Isolation: `SuggestOpts::policy_for_tab` hard-filters custom tabs
  (any non-empty tab outside the built-in set), so a custom tab shows
  its own words.
- Persistence: custom packs live in `SharedPreferences`
  (`kb_custom_packs`, same lean-IME rationale as `LayoutStore`);
  personal learning stays in SQLite (`:sync`). No new tables.
