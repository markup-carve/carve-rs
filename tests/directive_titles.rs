use carve::extensions::{Glossary, Index, TocPlacement};
use carve::{from_json, parse, render_carve, to_html, to_html_with_options, to_json, Options};

const NOTES: &str = "Intro[^a].\n\n::: footnotes \"Notes\" [End]\n:::\n\n[^a]: first note\n";

#[test]
fn footnotes_title_and_label_are_first_children() {
    assert_eq!(
        to_html(NOTES),
        "<p>Intro<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a>.</p>\n<section role=\"doc-endnotes\" aria-labelledby=\"adm-1\">\n  <p class=\"admonition-title\" id=\"adm-1\">Notes</p>\n  <p class=\"div-label\">End</p>\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>first note<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

#[test]
fn untitled_marker_keeps_its_name() {
    let html = to_html("Intro[^a].\n\n::: footnotes\n:::\n\n[^a]: first note\n");
    assert!(
        html.contains("<section role=\"doc-endnotes\" aria-label=\"Footnotes\">"),
        "{html}"
    );
}

#[test]
fn ids_share_the_admonition_sequence() {
    let before = to_html(
        "::: note \"Earlier\"\n:::\n\nIntro[^a].\n\n::: footnotes \"Notes\"\n:::\n\n[^a]: note\n",
    );
    assert!(
        before.contains("<aside class=\"admonition note\" aria-labelledby=\"adm-1\">"),
        "{before}"
    );
    assert!(
        before.contains("<section role=\"doc-endnotes\" aria-labelledby=\"adm-2\">"),
        "{before}"
    );

    let inside = to_html(
        "Intro[^a].\n\n::: footnotes \"Notes\"\n::: note \"Inside\"\n:::\n:::\n\n[^a]: note\n",
    );
    assert!(
        inside.contains("<section role=\"doc-endnotes\" aria-labelledby=\"adm-1\">"),
        "{inside}"
    );
    assert!(
        inside.contains("<aside class=\"admonition note\" aria-labelledby=\"adm-2\">"),
        "{inside}"
    );
}

#[test]
fn unplaced_directive_is_a_generic_div() {
    assert_eq!(to_html("::: toc \"Contents\" [T]\n:::\n"), "<div class=\"toc\">\n  <p class=\"admonition-title\">Contents</p>\n  <p class=\"div-label\">T</p>\n</div>");
}

#[test]
fn unplaced_directive_takes_no_generated_aria_name() {
    // The ENGINE writes neither naming attribute on a degraded div and mints no
    // id, because ARIA prohibits both on role `generic`. The AUTHOR's own
    // attribute is theirs and survives verbatim, casing included - carve-js and
    // carve-php both keep it (CARVE-P9-072).
    let html = to_html("{ARIA-LABEL=Custom}\n::: toc \"Contents\" [T]\n:::\n");
    assert!(
        html.starts_with("<div class=\"toc\" ARIA-LABEL=\"Custom\">"),
        "{html}"
    );
    assert!(!html.contains("aria-labelledby"), "{html}");
    assert!(!html.contains("adm-"), "{html}");
}

#[test]
fn a_reference_in_a_directive_title_is_numbered() {
    // The title moves onto the extension carrier that renders in the
    // directive's place, so collection has to reach it there; collecting on the
    // container alone numbered a copy nobody emits.
    let toc = TocPlacement::new();
    let mut options = Options::new();
    options.extensions.push(&toc);
    let html = to_html_with_options("::: toc \"See[^a]\"\n:::\n\n# H\n\n[^a]: note\n", &options);
    assert!(
        html.contains("<p class=\"admonition-title\" id=\"adm-1\">See<a id=\"fnref1\""),
        "{html}"
    );
    assert!(html.contains("role=\"doc-endnotes\""), "{html}");
}

#[test]
fn a_titled_nav_keeps_the_fragment_at_column_zero() {
    // Extensions section 8b.3 makes the nav fragment the cross-impl contract, so
    // a title must not shift the list that follows it.
    let toc = TocPlacement::new();
    let mut options = Options::new();
    options.extensions.push(&toc);
    let html = to_html_with_options("::: toc \"Contents\"\n:::\n\n# H\n", &options);
    assert!(
        html.contains("<nav class=\"toc\" aria-labelledby=\"adm-1\">\n<p class=\"admonition-title\" id=\"adm-1\">Contents</p>\n<ul>\n"),
        "{html}"
    );
}

#[test]
fn glossary_tokens_precede_its_definition_list() {
    let mut options = Options::new();
    let glossary = Glossary::new();
    options.extensions.push(&glossary);
    let html = to_html_with_options(
        "::: glossary \"Words\" [G]\n:: term\n: meaning\n:::\n",
        &options,
    );
    assert!(html.contains("<p class=\"admonition-title\">Words</p>\n<p class=\"div-label\">G</p>\n<dl class=\"glossary\">"), "{html}");
}

#[test]
fn glossary_tokens_stay_next_to_the_first_list() {
    let glossary = Glossary::new();
    let mut options = Options::new();
    options.extensions.push(&glossary);
    let html = to_html_with_options(
        "::: glossary \"Words\" [G]\nPreface\n\n:: term\n: meaning\n:::\n",
        &options,
    );
    assert!(html.contains("<p>Preface</p>\n<p class=\"admonition-title\">Words</p>\n<p class=\"div-label\">G</p>\n<dl class=\"glossary\">"), "{html}");
}

#[test]
fn index_tokens_precede_its_list() {
    let index = Index::new();
    let mut options = Options::new();
    options.extensions.push(&index);
    let html = to_html_with_options(
        "A :index[term] here.\n\n::: index \"Terms\" [I]\n:::\n",
        &options,
    );
    assert!(
        html.contains("<p class=\"admonition-title\">Terms</p>\n<p class=\"div-label\">I</p>\n<ul class=\"index\">"),
        "{html}"
    );
}

#[test]
fn footnotes_ignore_authored_name_on_the_marker() {
    let html =
        to_html("Intro[^a].\n\n{ARIA-LABEL=Custom}\n::: footnotes \"Notes\"\n:::\n\n[^a]: note\n");
    assert!(
        html.contains("<section role=\"doc-endnotes\" aria-labelledby=\"adm-1\">"),
        "{html}"
    );
}

#[test]
fn first_footnotes_marker_owns_the_placed_section() {
    let html = to_html(
        "A[^a].\n\n::: footnotes \"First\"\n:::\n\n::: footnotes \"Second\"\n:::\n\n[^a]: note\n",
    );
    assert!(
        html.contains("<p class=\"admonition-title\" id=\"adm-1\">First</p>"),
        "{html}"
    );
    assert!(
        html.contains("<div class=\"footnotes\">\n  <p class=\"admonition-title\">Second</p>"),
        "{html}"
    );
}

#[test]
fn toc_nav_contains_tokens_and_uses_the_title_as_its_name() {
    let mut options = Options::new();
    let toc = TocPlacement::new();
    options.extensions.push(&toc);
    let html = to_html_with_options("# Heading\n\n::: toc \"Contents\" [T]\n:::\n", &options);
    // Column 0 for everything inside the nav, whatever the nav's own indent:
    // extensions section 8b.3 makes this fragment the cross-impl contract.
    assert!(
        html.contains("<nav class=\"toc\" aria-labelledby=\"adm-1\">\n<p class=\"admonition-title\" id=\"adm-1\">Contents</p>\n<p class=\"div-label\">T</p>\n<ul>"),
        "{html}"
    );
}

#[test]
fn authored_toc_name_wins_without_minting_an_id() {
    let mut options = Options::new();
    let toc = TocPlacement::new();
    options.extensions.push(&toc);
    let html = to_html_with_options(
        "# Heading\n\n{ARIA-LABEL=Custom}\n::: toc \"Contents\"\n:::\n",
        &options,
    );
    assert!(html.contains("ARIA-LABEL=\"Custom\""), "{html}");
    assert!(
        html.contains("<p class=\"admonition-title\">Contents</p>"),
        "{html}"
    );
    assert!(!html.contains("adm-1"), "{html}");
}

#[test]
fn ast_and_source_round_trips_keep_the_title() {
    let source = "::: toc \"Contents\" [T]\n:::\n";
    let document = parse(source);
    let json = to_json(&document);
    assert!(
        json.contains("\"title\":[{\"type\":\"text\",\"value\":\"Contents\"}"),
        "{json}"
    );
    let ingested = from_json(&json).expect("directive title is in the schema");
    assert_eq!(to_json(&ingested), json);
    let formatted = render_carve(&ingested).expect("directive title formats");
    assert!(
        formatted.starts_with("::: toc \"Contents\" [T]\n"),
        "{formatted}"
    );
    assert!(to_json(&parse(&formatted))
        .contains("\"title\":[{\"type\":\"text\",\"value\":\"Contents\"}]"));
    let cli_json = carve::try_to_json_with_options(source, &Options::new().with_positions(true))
        .expect("the CLI AST path accepts the source");
    let cli_ingested = from_json(&cli_json).expect("CLI AST ingests");
    assert!(to_json(&cli_ingested).contains("\"value\":\"Contents\""));
    let cli_formatted = carve::to_carve(source);
    assert!(cli_formatted.starts_with("::: toc \"Contents\" [T]\n"));
    assert_eq!(carve::to_carve(&cli_formatted), cli_formatted);
}
