//! The colon fence's separator is a literal space; its metadata slots are not.
//!
//! `resources/grammar.ebnf` PART 7, MARKER SEPARATORS AND PADDING SLOTS, is
//! normative and splits the opener line into two roles that must NOT be swept
//! together (carve#878 step 2, spec edit carve#886):
//!
//! - The slot immediately after the fence run is a MARKER SEPARATOR: `space`,
//!   U+0020 only, because the token after it selects which of the four blocks
//!   the line opens.
//! - The admonition opener's title and label slots are PADDING: `whitespace`,
//!   which the grammar defines as a space or a tab and nothing else.
//!
//! Both were wrong here, in opposite directions: the separator admitted a tab,
//! and the padding slots used `char::is_whitespace`, which is every Unicode
//! whitespace character - a form feed and a vertical tab among them.

fn html(source: &str) -> String {
    carve::to_html(source)
}

/// Every token that can follow the separator, since each selects a different
/// block and they are not all decided in the same place.
const OPENER_TOKENS: [(&str, &str); 4] = [
    ("admonition", "note"),
    ("div label", "[lbl]"),
    ("line block", "|"),
    ("local hard break", "\\"),
];

#[test]
fn a_tab_after_the_fence_run_opens_nothing() {
    for (label, token) in OPENER_TOKENS {
        let out = html(&format!(":::\t{token}\nx\n:::\n"));
        // Asserted as "the opener line survives as text" rather than "there is
        // a paragraph": a div and a line block BOTH wrap a paragraph, so a
        // paragraph check passes for a container that should not have opened.
        assert!(
            out.contains(":::"),
            "{label}: opener did not survive as text: {out}"
        );
        assert!(
            !out.contains("<aside"),
            "{label}: opened an admonition: {out}"
        );
        assert!(!out.contains("<div"), "{label}: opened a div: {out}");
    }
}

#[test]
fn a_space_there_still_opens_it() {
    // The control per row: narrowing the class must not close the door on the
    // spelling the grammar does admit.
    for (label, token) in OPENER_TOKENS {
        let spaced = html(&format!("::: {token}\nx\n:::\n"));
        let tabbed = html(&format!(":::\t{token}\nx\n:::\n"));
        assert_ne!(spaced, tabbed, "{label}: the two spellings still agree");
    }
}

#[test]
fn a_tab_pads_the_metadata_slots() {
    assert!(html("::: note\t\"Title\"\nx\n:::\n").contains("admonition-title"));
    assert!(html("::: note\t\"T\"\t[lbl]\nx\n:::\n").contains("admonition-title"));
}

#[test]
fn the_space_spelling_is_unchanged() {
    assert!(html("::: note \"Title\"\nx\n:::\n").contains("admonition-title"));
}

#[test]
fn only_a_space_or_tab_pads() {
    // `whitespace` is a space or a tab, exhaustively. `char::is_whitespace`
    // admits a great deal more, none of which the grammar names.
    for (label, ws) in [
        ("form feed", '\u{000c}'),
        ("vertical tab", '\u{000b}'),
        ("en quad", '\u{2000}'),
    ] {
        let out = html(&format!("::: note{ws}\"Title\"\nx\n:::\n"));
        assert!(
            !out.contains("admonition-title"),
            "{label} padded the title: {out}"
        );
    }
}
