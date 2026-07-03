//! Extension-generated ids join the document id namespace (extensions
//! contract §2.6; markup-carve/carve#238): explicit `{#id}` attributes and
//! generated heading ids reserve names FIRST, and generated citation ids
//! (`cite-{key}-{n}` use-site anchors, `ref-{key}` reference entries) take
//! the next free 1-based suffix instead of colliding. Scenarios mirror
//! carve-php's and carve-js's test sets for cross-implementation parity
//! (minus tabs / code groups, which this port does not ship).

use carve::{Citations, CslEntry, Options};

fn bib_html(source: &str) -> String {
    let citations = Citations::new().with_bibliography(vec![CslEntry {
        id: "foo".to_string(),
        title: Some("Foo".to_string()),
        ..Default::default()
    }]);
    let options = Options::new().with_extension(&citations);
    carve::to_html_with_options(source, &options)
}

#[test]
fn no_collision_ids_remain_stable() {
    let html = bib_html("See [@foo].");
    assert!(
        html.contains("<a id=\"cite-foo-1\" data-cite-key=\"foo\" href=\"#ref-foo\">1</a>"),
        "unexpected citation anchor in: {html}"
    );
    assert!(
        html.contains("<li id=\"ref-foo\">Foo. <a href=\"#cite-foo-1\""),
        "unexpected references entry in: {html}"
    );
}

#[test]
fn citation_anchor_ids_avoid_heading_ids_and_backrefs_follow() {
    let html = bib_html("# cite foo 1\n\nSee [@foo].");
    assert!(
        html.contains("<section id=\"cite-foo-1\">"),
        "heading keeps its slug in: {html}"
    );
    assert!(
        html.contains("<a id=\"cite-foo-1-2\" data-cite-key=\"foo\" href=\"#ref-foo\">1</a>"),
        "citation anchor takes the next free suffix in: {html}"
    );
    assert!(
        html.contains("<a href=\"#cite-foo-1-2\" class=\"ref-backref\">"),
        "back-link follows the bumped anchor id in: {html}"
    );
}

#[test]
fn reference_ids_avoid_explicit_ids_and_citations_follow() {
    let html = bib_html("{#ref-foo}\nReserved.\n\nSee [@foo].");
    assert!(
        html.contains("<a id=\"cite-foo-1\" data-cite-key=\"foo\" href=\"#ref-foo-2\">1</a>"),
        "citation href follows the bumped reference id in: {html}"
    );
    assert!(
        html.contains("<li id=\"ref-foo-2\">Foo."),
        "reference entry takes the next free suffix in: {html}"
    );
}

#[test]
fn reference_ids_dedupe_without_a_bibliography_pool() {
    // Tier-2 citations (in-document defs, no pool): no use-site anchors, but
    // the `ref-{key}` entry and every forward href still join the namespace.
    let citations = Citations::new();
    let options = Options::new().with_extension(&citations);
    let html = carve::to_html_with_options(
        "{#ref-foo}\nReserved.\n\nSee [@foo].\n\n[@foo]: Entry foo.",
        &options,
    );
    assert!(
        html.contains("<a data-cite-key=\"foo\" href=\"#ref-foo-2\">1</a>"),
        "forward link follows the bumped reference id in: {html}"
    );
    assert!(
        html.contains("<li id=\"ref-foo-2\">Entry foo."),
        "reference entry takes the next free suffix in: {html}"
    );
}
