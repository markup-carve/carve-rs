//! An auto heading slug must never land on an id an explicit `{#id}` already
//! claims, or the document emits the same id twice - invalid HTML, where
//! `getElementById`, `:target` and every `#id` anchor resolve to the first
//! match and the second heading becomes unreachable (#335).

fn ids(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find("id=\"") {
        rest = &rest[i + 4..];
        let end = rest.find('"').unwrap_or(0);
        out.push(rest[..end].to_string());
        rest = &rest[end..];
    }
    out
}

const DOC: &str = "{#API-2}\n# Other\n\n# API\n\n# API\n";

#[test]
fn an_auto_slug_skips_an_id_an_explicit_one_claims() {
    let got = ids(&carve::to_html(DOC));

    assert_eq!(got, vec!["API-2", "API", "API-3"], "{got:?}");
}

#[test]
fn no_id_is_emitted_twice() {
    let got = ids(&carve::to_html(DOC));
    let mut unique = got.clone();
    unique.sort();
    unique.dedup();

    assert_eq!(got.len(), unique.len(), "duplicate id in {got:?}");
}

#[test]
fn a_cross_reference_resolves_to_the_heading_that_owns_the_id() {
    // The crossref index numbers headings too, and had its own copy of the rule
    // without the skip - so `</#api-2>` resolved to the id of the explicit
    // heading while carrying the WRONG heading's title.
    let html = carve::to_html("{#API-2}\n# Other\n\n# API\n\n# API\n\nSee </#api-2>.\n");

    assert!(html.contains("<a href=\"#API-2\">Other</a>"), "{html}");
}

#[test]
fn a_cross_reference_to_the_skipped_slug_resolves() {
    let html = carve::to_html("{#API-2}\n# Other\n\n# API\n\n# API\n\nSee </#api-3>.\n");

    assert!(html.contains("<a href=\"#API-3\">API</a>"), "{html}");
}

#[test]
fn the_markdown_target_agrees_with_the_html_one() {
    // Three separate copies of the numbering rule exist (seeder, renderer,
    // crossref index). They have to produce the same sequence or a reference
    // points at different headings depending on the target.
    let md = carve::to_markdown("{#API-2}\n# Other\n\n# API\n\n# API\n\nSee </#api-3>.\n");

    assert!(md.contains("[API](#API-3)"), "{md}");
}
