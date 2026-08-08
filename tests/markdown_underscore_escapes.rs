//! CommonMark does not honour an intraword underscore, so escaping one protects
//! nothing and only litters identifiers in output meant to be read and searched.
//! An asterisk is not symmetric here - `a*b*c` does emphasise - so `*` stays
//! escaped everywhere.
//!
//! SINCE PART 11 §8a (carve-rs#824) THE RULE IS WIDER THAN INTRAWORD. `_` is
//! escaped if and only if it is ADJACENT on the emitted line to an unescaped
//! delimiter of the same character, so a LONE underscore anywhere goes bare, not
//! only one between two word characters. The two cases below that used to pin
//! the narrower rule move with it, and `the_markdown_targets_escaping_narrows_on_the_line`
//! carries the clause itself.

#[test]
fn intraword_underscores_are_left_bare() {
    for source in [
        "company_id",
        "a_b_c",
        "snake_case_name",
        "read_write_delete",
    ] {
        assert_eq!(
            carve::to_markdown(source).trim(),
            source,
            "source: {source}"
        );
    }
}

/// A `_` at a word boundary is still not adjacent to a delimiter of its own
/// character, so §8a M1b emits it bare. The old intraword test kept it escaped;
/// that was the narrower rule, and it protected nothing - this writer spells
/// emphasis with `*`, so there is no `_` delimiter on the line for it to merge
/// with.
#[test]
fn underscores_that_could_open_or_close_emphasis_are_bare_too() {
    assert_eq!(carve::to_markdown("trailing_").trim(), "trailing_");
    assert_eq!(carve::to_markdown("_leading").trim(), "_leading");
}

#[test]
fn an_asterisk_between_word_characters_stays_escaped() {
    // `a*b*c` emphasises in CommonMark, unlike the underscore form.
    assert_eq!(carve::to_markdown("a*b*c").trim(), "a\\*b\\*c");
}

#[test]
fn code_spans_are_untouched() {
    assert_eq!(carve::to_markdown("`code_span`").trim(), "`code_span`");
}

/// A backslash the author typed is content, not an escape this renderer added.
/// The de-escaping used to run over the assembled document, where it could not
/// tell the two apart, and rewrote verbatim regions that carry a literal
/// backslash before an underscore (carve-js issue 400).
#[test]
fn a_backslash_the_renderer_did_not_write_is_kept() {
    for (source, expected) in [
        (r"`a\_b`", r"`a\_b`"),
        ("```\ncompany\\_id\n```", "```\ncompany\\_id\n```"),
        (r"[x](a\_b)", r"[x](a\_b)"),
        (r"![a](x\_y)", r"![a](x\_y)"),
        ("```=html\n<i>a\\_b</i>\n```", r"&lt;i&gt;a\_b&lt;/i&gt;"),
    ] {
        assert_eq!(
            carve::to_markdown(source).trim(),
            expected,
            "source: {source}"
        );
    }
}

/// PART 11 §8a M2: a character the AUTHOR escaped is an `escaped_text` node and
/// is emitted AS AN ESCAPE whatever the character, untouched by M1b. This used
/// to lose its backslash to the intraword rule, which was M1b deciding a node M1
/// never governed - and it made `a\_b` and `a_b` indistinguishable on this
/// target, taking out the one case where the author had said which reading they
/// meant.
#[test]
fn an_authored_escape_is_emitted_as_an_escape() {
    assert_eq!(carve::to_markdown(r"a\_b").trim(), r"a\_b");
}

#[test]
fn underline_emphasis_still_renders() {
    assert_eq!(carve::to_markdown("_underline_").trim(), "<u>underline</u>");
}

#[test]
fn an_identifier_beside_real_emphasis() {
    assert_eq!(
        carve::to_markdown("company_id and *strong*").trim(),
        "company_id and **strong**"
    );
}
