//! carve#6852: a reference key is read as inline content only to look up a heading (C2-I-084), so a document
//! without headings reads no key and one with headings still resolves them.

use carve::Options;
use carve::api;

fn render(source: &str) -> String {
    api::render(source, &Options::default()).expect("the source renders")
}

#[test]
fn a_collapsed_reference_finds_its_heading() {
    let html = render("# Intro *x*\n\nSee [Intro *x*][].\n");
    assert!(html.contains("<a href=\"#Intro-x\">"), "{html}");
}

#[test]
fn without_a_heading_the_reference_stays_text() {
    assert_eq!(
        render("See [Intro *x*][].\n"),
        "<p>See [Intro <strong>x</strong>][].</p>"
    );
}
