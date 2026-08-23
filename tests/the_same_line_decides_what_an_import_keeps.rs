//! PART 11 §7: the same line decides what an import keeps.
//!
//! §7 already draws the content-versus-layout line on the way OUT - a trailing
//! NO-BREAK space is content, not layout. This is the inbound face of it: an
//! importer meeting a block element whose text is entirely whitespace keeps
//! exactly the characters §7 calls content, and builds no node at all where
//! every character it holds is layout.
//!
//! THE CLASS, NOT THE ENTITY. What separates the two outcomes is PART 2's
//! two-character `whitespace` terminal and nothing else, together with the line
//! terminators an HTML parser folds into it. So U+202F and U+3000 are kept
//! exactly as U+00A0 is, and an importer that special-cases `&nbsp;` has
//! implemented a different rule that happens to agree on one row.
//!
//! NORMALIZING TO A SPACE IS THE ONE ANSWER FORBIDDEN OUTRIGHT, and it is what
//! this engine did (markup-carve/carve-rs#1299, ruled in markup-carve/carve#1628).
//! It kept a node while discarding the single property that separates U+00A0
//! from a space, and the paragraph it left is unspellable, so it vanished when
//! the writer ran: `html_to_ast` published a paragraph that `html_to_carve`
//! did not, on the same input.
//!
//! The shapes here are the spec fixture `tests/html-import/whitespace-only-block`
//! (added in markup-carve/carve@5a8e4ef0), replicated because this engine's
//! submodule pin predates it; `shared_contract_fixtures_match` runs the fixture
//! itself once the pin advances.

use carve::{html_to_ast, html_to_carve, HtmlImportOptions};

fn imported(html: &str) -> (String, Vec<(String, String, String)>) {
    let result = html_to_carve(html, &HtmlImportOptions::default()).expect("import");
    (
        result.value,
        result
            .report
            .diagnostics
            .iter()
            .map(|d| {
                (
                    d.code.as_str().to_string(),
                    d.path.clone().unwrap_or_default(),
                    d.message.clone(),
                )
            })
            .collect(),
    )
}

fn ast_json(html: &str) -> String {
    carve::to_json(
        &html_to_ast(html, &HtmlImportOptions::default())
            .expect("import")
            .value,
    )
}

#[test]
fn a_content_space_is_kept_as_itself() {
    // The first of §7's three rows. Every one of these is a lone content space,
    // and a lone content-space line parses back as a PARAGRAPH.
    for (html, kept) in [
        ("<p>&nbsp;</p>", '\u{a0}'),
        ("<p>&#8239;</p>", '\u{202f}'),
        ("<p>&#12288;</p>", '\u{3000}'),
    ] {
        let (written, diagnostics) = imported(html);

        assert_eq!(written, format!("{kept}\n"), "importing {html}");
        assert!(
            diagnostics.is_empty(),
            "keeping owes no report, nothing was given up: {diagnostics:?}"
        );
        assert!(
            ast_json(html).contains(&format!("\"value\":\"{kept}\"")),
            "the published tree lost the character: {}",
            ast_json(html)
        );
    }
}

#[test]
fn a_layout_only_block_builds_no_node_and_is_reported() {
    // §7's other two rows. The drop IS reported: an element the input had
    // contributes nothing, which is a real loss and needs no new vocabulary.
    for html in ["<p> </p>", "<p>&#9;</p>", "<p>\n</p>", "<p>  \t </p>"] {
        let (written, diagnostics) = imported(html);

        // An empty document writes one newline, which is this engine's
        // spelling of "nothing".
        assert_eq!(written, "\n", "importing {html}");
        assert_eq!(
            diagnostics,
            vec![(
                "element-dropped".to_string(),
                "/p[1]".to_string(),
                "Dropped whitespace-only <p> holding no content character".to_string(),
            )],
            "importing {html}"
        );
        assert_eq!(
            ast_json(html),
            "{\"type\":\"document\",\"children\":[],\"srcByteLength\":0}"
        );
    }
}

#[test]
fn a_content_space_inside_a_line_is_not_normalized_either() {
    // The rule is over the CHARACTER, not over whether the block holds only it.
    // Collapsing through `char::is_whitespace` - Unicode `White_Space`, which
    // holds all three - is what reached both.
    assert_eq!(imported("<p>a&nbsp;b</p>").0, "a\u{a0}b\n");
    assert_eq!(imported("<p>x&nbsp;</p>").0, "x\u{a0}\n");
    assert_eq!(imported("<p>a&#8239;b</p>").0, "a\u{202f}b\n");
}

#[test]
fn ascii_layout_still_collapses_the_way_html_reads_it() {
    // The control that keeps the fix from becoming "preserve all whitespace".
    // A line terminator is folded into the same run an HTML parser folds it
    // into, and a run of them is one space.
    assert_eq!(imported("<p>a\nb</p>").0, "a b\n");
    assert_eq!(imported("<p>a  b</p>").0, "a b\n");
    assert_eq!(imported("<p>a\t\tb</p>").0, "a b\n");
}

#[test]
fn an_empty_block_is_not_this_shape_and_does_not_move() {
    // `<p></p>` holds no character to classify and nothing was dropped, so it
    // reports nothing and keeps the node it always kept. PART 11 §10j names the
    // empty paragraph as the sibling shape whose handling already keeps §1.
    let (written, diagnostics) = imported("<p></p>");

    assert_eq!(written, "\n");
    assert!(diagnostics.is_empty());
    assert!(ast_json("<p></p>").contains("\"type\":\"paragraph\""));
}

#[test]
fn the_shared_fixture_shape_matches_end_to_end() {
    // `tests/html-import/whitespace-only-block` from the spec repo, input and
    // both expected exits. Replicated rather than referenced: this engine's
    // `tests/spec` pin predates the fixture, and the contract runner reads only
    // the directories the pin has.
    let input = "<ul><li>a</li></ul><p>&nbsp;</p><ul><li>b</li></ul>\
                 <p> </p><p>c</p><p>&#9;</p><p>&#8239;</p><p>&#12288;</p>";
    let (written, diagnostics) = imported(input);

    assert_eq!(
        written,
        "- a\n\n\u{a0}\n\n- b\n\nc\n\n\u{202f}\n\n\u{3000}\n"
    );
    assert_eq!(
        diagnostics
            .iter()
            .map(|(code, path, _)| (code.as_str(), path.as_str()))
            .collect::<Vec<_>>(),
        vec![("element-dropped", "/p[4]"), ("element-dropped", "/p[6]"),]
    );
}
