//! CommonMark does not honour an intraword underscore, so escaping one protects
//! nothing and only litters identifiers in output meant to be read and searched.
//! An asterisk is not symmetric here - `a*b*c` does emphasise - so `*` stays
//! escaped everywhere.

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

#[test]
fn underscores_that_could_open_or_close_emphasis_stay_escaped() {
    assert_eq!(carve::to_markdown("trailing_").trim(), "trailing\\_");
    assert_eq!(carve::to_markdown("_leading").trim(), "\\_leading");
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
