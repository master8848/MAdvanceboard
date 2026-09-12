# 07 — Local-Scope Token/Snippet Packs for Code Categories

Resolves `INTENT.md:24` risk. Job is narrower: make *characters* cheaper, not *logic*. No multi-token intent, no next-line, no tiny LM (IDE territory). Shared `suggest()` Tier-0 for all tabs; Tier-1 local table + macro snippets for `js/rust/html` only. UI composition, not core fork.

## Taxonomy

- `words`, `ne`: YES pure `suggest()` (`hello→43556`). Bigram+personal+recency suffices.
- `emoji`: YES pure (`hea→432→❤️` `seq` override). Closed set. Skin-tone strip deferred.
- `numbers`: YES trivially — digit-commit, no prediction (`SPEC §1`).
- `medical` (545 terms): YES pure flat domain vocab. Benefits 10-90 priority + personal learn. Phrase templates out-of-MVP.
- `★personal`: overlay, not engine. OOV `≥2 accepts/7d` promotion (QWERTY-learn `SPEC §4`) feeds code identifiers as `cat=js/rust` (+1.2 personal + bigram only when that tab active).
- `js`/`rust` EPHEMERAL (owner decision, no index): no file/project scan, no cross-file state, no LM. If user types `if` suggest `if (`; once typed, suggest next 2-3 syntax tokens (`{ } : ; ` " '`) in same layout. Keep 2-3 tokens in memory max, TTL ~2 minutes, delete after snippet expansion done. Little at a time. Different files are non-issue because nothing persists. Tier-0 static token pack (keywords, stdlib names, `(); => ->`) stays in `suggest()`; Tier-1 is this 2-min snippet helper only. Spike (#4 in `09-…`): does 2-min helper beat static-alone for identifier recall, <50ms hash map.
- `html` PARTIAL: Tier-0 tag/attr vocab (`div/span/href/src`). Tier-1: skeleton `<tag>$0</tag>`, context attrs (`<img→src/alt`, `<a→href`), `&nbsp;/&amp;`, auto-close.
- `math` SIMPLE like code (owner decision): 12/16-key with simple suggestion, same ephemeral pattern as code — no structural wizard for MVP. Tier-0 command vocab (`alp→257→\alpha`, `\sum\to`, `learn=false` stays). Tier-1 later (deferred): `\frac{}{}` tabstops, `^{}/_{}`, matrix `Tab→&` — usability test in `09-…` #5 decides if needed. Flat token is baseline.
- Fallback rule (to flush out): today only manual QWERTY toggle. Option: per-category `always-fallback-to-qwerty` (e.g. user sets `ne` or `math` to always open QWERTY/Nepali-QWERTY). Plugin question: how do expansion/plugin fallback keyboards (Nepali-QWERTY etc.) register alongside built-in QWERTY? Deferred — file as open item in `08-dict-stack-ordering.md`.

## Layout per palette tab

- Two zones: top-3 `suggest()` hits + palette strip below. Dragging `math` low never removes `\frac{}{}` template; it only demotes `\alpha` ranking + moves tab right. Tab order = reach cost; priority = ranking weight.
- `Suggestion` has no cursor/tabstop/AST field — don't force it. Palette returns its own item type with `body + tabstops + commit behavior`.

## Acceptance

- `words/ne/emoji/numbers/medical/personal` pass gate on pure `suggest()`. `js/rust/html/math` pass keyword Tier-0 + demonstrate one snippet + one symbol/palette action each without engine fork.
