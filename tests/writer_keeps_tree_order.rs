//! A hoisted definition is written where the tree puts it.
//!
//! §7: "Definitions appear in DOCUMENT ORDER by source position", which
//! `ast_json::ordered_document_entries` implements for the published tree
//! (carve#746). PART 11 §6 binds the writer to the same order - "fmt does not
//! reorder ... those are the author's choices and the AST records them".
//!
//! The writer rendered `children` and appended the footnote map afterwards, so
//! a link definition hoisted from INSIDE a footnote body came out before the
//! footnote containing it (carve-rs#682). A fixed kind order passes one of the
//! two shapes below and fails the other, whichever order it picks, so both are
//! pinned.

fn written(src: &str) -> String {
    carve::to_carve(src)
}

#[test]
fn a_footnote_comes_before_a_link_definition_that_follows_it() {
    // The link definition sits on the footnote body's continuation line, so it
    // is hoisted from inside the footnote and lands after it (corpus 202).
    let src = "[^a]: note\n  [r]: /u\n\nsee[^a] and [t][r]\n";
    assert_eq!(
        written(src),
        "see[^a] and [t][r]\n\n[^a]: note\n\n[r]: /u\n"
    );
}

#[test]
fn a_link_definition_comes_before_a_footnote_that_follows_it() {
    let src = "see[^a] and [t][r]\n\n[r]: /u\n\n[^a]: note\n";
    assert_eq!(written(src), src);
}

#[test]
fn two_footnotes_keep_source_order() {
    let src = "see[^b] and[^a]\n\n[^b]: bee\n\n[^a]: ay\n";
    assert_eq!(written(src), src);
}

#[test]
fn the_written_source_still_renders_the_same_html() {
    // PART 11 §1, so a reordering fix cannot change the document.
    let src = "[^a]: note\n  [r]: /u\n\nsee[^a] and [t][r]\n";
    let from_source = carve::render_html(&carve::parse(src)).expect("render");
    let from_written = carve::render_html(&carve::parse(&written(src))).expect("render");
    assert_eq!(from_source, from_written);
}

#[test]
fn non_html_targets_keep_an_unused_definition_between_blocks() {
    let source = "before\n\n[r]: /u\n\nafter\n";
    for output in [
        carve::to_markdown(source),
        carve::to_plain_text(source),
        carve::to_ansi(source),
    ] {
        let before = output.find("before").unwrap();
        let definition = output.find("[r]: /u").unwrap();
        let after = output.find("after").unwrap();
        assert!(before < definition && definition < after, "{output:?}");
    }
}

#[test]
fn a_definition_from_another_file_stays_at_its_tree_slot() {
    let mut doc = carve::parse_with_options(
        "before\n\n[a]: /a\n\n[b]: /b\n\nafter\n",
        &carve::Options::new().with_positions(true),
    );
    let mut ordinary = Vec::new();
    let mut definitions = Vec::new();
    for mut block in std::mem::take(&mut doc.children) {
        if let carve::ast::BlockNode::LinkReferenceDefinition(def) = &mut block {
            def.pos.as_mut().unwrap().file = Some("included.crv".into());
            definitions.push(block);
        } else {
            ordinary.push(block);
        }
    }
    doc.children = vec![
        ordinary.remove(0),
        definitions.remove(0),
        definitions.remove(0),
        ordinary.remove(0),
    ];
    let output = carve::render_markdown(&doc).unwrap();
    let indices = ["before", "[a]: /a", "[b]: /b", "after"].map(|text| output.find(text).unwrap());
    assert!(
        indices.windows(2).all(|pair| pair[0] < pair[1]),
        "{output:?}"
    );
}

#[test]
fn a_main_file_definition_keeps_its_tree_slot_around_an_include() {
    let mut doc = carve::parse_with_options(
        "a\n\nincluded\n\nb\n\n[r]: /u\n",
        &carve::Options::new().with_positions(true),
    );
    if let carve::ast::BlockNode::Paragraph(paragraph) = &mut doc.children[1] {
        paragraph.pos.as_mut().unwrap().file = Some("included.crv".into());
    }
    let definition = doc
        .children
        .iter_mut()
        .find_map(|block| match block {
            carve::ast::BlockNode::LinkReferenceDefinition(def) => Some(def),
            _ => None,
        })
        .unwrap();
    definition.pos.as_mut().unwrap().start_offset = 3;
    let output = carve::render_markdown(&doc).unwrap();
    let indices = ["a\n", "included", "b\n", "[r]: /u"].map(|text| output.find(text).unwrap());
    assert!(
        indices.windows(2).all(|pair| pair[0] < pair[1]),
        "{output:?}"
    );
}

#[test]
fn a_main_file_definition_before_an_include_keeps_its_source_place() {
    let mut doc = carve::parse_with_options(
        "[r]: /u\n\na\n\nincluded\n\nb\n",
        &carve::Options::new().with_positions(true),
    );
    if let carve::ast::BlockNode::Paragraph(paragraph) = &mut doc.children[1] {
        paragraph.pos.as_mut().unwrap().file = Some("included.crv".into());
    }
    let output = carve::render_markdown(&doc).unwrap();
    let indices = ["[r]: /u", "a\n", "included", "b\n"].map(|text| output.find(text).unwrap());
    assert!(
        indices.windows(2).all(|pair| pair[0] < pair[1]),
        "{output:?}"
    );
}

#[test]
fn ansi_blanks_a_denied_definition_destination() {
    let source = "Text.\n\n[y]: javascript:alert(3)\n";
    let output = carve::to_ansi(source);
    assert!(output.contains("[y]: "), "{output:?}");
    assert!(!output.contains("javascript:"), "{output:?}");
}

#[test]
fn markdown_and_plain_blank_a_denied_definition_destination() {
    let source = "Text.\n\n[y]: javascript:alert(3)\n";
    let markdown = carve::to_markdown(source);
    let plain = carve::to_plain_text(source);
    assert!(markdown.contains("[y]: <>") && !markdown.contains("javascript:"));
    assert!(plain.contains("[y]:") && !plain.contains("javascript:"));
}

#[test]
fn markdown_blanks_a_scheme_with_an_embedded_control() {
    let mut doc = carve::parse("[y]: /safe\n");
    let definition = doc
        .children
        .iter_mut()
        .find_map(|block| match block {
            carve::ast::BlockNode::LinkReferenceDefinition(def) => Some(def),
            _ => None,
        })
        .unwrap();
    definition.href = "java\u{1}script:alert(3)".into();
    let output = carve::render_markdown(&doc).unwrap();
    assert_eq!(output, "[y]: <>\n");
}

#[test]
fn non_html_corpus_writer_rows_follow_their_clauses() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/non-html-writer-output.json")).unwrap();
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 47);
    for row in rows {
        let slug = row["slug"].as_str().unwrap();
        let source = std::fs::read_to_string(format!(
            "{}/tests/spec/tests/corpus/{slug}.crv",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        let actual = match row["target"].as_str().unwrap() {
            "markdown" => carve::to_markdown(&source),
            "plain" => carve::to_plain_text(&source),
            "ansi" => carve::to_ansi(&source),
            target => panic!("unexpected target {target}"),
        };
        assert_eq!(actual, row["expected"].as_str().unwrap(), "{slug}");
    }
}
