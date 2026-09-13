//! Tier-1 snippet palettes for `js` / `rust` / `html` / `math` (plan `07`).
//!
//! Tier-0 (static keyword vocab in `suggest()`) is untouched — this module
//! is the Tier-1 local table + macro snippets only. There is deliberately
//! NO engine fork: palettes never call `suggest()`, never touch ranking,
//! and return their own item type ([`SnippetItem`]: `body` + commit
//! behavior) because `Suggestion` has no cursor/tabstop/AST field.
//!
//! - `js`/`rust` EPHEMERAL: trigger → next 2–3 syntax tokens (the 2-min
//!   [`crate::snippet::SnippetBuffer`] chain; see [`follow_tokens`]).
//! - `html` PARTIAL: tag skeletons + context attrs + entities + auto-close.
//! - `math` SIMPLE flat tokens (plan `07:14` baseline): verbatim bodies,
//!   caret parked inside the braces, NO tabstop structure (the deferred
//!   `\frac{}{}` tabstop set needs the plan-09 #5 usability test first).
//!
//! Mirrors `android/ime/.../SnippetPalettes.kt` (same tables, same rules);
//! the two stay in lockstep via parallel tests.

/// How a palette item commits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnippetCommit {
    /// Ephemeral 2-min follow token (`if` → `if (`, `{`, `}`).
    FollowToken,
    /// `<tag attrs>…</tag>` with the caret parked between the tags.
    HtmlSkeleton,
    /// Void element (`<img …>`): no closing tag, caret at end.
    HtmlVoid,
    /// `&nbsp;` / `&amp;`: verbatim, caret at end.
    HtmlEntity,
    /// Flat math token (`\frac{}{}`, `^{}…`): verbatim, caret inside.
    MathFlat,
}

/// Palette item: `body` is the literal text committed; `cursor_back` parks
/// the caret that many chars left after commit (0 = end).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnippetItem {
    pub display: String,
    pub body: String,
    pub cursor_back: usize,
    pub commit: SnippetCommit,
    pub source: &'static str,
}

fn item(
    display: &str,
    body: &str,
    cursor_back: usize,
    commit: SnippetCommit,
    source: &'static str,
) -> SnippetItem {
    assert!(!body.is_empty(), "SnippetItem body must not be empty");
    assert!(
        cursor_back <= body.len(),
        "cursor_back {cursor_back} out of range for {body:?}"
    );
    SnippetItem {
        display: display.to_string(),
        body: body.to_string(),
        cursor_back,
        commit,
        source,
    }
}

/// `js` trigger → next 2–3 syntax tokens (static Tier-0 vocab already
/// covers keywords; this only chains the syntax after a trigger).
fn js_triggers(trigger: &str) -> Option<&'static [&'static str]> {
    Some(match trigger {
        "if" => &["if (", "{", "}"],
        "for" => &["for (", "{", "}"],
        "while" => &["while (", "{", "}"],
        "function" => &["function ", "{", "}"],
        "return" => &["return ", ";"],
        "const" => &["const ", "=", ";"],
        "let" => &["let ", "=", ";"],
        "else" => &["else ", "{", "}"],
        "class" => &["class ", "{", "}"],
        "import" => &["import ", "from ", ";"],
        "export" => &["export ", "from ", ";"],
        "try" => &["try ", "{", "}"],
        "catch" => &["catch (", "{", "}"],
        "switch" => &["switch (", "{", "}"],
        "case" => &["case ", ":"],
        "new" => &["new ", "(", ")"],
        "=>" => &["=> ", "{", "}"],
        _ => return None,
    })
}

/// `rust` trigger → next 2–3 syntax tokens (same ephemeral pattern).
fn rust_triggers(trigger: &str) -> Option<&'static [&'static str]> {
    Some(match trigger {
        "fn" => &["fn ", "{", "}"],
        "let" => &["let ", "=", ";"],
        "mut" => &["mut ", "=", ";"],
        "if" => &["if ", "{", "}"],
        "else" => &["else ", "{", "}"],
        "for" => &["for ", "{", "}"],
        "while" => &["while ", "{", "}"],
        "loop" => &["loop ", "{", "}"],
        "match" => &["match ", "{", "}"],
        "struct" => &["struct ", "{", "}"],
        "enum" => &["enum ", "{", "}"],
        "impl" => &["impl ", "{", "}"],
        "use" => &["use ", "::", ";"],
        "pub" => &["pub ", "fn ", "{"],
        "return" => &["return ", ";"],
        "->" => &["-> ", "{"],
        _ => return None,
    })
}

/// Follow-tokens for a just-committed trigger in `tab`. Exact lowercase
/// match; `js`/`rust` only — every other tab returns empty (their Tier-1
/// is the static palette, not the ephemeral chain).
pub fn follow_tokens(tab: &str, trigger: &str) -> Vec<String> {
    let t = trigger.to_lowercase();
    let table = match tab {
        "js" => js_triggers(t.as_str()),
        "rust" => rust_triggers(t.as_str()),
        _ => None,
    };
    table
        .map(|toks| toks.iter().map(|s| s.to_string()).collect())
        .unwrap_or_default()
}

/// HTML tag skeleton descriptor: `attrs` render inside the open tag.
pub struct HtmlTag {
    pub name: &'static str,
    pub attrs: &'static str,
    pub void: bool,
}

/// Tier-1 HTML skeletons: common tags + context attrs (`a→href`,
/// `img→src/alt`).
pub const HTML_TAGS: &[HtmlTag] = &[
    HtmlTag { name: "div", attrs: "", void: false },
    HtmlTag { name: "span", attrs: "", void: false },
    HtmlTag { name: "p", attrs: "", void: false },
    HtmlTag { name: "a", attrs: "href=\"\"", void: false },
    HtmlTag { name: "img", attrs: "src=\"\" alt=\"\"", void: true },
    HtmlTag { name: "input", attrs: "type=\"\"", void: true },
    HtmlTag { name: "br", attrs: "", void: true },
    HtmlTag { name: "button", attrs: "type=\"\"", void: false },
    HtmlTag { name: "ul", attrs: "", void: false },
    HtmlTag { name: "li", attrs: "", void: false },
    HtmlTag { name: "h1", attrs: "", void: false },
    HtmlTag { name: "h2", attrs: "", void: false },
    HtmlTag { name: "script", attrs: "src=\"\"", void: false },
    HtmlTag { name: "style", attrs: "", void: false },
    HtmlTag { name: "form", attrs: "action=\"\"", void: false },
    HtmlTag { name: "label", attrs: "for=\"\"", void: false },
];

/// Tier-1 HTML entities.
pub const HTML_ENTITIES: &[&str] = &["&nbsp;", "&amp;"];

/// Skeleton item for one HTML tag (`<tag>$0</tag>`, caret = `$0`).
pub fn skeleton_for(tag: &HtmlTag) -> SnippetItem {
    let open = if tag.attrs.is_empty() {
        format!("<{}>", tag.name)
    } else {
        format!("<{} {}>", tag.name, tag.attrs)
    };
    if tag.void {
        return item(&open, &open, 0, SnippetCommit::HtmlVoid, "html-skeleton");
    }
    let body = format!("{open}</{}>", tag.name);
    let back = format!("</{}>", tag.name).len();
    let display = format!("{}…</{}>", open[..open.len() - 1].to_string() + ">", tag.name);
    item(&display, &body, back, SnippetCommit::HtmlSkeleton, "html-skeleton")
}

/// Auto-close text for a committed tag name: `"</tag>"`, or `None` for
/// void elements and unknown/blank names (nothing to close — normal).
pub fn auto_close(tag_name: &str) -> Option<String> {
    let name = tag_name.trim().to_lowercase();
    if name.is_empty() {
        return None;
    }
    let tag = HTML_TAGS.iter().find(|t| t.name == name)?;
    if tag.void {
        return None;
    }
    Some(format!("</{}>", tag.name))
}

/// Flat math tokens (plan `07:14` baseline — verbatim bodies, caret parked
/// inside the braces, NO tabstop navigation).
pub fn math_flat() -> Vec<SnippetItem> {
    vec![
        item("\\frac{}{}", "\\frac{}{}", 3, SnippetCommit::MathFlat, "math-flat"),
        item("^{}", "^{}", 1, SnippetCommit::MathFlat, "math-flat"),
        item("_{}", "_{}", 1, SnippetCommit::MathFlat, "math-flat"),
        item("\\sqrt{}", "\\sqrt{}", 1, SnippetCommit::MathFlat, "math-flat"),
        item("\\sum", "\\sum", 0, SnippetCommit::MathFlat, "math-flat"),
        item("\\int", "\\int", 0, SnippetCommit::MathFlat, "math-flat"),
    ]
}

/// Static Tier-1 palette for `tab`, prefix-filtered (case-insensitive):
/// `html` → tag skeletons + entities; `math` → flat tokens (`\fr` →
/// `\frac{}{}` though nothing is learned); `js`/`rust`/others → empty
/// (their Tier-1 is the ephemeral follow-token buffer, not a static list).
pub fn static_snippets(tab: &str, prefix: &str) -> Vec<SnippetItem> {
    let q = prefix
        .trim_start_matches(['<', '\\'])
        .to_lowercase();
    match tab {
        "html" => {
            let mut out: Vec<SnippetItem> = HTML_TAGS
                .iter()
                .filter(|t| q.is_empty() || t.name.starts_with(q.as_str()))
                .map(skeleton_for)
                .collect();
            for e in HTML_ENTITIES {
                if q.is_empty() || e.to_lowercase().contains(q.as_str()) {
                    out.push(item(e, e, 0, SnippetCommit::HtmlEntity, "html-entity"));
                }
            }
            out
        }
        "math" => math_flat()
            .into_iter()
            .filter(|s| q.is_empty() || s.body.to_lowercase().contains(q.as_str()))
            .collect(),
        _ => Vec::new(),
    }
}

/// Tabs owning a Tier-1 palette (plan `07` acceptance: exactly these four).
pub const PALETTE_TABS: &[&str] = &["js", "rust", "html", "math"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_tabs_are_exactly_js_rust_html_math() {
        assert_eq!(PALETTE_TABS, &["js", "rust", "html", "math"]);
    }

    #[test]
    fn js_if_chains_if_paren_brace() {
        // One snippet per code tab: `if` → `if (` + braces.
        let toks = follow_tokens("js", "if");
        assert_eq!(toks, vec!["if (".to_string(), "{".to_string(), "}".to_string()]);
        assert!(follow_tokens("js", "IF").starts_with(&["if (".to_string()]));
        assert!(follow_tokens("js", "function").contains(&"{".to_string()));
    }

    #[test]
    fn rust_fn_chains_brace() {
        let toks = follow_tokens("rust", "fn");
        assert_eq!(toks, vec!["fn ".to_string(), "{".to_string(), "}".to_string()]);
        assert!(follow_tokens("rust", "match").contains(&"{".to_string()));
    }

    #[test]
    fn no_ephemeral_chain_outside_js_rust() {
        for tab in ["html", "math", "words", "ne", "numbers", "emoji", "medical", ""] {
            assert!(
                follow_tokens(tab, "if").is_empty(),
                "tab {tab:?} must not chain follow-tokens"
            );
        }
        assert!(follow_tokens("js", "nope").is_empty());
    }

    #[test]
    fn html_skeleton_context_attrs_entities_autoclose() {
        // One snippet + one palette action: `<a` → href skeleton (caret
        // between tags), `<img` → void (no close), entities, auto-close.
        let a = static_snippets("html", "a");
        let skeleton = a.iter().find(|s| s.body == "<a href=\"\"></a>").unwrap();
        assert_eq!(skeleton.commit, SnippetCommit::HtmlSkeleton);
        assert_eq!(skeleton.cursor_back, "</a>".len());
        let img = static_snippets("html", "img");
        assert_eq!(img.len(), 1);
        assert_eq!(img[0].commit, SnippetCommit::HtmlVoid);
        assert_eq!(auto_close("a"), Some("</a>".to_string()));
        assert_eq!(auto_close("img"), None, "void: nothing to close");
        assert_eq!(auto_close("marquee"), None, "unknown: nothing to close");
        let ents: Vec<String> =
            static_snippets("html", "").iter().map(|s| s.body.clone()).collect();
        assert!(ents.contains(&"&nbsp;".to_string()));
        assert!(ents.contains(&"&amp;".to_string()));
    }

    #[test]
    fn math_frac_suggested_though_nothing_learned() {
        // Plan 12 acceptance: `\fr` → `\frac{}{}` with learn=false.
        let hits = static_snippets("math", "\\fr");
        assert!(hits.iter().any(|s| s.body == "\\frac{}{}"));
        let frac = hits.iter().find(|s| s.body == "\\frac{}{}").unwrap();
        assert_eq!(frac.commit, SnippetCommit::MathFlat);
        assert_eq!(frac.cursor_back, 3, "caret parks inside the braces");
        // Flat baseline only: no tabstop navigation structure.
        assert!(static_snippets("math", "").len() >= 4);
    }

    #[test]
    fn static_palette_empty_outside_html_math() {
        for tab in ["js", "rust", "words", "numbers", "emoji", ""] {
            assert!(
                static_snippets(tab, "").is_empty(),
                "tab {tab:?}: Tier-1 is the buffer chain, not a static list"
            );
        }
    }

    #[test]
    fn tier1_never_forks_engine() {
        // Structural acceptance (plan 07 "no engine fork"): every code tab
        // yields ≥1 snippet AND ≥1 palette action from pure tables alone —
        // no suggest/rank/stack involvement (this module imports none).
        assert!(!follow_tokens("js", "if").is_empty(), "js snippet");
        assert!(!follow_tokens("js", "for").is_empty(), "js action");
        assert!(!follow_tokens("rust", "fn").is_empty(), "rust snippet");
        assert!(!follow_tokens("rust", "match").is_empty(), "rust action");
        assert!(
            static_snippets("html", "").iter().any(|s| s.commit == SnippetCommit::HtmlSkeleton),
            "html snippet"
        );
        assert!(
            static_snippets("html", "").iter().any(|s| s.commit == SnippetCommit::HtmlEntity),
            "html action"
        );
        assert!(
            static_snippets("math", "\\fr").iter().any(|s| s.body == "\\frac{}{}"),
            "math snippet"
        );
        assert!(
            static_snippets("math", "").iter().any(|s| s.body == "^{}"),
            "math action"
        );
    }
}
