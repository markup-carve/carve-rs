//! Optional Tier-2 corpus tests.
//!
//! These fixtures require explicit configuration. Unsupported optional features
//! stay visible as ignored tests instead of weakening the mandatory corpus.

use std::fs;
use std::path::PathBuf;

fn optional_corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus-optional")
}

fn read_pair(slug: &str) -> (String, String) {
    let dir = optional_corpus_dir();
    let crv = dir.join(format!("{slug}.crv"));
    let html = dir.join(format!("{slug}.html"));
    let source = fs::read_to_string(&crv).unwrap_or_else(|e| panic!("read {}: {e}", crv.display()));
    let expected =
        fs::read_to_string(&html).unwrap_or_else(|e| panic!("read {}: {e}", html.display()));
    (source, expected)
}

fn assert_optional_pair(slug: &str, options: carve::Options<'_>) {
    let (source, expected) = read_pair(slug);
    let actual = carve::to_html_with_options(&source, &options);
    assert_eq!(
        expected.trim(),
        actual.trim(),
        "optional corpus pair `{slug}`"
    );
}

#[test]
fn social_link_templates() {
    let options = carve::Options::new()
        .with_mention_url("/users/{name}")
        .with_tag_url("/topics/{name}");
    assert_optional_pair("01-social-link-templates", options);
}

#[test]
fn social_link_templates_sanitize_final_href() {
    let options = carve::Options::new()
        .with_mention_url("javascript:alert({name})")
        .with_tag_url("javascript:alert({name})");
    let html = carve::to_html_with_options("@alice #topic", &options);
    assert!(
        html.contains("<a class=\"mention\" href=\"\">@alice</a>"),
        "{html}"
    );
    assert!(
        html.contains("<a class=\"tag\" href=\"\">#topic</a>"),
        "{html}"
    );
}

#[test]
fn symbol_map() {
    let options = carve::Options::new()
        .with_symbol("rocket", "🚀")
        .with_symbol("tada", "🎉")
        .with_symbol("+1", "👍")
        .with_symbol("UPPER", "⬆️");
    assert_optional_pair("02-symbol-map", options);
}

#[test]
fn list_table_columns_and_foot() {
    let list_table = carve::ListTable::new();
    let options = carve::Options::new().with_extension(&list_table);
    assert_optional_pair("44-list-table-columns-and-foot", options);
}

#[test]
fn citations_numbered() {
    let citations = carve::Citations::new();
    let options = carve::Options::new().with_extension(&citations);
    assert_optional_pair("05-citations-numbered", options);
}

#[test]
fn citations_author_date() {
    let citations = carve::Citations::author_date();
    let options = carve::Options::new().with_extension(&citations);
    assert_optional_pair("06-citations-author-date", options);
}

#[test]
fn citations_undefined_key() {
    let citations = carve::Citations::new();
    let options = carve::Options::new().with_extension(&citations);
    assert_optional_pair("07-citations-undefined-key", options);
}

#[test]
fn citations_attr_span() {
    let citations = carve::Citations::new();
    let options = carve::Options::new().with_extension(&citations);
    assert_optional_pair("08-citations-attr-span", options);
}

#[test]
fn citations_locator_semicolon() {
    let citations = carve::Citations::new();
    let options = carve::Options::new().with_extension(&citations);
    assert_optional_pair("09-citations-locator-semicolon", options);
}

#[test]
fn citations_locator_page() {
    let citations = carve::Citations::new();
    let options = carve::Options::new().with_extension(&citations);
    assert_optional_pair("13-citations-locator-page", options);
}

#[test]
fn citations_locator_range_suffix() {
    let citations = carve::Citations::new();
    let options = carve::Options::new().with_extension(&citations);
    assert_optional_pair("14-citations-locator-range-suffix", options);
}

#[test]
fn citations_locator_labels() {
    let citations = carve::Citations::new();
    let options = carve::Options::new().with_extension(&citations);
    assert_optional_pair("15-citations-locator-labels", options);
}

#[test]
fn citations_locator_default_page() {
    let citations = carve::Citations::new();
    let options = carve::Options::new().with_extension(&citations);
    assert_optional_pair("16-citations-locator-default-page", options);
}

#[test]
fn citations_locator_roman() {
    let citations = carve::Citations::new();
    let options = carve::Options::new().with_extension(&citations);
    assert_optional_pair("17-citations-locator-roman", options);
}

#[test]
fn citations_label_boundary() {
    let citations = carve::Citations::new();
    let options = carve::Options::new().with_extension(&citations);
    assert_optional_pair("18-citations-label-boundary", options);
}

#[test]
fn citations_comma_suffix_trim() {
    let citations = carve::Citations::new();
    let options = carve::Options::new().with_extension(&citations);
    assert_optional_pair("19-citations-comma-suffix-trim", options);
}

#[test]
fn citations_empty_value() {
    let citations = carve::Citations::new();
    let options = carve::Options::new().with_extension(&citations);
    assert_optional_pair("20-citations-empty-value", options);
}

#[test]
fn citations_integral() {
    let citations = carve::Citations::new();
    let options = carve::Options::new().with_extension(&citations);
    assert_optional_pair("21-citations-integral", options);
}

#[test]
fn citations_group_marker_vs_item() {
    let citations = carve::Citations::new();
    let options = carve::Options::new().with_extension(&citations);
    assert_optional_pair("22-citations-group-marker-vs-item", options);
}

#[test]
fn citations_suppress_per_item() {
    let citations = carve::Citations::new();
    let options = carve::Options::new().with_extension(&citations);
    assert_optional_pair("23-citations-suppress-per-item", options);
}

#[test]
fn smart_quotes_locale_de() {
    let extension = carve::SmartQuotes::new("de");
    let options = carve::Options::new().with_extension(&extension);
    assert_optional_pair("03-smart-quotes-locale-de", options);
}

#[test]
#[ignore = "optional Tier-2 feature not supported by carve-rs yet"]
fn bare_url_autolink() {
    assert_optional_pair("04-bare-url-autolink", carve::Options::new());
}
