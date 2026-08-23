//! A block that renders to nothing but whitespace does not stand between two
//! lists, so it does not cost them PART 9 §11 N1a's hard boundary
//! (markup-carve/carve-rs#1290).
//!
//! The writer trims every line's trailing run and then collapses the blank run
//! around it, so a paragraph holding only spaces reaches the output as nothing
//! at all. The three loops that write the boundary asked `is_empty` instead,
//! which called that paragraph content: they concluded it separated the two
//! lists, wrote no boundary, and the paragraph then trimmed away and left the
//! lists adjacent. They merged on re-parse, and `parse(fmt(x)) == parse(x)` --
//! PART 11 §1's primary invariant, and the parse form §1a names as the rule --
//! was false.
//!
//! An EMPTY paragraph in the same position was written correctly all along.
//! That asymmetry is the tell that this was one predicate's defect rather than a
//! question about what a blank paragraph means, and it is why the fix belongs to
//! the writer: the shape arrives from an encoded AST as readily as from an
//! importer, and the `--from-json` case below carries no HTML at all.

/// Top-level node kinds, as a comma-joined string. Anything the cases do not
/// expect shows up as `other` and fails loudly rather than matching in silence.
fn top_kinds(document: &carve::Document) -> String {
    document
        .children
        .iter()
        .map(|block| match block {
            carve::BlockNode::List(_) => "list",
            carve::BlockNode::Paragraph(_) => "paragraph",
            _ => "other",
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// Two lists with `separator` between them, as an encoded AST. The separator is
/// a paragraph holding exactly that text, which is the node no Carve source can
/// spell: a line of spaces IS a blank line to the parser, so the shape only ever
/// arrives from an importer or an encoded tree.
fn two_lists_separated_by(separator: &str) -> carve::Document {
    let json = format!(
        r#"{{"type":"document","srcByteLength":0,"children":[
            {{"type":"list","ordered":false,"tight":true,"items":[{{"type":"list_item",
              "children":[{{"type":"paragraph","children":[{{"type":"text","value":"a"}}]}}]}}]}},
            {{"type":"paragraph","children":[{{"type":"text","value":{separator}}}]}},
            {{"type":"list","ordered":false,"tight":true,"items":[{{"type":"list_item",
              "children":[{{"type":"paragraph","children":[{{"type":"text","value":"b"}}]}}]}}]}}
        ]}}"#,
        separator = serde_json::to_string(separator).unwrap()
    );
    carve::ast_json::from_json(&json).unwrap()
}

#[test]
fn a_space_only_paragraph_between_two_lists_is_written_with_the_boundary() {
    let document = two_lists_separated_by(" ");
    let written = carve::render_carve(&document).unwrap();
    assert_eq!(written, "- a\n\n\n\n- b\n");
    assert_eq!(top_kinds(&carve::parse(&written)), "list,list");
}

/// Every whitespace run the writer trims, not only the single space. Each of
/// these is a separate spelling of the same unwritable paragraph, and the
/// predicate has to answer all of them the same way.
#[test]
fn every_trimmed_whitespace_run_keeps_the_boundary() {
    for separator in [" ", "\t", "\n", "  \t  ", "\r\n"] {
        let document = two_lists_separated_by(separator);
        let written = carve::render_carve(&document).unwrap();
        assert_eq!(
            top_kinds(&carve::parse(&written)),
            "list,list",
            "separator {separator:?} lost the boundary: {written:?}"
        );
    }
}

/// The half that was already right, kept honest. If this ever fails alongside
/// the cases above, the fix went too far and swept the empty paragraph's own
/// handling with it.
#[test]
fn an_empty_paragraph_between_two_lists_still_keeps_the_boundary() {
    let document = carve::ast_json::from_json(
        r#"{"type":"document","srcByteLength":0,"children":[
            {"type":"list","ordered":false,"tight":true,"items":[{"type":"list_item",
              "children":[{"type":"paragraph","children":[{"type":"text","value":"a"}]}]}]},
            {"type":"paragraph","children":[]},
            {"type":"list","ordered":false,"tight":true,"items":[{"type":"list_item",
              "children":[{"type":"paragraph","children":[{"type":"text","value":"b"}]}]}]}
        ]}"#,
    )
    .unwrap();
    let written = carve::render_carve(&document).unwrap();
    assert_eq!(written, "- a\n\n\n\n- b\n");
    assert_eq!(top_kinds(&carve::parse(&written)), "list,list");
}

/// U+00A0 IS CONTENT and is deliberately outside the sweep. `trim_non_nbsp` is
/// the writer's own trimming and preserves it, and a lone U+00A0 line parses
/// back as a paragraph -- so a paragraph holding one really does stand between
/// the two lists, and writing the boundary there would be wrong.
#[test]
fn a_no_break_space_paragraph_is_content_and_separates_the_lists() {
    let document = two_lists_separated_by("\u{a0}");
    let written = carve::render_carve(&document).unwrap();
    assert_eq!(written, "- a\n\n\u{a0}\n\n- b\n");
    assert_eq!(top_kinds(&carve::parse(&written)), "list,paragraph,list");
}

/// The item-level writer is a second loop with the same question, and it had the
/// same answer. Two sub-lists inside one tight item, separated by the same
/// unwritable paragraph, came back as ONE sub-list.
#[test]
fn the_boundary_inside_a_tight_item_survives_a_whitespace_only_child() {
    let html = "<ul><li>x<ul><li>a</li></ul><p> </p><ul><li>b</li></ul></li></ul>";
    let written = carve::html_to_carve(html, &carve::HtmlImportOptions::default())
        .unwrap()
        .value;
    assert_eq!(written, "- x\n\n  - a\n\n\n\n  - b\n");
    let carve::BlockNode::List(list) = &carve::parse(&written).children[0] else {
        panic!("expected a list");
    };
    let sub_lists = list.items[0]
        .children
        .iter()
        .filter(|block| matches!(block, carve::BlockNode::List(_)))
        .count();
    assert_eq!(sub_lists, 2, "wrote {written:?}");
}

/// The ticket's own repro, through the importer that found it. Every
/// LAYOUT-only spelling of the separator `<p>`.
///
/// `<p>&nbsp;</p>` USED TO BE IN THIS LIST, on the strength of the importer
/// normalizing the character to a plain space. PART 11 §7 has since forbidden
/// that normalization outright, so the row moved to
/// `a_content_space_separator_is_content_and_writes_no_boundary` below and
/// asserts the opposite - which is the same rule, not an exception to it: a
/// block that SPELLS something separates the two lists, and a NO-BREAK space is
/// something (markup-carve/carve-rs#1299, markup-carve/carve#1628).
#[test]
fn an_html_import_keeps_the_boundary_across_a_whitespace_only_paragraph() {
    for separator in ["<p> </p>", "<p>\n</p>", "<p>\t</p>", "<p></p>"] {
        let html = format!("<ul><li>a</li></ul>{separator}<ul><li>b</li></ul>");
        let written = carve::html_to_carve(&html, &carve::HtmlImportOptions::default())
            .unwrap()
            .value;
        assert_eq!(
            written, "- a\n\n\n\n- b\n",
            "separator {separator:?} was not written with the boundary"
        );
        assert_eq!(
            top_kinds(&carve::parse(&written)),
            "list,list",
            "separator {separator:?} merged the two lists on re-parse"
        );
    }
}

/// The other side of the same rule, and the row this file used to get wrong.
///
/// PART 11 §7 keeps a NO-BREAK space as itself on an import, so the paragraph
/// between the two lists is no longer empty of characters: it spells a line of
/// its own, the lists are NOT adjacent, and §10j's boundary must not appear.
/// U+202F and U+3000 are the same class and behave the same way - what decides
/// it is PART 2's two-character `whitespace` terminal and nothing else.
#[test]
fn a_content_space_separator_is_content_and_writes_no_boundary() {
    for (separator, kept) in [
        ("<p>&nbsp;</p>", '\u{a0}'),
        ("<p>&#8239;</p>", '\u{202f}'),
        ("<p>&#12288;</p>", '\u{3000}'),
    ] {
        let html = format!("<ul><li>a</li></ul>{separator}<ul><li>b</li></ul>");
        let written = carve::html_to_carve(&html, &carve::HtmlImportOptions::default())
            .unwrap()
            .value;

        assert_eq!(
            written,
            format!("- a\n\n{kept}\n\n- b\n"),
            "separator {separator:?} did not survive as content"
        );
        assert_eq!(
            top_kinds(&carve::parse(&written)),
            "list,paragraph,list",
            "separator {separator:?} did not read back as a paragraph"
        );
    }
}
