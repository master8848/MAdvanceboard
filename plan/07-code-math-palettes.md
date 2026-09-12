# 07 — Code/Math Palettes (Shared suggest() + Tier-1 Template Layer)

Resolves `INTENT.md:24` "no special engine branch" risk: keep shared `suggest()` as Tier-0 vocab for all tabs (preserves H1/H2, `SPEC §2` budgets), add Tier-1 palette/template provider behind existing `CategoryProvider.candidates()` for `js/rust/html/math` only. UI-side composition, not core fork — engine stays `seq→ranked words`; palette handles what `Suggestion{word,seq,cat}` cannot (symbols, skeletons, tabstops, scope).

## Taxonomy

- `words`, `ne`: YES pure `suggest()` (`hello→43556`). Bigram+personal+recency suffices.
- `emoji`: YES pure (`hea→432→❤️` `seq` override). Closed set. Skin-tone strip deferred.
- `numbers`: YES trivially — digit-commit, no prediction (`SPEC §1`).
- `medical` (545 terms): YES pure flat domain vocab. Benefits 10-90 priority + personal learn. Phrase templates out-of-MVP.
- `★personal`: overlay, not engine. OOV `≥2 accepts/7d` promotion (QWERTY-learn `SPEC §4`) feeds code identifiers as `cat=js/rust` (+1.2 personal + bigram only when that tab active).
- `js`/`rust` PARTIAL: Tier-0 keyword prefixes in `suggest()` (`fun→386→function`, `let/const/fn/impl/match`). Tier-1: (1) scope identifiers — unbounded, via personal + doc/project index, never static pack; (2) symbol palette — `:: => { } ( ) ; " '` unencodable via `encode_word()` (skipped `core-rust/src/pack.rs:42`, `core-rust/src/mapping.rs:58`); (3) snippets — `for→for(…){…}`, `fun→function ${1:name}(${2}){$0}`, `if-let/match/try-catch`, `fn/print!/derive` with `$1/$0` placeholders.
- `html` PARTIAL: Tier-0 tag/attr vocab (`div/span/href/src`). Tier-1: skeleton `<tag>$0</tag>`, context attrs (`<img→src/alt`, `<a→href`), `&nbsp;/&amp;`, auto-close.
- `math` PARTIAL (strongest palette case): Tier-0 command vocab (`alp→257→\alpha`, `\sum\to`, `learn=false` stays). Tier-1: `\frac{}{}` 2-tabstop, `^{}/_{}`, `\sqrt{}`, `\begin{matrix/pmatrix}` with `Tab→& / Enter→\\`, auto-frac `x/→\frac{x}{}`, bracket enlarge, ~400-symbol panel, array wizard. Flat `\frac` token without navigation is unusable. Pattern: TeXstudio CWL + `%<>%/` placeholders, snippet `a/b→\frac{a}{b}`, `\ref/\cite` from labels/`.bib`, package-gated.

## Layout per palette tab

- Two zones: top-3 `suggest()` hits + palette strip below. Dragging `math` low never removes `\frac{}{}` template; it only demotes `\alpha` ranking + moves tab right. Tab order = reach cost; priority = ranking weight.
- `Suggestion` has no cursor/tabstop/AST field — don't force it. Palette returns its own item type with `body + tabstops + commit behavior`.

## Acceptance

- `words/ne/emoji/numbers/medical/personal` pass gate on pure `suggest()`. `js/rust/html/math` pass keyword Tier-0 + demonstrate one snippet + one symbol/palette action each without engine fork.
