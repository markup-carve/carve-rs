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
fn emoji_map() {
    let options = carve::Options::new()
        .with_emoji("rocket", "🚀")
        .with_emoji("tada", "🎉");
    assert_optional_pair("02-emoji-map", options);
}

// Citations optional-corpus runners are deferred until carve-rs's spec
// submodule catches up to carve main (the citation cases live on carve#101,
// which also carries newer main-corpus cases carve-rs does not yet pass).
// The feature is covered by tests/citations.rs in the meantime.

#[test]
#[ignore = "optional Tier-2 feature not supported by carve-rs yet"]
fn smart_quotes_locale_de() {
    assert_optional_pair("03-smart-quotes-locale-de", carve::Options::new());
}

#[test]
#[ignore = "optional Tier-2 feature not supported by carve-rs yet"]
fn bare_url_autolink() {
    assert_optional_pair("04-bare-url-autolink", carve::Options::new());
}
