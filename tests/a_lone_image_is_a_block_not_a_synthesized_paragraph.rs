//! The resolved AST represents a standalone image as a paragraph with
//! `blockImage: true`, including images imported without an authored `<p>`.
//!
//! Measured on `main` at `6ae35bd6`:
//!
//! ```text
//! html_to_carve -> "![G](g.jpg)\n"                      diagnostics = []
//! parse(that)   -> [Paragraph { block_image: true }]
//! html_to_ast   -> [Paragraph { block_image: true }]    diagnostics = []
//! ```
//!
//! `docs/html-import.md` makes `parse(html_to_carve(h)) == html_to_ast(h)` the
//! invariant and `structure-unspellable` its ONE carve-out. No row was emitted
//! here, so this was not the carve-out - it was the invariant failing.
//!
//! PART 12 section 23 makes the block-image flag a paragraph field. The
//! importer's synthesized and authored image runs use that same wire shape.

use carve::{html_to_ast, html_to_carve, parse, render_html, HtmlImportOptions};

fn kinds(document: &carve::Document) -> Vec<String> {
    document
        .children
        .iter()
        .map(|block| {
            format!("{block:?}")
                .split('(')
                .next()
                .expect("a variant name")
                .to_string()
        })
        .collect()
}

fn tree(html: &str) -> carve::Document {
    html_to_ast(html, &HtmlImportOptions::default())
        .expect("import")
        .value
}

fn carve(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .expect("import")
        .value
}

fn rows(html: &str) -> usize {
    html_to_carve(html, &HtmlImportOptions::default())
        .expect("import")
        .report
        .diagnostics
        .iter()
        .filter(|d| d.code == carve::HtmlImportDiagnosticCode::StructureUnspellable)
        .count()
}

#[test]
fn a_bare_image_is_a_block_image_paragraph_on_both_exits() {
    let html = r#"<img src="g.jpg" alt="G">"#;
    assert_eq!(kinds(&tree(html)), vec!["Paragraph".to_string()]);
    assert_eq!(carve(html), "![G](g.jpg)\n");
    assert_eq!(kinds(&parse(&carve(html))), vec!["Paragraph".to_string()]);
}

/// The shape the spec fixture `detached-caption-caret` records, which is what
/// found this in carve-js.
#[test]
fn the_detached_caption_caret_shape_agrees_on_both_exits() {
    let html = "<img src=\"g.jpg\" alt=\"G\">\n<p>^ c</p>";
    assert_eq!(
        kinds(&tree(html)),
        vec!["Paragraph".to_string(), "Paragraph".to_string()]
    );
    assert_eq!(
        render_html(&parse(&carve(html))).unwrap(),
        render_html(&tree(html)).unwrap()
    );
}

/// IT IS NOT A TOP-LEVEL RULE. The wrapper is synthesized wherever a stray
/// inline run needs somewhere to live, so the fix has to reach every container -
/// a walker that only looked at the document's own children would leave the
/// disagreement in place one level down.
#[test]
fn it_reaches_a_bare_image_inside_a_container() {
    let cases: [(&str, &str); 5] = [
        (r#"<div><img src="g.jpg" alt="G"></div>"#, "![G](g.jpg)\n"),
        (
            r#"<blockquote><img src="g.jpg" alt="G"></blockquote>"#,
            "> ![G](g.jpg)\n",
        ),
        (
            r#"<ul><li><img src="g.jpg" alt="G"></li></ul>"#,
            "- ![G](g.jpg)\n",
        ),
        (
            r#"<dl><dt>t</dt><dd><img src="g.jpg" alt="G"></dd></dl>"#,
            ":: t\n: ![G](g.jpg)\n",
        ),
        (
            r#"<section><img src="g.jpg" alt="G"></section>"#,
            "![G](g.jpg)\n",
        ),
    ];
    for (html, written) in cases {
        assert_eq!(carve(html), written, "{html}");
        assert_eq!(
            render_html(&parse(&carve(html))).unwrap(),
            render_html(&tree(html)).unwrap(),
            "{html}: the two exits must render alike"
        );
        assert!(
            format!("{:?}", tree(html)).contains("block_image: true"),
            "{html}: the image paragraph needs its block-image flag"
        );
    }
}

/// The paragraph shape carries no import loss.
#[test]
fn removing_the_addition_reports_nothing() {
    for html in [
        r#"<img src="g.jpg" alt="G">"#,
        r#"<div><img src="g.jpg" alt="G"></div>"#,
        r#"<ul><li><img src="g.jpg" alt="G"></li></ul>"#,
    ] {
        assert_eq!(rows(html), 0, "{html}");
        assert!(
            html_to_ast(html, &HtmlImportOptions::default())
                .expect("import")
                .report
                .diagnostics
                .is_empty(),
            "{html}"
        );
    }
}

/// THE RENDERED DOCUMENT DOES NOT MOVE. The tree shape changes and the meaning
/// does not - which is the claim that makes removing the wrapper safe rather
/// than merely tidier.
#[test]
fn the_rendered_html_is_unchanged() {
    let cases: [(&str, &str); 3] = [
        (
            r#"<img src="g.jpg" alt="G">"#,
            "<img src=\"g.jpg\" alt=\"G\">",
        ),
        (
            r#"<blockquote><img src="g.jpg" alt="G"></blockquote>"#,
            "<blockquote>\n  <img src=\"g.jpg\" alt=\"G\">\n</blockquote>",
        ),
        (
            r#"<ul><li><img src="g.jpg" alt="G"></li></ul>"#,
            "<ul>\n  <li>\n    <img src=\"g.jpg\" alt=\"G\">\n  </li>\n</ul>",
        ),
    ];
    for (html, rendered) in cases {
        assert_eq!(
            render_html(&parse(&carve(html))).expect("render"),
            rendered,
            "{html}"
        );
    }
}

// -- THE BOUNDS ------------------------------------------------------------

/// ONLY A RUN THAT HOLDS NOTHING ELSE. A run carrying text, or a second image,
/// is a paragraph the document really has - it is what `![a](a) folding content`
/// parses to as well - so the wrapper is not ours to remove there.
#[test]
fn a_run_holding_anything_else_stays_a_paragraph() {
    let cases: [(&str, &str); 3] = [
        (r#"<img src="g.jpg" alt="G"> text"#, "![G](g.jpg) text\n"),
        (
            r#"<img src="a" alt="a"><img src="b" alt="b">"#,
            "![a](a)![b](b)\n",
        ),
        (r#"text <img src="g.jpg" alt="G">"#, "text ![G](g.jpg)\n"),
    ];
    for (html, written) in cases {
        assert_eq!(carve(html), written, "{html}");
        assert_eq!(kinds(&tree(html)), vec!["Paragraph".to_string()], "{html}");
        assert_eq!(kinds(&parse(&carve(html))), kinds(&tree(html)), "{html}");
    }
}

/// An authored image-only paragraph follows the same block-image rule.
#[test]
fn an_authored_paragraph_around_the_same_image_is_promoted() {
    let html = r#"<p><img src="g.jpg" alt="G"></p>"#;
    assert_eq!(kinds(&tree(html)), vec!["Paragraph".to_string()]);
    assert_eq!(rows(html), 0);
    let bare = r#"<img src="g.jpg" alt="G">"#;
    assert_eq!(kinds(&tree(bare)), vec!["Paragraph".to_string()]);
    assert_eq!(rows(bare), 0);
    assert_eq!(carve(html), carve(bare));
}

/// A FIGURE BODY WAS ALREADY RIGHT, and stays right. `caption_host` is where
/// this reasoning was already applied, so the target is the image whichever
/// spelling the HTML used.
#[test]
fn a_figure_body_still_takes_the_image_as_its_target() {
    for html in [
        r#"<figure><img src="i.png" alt="a"><figcaption>cap</figcaption></figure>"#,
        r#"<figure><p><img src="i.png" alt="a"></p><figcaption>cap</figcaption></figure>"#,
    ] {
        assert_eq!(kinds(&tree(html)), vec!["Figure".to_string()], "{html}");
        assert_eq!(carve(html), "![a](i.png)\n^ cap\n", "{html}");
        assert_eq!(rows(html), 0, "{html}");
    }
}

/// A TABLE CELL HOLDS INLINES, so no wrapper is ever built there and there is
/// nothing to remove.
#[test]
fn a_table_cell_builds_no_wrapper_to_remove() {
    let html = r#"<table><tr><td><img src="g.jpg" alt="G"></td></tr></table>"#;
    assert_eq!(carve(html), "| ![G](g.jpg) |\n");
    assert_eq!(kinds(&parse(&carve(html))), kinds(&tree(html)));
    assert!(!format!("{:?}", tree(html)).contains("Paragraph"));
}
