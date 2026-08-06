//! The colon fence's separator is a RUN of literal spaces.
//!
//! `resources/grammar.ebnf` PART 7, MARKER SEPARATORS AND PADDING SLOTS, is
//! normative. It decides the terminal by POSITION: a tab is syntax only inside
//! a line's leading indentation run, and from the first non-whitespace
//! character onward it is not syntax at all. The slot immediately after the
//! fence run is therefore `space`, U+0020 and nothing else, and so are the
//! opener's metadata slots - those live in
//! `colon_fence_metadata_slots_are_a_space.rs`.
//!
//! carve-rs#712 fixed the FIRST character of this slot and stopped there:
//! `detect_container_open` tested `after_fence.starts_with(' ')` and then read
//! the token out of `after_fence.trim()`, whose Unicode trim swallowed
//! whatever followed that space. A lone tab was rejected while `::: <TAB>note`
//! still opened an admonition. Only a MIXED run exposes it, which is why every
//! test below runs both directions (carve-rs#722, corpus category 254).

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

/// Asserted as "the opener line survives as text" rather than "there is a
/// paragraph": a div and a line block BOTH wrap a paragraph, so a paragraph
/// check passes for a container that should not have opened at all.
fn assert_opened_nothing(label: &str, out: &str) {
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

#[test]
fn a_tab_after_the_fence_run_opens_nothing() {
    for (label, token) in OPENER_TOKENS {
        assert_opened_nothing(label, &html(&format!(":::\t{token}\nx\n:::\n")));
    }
}

#[test]
fn a_tab_anywhere_in_the_separator_run_opens_nothing() {
    // Both directions, per token. A tab-FIRST run is caught by a check on the
    // separator's first character; a space-then-tab run is caught only by a
    // check on the whole run, and that is the one carve-rs#712 left open for
    // the admonition and the bare label.
    for (label, token) in OPENER_TOKENS {
        assert_opened_nothing(
            &format!("{label}, space then tab"),
            &html(&format!("::: \t{token}\nx\n:::\n")),
        );
        assert_opened_nothing(
            &format!("{label}, tab then space"),
            &html(&format!(":::\t {token}\nx\n:::\n")),
        );
    }
}

#[test]
fn a_unicode_space_in_the_separator_run_opens_nothing() {
    // The same run check, reached with a character that is neither a space nor
    // a tab. `str::trim` is `char::is_whitespace`, so reading the token out of
    // a trimmed copy admitted these too.
    for (label, ws) in [
        ("no-break space", '\u{00a0}'),
        ("em space", '\u{2003}'),
        ("form feed", '\u{000c}'),
    ] {
        assert_opened_nothing(
            &format!("{label}, leading"),
            &html(&format!(":::{ws}note\nx\n:::\n")),
        );
        assert_opened_nothing(
            &format!("{label}, after a space"),
            &html(&format!("::: {ws}note\nx\n:::\n")),
        );
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
fn the_separator_is_a_run_not_a_single_space() {
    // Load-bearing rather than a control: narrowing this slot to exactly one
    // U+0020 passes every case above and breaks only this one. Corpus
    // 254-colon-fence-separator-must-be-a-space-10 pins the same shape.
    for (label, token) in OPENER_TOKENS {
        let one = html(&format!("::: {token}\nx\n:::\n"));
        let two = html(&format!(":::  {token}\nx\n:::\n"));
        assert_eq!(
            two, one,
            "{label}: a two-space separator opened something else"
        );
    }
}

#[test]
fn a_label_glued_to_the_fence_still_opens_a_div() {
    // `div_open = colon_fence, [[space], label]` - the separator is OPTIONAL
    // before a bare label, so a zero-length run is legal here and only here.
    let out = html(":::[lbl]\nx\n:::\n");
    assert!(
        out.contains("div-label"),
        "glued label did not open a div: {out}"
    );
}
