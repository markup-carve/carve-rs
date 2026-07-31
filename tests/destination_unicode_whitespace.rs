//! `unicode_url_char` is "any non-whitespace, non-ASCII Unicode character",
//! with no qualifier - so Unicode whitespace ends a destination exactly as a
//! plain space does, in both the inline and the reference-definition form.
//!
//! The byte scans read ASCII whitespace only, so a narrow no-break space passed
//! for an ordinary destination character and rode into the href. HTML hid it in
//! the one corpus case that exercised it, because the destination was denied
//! and blanked anyway; only the ANSI target, which prints the destination,
//! showed it (carve#404).

const NNBSP: &str = "\u{202F}";
const THIN: &str = "\u{2009}";
const IDEOGRAPHIC: &str = "\u{3000}";

#[test]
fn an_inline_destination_cannot_contain_unicode_whitespace() {
    for space in [NNBSP, THIN, IDEOGRAPHIC] {
        let out = carve::to_html(&format!("[x]({space}https://e.com)\n"));
        assert!(!out.contains("<a"), "{space:?} formed a link: {out}");
    }
}

#[test]
fn an_inline_destination_still_forms_without_it() {
    // The guard must not reject ordinary destinations.
    assert!(carve::to_html("[x](https://e.com)\n").contains(r#"href="https://e.com""#));
}

#[test]
fn a_definition_destination_ends_at_unicode_whitespace() {
    for space in [NNBSP, THIN, IDEOGRAPHIC] {
        let out = carve::to_html(&format!("[x][r]\n\n[r]: https://e.com{space}/path\n"));
        assert!(
            out.contains(r#"href="https://e.com""#),
            "{space:?} stayed in the href: {out}"
        );
    }
}

#[test]
fn a_definition_destination_is_trimmed_at_the_ends() {
    for space in [NNBSP, THIN, IDEOGRAPHIC] {
        let out = carve::to_html(&format!("[x][r]\n\n[r]: {space}https://e.com\n"));
        assert!(
            out.contains(r#"href="https://e.com""#),
            "leading {space:?} survived: {out}"
        );
    }
}

#[test]
fn a_zero_width_character_is_not_whitespace() {
    // The test is the Unicode White_Space property, not "is invisible".
    // U+200B and U+FEFF are ordinary destination characters.
    for zw in ["\u{200B}", "\u{FEFF}"] {
        let out = carve::to_html(&format!("[x][r]\n\n[r]: {zw}https://e.com\n"));
        assert!(out.contains(zw), "{zw:?} was stripped: {out}");
    }
}
