# Plugin SDK — code, privileged

A plugin executes code inside (or beside) a keyboard that sees personal
data. That makes this SDK privileged and **second choice**: if your
feature is words in a category, ship a vocab pack (`docs/VOCAB_SDK.md`)
instead. The separation is a security decision — see §4.

Status: the trait below is the contract
(`android/plugin-api/.../com/kb/plugin/`). No dynamic code loading exists
on Android today: plugins ship **dict only**; the host never locks the
loader (any pack installable via file/URL, store optional never
exclusive).

## 1. `CategoryProvider.candidates()` contract

`CategoryProvider.kt` (verbatim, 12 lines):

```kotlin
package com.kb.plugin

/** Static dictionary pack. No arbitrary code: data + optional tokenizer only. */
interface CategoryProvider {
    val id: String
    val displayName: String
    val priority: Int
    val learnEnabled: Boolean

    fun candidates(query: SuggestionQuery): List<SuggestionItem>
    fun contains(word: String): SuggestionItem?
}
```

Rules:

- `candidates(query)` is called **per keystroke** with
  `(seq, contextPrev, activeCategory, limit)` and returns ranked items.
  Keep it pure, fast (<50 ms pattern per `plan/07`), no I/O, no network,
  no personal-store read. It receives the query and returns words —
  nothing else.
- `contains(word)` answers placement ("where is this word from"):
  backing `packId • freq • accepts` for the long-press popup.
- `priority` is a hint `10–90`; the user reorders. `learnEnabled`
  mirrors the manifest `privacy.learn` flag (`false` for `numbers`,
  `math`-style non-learnable categories).

## 2. `Models` (`Models.kt`, verbatim fields)

```kotlin
data class SuggestionQuery(
    val seq: String,                 // T9 digit sequence, e.g. "386"
    val contextPrev: String? = null, // previous word (bigram context)
    val activeCategory: String = "words",
    val limit: Int = 30              // max items to return
)

data class SuggestionItem(
    val word: String,                // surface form, e.g. "function" / "❤️"
    val seq: String,                 // digit sequence it matches
    val freq: Long = 1L,             // base count, Zipf 1..1e6
    val category: String = "words",  // display cat
    val lang: String = "en",
    val sourcePackId: String? = null // placement: which dict shipped it
)

data class CommitContext(
    val word: String,
    val activeCategory: String,
    val isPasswordField: Boolean,    // true => suppress learning, always
    val timestampMillis: Long = System.currentTimeMillis()
)
```

## 3. `CategoryRegistry` (manifest loading)

`CategoryRegistry.kt` loads one manifest per category from
`assets/categories/<id>.json`:

```json
{
  "id": "pets",
  "displayName": "pets",
  "priority": 20,
  "langs": ["en"],
  "description": "Pet breed names",
  "privacy": {"learn": true}
}
```

- `learn` nests under `"privacy"`; a flat `"learn"` is also accepted.
- Missing file → `loadOne` returns `null` (pack skipped, base keeps
  working). Unknown fields are ignored.
- Real example: `android/ime/src/main/assets/categories/math.json`
  ships `"priority": 30`, `"langs": ["en","latex"]`,
  `"privacy": {"learn": false}`.

## 4. Separation rationale — security decision (core requirement)

Threat model: a keyboard observes **every keystroke**. Personal surface
in this repo: `personal.jsonl` (learned words + counters), `bigrams.jsonl`
(personal bigram counts), `snapshot-<ts>.gz` + `wal.log` (recovery chain),
`session-<date>.jsonl.gz` + `log.jsonl` (accepted/rejected events with
`{ts, seq, ctx, chosen, shown, action}`), and live typed text including
password fields (`CommitContext.isPasswordField`).

A code-execution plugin runs adjacent to that surface, so it gets
**ambient access** to it. Worst case of getting this wrong: silent,
unlimited exfiltration of everything the user ever typed. Hence the rule:

> **If only vocab is required, use only vocab (data-only pack, no code).**

Capability matrix:

| Capability | Vocab pack | Plugin |
|---|---|---|
| Ranking weight (`freq` sum, priority `10–90`) | ✅ | ✅ (via returned items) |
| Display `cat` on its words | ✅ | ✅ (`SuggestionItem.category`) |
| Personal-learn overlay on its words | ✅ (`privacy.learn`, default on) | ✅ (`learnEnabled`) |
| Placement info (`packId • freq • accepts`) | ✅ (via `contains`) | ✅ (via `contains`) |
| Read personal store (`personal.jsonl`, bigrams, session log) | ❌ **never** | ❌ **never directly** |
| File / network I/O | ❌ **never** | ❌ **never** (no I/O, no network) |
| Arbitrary code on Android | ❌ **never** | ❌ static data + optional WASM tokenizer only |
| Query surface | none (engine matches `seq`) | narrow: one `candidates(seq, ctx, limit)` call per keystroke, returns ranked items |
| Password-field text | never sees it | never sees it; `isPasswordField` suppresses learning |
| `paid` flag effect | informational only | informational only — loader never gates |

Signing / SHA review: signature is **optional**. At install (Settings →
Packs → `+ .kbpack / URL`) the host shows the **SHA-256**; unsigned
packs install **with a warning**. Verify the hash out-of-band before
enabling. The host keeps the loader open: any pack installs via
file/URL; a store is optional, never exclusive — so a malicious or
overpriced plugin can always be routed around, and a vocab pack can
always replace a plugin where only words are needed.

## 5. Sandbox rules

1. Static data + optional WASM tokenizer (`tokenize(word)->seq`) only.
   No arbitrary code on Android.
2. Dynamic plugins (including paid ones) implement the same trait but
   ship **dict only**.
3. The host app **never locks the loader**: `paid` never gates loading.
4. Signature optional, with an explicit unsigned warning at install.
5. A plugin that needs I/O, network, or personal-store access is
   rejected by design — restate the feature as vocab or it does not ship.

## 6. Worked example: minimal plugin (`pets`)

The plugin ships the same dict as `docs/VOCAB_SDK.md` §4 plus a manifest
and (reference) provider. Every command below was run; outputs verbatim.

**Step 1 — dict.** Reuse the vocab dict:

```sh
cp /tmp/vocabdemo/dict.jsonl.gz /tmp/plugindemo/dict.jsonl.gz
```

**Step 2 — manifest** (`/tmp/plugindemo/kbpack.json`, note
`"paid": true` — informational only, loader ignores it):

```json
{
  "id": "com.ex.pets-paid",
  "version": "1.0.0",
  "kind": "vocab",
  "name": "Pet Breeds Plus",
  "license": "Proprietary",
  "paid": true,
  "langs": ["en"],
  "categories": ["pets"],
  "priority": 20,
  "dict": ["dict.jsonl.gz"],
  "minCore": "0.3.0"
}
```

Category manifest (`assets/categories/pets.json` shape, verified to
parse with `privacy.learn: true`):

```json
{
  "id": "pets",
  "displayName": "pets",
  "priority": 20,
  "langs": ["en"],
  "description": "Pet breed names",
  "privacy": {"learn": true}
}
```

Verification run:

```text
paid flag loads (informational only): True
privacy.learn: True
```

**Step 3 — provider skeleton** (reference — mirrors
`CategoryProvider.kt` exactly; no compiler is invoked because dynamic
code does not load on Android, rule §5.1):

```kotlin
package com.ex.pets

import com.kb.plugin.*

class PetsProvider(private val words: Map<String, SuggestionItem>) : CategoryProvider {
    override val id = "pets"
    override val displayName = "pets"
    override val priority = 20
    override val learnEnabled = true

    override fun candidates(query: SuggestionQuery): List<SuggestionItem> =
        words.values
            .filter { it.seq == query.seq }           // exact seq only; engine adds neighbors
            .sortedByDescending { it.freq }
            .take(query.limit)

    override fun contains(word: String): SuggestionItem? = words[word]
}
```

Contract parity check (run against the real interface):

```sh
grep -n "fun candidates\|fun contains\|val id\|val priority\|val learnEnabled" \
  android/plugin-api/src/main/java/com/kb/plugin/CategoryProvider.kt
```

Expected output:

```text
5:    val id: String
7:    val priority: Int
8:    val learnEnabled: Boolean
10:    fun candidates(query: SuggestionQuery): List<SuggestionItem>
11:    fun contains(word: String): SuggestionItem?
```

Every member the skeleton overrides exists on the interface; no member
is missing.

**Step 4 — bundle + hash** (same envelope as vocab; the reviewer
compares this hash at install):

```sh
(cd /tmp/plugindemo && zip -q pets-paid.kbpack kbpack.json dict.jsonl.gz \
  && unzip -l pets-paid.kbpack)
sha256sum /tmp/plugindemo/pets-paid.kbpack
```

Expected output:

```text
Archive:  pets-paid.kbpack
  Length      Date    Time    Name
---------  ---------- -----   ----
      259  09-12-2026 09:22   kbpack.json
      193  09-12-2026 09:22   dict.jsonl.gz
---------                     -------
      452                     2 files
d9e22515fe3fefbbc1cae978afcaa63a0132cc09cdc7cd890a6b807edc89374c  /tmp/plugindemo/pets-paid.kbpack
```

**Step 5 — install.** Settings → Packs → `+ .kbpack / URL` → select
`pets-paid.kbpack` → confirm SHA-256 matches `d9e22515…` → accept the
unsigned warning → Enable, set priority `10–90`. `paid: true` changes
nothing about loading.

## 7. Failure modes

- Vocab-side failures (F1–F7) apply to the dict half — see
  `docs/VOCAB_SDK.md` §6.
- **P1 — `candidates` does I/O / reads personal store / touches
  network.** Rejected at review; no API for it is exposed to the
  provider. Restate as vocab.
- **P2 — manifest missing.** `CategoryRegistry.loadOne` returns `null`;
  the pack is skipped, base keeps working. No crash, no fallback load.
- **P3 — unsigned bundle.** Installs with a warning, never blocked.
  Verify `sha256sum` out-of-band before enabling.
- **P4 — `paid: true` expected to gate.** It does not — informational
  only. Monetization lives outside the loader by design (open loader,
  §4).
