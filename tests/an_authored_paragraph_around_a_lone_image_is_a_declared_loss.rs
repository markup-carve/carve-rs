//! markup-carve/carve-rs#1331. `<p><img></p>` - a paragraph the AUTHOR wrote,
//! holding nothing but an image - is written `![G](g.jpg)`, which re-reads as a
//! BLOCK image. The tree keeps the paragraph, the source does not, and nothing
//! said so.
//!
//! ## Why it is a declared loss and not a change of output
//!
//! Because Carve source cannot spell it. `resources/examples/edge-cases.md`
//! settles the shape - "a paragraph whose whole content is one image is still
//! the standalone image shape, not a wrapped one" - and `docs/html-import.md`
//! settles what to do about it:
//!
//!   `structure-unspellable`: the import produced a structure Carve source has
//!   no spelling for, so it survives in the AST and not in written Carve. The
//!   AST-returning entry point loses nothing and reports nothing; the one that
//!   writes source reports this.
//!
//! The same page makes it the ONE carve-out to `parse(html_to_carve(h)) ==
//! html_to_ast(h)`: "where a row carries it the two exits differ by exactly the
//! structure that row names". This engine has BOTH exits, so every case below
//! asserts both.
//!
//! ## The indented spelling was measured, and on this engine it is not one
//!
//! carve-js reads ` ![G](g.jpg)` - one leading space - as a paragraph holding
//! one image, so the shape looks spellable at first reach there; it rejected the
//! reading because the canonical writer normalizes the indent away and a list
//! marker absorbs the padding at every width. This engine reads the indented
//! line as a block image too, so there is no indent at which the source says
//! "paragraph" here at all. The near miss is pinned below; the wider ruling
//! question is markup-carve/carve#1658.
//!
//! Ported from markup-carve/carve-js#1419; the fix and its measurements are in
//! markup-carve/carve-js#1422.

use carve::{html_to_ast, html_to_carve, parse, HtmlImportDiagnosticCode, HtmlImportOptions};

const HEAD: &str =
    "A paragraph holding nothing but an image has no Carve spelling; the image is written as a block";

fn rows(html: &str) -> Vec<String> {
    html_to_carve(html, &HtmlImportOptions::default())
        .expect("import")
        .report
        .diagnostics
        .into_iter()
        .filter(|d| d.code == HtmlImportDiagnosticCode::StructureUnspellable)
        .map(|d| d.message)
        .collect()
}

fn carve(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .expect("import")
        .value
}

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

#[test]
fn it_reports_the_paragraph_it_cannot_spell() {
    let html = r#"<p><img src="g.jpg" alt="G"></p>"#;
    assert_eq!(carve(html), "![G](g.jpg)\n");
    assert_eq!(
        rows(html),
        vec![format!("{HEAD}, which renders without the <p> around it")]
    );
}

/// The row names EXACTLY the structure the two exits differ by, which is what
/// makes it a carve-out rather than an excuse: the tree keeps the paragraph, the
/// re-parsed source has the image as a block, and nothing else moves.
#[test]
fn it_declares_the_difference_the_two_exits_actually_have() {
    let html = r#"<p><img src="g.jpg" alt="G"></p>"#;
    assert_eq!(kinds(&tree(html)), vec!["Paragraph".to_string()]);
    assert_eq!(
        kinds(&parse(&carve(html))),
        vec!["BlockImage".to_string()],
        "the written source re-reads as a block image, not as the paragraph it was"
    );
    assert_eq!(rows(html).len(), 1);
}

/// PART 12 section 16's split, and the half that is easy to get wrong: the tree
/// exit loses nothing here, so it must stay silent.
#[test]
fn it_says_nothing_on_the_exit_that_keeps_the_tree() {
    let report = html_to_ast(
        r#"<p><img src="g.jpg" alt="G"></p>"#,
        &HtmlImportOptions::default(),
    )
    .expect("import")
    .report;
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
}

/// LAYOUT IS NOT CONTENT (PART 11 section 7), so the padded spelling is the same
/// paragraph and takes the same row.
///
/// WHAT MAKES IT THE SAME PARAGRAPH IS THE IMPORT-SIDE TRIM, not a tolerance in
/// the predicate. `trim_edge_whitespace` runs on the authored arm since
/// carve-rs#1336, so the run reaches `lone_image` already trimmed and the plain
/// question answers it. Before that this test passed through a whitespace
/// tolerance inside the predicate, which is now removed - reachable by no input
/// once the trim was in front of it.
#[test]
fn it_reports_the_whitespace_padded_spelling_of_the_same_paragraph() {
    let html = "<p>\n  <img src=\"g.jpg\" alt=\"G\">\n</p>";
    assert_eq!(carve(html), "![G](g.jpg)\n");
    assert_eq!(rows(html).len(), 1);
}

/// The paragraph's OWN attributes do not vanish - they land on the image, so
/// `<p class="x">` comes back as `<img class="x">`. That is a different element
/// carrying them, and the row has to say which outcome happened.
#[test]
fn it_says_where_a_paragraph_attribute_went() {
    let html = r#"<p class="x"><img src="g.jpg" alt="G"></p>"#;
    assert_eq!(carve(html), "{.x}\n![G](g.jpg)\n");
    assert_eq!(
        rows(html),
        vec![format!(
            "{HEAD}, so the <p> is lost and the attributes it carried are written on the image instead"
        )]
    );
}

/// A MESSAGE THAT OVERCLAIMS LEAVES A LOSS UNDECLARED, which is the same defect
/// one level down. The paragraph's attributes are written as a block ABOVE the
/// image and the image's own braces after it, so a name BOTH set is decided by
/// the image: `{#p}` above `![a](a){#i}` reads back with `id="i"` alone and
/// `id="p"` is gone.
#[test]
fn it_names_each_attribute_the_image_overwrites() {
    let cases: [(&str, &str, &str); 3] = [
        (
            r#"<p id="p"><img id="i" src="a" alt="a"></p>"#,
            "{#p}\n![a](a){#i}\n",
            "id",
        ),
        (
            r#"<p data-x="p"><img data-x="i" src="a" alt="a"></p>"#,
            "{data-x=p}\n![a](a){data-x=i}\n",
            "data-x",
        ),
        (
            r#"<p id="p" data-x="p"><img id="i" data-x="i" src="a" alt="a"></p>"#,
            "{#p data-x=p}\n![a](a){#i data-x=i}\n",
            "data-x, id",
        ),
    ];
    for (html, written, named) in cases {
        assert_eq!(carve(html), written, "{html}");
        assert_eq!(
            rows(html),
            vec![format!(
                "{HEAD}, so the <p> is lost and the attributes it carried are written on the image - except {named}, which the image's own value overwrites"
            )],
            "{html}"
        );
    }
}

/// THE TWO NAMES THAT MUST NOT BE IN THAT SET, each for its own reason.
///
/// A class is not overwritten: the class slot MERGES, so `{.p}` and `{.i}` both
/// reach the rendered element. An image's `title` is not either - it is a field
/// of its own that goes into the DESTINATION's title slot, which is not the
/// attribute block, so it never collides with a `title=` the paragraph carried.
#[test]
fn it_names_no_attribute_the_image_does_not_overwrite() {
    let cases: [(&str, &str); 3] = [
        (
            r#"<p class="p"><img class="i" src="a" alt="a"></p>"#,
            "{.p}\n![a](a){.i}\n",
        ),
        // THE DISCRIMINATING SHAPE, and the one a different-named pair cannot
        // reach: BOTH carry the SAME class. The slot merges, so `x` is on the
        // element either way and nothing is overwritten - a rule that treated a
        // shared name as a collision would name it here and nowhere else.
        (
            r#"<p class="x"><img class="x" src="a" alt="a"></p>"#,
            "{.x}\n![a](a){.x}\n",
        ),
        (
            r#"<p title="t"><img title="i" src="a" alt="a"></p>"#,
            "{title=t}\n![a](a \"i\")\n",
        ),
    ];
    for (html, written) in cases {
        assert_eq!(carve(html), written, "{html}");
        assert_eq!(
            rows(html),
            vec![format!(
                "{HEAD}, so the <p> is lost and the attributes it carried are written on the image instead"
            )],
            "{html}"
        );
    }
}

#[test]
fn it_reports_it_at_every_level_the_tree_keeps_it() {
    let cases: [(&str, &str); 5] = [
        (r#"<p><img src="g.jpg" alt="G"></p>"#, "![G](g.jpg)\n"),
        (
            r#"<div><p><img src="g.jpg" alt="G"></p></div>"#,
            "![G](g.jpg)\n",
        ),
        (
            r#"<blockquote><p><img src="g.jpg" alt="G"></p></blockquote>"#,
            "> ![G](g.jpg)\n",
        ),
        (
            r#"<ul><li><p><img src="g.jpg" alt="G"></p></li></ul>"#,
            "{loose}\n- ![G](g.jpg)\n",
        ),
        (
            r#"<dl><dt>t</dt><dd><p><img src="g.jpg" alt="G"></p></dd></dl>"#,
            ":: t\n:  ![G](g.jpg)\n",
        ),
    ];
    for (html, written) in cases {
        assert_eq!(carve(html), written, "{html}");
        assert_eq!(rows(html).len(), 1, "{html}");
    }
}

#[test]
fn it_reports_each_of_two_such_paragraphs_once() {
    assert_eq!(
        rows(r#"<p><img src="g.jpg" alt="G"></p><p><img src="h.jpg" alt="H"></p>"#).len(),
        2
    );
}

// -- THE BOUNDS ------------------------------------------------------------
//
// A row that fires on a shape the writer CAN spell is worse than no row: it
// declares a loss that did not happen, and `docs/html-import.md` reads
// `structure-unspellable` as the licence to stop comparing the two exits. So
// the composition is checked, not the direction - every shape below keeps its
// meaning through the writer and must stay silent.

/// NO AUTHOR PARAGRAPH TO LOSE. A bare image is wrapped by `blocks_at` in a
/// paragraph the document never contained, so there is no `<p>` to declare -
/// and an image sharing its run builds a real paragraph that `![G](g.jpg) text`
/// re-reads as.
#[test]
fn it_reports_nothing_where_no_authored_paragraph_is_lost() {
    let cases: [(&str, &str); 4] = [
        (r#"<img src="g.jpg" alt="G">"#, "![G](g.jpg)\n"),
        (r#"<div><img src="g.jpg" alt="G"></div>"#, "![G](g.jpg)\n"),
        (
            r#"<p><img src="g.jpg" alt="G"> text</p>"#,
            "![G](g.jpg) text\n",
        ),
        (
            r#"<p><img src="g.jpg" alt="G"><img src="h.jpg" alt="H"></p>"#,
            "![G](g.jpg)![H](h.jpg)\n",
        ),
    ];
    for (html, written) in cases {
        assert_eq!(carve(html), written, "{html}");
        assert!(rows(html).is_empty(), "{html}: {:?}", rows(html));
    }
}

/// THE OVER-REACH A PREDICATE ON `block` ALONE MAKES. `caption_host` takes the
/// paragraph back off, so the figure's target is the image on BOTH exits and
/// there is no wrapper left to lose. The candidate is recorded and its
/// paragraph never reaches the tree.
#[test]
fn a_paragraph_a_figure_already_unwrapped_is_not_a_loss() {
    let html = r#"<figure><p><img src="i.png" alt="a"></p><figcaption>cap</figcaption></figure>"#;
    assert_eq!(carve(html), "![a](i.png)\n^ cap\n");
    assert_eq!(kinds(&tree(html)), vec!["Figure".to_string()]);
    assert!(rows(html).is_empty(), "{:?}", rows(html));
}

/// WHY THE SURVIVOR SCAN MARKS RATHER THAN COMPARES. These two paragraphs are
/// EQUAL as values - same image, no attributes - and one of them is unwrapped
/// into a figure target while the other survives. A scan that matched a
/// candidate against the finished tree by equality would credit the survivor to
/// the figure's paragraph and report a loss that did not happen; the mark tells
/// them apart, so exactly one row comes back and it names the paragraph that is
/// actually gone from the source.
#[test]
fn an_unwrapped_paragraph_is_not_credited_with_an_identical_survivor() {
    let html = r#"<figure><p><img src="a" alt="a"></p><figcaption>c</figcaption></figure><p><img src="a" alt="a"></p>"#;
    assert_eq!(carve(html), "![a](a)\n^ c\n\n![a](a)\n");
    assert_eq!(
        kinds(&tree(html)),
        vec!["Figure".to_string(), "Paragraph".to_string()]
    );
    let reported = html_to_carve(html, &HtmlImportOptions::default())
        .expect("import")
        .report
        .diagnostics
        .into_iter()
        .filter(|d| d.code == HtmlImportDiagnosticCode::StructureUnspellable)
        .map(|d| d.path)
        .collect::<Vec<_>>();
    assert_eq!(reported, vec![Some("/p[2]".to_string())]);
}

/// NOT A SURVIVOR BOUND, and it does not pretend to be one: a table cell holds
/// INLINES, so no paragraph is ever built for the scan to drop. What this pins
/// is the reason it is silent, which is that both exits already agree here.
#[test]
fn a_paragraph_a_table_cell_holds_as_inlines_is_not_a_loss() {
    let html = r#"<table><tr><td><p><img src="g.jpg" alt="G"></p></td></tr></table>"#;
    assert_eq!(carve(html), "| ![G](g.jpg) |\n");
    assert!(!format!("{:?}", tree(html)).contains("Paragraph"));
    assert!(rows(html).is_empty(), "{:?}", rows(html));
}

/// THE MARK COMES BACK OFF, on both exits and at every level. A mark left on a
/// paragraph would be a source position the parser never produced, handed to an
/// `html_to_ast` caller as if the document had been parsed from text.
#[test]
fn no_candidate_mark_survives_on_either_exit() {
    let cases = [
        r#"<p><img src="g.jpg" alt="G"></p>"#,
        r#"<blockquote><p><img src="g.jpg" alt="G"></p></blockquote>"#,
        r#"<ul><li><p><img src="g.jpg" alt="G"></p></li></ul>"#,
        r#"<dl><dt>t</dt><dd><p><img src="g.jpg" alt="G"></p></dd></dl>"#,
        r#"<div><p><img src="g.jpg" alt="G"></p></div>"#,
        r#"<figure><p><img src="i.png" alt="a"></p><figcaption>cap</figcaption></figure>"#,
        r#"<p><img src="g.jpg" alt="G"></p><p><img src="h.jpg" alt="H"></p>"#,
    ];
    for html in cases {
        let ast = html_to_ast(html, &HtmlImportOptions::default()).expect("import");
        assert!(
            !format!("{:?}", ast.value).contains("pos: Some"),
            "{html}: a candidate mark reached the tree exit"
        );
        let written = html_to_carve(html, &HtmlImportOptions::default()).expect("import");
        assert!(
            !written.value.contains('\u{0}'),
            "{html}: {:?}",
            written.value
        );
    }
}

/// THE NEAR MISS, and on this engine it is not even near. A reading that called
/// the shape spellable would point at an indented image: carve-js parses one
/// leading space as a paragraph holding one image. This engine reads it as a
/// block image, exactly as it reads the column-0 spelling, so there is no indent
/// at which the source says "paragraph" here.
#[test]
fn no_indent_spells_a_paragraph_holding_one_image() {
    for source in ["![G](g.jpg)", " ![G](g.jpg)", "   ![G](g.jpg)"] {
        assert_eq!(
            kinds(&parse(source)),
            vec!["BlockImage".to_string()],
            "{source:?}"
        );
    }
    assert_eq!(
        kinds(&parse("![G](g.jpg) t")),
        vec!["Paragraph".to_string()],
        "an image sharing its run is still a paragraph"
    );
}

// -- THE WRITER'S SIDE OF THE SAME RULING ----------------------------------
//
// markup-carve/carve#1658 ruled the shape a DECLARED WRITER CEILING, and named
// the half the ticket had not: `render_carve` states its contract as an
// absolute while carrying an exception nothing declares. A contract that is true
// except quietly is worse than a narrower one that is true as written. So the
// doc comment on `render_carve` now names its carve-outs, and the two tests
// below are what make that text load-bearing rather than decoration.
//
// The two rejected options are pinned as well as the chosen one, so a later
// change that reaches for either fails here rather than in review:
//
//  - the writer must NOT indent. It is lossless at the top level and nowhere
//    else, and it would make the writer emit meaning-bearing leading whitespace.
//  - the writer must NOT refuse. It is what it already does for an empty raw
//    inline, but it would break every import of a paragraph-wrapped image and it
//    contradicts `docs/html-import.md`, which says this exit REPORTS the loss.

/// THE CHOSEN OPTION, PINNED. `render_carve` normalizes and returns Ok: it does
/// not indent, and it does not refuse.
#[test]
fn the_writer_normalizes_the_shape_rather_than_indenting_or_refusing() {
    let document = tree(r#"<p><img src="g.jpg" alt="G"></p>"#);
    let written = carve::render_carve(&document).expect("the writer must not refuse this shape");
    assert_eq!(written, "![G](g.jpg)\n");
    assert!(
        !written.starts_with(' '),
        "the writer must not reach for an indented spelling: {written:?}"
    );
    assert_eq!(kinds(&parse(&written)), vec!["BlockImage".to_string()]);
}

/// THE CARVE-OUT IS EXACTLY ONE SHAPE, and this is the measurement that says so
/// rather than an assumption. carve#1658 asked for the property to be stated
/// rather than the node type, and for the answer to be closed rather than left
/// open: every other single-child paragraph the importer can build comes back as
/// the paragraph it was, so a paragraph holding one image is the whole list.
#[test]
fn a_lone_image_is_the_only_single_child_paragraph_the_writer_normalizes() {
    let one_child: [&str; 25] = [
        r#"<p><a href="u">t</a></p>"#,
        r#"<p><code>c</code></p>"#,
        r#"<p><em>e</em></p>"#,
        r#"<p><strong>b</strong></p>"#,
        r#"<p><span class="x">s</span></p>"#,
        r#"<p>text</p>"#,
        r#"<p><br></p>"#,
        r#"<p><q>q</q></p>"#,
        r#"<p><sub>s</sub></p>"#,
        r#"<p><sup>s</sup></p>"#,
        r#"<p><kbd>k</kbd></p>"#,
        r#"<p><abbr title="t">A</abbr></p>"#,
        r#"<p><del>d</del></p>"#,
        r#"<p><ins>i</ins></p>"#,
        r#"<p><mark>m</mark></p>"#,
        r#"<p><cite>c</cite></p>"#,
        r#"<p><time datetime="2020">t</time></p>"#,
        r#"<p><var>v</var></p>"#,
        r#"<p><samp>s</samp></p>"#,
        r#"<p><dfn>d</dfn></p>"#,
        r#"<p><u>u</u></p>"#,
        r#"<p><s>s</s></p>"#,
        r#"<p><i>i</i></p>"#,
        r#"<p><b>b</b></p>"#,
        // An image inside a LINK is not the shape: the paragraph's one child is
        // the link, which has a spelling of its own and keeps the paragraph.
        r#"<p><a href="u"><img src="g.jpg" alt="G"></a></p>"#,
    ];
    for html in one_child {
        assert_eq!(
            kinds(&parse(&carve(html))),
            vec!["Paragraph".to_string()],
            "{html} must survive the writer as the paragraph it was"
        );
        assert!(rows(html).is_empty(), "{html}: {:?}", rows(html));
    }
    // The shape itself, and the same shape reached through a `<picture>`, are
    // the two that do not.
    for html in [
        r#"<p><img src="g.jpg" alt="G"></p>"#,
        r#"<p><picture><img src="g.jpg" alt="G"></picture></p>"#,
    ] {
        assert_eq!(
            kinds(&parse(&carve(html))),
            vec!["BlockImage".to_string()],
            "{html}"
        );
        assert_eq!(rows(html).len(), 1, "{html}");
    }
}

/// THE SECOND CARVE-OUT, and it is the one a hand-built tree reaches. An empty
/// paragraph writes nothing, so the re-read document is one block shorter - and
/// the writer neither indents nor refuses there either. No source spells it: a
/// blank line is a separator, not a block.
///
/// The parser cannot build one, which is exactly why it has to be NAMED rather
/// than discovered - a caller handing the writer an ingested tree is entitled to
/// know before it happens (markup-carve/carve#1658).
#[test]
fn an_empty_paragraph_is_the_other_carve_out() {
    let document = carve::Document {
        frontmatter: Default::default(),
        frontmatter_raw: None,
        footnote_defs: Default::default(),
        footnote_def_pos: Default::default(),
        children: vec![
            carve::BlockNode::Paragraph(carve::Paragraph {
                attrs: None,
                children: Vec::new(),
                at_content_column: true,
                pos: None,
            }),
            carve::BlockNode::Paragraph(carve::Paragraph {
                attrs: None,
                children: vec![carve::InlineNode::Text(carve::Text {
                    value: "after".into(),
                    pos: None,
                })],
                at_content_column: true,
                pos: None,
            }),
        ],
        source_len: 0,
        ingest_payload_len: 0,
    };

    let written = carve::render_carve(&document).expect("the writer must not refuse this shape");
    assert_eq!(written, "after\n");
    assert_eq!(
        parse(&written).children.len(),
        1,
        "the empty paragraph is gone from the source, and the writer said nothing"
    );
}
