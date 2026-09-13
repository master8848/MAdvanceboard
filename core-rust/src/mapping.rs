//! Layout-driven key→seq encoding (t9-9 default): encode words to digit sequences, adjacency for fuzzy match.
//!
//! Layout (3x3 + bottom row):
//! ```text
//! 1 2 3
//! 4 5 6
//! 7 8 9
//! * 0 #
//! ```
//! - 2:abc 3:def 4:ghi 5:jkl 6:mno 7:pqrs 8:tuv 9:wxyz
//! - 0:space, 1:punct, `*`:back/cycle (control), `#`:mode (control)
//! - Devanagari initials are collapsed phonetically onto the same keys
//!   (k-series->2, c/j->3, retroflex->4, dental->5, p/b/m->6,
//!   y/r/l/w->7, s/h->8, vowels->9).
//! - Devanagari matras, virama, and signs (ा ि ी ु ू े ै ो ौ ं ः ँ ॉ ्)
//!   are NON-EMITTING modifiers: `char_to_digit` returns `None` for them,
//!   so `encode_word` of Devanagari yields the consonant skeleton
//!   (plan/03-nepali-transliteration.md). Roman typing matches through
//!   `PackWord.tr` seqs, which are primary; the skeleton is fallback only.
//!   (This intentionally supersedes SPEC section 1's older
//!   "matra -> base vowel" rule, which produced spurious `9`s.)

/// Map a single character to its T9 digit. Returns `None` for control
/// keys (`*`, `#`) and unmapped characters.
pub fn char_to_digit(c: char) -> Option<char> {
    match c {
        // Latin (case-insensitive)
        'a' | 'b' | 'c' | 'A' | 'B' | 'C' => Some('2'),
        'd' | 'e' | 'f' | 'D' | 'E' | 'F' => Some('3'),
        'g' | 'h' | 'i' | 'G' | 'H' | 'I' => Some('4'),
        'j' | 'k' | 'l' | 'J' | 'K' | 'L' => Some('5'),
        'm' | 'n' | 'o' | 'M' | 'N' | 'O' => Some('6'),
        'p' | 'q' | 'r' | 's' | 'P' | 'Q' | 'R' | 'S' => Some('7'),
        't' | 'u' | 'v' | 'T' | 'U' | 'V' => Some('8'),
        'w' | 'x' | 'y' | 'z' | 'W' | 'X' | 'Y' | 'Z' => Some('9'),
        // Space / digits
        ' ' => Some('0'),
        '0'..='9' => Some(c),
        // Punctuation (key 1)
        '.' | ',' | '?' | '!' | '\'' | '।' | '॥' => Some('1'),
        // Devanagari: k-series -> 2
        'क' | 'ख' | 'ग' | 'घ' | 'ङ' => Some('2'),
        // c/j-series -> 3
        'च' | 'छ' | 'ज' | 'झ' | 'ञ' => Some('3'),
        // retroflex -> 4
        'ट' | 'ठ' | 'ड' | 'ढ' | 'ण' => Some('4'),
        // dental -> 5
        'त' | 'थ' | 'द' | 'ध' | 'न' => Some('5'),
        // p/b/m -> 6
        'प' | 'फ' | 'ब' | 'भ' | 'म' => Some('6'),
        // y/r/l/w + श -> 7
        'य' | 'र' | 'ल' | 'व' | 'श' => Some('7'),
        // s/h -> 8
        'ष' | 'स' | 'ह' => Some('8'),
        // Independent vowels -> 9. Matras, virama, and nasal/sign
        // modifiers are NON-EMITTING (return None, skipped by
        // `encode_word`): they can never stand alone as a keypress, and
        // emitting them produced spurious `9`s in every word
        // (plan/03-nepali-transliteration.md). Roman `tr` seqs are the
        // primary search path; the Devanagari consonant skeleton that
        // remains is fallback only.
        'अ' | 'आ' | 'इ' | 'ई' | 'उ' | 'ऊ' | 'ए' | 'ऐ' | 'ओ' | 'औ' => Some('9'),
        'ा' | 'ि' | 'ी' | 'ु' | 'ू' | 'े' | 'ै' | 'ो' | 'ौ' | 'ं' | 'ः' | 'ँ' | 'ॉ' | '्' => {
            None
        }
        _ => None,
    }
}

/// Encode a word into its digit sequence. Unmapped/control characters
/// (`*`, `#`, symbols) are skipped.
pub fn encode_word(word: &str) -> String {
    word.chars().filter_map(char_to_digit).collect()
}

/// Keypad coordinates for adjacency checks.
fn key_pos(d: char) -> Option<(i32, i32)> {
    match d {
        '1' => Some((0, 0)),
        '2' => Some((0, 1)),
        '3' => Some((0, 2)),
        '4' => Some((1, 0)),
        '5' => Some((1, 1)),
        '6' => Some((1, 2)),
        '7' => Some((2, 0)),
        '8' => Some((2, 1)),
        '9' => Some((2, 2)),
        '*' => Some((3, 0)),
        '0' => Some((3, 1)),
        '#' => Some((3, 2)),
        _ => None,
    }
}

/// True if two digits are neighboring keys (Chebyshev distance <= 1,
/// excluding identity).
pub fn is_neighbor(a: char, b: char) -> bool {
    if a == b {
        return false;
    }
    match (key_pos(a), key_pos(b)) {
        (Some((r1, c1)), Some((r2, c2))) => {
            (r1 - r2).abs() <= 1 && (c1 - c2).abs() <= 1
        }
        _ => false,
    }
}

/// True if `candidate_seq` differs from `seq` by exactly one substitution
/// to an adjacent key (fat-finger fuzzy, 1-edit).
pub fn is_one_edit_neighbor(seq: &str, candidate_seq: &str) -> bool {
    if seq.len() != candidate_seq.len() {
        return false;
    }
    let mut edits = 0;
    for (a, b) in seq.chars().zip(candidate_seq.chars()) {
        if a == b {
            continue;
        }
        if !is_neighbor(a, b) {
            return false;
        }
        edits += 1;
        if edits > 1 {
            return false;
        }
    }
    edits == 1
}

/// Human-readable key label for a digit.
pub fn key_label(digit: char) -> &'static str {
    match digit {
        '1' => ".,?!'",
        '2' => "abc",
        '3' => "def",
        '4' => "ghi",
        '5' => "jkl",
        '6' => "mno",
        '7' => "pqrs",
        '8' => "tuv",
        '9' => "wxyz",
        '0' => "space",
        '*' => "back/cycle",
        '#' => "mode",
        _ => "?",
    }
}

/// Frozen 9-key reference mapping as a [`crate::layout::KeyMapping`].
/// Behavior is exactly the free functions above; the data-driven
/// `layouts/t9-9.json` spec is checked against it in
/// `layout::tests::t9_parity_with_mapping`.
pub struct T9Mapping;

impl crate::layout::KeyMapping for T9Mapping {
    fn layout_id(&self) -> &str {
        crate::layout::DEFAULT_LAYOUT_ID
    }

    fn encode_word(&self, word: &str) -> String {
        encode_word(word)
    }

    fn neighbors(&self, a: char, b: char) -> bool {
        is_neighbor(a, b)
    }

    /// Neighbor codes over the frozen 12-key pad (`1 2 3 / 4 5 6 / 7 8 9 /
    /// * 0 #`), in fixed pad order. Matches [`is_neighbor`] exactly (locked
    /// by `layout::tests::t9_parity_with_mapping` + the test below).
    fn neighbor_codes(&self, code: char) -> Vec<char> {
        const CODES: [char; 12] = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '*', '0', '#'];
        CODES
            .into_iter()
            .filter(|&b| is_neighbor(code, b))
            .collect()
    }

    fn key_label(&self, code: char) -> &str {
        key_label(code)
    }

    /// Mirrors the frozen `suggest` input validation (ASCII digits only).
    fn contains_code(&self, c: char) -> bool {
        c.is_ascii_digit()
    }
}

/// Emergency fallback `t9-9` spec built from the frozen behavior above,
/// used only if the embedded `layouts/t9-9.json` fails to parse.
pub fn t9_fallback_spec() -> crate::layout::LayoutSpec {
    use crate::layout::LayoutKey;
    let keys = vec![
        ("1", ".,?!'", ".,?!'।॥", "text", 0, 0),
        ("2", "abc", "abcABCकखगघङ", "text", 0, 1),
        ("3", "def", "defDEFचछजझञ", "text", 0, 2),
        ("4", "ghi", "ghiGHIटठडढण", "text", 1, 0),
        ("5", "jkl", "jklJKLतथदधन", "text", 1, 1),
        ("6", "mno", "mnoMNOपफबभम", "text", 1, 2),
        ("7", "pqrs", "pqrsPQRSयरलवश", "text", 2, 0),
        ("8", "tuv", "tuvTUVषसह", "text", 2, 1),
        ("9", "wxyz", "wxyzWXYZअआइईउऊएऐओऔ", "text", 2, 2),
        ("*", "back/cycle", "", "control", 3, 0),
        ("0", "space", " ", "space", 3, 1),
        ("#", "mode", "", "control", 3, 2),
    ];
    let pos: std::collections::HashMap<char, (i32, i32)> = keys
        .iter()
        .map(|(c, _, _, _, r, col)| (c.chars().next().unwrap(), (*r, *col)))
        .collect();
    let mut adjacency = std::collections::HashMap::new();
    for (a, (r1, c1)) in &pos {
        let mut ns = Vec::new();
        for (b, (r2, c2)) in &pos {
            if a != b && (r1 - r2).abs() <= 1 && (c1 - c2).abs() <= 1 {
                ns.push(*b);
            }
        }
        ns.sort();
        adjacency.insert(*a, ns);
    }
    crate::layout::LayoutSpec::assemble(
        crate::layout::DEFAULT_LAYOUT_ID.to_string(),
        "Classic 9-key T9 (fallback)".to_string(),
        3,
        4,
        keys.into_iter()
            .map(|(c, label, symbols, role, row, col)| LayoutKey {
                code: c.chars().next().unwrap(),
                label: label.to_string(),
                symbols: symbols.to_string(),
                role: role.to_string(),
                row,
                col,
            })
            .collect(),
        adjacency,
    )
    .expect("fallback t9-9 spec must assemble")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_hello() {
        assert_eq!(encode_word("hello"), "43556");
    }

    #[test]
    fn encode_case_insensitive() {
        assert_eq!(encode_word("Hello"), encode_word("hello"));
    }

    #[test]
    fn encode_space_is_zero() {
        assert_eq!(encode_word("a b"), "202");
    }

    #[test]
    fn encode_punct_is_one() {
        assert_eq!(encode_word("hi!"), "441");
    }

    #[test]
    fn encode_digits_passthrough() {
        assert_eq!(encode_word("a1"), "21");
    }

    #[test]
    fn control_keys_skipped() {
        assert_eq!(encode_word("a*c"), "22");
    }

    #[test]
    fn devanagari_collapsed_onto_latin_keys() {
        assert_eq!(char_to_digit('क'), Some('2'));
        assert_eq!(char_to_digit('म'), Some('6'));
        assert_eq!(char_to_digit('अ'), Some('9'));
        assert_eq!(encode_word("कमल"), "267");
    }

    #[test]
    fn matras_virama_signs_are_non_emitting() {
        // plan/03: matras can never stand alone, so they must not emit `9`.
        for m in [
            'ा', 'ि', 'ी', 'ु', 'ू', 'े', 'ै', 'ो', 'ौ', 'ं', 'ः', 'ँ', 'ॉ', '्',
        ] {
            assert_eq!(char_to_digit(m), None, "matra/sign {m:?} must not emit");
        }
        // `encode` of Devanagari is the consonant skeleton: independent
        // vowels still emit 9, matras vanish.
        // नमस्ते = न(5) म(6) स(8) ्(-) त(5) े(-) -> "5685".
        assert_eq!(encode_word("नमस्ते"), "5685");
        // कि = क(2) ि(-) -> "2", not "29".
        assert_eq!(encode_word("कि"), "2");
        assert_eq!(encode_word("कमल"), "267");
    }

    #[test]
    fn neighbors() {
        assert!(is_neighbor('2', '3'));
        assert!(is_neighbor('2', '5'));
        assert!(is_neighbor('5', '9'));
        assert!(!is_neighbor('2', '2'));
        assert!(!is_neighbor('2', '9'));
        assert!(!is_neighbor('1', '9'));
    }

    #[test]
    fn neighbor_codes_match_is_neighbor() {
        use crate::layout::KeyMapping;
        let m = T9Mapping;
        const CODES: [char; 12] = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '*', '0', '#'];
        for &a in &CODES {
            let mut got = m.neighbor_codes(a);
            got.sort();
            let mut want: Vec<char> = CODES.into_iter().filter(|&b| is_neighbor(a, b)).collect();
            want.sort();
            assert_eq!(got, want, "neighbor_codes({a})");
            assert!(!got.contains(&a), "never contains self");
        }
        assert!(m.neighbor_codes('z').is_empty(), "unknown code -> empty");
    }

    #[test]
    fn one_edit_neighbor() {
        assert!(is_one_edit_neighbor("43556", "43555")); // 6 vs 5 adjacent
        assert!(!is_one_edit_neighbor("43556", "43556")); // exact is not neighbor
        assert!(!is_one_edit_neighbor("43556", "43551")); // 6 vs 1 not adjacent
        assert!(!is_one_edit_neighbor("43556", "4355")); // length differs
        assert!(!is_one_edit_neighbor("43556", "42545")); // two edits
    }
}
