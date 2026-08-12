use carve::{parse_with_options, to_html_with_options, Options, SmartQuotes};

#[test]
fn german_matches_the_optional_corpus() {
    let extension = SmartQuotes::new("de");
    let options = Options::new().with_extension(&extension);
    assert_eq!(
        to_html_with_options("\"Hello\" and 'bye'", &options),
        "<p>„Hello“ and ‚bye‘</p>"
    );
}

#[test]
fn locale_resolution_matches_php() {
    for (locale, expected) in [
        ("de-CH", "«Hello»"),
        ("de_AT", "„Hello“"),
        ("xx", "“Hello”"),
    ] {
        let extension = SmartQuotes::new(locale);
        let options = Options::new().with_extension(&extension);
        assert!(to_html_with_options("\"Hello\"", &options).contains(expected));
    }
}

#[test]
fn explicit_quotes_and_locale_independent_apostrophes_work() {
    let extension = SmartQuotes::new("de")
        .with_open_double_quote("[[")
        .with_close_double_quote("]]");
    let options = Options::new().with_extension(&extension);
    let html = to_html_with_options("\"Hello\" 'bye' don't '70s", &options);
    assert!(html.contains("[[Hello]] ‚bye‘ don’t ’70s"));
}

#[test]
fn ast_carries_the_resolved_locale_glyph() {
    let extension = SmartQuotes::new("de");
    let options = Options::new().with_extension(&extension);
    let doc = parse_with_options("\"Hallo\"", &options);
    assert!(format!("{doc:?}").contains('„'));
}

#[test]
fn supported_locale_surface_matches_php() {
    assert_eq!(SmartQuotes::supported_locales().count(), 20);
    assert!(SmartQuotes::is_locale_supported("de-AT"));
    assert!(SmartQuotes::is_locale_supported("fr_FR"));
    assert!(!SmartQuotes::is_locale_supported("xx"));
}
