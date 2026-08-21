//! The `sections` option (spec PART 9 §13) and attribute order on an
//! unwrapped heading (PART 10 §1).
//!
//! Off, the renderer emits no `<section>`: the id returns to the `<h*>` and the
//! blocks that would have been section children stay as siblings. That is the
//! shape a heading inside a container has always rendered, which is the point -
//! one placement rule for the whole document instead of two.
//!
//! The attribute-order cases matter more than they look. carve-rs used to write
//! the id FIRST on an unwrapped heading, which agreed with no other engine, and
//! nothing caught it: the only way to reach that code was a heading inside a
//! container, and no corpus case gave such a heading attributes. The option
//! makes every heading take that path, so the divergence would have stopped
//! being rare. carve-js is canonical; these pin its answer.

use carve::{to_html, to_html_with_options, Options};

fn flat(src: &str) -> String {
    to_html_with_options(src, &Options::new().with_sections(false))
}

#[test]
fn wraps_by_default() {
    assert_eq!(
        to_html("# A\n\np\n"),
        "<section id=\"A\">\n  <h1>A</h1>\n  <p>p</p>\n</section>"
    );
}

#[test]
fn emits_no_wrapper_and_keeps_the_id_on_the_heading() {
    assert_eq!(flat("# A\n\np\n"), "<h1 id=\"A\">A</h1>\n<p>p</p>");
}

#[test]
fn flattens_nested_levels() {
    assert_eq!(
        flat("# A\n\np\n\n## B\n\nq\n"),
        "<h1 id=\"A\">A</h1>\n<p>p</p>\n<h2 id=\"B\">B</h2>\n<p>q</p>"
    );
}

#[test]
fn flattens_adjacent_same_level_headings() {
    assert_eq!(
        flat("# A\n\n# B\n"),
        "<h1 id=\"A\">A</h1>\n<h1 id=\"B\">B</h1>"
    );
}

#[test]
fn flattens_a_skipped_level() {
    assert_eq!(
        flat("# A\n\n### C\n"),
        "<h1 id=\"A\">A</h1>\n<h3 id=\"C\">C</h3>"
    );
}

#[test]
fn changes_nothing_without_headings() {
    let src = "just a paragraph\n\n- and a list\n";
    assert_eq!(flat(src), to_html(src));
}

#[test]
fn leaves_container_headings_alone() {
    let src = "> # Quoted\n>\n> Quoted body.\n\n:::\n# Divved\n:::\n";
    assert_eq!(flat(src), to_html(src));
}

#[test]
fn a_top_level_heading_matches_the_same_heading_inside_a_div() {
    // The equivalence the option is built on.
    let in_div = to_html(":::\n{a=b .c}\n# Same\n:::\n");
    let inner: Vec<&str> = in_div.lines().collect();
    let inner = inner[1..inner.len() - 1]
        .iter()
        .map(|l| l.strip_prefix("  ").unwrap_or(l))
        .collect::<Vec<_>>()
        .join("\n");

    assert_eq!(flat("{a=b .c}\n# Same\n"), inner);
}

#[test]
fn resolves_crossrefs_and_implicit_heading_references() {
    assert_eq!(
        flat("# Target\n\nSee </#target> and [Target][].\n"),
        "<h1 id=\"Target\">Target</h1>\n\
         <p>See <a href=\"#Target\">Target</a> and <a href=\"#Target\">Target</a>.</p>"
    );
}

#[test]
fn keeps_the_dedup_namespace_intact() {
    assert_eq!(
        flat("# abc\n\n> # abc\n\n# abc\n"),
        "<h1 id=\"abc\">abc</h1>\n\
         <blockquote>\n  <h1 id=\"abc-2\">abc</h1>\n</blockquote>\n\
         <h1 id=\"abc-3\">abc</h1>"
    );
}

#[test]
fn still_emits_the_endnotes_region() {
    let out = flat("# A\n\nText[^n].\n\n[^n]: Note.\n");
    assert!(out.contains("<h1 id=\"A\">A</h1>"), "{out}");
    assert!(
        out.contains("<section role=\"doc-endnotes\" aria-label=\"Footnotes\">"),
        "{out}"
    );
    assert!(!out.contains("<section id="), "{out}");
}

#[test]
fn appends_a_generated_id_after_the_authors_attributes() {
    assert_eq!(
        to_html("> {a=b .c}\n> # Auto\n"),
        "<blockquote>\n  <h1 a=\"b\" class=\"c\" id=\"Auto\">Auto</h1>\n</blockquote>"
    );
    assert_eq!(
        flat("{a=b .c}\n# Auto\n"),
        "<h1 a=\"b\" class=\"c\" id=\"Auto\">Auto</h1>"
    );
}

#[test]
fn keeps_an_authored_id_in_its_source_position() {
    // Written by the author, so not generated: it is not moved to the end the
    // way an auto slug is.
    assert_eq!(
        to_html("> {#x a=b}\n> # Written\n"),
        "<blockquote>\n  <h1 id=\"x\" a=\"b\">Written</h1>\n</blockquote>"
    );
    assert_eq!(
        flat("{#x a=b}\n# Written\n"),
        "<h1 id=\"x\" a=\"b\">Written</h1>"
    );
}

#[test]
fn the_generated_id_precedes_the_source_line_stamp() {
    // `data-source-line` is a render annotation and is emitted last, so the
    // generated id goes before it. carve-rs stamps it as an ordinary key-value
    // at parse time, which is exactly how a first cut of this change put the id
    // behind it. Canonical (carve-js): <h2 id="Nested" data-source-line="4">.
    let opts = Options::new().with_source_lines(true);
    let html = to_html_with_options("> ## Nested\n", &opts);
    assert!(
        html.contains("<h2 id=\"Nested\" data-source-line=\"1\">"),
        "{html}"
    );

    let flat_opts = Options::new().with_source_lines(true).with_sections(false);
    let flat_html = to_html_with_options("{a=b}\n## Nested\n", &flat_opts);
    assert!(
        flat_html.contains("<h2 a=\"b\" id=\"Nested\" data-source-line=\"2\">"),
        "{flat_html}"
    );
}
