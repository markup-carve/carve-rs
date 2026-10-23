#[test]
fn a_quote_after_a_trailing_slash_closes_before_punctuation_or_whitespace() {
    let cases = [
        (
            "Exclude \"\\/trunk/app/config/\".\n",
            "<p>Exclude “/trunk/app/config/”.</p>",
        ),
        (
            "see \"https://example.org/\" here\n",
            "<p>see “https://example.org/” here</p>",
        ),
        (
            "the 'tmp/' folder, and/\"or\" too\n",
            "<p>the ‘tmp/’ folder, and/“or” too</p>",
        ),
        ("end/\"\n", "<p>end/”</p>"),
    ];
    for (source, expected) in cases {
        assert_eq!(carve::to_html(source).trim(), expected, "{source:?}");
    }
}
