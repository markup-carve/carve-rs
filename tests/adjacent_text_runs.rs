//! PART 12 §1a: a serialized node's children hold no two adjacent `text`
//! nodes. Where the parser's internal tree has a run of them -- a reference
//! that never resolved and reverted to its source, an autolink unwrapped
//! because links do not nest, a table cell rebuilt from several lines -- they
//! join into one on the way to the wire.
//!
//! The merge happens in the tree, not on the way out, because §6 requires
//! `parse(x)` serialized and deserialized to equal `parse(x)`: an encoder that
//! joined runs during serialization would satisfy §1a and break §6 on the same
//! document.
//!
//! These tests therefore read back through `carve::from_json`, which is what a
//! consumer sees and also proves the wire form the encoder wrote decodes.

use carve::ast::*;
use carve::Document;

/// Parse, serialize, and decode back -- i.e. exactly what a consumer sees.
fn published(source: &str) -> Document {
    let doc = carve::parse_with_options(source, &carve::Options::new().with_positions(true));
    carve::from_json(&carve::to_json(&doc)).expect("the encoder writes decodable JSON")
}

fn text_values(nodes: &[InlineNode]) -> Vec<&str> {
    nodes
        .iter()
        .filter_map(|n| match n {
            InlineNode::Text(t) => Some(t.value.as_str()),
            _ => None,
        })
        .collect()
}

fn paragraph_inlines(doc: &Document) -> &[InlineNode] {
    match &doc.children[0] {
        BlockNode::Paragraph(p) => &p.children,
        other => panic!("expected a paragraph, got {other:?}"),
    }
}

#[test]
fn an_unresolved_reference_link_is_published_as_one_text_node() {
    // The reference never resolved, so it is literal text -- but the parser
    // knows it as three pieces (before it, the reverted source, after it).
    let doc = published("A [missing][nope] ref stays literal.\n");
    assert_eq!(
        text_values(paragraph_inlines(&doc)),
        vec!["A [missing][nope] ref stays literal."]
    );
}

#[test]
fn an_autolink_unwrapped_in_a_link_label_is_published_as_one_text_node() {
    // `[pre <http://h> post](/u)`: links never nest, so the autolink becomes
    // text between two runs that were already there.
    let doc = published("[pre <http://h> post](/u)\n");
    let link = match &paragraph_inlines(&doc)[0] {
        InlineNode::Link(l) => l,
        other => panic!("expected a link, got {other:?}"),
    };
    assert_eq!(text_values(&link.children), vec!["pre http://h post"]);
}

#[test]
fn a_multi_line_table_cell_is_published_as_one_text_node() {
    // `+` in the first column continues the row above, so the second cell is
    // rebuilt from two source lines.
    let doc = published("|= a |= b |\n| x | A long description |\n+     | that continues     |\n");
    let table = match &doc.children[0] {
        BlockNode::Table(t) => t,
        other => panic!("expected a table, got {other:?}"),
    };
    assert_eq!(
        text_values(&table.rows[1].cells[1].children),
        vec!["A long description that continues"]
    );
}

#[test]
fn an_escape_does_not_merge_into_the_text_around_it() {
    // The rule is about `text` only. `escaped_text` is a different type and
    // carries authored form, so it stays its own node between its neighbours.
    let doc = published("a \\* b\n");
    let kinds: Vec<&str> = paragraph_inlines(&doc)
        .iter()
        .map(|n| match n {
            InlineNode::Text(_) => "text",
            InlineNode::EscapedText(_) => "escaped_text",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, vec!["text", "escaped_text", "text"]);
}

#[test]
fn a_merged_run_spans_from_the_first_piece_to_the_last() {
    // The three pieces are contiguous, so the merged span is real: it selects
    // exactly the text the node carries.
    let source = "A [missing][nope] ref stays literal.\n";
    let doc = published(source);
    let (value, pos) = match &paragraph_inlines(&doc)[0] {
        InlineNode::Text(t) => (
            t.value.clone(),
            t.pos.expect("a contiguous run keeps its span"),
        ),
        other => panic!("expected text, got {other:?}"),
    };
    assert_eq!(&source[pos.start_offset..pos.end_offset], value);
    assert_eq!(pos.start_offset, 0);
    assert_eq!(pos.start_column, 1);
}

#[test]
fn a_run_joined_across_a_gap_in_the_source_publishes_no_span() {
    // The two halves of a wrapped table cell are separated by a delimiter and a
    // newline the merged value does not contain, so a span across them would
    // not select its own text. Absent beats wrong (PART 12 §4).
    let doc = published("|= a |= b |\n| x | A long description |\n+     | that continues     |\n");
    let table = match &doc.children[0] {
        BlockNode::Table(t) => t,
        other => panic!("expected a table, got {other:?}"),
    };
    match &table.rows[1].cells[1].children[0] {
        InlineNode::Text(t) => assert!(
            t.pos.is_none(),
            "a run joined across a gap should carry no span, got {:?}",
            t.pos
        ),
        other => panic!("expected text, got {other:?}"),
    }
}

#[test]
fn the_merge_happens_in_the_tree_because_section_6_requires_it() {
    // §6: "`parse(x)` serialized and deserialized MUST equal `parse(x)`". So
    // the merge cannot be a serialization step -- an encoder that joined runs
    // on the way out would satisfy §1a and break §6 on the same document,
    // because decoding its output could not reproduce the split tree behind it.
    let source = "A [missing][nope] ref stays literal.\n";
    let doc = carve::parse_with_options(source, &carve::Options::new().with_positions(true));
    assert_eq!(
        text_values(paragraph_inlines(&doc)),
        vec!["A [missing][nope] ref stays literal."],
        "parse() itself must already hold the merged run"
    );
    let decoded = carve::from_json(&carve::to_json(&doc)).expect("decodable");
    assert_eq!(carve::to_json(&decoded), carve::to_json(&doc));
}

#[test]
fn no_document_in_the_spec_corpus_publishes_an_adjacent_text_run() {
    // The rule is a property of every published tree, so the corpus is where it
    // is measured. Six documents violated it before this landed, which is why
    // this sweeps the corpus rather than pinning three hand-written cases.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus");
    assert!(
        dir.exists(),
        "spec corpus missing; run `git submodule update --init`"
    );
    let mut sources: Vec<_> = std::fs::read_dir(&dir)
        .expect("corpus readable")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|p| p.extension().is_some_and(|e| e == "crv"))
        .collect();
    sources.sort();
    assert!(sources.len() > 400, "corpus looks truncated");

    let mut offenders = Vec::new();
    for path in &sources {
        let source = std::fs::read_to_string(path).expect("readable");
        let doc = published(&source);
        let mut found = 0usize;
        walk_document(&doc, &mut |inlines| {
            for pair in inlines.windows(2) {
                if matches!(pair[0], InlineNode::Text(_)) && matches!(pair[1], InlineNode::Text(_))
                {
                    found += 1;
                }
            }
        });
        if found > 0 {
            offenders.push(format!(
                "{}: {found}",
                path.file_name().unwrap().to_string_lossy()
            ));
        }
    }
    assert!(
        offenders.is_empty(),
        "documents publishing adjacent text runs:\n{}",
        offenders.join("\n")
    );
}

/// Visits every inline child list in the document, including footnote bodies.
fn walk_document(doc: &Document, visit: &mut impl FnMut(&[InlineNode])) {
    for block in &doc.children {
        walk_block(block, visit);
    }
    for body in doc.footnote_defs.values() {
        for block in body {
            walk_block(block, visit);
        }
    }
}

fn walk_block(block: &BlockNode, visit: &mut impl FnMut(&[InlineNode])) {
    let mut inline_lists: Vec<&[InlineNode]> = Vec::new();
    let mut child_blocks: Vec<&BlockNode> = Vec::new();
    match block {
        BlockNode::Paragraph(n) => inline_lists.push(&n.children),
        BlockNode::Heading(n) => inline_lists.push(&n.children),
        BlockNode::BlockQuote(n) => {
            if let Some(attribution) = &n.attribution {
                inline_lists.push(attribution);
            }
            child_blocks.extend(n.children.iter());
        }
        BlockNode::Div(n) => child_blocks.extend(n.children.iter()),
        BlockNode::LineBlock(n) => child_blocks.extend(n.children.iter()),
        BlockNode::Admonition(n) => {
            if let Some(title) = &n.title {
                inline_lists.push(title);
            }
            child_blocks.extend(n.children.iter());
        }
        BlockNode::List(n) => {
            for item in &n.items {
                child_blocks.extend(item.children.iter());
            }
        }
        BlockNode::Table(n) => {
            for row in &n.rows {
                for cell in &row.cells {
                    inline_lists.push(&cell.children);
                }
            }
            if let Some(caption) = &n.caption {
                inline_lists.push(caption);
            }
        }
        BlockNode::DefinitionList(n) => {
            for item in &n.items {
                for term in &item.terms {
                    inline_lists.push(&term.children);
                }
                for definition in &item.definitions {
                    child_blocks.extend(definition.children.iter());
                }
            }
        }
        BlockNode::Figure(n) => inline_lists.push(&n.caption),
        BlockNode::Extension(n) => {
            if let Some(summary) = &n.summary {
                inline_lists.push(summary);
            }
            child_blocks.extend(n.children.iter());
        }
        _ => {}
    }
    for inlines in inline_lists {
        visit(inlines);
        walk_inlines(inlines, visit);
    }
    for child in child_blocks {
        walk_block(child, visit);
    }
}

fn walk_inlines(nodes: &[InlineNode], visit: &mut impl FnMut(&[InlineNode])) {
    for node in nodes {
        let children: Option<&[InlineNode]> = match node {
            InlineNode::Emphasis(n) => Some(&n.children),
            InlineNode::Link(n) => Some(&n.children),
            InlineNode::Span(n) => Some(&n.children),
            InlineNode::Extension(n) => Some(&n.children),
            InlineNode::CriticInsert(n) => Some(&n.children),
            InlineNode::CriticDelete(n) => Some(&n.children),
            InlineNode::Footnote(n) => n.inline.as_deref(),
            InlineNode::CitationGroup(group) => {
                for item in &group.items {
                    for field in [&item.prefix, &item.locator, &item.suffix]
                        .into_iter()
                        .flatten()
                    {
                        visit(field);
                        walk_inlines(field, visit);
                    }
                }
                None
            }
            _ => None,
        };
        if let Some(children) = children {
            visit(children);
            walk_inlines(children, visit);
        }
    }
}

// Coverage gaps found by review after the first pass landed: inline content
// does not only live in `children`. Two node kinds carry it in fields the
// original walk never reached, and both are reachable from a built-in
// extension -- so the invariant held for the corpus and not for the vocabulary.

#[test]
fn a_citation_prefix_is_coalesced() {
    // `prefix`, `locator` and `suffix` are inline arrays on a citation item,
    // not `children`, so a walk that only follows `children` publishes runs
    // inside them.
    let citations = carve::Citations::new();
    let options = carve::Options::new()
        .with_extension(&citations)
        .with_positions(true);
    let doc = carve::parse_with_options("[see [missing][nope] @a, p. 3].\n\n[@a]: A.\n", &options);
    let decoded = carve::from_json(&carve::to_json(&doc)).expect("decodable");
    let mut runs = 0usize;
    walk_document(&decoded, &mut |inlines| {
        for pair in inlines.windows(2) {
            if matches!(pair[0], InlineNode::Text(_)) && matches!(pair[1], InlineNode::Text(_)) {
                runs += 1;
            }
        }
    });
    assert_eq!(runs, 0, "a citation's inline fields must be coalesced too");
}

#[test]
fn a_block_extension_body_and_summary_are_coalesced() {
    // An extension that wraps parsed blocks in `BlockNode::Extension` puts them
    // behind a node the walk has to descend through; its `summary` is a second
    // inline field beside `children`.
    let details = carve::Details::new();
    let options = carve::Options::new()
        .with_extension(&details)
        .with_positions(true);
    let doc = carve::parse_with_options(
        "::: details \"see [missing][nope] here\"\nA [missing][nope] ref stays literal.\n:::\n",
        &options,
    );
    let decoded = carve::from_json(&carve::to_json(&doc)).expect("decodable");
    let mut runs = 0usize;
    walk_document(&decoded, &mut |inlines| {
        for pair in inlines.windows(2) {
            if matches!(pair[0], InlineNode::Text(_)) && matches!(pair[1], InlineNode::Text(_)) {
                runs += 1;
            }
        }
    });
    assert_eq!(
        runs, 0,
        "an extension's wrapped blocks and summary must be coalesced too"
    );
}

/// The documents whose parse tree used to hold a run of adjacent text nodes.
///
/// Read through `parse` DIRECTLY, not through the wire. Every test above goes
/// via `from_json`, which is satisfied by an encoder-side merge and so could
/// never fail while §6 was broken - `parse(x)` kept the split that
/// `decode(encode(parse(x)))` removed. These are the checks that can fail.
const ONCE_SPLIT: [(&str, &str); 4] = [
    // An autolink unwrapped inside a link label (corpus 03-links-12), which is
    // where the corpus scan found the surviving run.
    ("[pre <http://h> post](/u)\n", "pre http://h post"),
    // A reference that never resolved and reverted to its source.
    (
        "A [missing][nope] ref stays literal.\n",
        "A [missing][nope] ref stays literal.",
    ),
    // A nested link dropped down to its own text.
    ("[a [b](/i) c](/o)\n", "a b c"),
    // Plain prose, which must not be split in the first place.
    ("just one run\n", "just one run"),
];

#[test]
fn parse_itself_coalesces_adjacent_runs() {
    for (source, want) in ONCE_SPLIT {
        let doc = carve::parse_with_options(source, &carve::Options::new().with_positions(true));
        let mut runs = 0usize;
        walk_document(&doc, &mut |inlines| {
            for pair in inlines.windows(2) {
                if matches!(pair[0], InlineNode::Text(_)) && matches!(pair[1], InlineNode::Text(_))
                {
                    runs += 1;
                }
            }
        });
        assert_eq!(runs, 0, "parse left an adjacent text run in {source:?}");
        assert!(
            find_text(&doc, want),
            "expected one text node {want:?} in {source:?}"
        );
    }
}

#[test]
fn parse_and_the_round_trip_agree_on_the_text_runs() {
    // PART 12 §6 as it reads: the two sides are the SAME tree, not two shapes
    // that serialize alike.
    //
    // Comparing `to_json(doc)` with `to_json(round_tripped)` would NOT do:
    // under an encoder-side merge both sides pass through the encoder, so both
    // come out merged and the comparison is satisfied while §6 is broken. Read
    // the values off each TREE instead.
    for (source, _) in ONCE_SPLIT {
        let doc = carve::parse_with_options(source, &carve::Options::new().with_positions(true));
        let round_tripped = carve::from_json(&carve::to_json(&doc)).expect("decodable");
        assert_eq!(
            all_text_values(&doc),
            all_text_values(&round_tripped),
            "decode(encode(parse(x))) != parse(x) for {source:?}"
        );
    }
}

fn all_text_values(doc: &Document) -> Vec<String> {
    let mut values = Vec::new();
    walk_document(doc, &mut |inlines| {
        values.extend(text_values(inlines).into_iter().map(str::to_string));
    });
    values
}

#[test]
fn a_merged_run_keeps_a_span_only_where_the_source_is_contiguous() {
    // The unwrapped autolink leaves its `<` and `>` behind, so the joined value
    // is not a slice of the source at any offset. PART 12 §4 rates a wrong span
    // as worse than none, so the merged node publishes no position.
    let doc = carve::parse_with_options(
        "[pre <http://h> post](/u)\n",
        &carve::Options::new().with_positions(true),
    );
    let mut checked = 0usize;
    walk_document(&doc, &mut |inlines| {
        for node in inlines {
            if let InlineNode::Text(text) = node {
                if text.value == "pre http://h post" {
                    assert!(
                        text.pos.is_none(),
                        "a run joined across a gap must publish no span, got {:?}",
                        text.pos
                    );
                    checked += 1;
                }
            }
        }
    });
    assert_eq!(checked, 1, "the merged run was not found");

    // A run that WAS contiguous keeps its span, so the rule above is a
    // narrowing rather than "merged runs never place".
    let plain = carve::parse_with_options(
        "just one run\n",
        &carve::Options::new().with_positions(true),
    );
    let mut placed = 0usize;
    walk_document(&plain, &mut |inlines| {
        for node in inlines {
            if let InlineNode::Text(text) = node {
                assert!(text.pos.is_some(), "an unsplit run lost its span");
                placed += 1;
            }
        }
    });
    assert_eq!(placed, 1);
}

fn find_text(doc: &Document, want: &str) -> bool {
    let mut found = false;
    walk_document(doc, &mut |inlines| {
        if text_values(inlines).contains(&want) {
            found = true;
        }
    });
    found
}
