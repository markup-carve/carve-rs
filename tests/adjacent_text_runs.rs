//! PART 12 §1a: a serialized node's children hold no two adjacent `text`
//! nodes. Where the parser's internal tree has a run of them -- a table cell
//! rebuilt from several lines -- they join into one on the way to the wire.
//!
//! The two link-label splitters this file was built on are gone. An autolink
//! and a nested link inside a label used to be dropped to their text, leaving
//! runs either side; under PART 12 section 3a they stay NODES and the renderer
//! unwraps them (markup-carve/carve#817), so they split nothing. They are kept
//! below as cases in their own right, because "the node survives and its
//! neighbours are still one run each" is what section 1a has to say about them
//! now -- the same move the unresolved reference made before them.
//!
//! The merge happens in the tree, not on the way out, because §6 requires
//! `parse(x)` serialized and deserialized to equal `parse(x)`: an encoder that
//! joined runs during serialization would satisfy §1a and break §6 on the same
//! document.
//!
//! These tests therefore read back through `carve::from_json`, which is what a
//! consumer sees and also proves the wire form the encoder wrote decodes.

mod common;

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
fn an_unresolved_reference_link_is_published_as_a_link_between_two_runs() {
    // It reverted to one text node before PART 12 section 3a, which is why this
    // lived here as a coalescing case at all. It is a LINK now -- the tree
    // records what the author wrote -- so the document is three nodes and none
    // of them are adjacent text runs. The characters either side are still one
    // run each, which is what section 1a has to say about it.
    let doc = published("A [missing][nope] ref stays literal.\n");
    let inlines = paragraph_inlines(&doc);

    assert_eq!(text_values(inlines), vec!["A ", " ref stays literal."]);
    match &inlines[1] {
        InlineNode::Link(l) => {
            assert_eq!(l.href, "");
            assert_eq!(l.ref_label.as_deref(), Some("nope"));
            assert_eq!(l.raw_ref.as_deref(), Some("[missing][nope]"));
        }
        other => panic!("expected the unresolved reference to stay a link, got {other:?}"),
    }
}

#[test]
fn an_autolink_in_a_link_label_is_published_as_a_node_between_two_runs() {
    // `[pre <http://h> post](/u)`: this used to assert one merged run, because
    // the autolink was dropped to its display text. It is an `autolink` NODE now
    // (PART 12 section 3a, markup-carve/carve#817) -- "links never nest" binds
    // the renderer, not the encoder -- so the label is three nodes and none of
    // them are adjacent text runs.
    let doc = published("[pre <http://h> post](/u)\n");
    let link = match &paragraph_inlines(&doc)[0] {
        InlineNode::Link(l) => l,
        other => panic!("expected a link, got {other:?}"),
    };
    assert_eq!(text_values(&link.children), vec!["pre ", " post"]);
    match &link.children[1] {
        InlineNode::AutoLink(a) => assert_eq!(a.href, "http://h"),
        other => panic!("expected the autolink to stay a node, got {other:?}"),
    }
}

#[test]
fn a_nested_link_in_a_label_is_published_as_a_node_between_two_runs() {
    // The same move for the other half of section 3a. A flattened inner link
    // loses its destination outright, which is strictly worse than the
    // unresolved reference this file opens with: `[pre [in](/i) post](/o)` had
    // no `/i` anywhere in the published tree.
    let doc = published("[pre [in](/i) post](/o)\n");
    let link = match &paragraph_inlines(&doc)[0] {
        InlineNode::Link(l) => l,
        other => panic!("expected a link, got {other:?}"),
    };
    assert_eq!(text_values(&link.children), vec!["pre ", " post"]);
    match &link.children[1] {
        InlineNode::Link(inner) => {
            assert_eq!(inner.href, "/i");
            assert_eq!(text_values(&inner.children), vec!["in"]);
        }
        other => panic!("expected the inner link to stay a node, got {other:?}"),
    }
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
fn a_published_run_beside_a_reference_selects_exactly_its_own_text() {
    // This measured a MERGED run's span while an unresolved reference reverted
    // to text and made the whole line one node. Under PART 12 section 3a the
    // line is three nodes, so what is worth pinning is that each published span
    // still selects exactly the characters its node carries -- including the
    // link's, which is where a consumer most wants one: the author wrote a
    // reference that does not resolve, and `raw_ref` has to BE that source.
    let source = "A [missing][nope] ref stays literal.\n";
    let doc = published(source);
    let inlines = paragraph_inlines(&doc);

    let selects = |node: &InlineNode| -> (String, String) {
        let (value, pos) = match node {
            InlineNode::Text(t) => (t.value.clone(), t.pos.expect("a run keeps its span")),
            InlineNode::Link(l) => (
                l.raw_ref
                    .clone()
                    .expect("an unresolved reference keeps its source"),
                l.pos.expect("a link keeps its span"),
            ),
            other => panic!("unexpected node {other:?}"),
        };
        (source[pos.start_offset..pos.end_offset].to_string(), value)
    };

    for node in inlines {
        let (selected, value) = selects(node);
        assert_eq!(selected, value);
    }

    match &inlines[0] {
        InlineNode::Text(t) => {
            let pos = t.pos.expect("a run keeps its span");
            assert_eq!(pos.start_offset, 0);
            assert_eq!(pos.start_column, 1);
        }
        other => panic!("expected text, got {other:?}"),
    }
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
    // A table cell rebuilt from two source lines: the parser holds two runs
    // where the document has one. It is the vehicle because it is the last
    // splitter left - the unresolved reference and then the unwrapped autolink
    // both stopped splitting when PART 12 section 3a kept their nodes.
    let source = "|= a |= b |\n| x | A long description |\n+     | that continues     |\n";
    let doc = carve::parse_with_options(source, &carve::Options::new().with_positions(true));
    let table = match &doc.children[0] {
        BlockNode::Table(t) => t,
        other => panic!("expected a table, got {other:?}"),
    };
    assert_eq!(
        text_values(&table.rows[1].cells[1].children),
        vec!["A long description that continues"],
        "parse() itself must already hold the merged run"
    );
    let decoded = carve::from_json(&carve::to_json(&doc)).expect("decodable");
    assert_eq!(carve::to_json(&decoded), carve::to_json(&doc));
}

#[test]
fn no_document_in_the_spec_corpus_publishes_an_adjacent_text_run() {
    // On a thread with room: the corpus now holds a document nested to the
    // parser's cap (200 containers), and the encode/decode pass over it needs
    // more than the 2 MiB a test thread gets by default in a debug build. The
    // library is fine with it - the same document parses, encodes, decodes and
    // renders on the main thread - so what this buys is the sweep, not a
    // behaviour change. `MAX_NESTING_DEPTH` is what bounds the depth itself.
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(sweep_the_corpus_for_adjacent_runs)
        .expect("thread spawns")
        .join()
        .expect("the sweep finishes");
}

fn sweep_the_corpus_for_adjacent_runs() {
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
    assert_eq!(
        sources.len(),
        common::expected_corpus_size(),
        "the corpus sweep read a different number of documents than the spec examples define"
    );

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
    // An explicit worklist, not recursion: the corpus now holds a document
    // nested to the parser's cap (200 containers, `181-openers-past-the-
    // nesting-cap-are-one-paragraph`), and one debug-build frame per level
    // overflowed the thread stack here. The library itself walks it fine -
    // parse, encode, decode and render all complete - so the limit was this
    // test's own recursion, and it is the test that had to stop recursing.
    let mut queue: Vec<&BlockNode> = Vec::new();
    queue.extend(doc.children.iter());
    for body in doc.footnote_defs.values() {
        queue.extend(body.iter());
    }
    while let Some(block) = queue.pop() {
        walk_block(block, visit, &mut queue);
    }
}

fn walk_block<'a>(
    block: &'a BlockNode,
    visit: &mut impl FnMut(&[InlineNode]),
    queue: &mut Vec<&'a BlockNode>,
) {
    let mut inline_lists: Vec<&[InlineNode]> = Vec::new();
    let mut child_blocks: Vec<&BlockNode> = Vec::new();
    match block {
        BlockNode::Paragraph(n) => inline_lists.push(&n.children),
        BlockNode::Heading(n) => inline_lists.push(&n.children),
        BlockNode::BlockQuote(n) => {
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
    queue.extend(child_blocks);
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
const ONCE_SPLIT: [(&str, &str); 2] = [
    // A table cell rebuilt from two source lines: the `+` continuation row
    // appends to the cell above, so the parser holds two runs where the
    // document has one. This is the LAST splitter in the language, and the
    // three that used to stand beside it here were all link labels - an
    // autolink and two nested links, each dropped to its text. Under PART 12
    // section 3a they stay nodes and split nothing (markup-carve/carve#817), so
    // keeping them would have made this table assert nothing.
    (
        "|= a |= b |\n| x | A long description |\n+     | that continues     |\n",
        "A long description that continues",
    ),
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
    // A multi-line table cell joins across the row delimiter between its two
    // halves, so the joined value is not a slice of the source at any offset.
    // PART 12 §4 rates a wrong span as worse than none, so the merged node
    // publishes no position. (The unwrapped autolink used to stand here, for
    // the `<` and `>` it left behind; it publishes an `autolink` node now and
    // joins nothing - PART 12 section 3a.)
    let doc = carve::parse_with_options(
        "|= a |= b |\n| x | A long description |\n+     | that continues     |\n",
        &carve::Options::new().with_positions(true),
    );
    let mut checked = 0usize;
    walk_document(&doc, &mut |inlines| {
        for node in inlines {
            if let InlineNode::Text(text) = node {
                if text.value == "A long description that continues" {
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
