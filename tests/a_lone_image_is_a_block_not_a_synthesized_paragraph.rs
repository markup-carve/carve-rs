//! markup-carve/carve-rs#1334. `html_to_ast` wrapped a bare `<img>` in a
//! paragraph the document never contained, while `html_to_carve` wrote the bare
//! image beside it - so the two exits disagreed about an import this engine
//! built itself.
//!
//! Measured on `main` at `6ae35bd6`:
//!
//! ```text
//! html_to_carve -> "![G](g.jpg)\n"                      diagnostics = []
//! parse(that)   -> [BlockImage]
//! html_to_ast   -> [Paragraph { children: [Image] }]    diagnostics = []
//! ```
//!
//! `docs/html-import.md` makes `parse(html_to_carve(h)) == html_to_ast(h)` the
//! invariant and `structure-unspellable` its ONE carve-out. No row was emitted
//! here, so this was not the carve-out - it was the invariant failing.
//!
//! ## Why the wrapper goes, rather than the difference being declared
//!
//! `caption_host` already takes this wrapper off a `<figure>` body and says why:
//! HTML has no block/inline slot distinction, so `blocks_at` puts a stray inline
//! into a paragraph to have somewhere to put it, and the wrapper is OURS rather
//! than the author's. That reasoning never depended on a `<figure>` being
//! present - `caption_host` was simply the only place it was reached from.
//! `resources/examples/edge-cases.md` rules the shape the same way: "a paragraph
//! whose whole content is one image is still the standalone image shape, not a
//! wrapped one".
//!
//! AN ADDITION IS NOT A LOSS. A declared loss is a ceiling an import may sit
//! inside; a synthesized paragraph is the document coming back saying something
//! it never said. Only the second changes what the document MEANS, so it takes
//! no diagnostic row - it gets removed.
//!
//! ## The opposite call on the same two nodes
//!
//! carve-rs#1331 declares a LOSS for `<p><img></p>`: there the paragraph is the
//! AUTHOR's, the tree is faithful, and the WRITER is the exit that changes the
//! document. Here nothing authored a paragraph. The deciding question is only
//! ever whether the document contained a `<p>` - which is why an authored one
//! arrives through `block` and never through the inline buffer this touches.
//! Both directions are asserted below, in the same file, so a change that
//! collapses them fails here.
//!
//! Ported from markup-carve/carve-js#1411, fixed there in carve-js#1414.

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
fn a_bare_image_is_a_block_image_on_both_exits() {
    let html = r#"<img src="g.jpg" alt="G">"#;
    assert_eq!(kinds(&tree(html)), vec!["BlockImage".to_string()]);
    assert_eq!(carve(html), "![G](g.jpg)\n");
    assert_eq!(kinds(&parse(&carve(html))), vec!["BlockImage".to_string()]);
}

/// The shape the spec fixture `detached-caption-caret` records, which is what
/// found this in carve-js.
#[test]
fn the_detached_caption_caret_shape_agrees_on_both_exits() {
    let html = "<img src=\"g.jpg\" alt=\"G\">\n<p>^ c</p>";
    assert_eq!(
        kinds(&tree(html)),
        vec!["BlockImage".to_string(), "Paragraph".to_string()]
    );
    assert_eq!(kinds(&parse(&carve(html))), kinds(&tree(html)));
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
            ":: t\n:  ![G](g.jpg)\n",
        ),
        (
            r#"<section><img src="g.jpg" alt="G"></section>"#,
            "![G](g.jpg)\n",
        ),
    ];
    for (html, written) in cases {
        assert_eq!(carve(html), written, "{html}");
        assert_eq!(
            kinds(&parse(&carve(html))),
            kinds(&tree(html)),
            "{html}: the two exits must agree"
        );
        assert!(
            !format!("{:?}", tree(html)).contains("Paragraph"),
            "{html}: a synthesized wrapper is still in the tree"
        );
    }
}

/// AN ADDITION IS REMOVED, NOT DECLARED. None of these owes a row: nothing was
/// lost, and a `structure-unspellable` row is read as licence to stop comparing
/// the two exits.
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

/// THE OPPOSITE CALL, ON THE SAME TWO NODES. An AUTHORED `<p>` around a lone
/// image keeps its paragraph in the tree and takes a declared
/// `structure-unspellable` row for what the writer loses (carve-rs#1331). A fix
/// that removed the wrapper here as well would be taking off something the
/// document held, which is a loss and a different call entirely.
#[test]
fn an_authored_paragraph_around_the_same_image_is_untouched() {
    let html = r#"<p><img src="g.jpg" alt="G"></p>"#;
    assert_eq!(kinds(&tree(html)), vec!["Paragraph".to_string()]);
    assert_eq!(rows(html), 1);
    // And the bare spelling beside it, so the two are pinned against each other
    // rather than each on its own.
    let bare = r#"<img src="g.jpg" alt="G">"#;
    assert_eq!(kinds(&tree(bare)), vec!["BlockImage".to_string()]);
    assert_eq!(rows(bare), 0);
    assert_eq!(carve(html), carve(bare), "the two write the same source");
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
