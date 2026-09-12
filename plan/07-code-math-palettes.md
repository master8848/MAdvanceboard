# 07 — Local-Scope Token/Snippet Packs for Code Categories

Resolves `INTENT.md:24` risk. Job is narrower: make *characters* cheaper, not *logic*. No multi-token intent, no next-line, no tiny LM (IDE territory). Shared `suggest()` Tier-0 for all tabs; Tier-1 local table + macro snippets for `js/rust/html` only. UI composition, not core fork.

## Taxonomy

- `words`, `ne`: YES pure `suggest()` (`hello→43556`). Bigram+personal+recency suffices.
- `emoji`: YES pure (`hea→432→❤️` `seq` override). Closed set. Skin-tone strip deferred.
- `numbers`: YES trivially — digit-commit, no prediction (`SPEC §1`).
- `medical` (545 terms): YES pure flat domain vocab. Benefits 10-90 priority + personal learn. Phrase templates out-of-MVP.
- `★personal`: overlay, not engine. OOV `≥2 accepts/7d` promotion (QWERTY-learn `SPEC §4`) feeds code identifiers as `cat=js/rust` (+1.2 personal + bigram only when that tab active).
- `js`/`rust` PARTIAL: Tier-0 static token pack in `suggest()` (keywords, common stdlib names, punctuation clusters `();`, `=>`, `->`, `fun→386→function`, `let/const/fn/impl/match`) ranked by corpus freq — same arch as medical/Nepali. Tier-1: (1) local frequency table scanning current file tokens (not parsing, not LSP) — sliding window last N files/edits, halve every session, unbounded identifiers (`userId`, `fetchData`) via personal + doc index, never static pack; (2) single-level snippet macros — `for→for(…){…}`, `fun→function ${1:name}(${2}){$0}`, `if-let/match/try-catch` with `$1/$0` tab stops. Spike: doc-table vs frequency-alone identifier top-3, <50ms hash map (see `09-research-open-questions.md` #4).
- `html` PARTIAL: Tier-0 tag/attr vocab (`div/span/href/src`). Tier-1: skeleton `<tag>$0</tag>`, context attrs (`<img→src/alt`, `<a→href`), `&nbsp;/&amp;`, auto-close.
- `math` PARTIAL (strongest palette case): Tier-0 command vocab (`alp→257→\alpha`, `\sum\to`, `learn=false` stays). Tier-1: `\frac{}{}` 2-tabstop, `^{}/_{}`, `\sqrt{}`, `\begin{matrix/pmatrix}` with `Tab→& / Enter→\\`, auto-frac `x/→\frac{x}{}`, bracket enlarge, ~400-symbol panel, array wizard. Flat `\frac` token without navigation is unusable. Pattern: TeXstudio CWL + `%<>%/` placeholders, snippet `a/b→\frac{a}{b}`, `\ref/\cite` from labels/`.bib`, package-gated.

## Layout per palette tab

- Two zones: top-3 `suggest()` hits + palette strip below. Dragging `math` low never removes `\frac{}{}` template; it only demotes `\alpha` ranking + moves tab right. Tab order = reach cost; priority = ranking weight.
- `Suggestion` has no cursor/tabstop/AST field — don't force it. Palette returns its own item type with `body + tabstops + commit behavior`.

## Acceptance

- `words/ne/emoji/numbers/medical/personal` pass gate on pure `suggest()`. `js/rust/html/math` pass keyword Tier-0 + demonstrate one snippet + one symbol/palette action each without engine fork.
