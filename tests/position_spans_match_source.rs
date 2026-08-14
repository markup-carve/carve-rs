//! Every block and inline span the parser emits must slice back to the node it belongs
//! to. PART 12 §4 lets an implementation omit a position it cannot determine,
//! but a position that points somewhere else is worse than none at all: a
//! consumer gets a number and lands on unrelated source.
//!
//! This walks the whole spec corpus rather than a handful of pinned inputs,
//! because the failures found while writing it were all in shapes nobody would
//! have thought to pin - a lazily continued paragraph that starts inside a
//! blockquote and ends flush left, a definition body, a colon fence nested in a
//! list item.

use carve::ast::*;

/// Line blocks expand indentation to a private-use sentinel, so that value is
/// not a verbatim source slice even when the surrounding inline span is right.
const LINE_BLOCK_INDENT: char = '\u{e000}';
const BLOCK_ANCHOR_SENTINELS: [char; 3] = ['\u{e000}', '\u{e001}', '\u{e002}'];

#[test]
fn every_positioned_span_slices_back_to_its_own_text() {
    // A cap-deep corpus document costs one debug frame per level, and a test
    // thread gets 2 MiB (carve-rs#530).
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(every_positioned_span_slices_back_to_its_own_text_inner)
        .expect("thread spawns")
        .join()
        .expect("the sweep finishes");
}

fn every_positioned_span_slices_back_to_its_own_text_inner() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus");
    // No early return on a missing corpus: a test that quietly passes when it
    // cannot find its inputs reads exactly like one that checked everything.
    assert!(
        dir.is_dir(),
        "spec corpus not found at {}. Did you run: git submodule update --init",
        dir.display()
    );

    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("read corpus dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "crv"))
        .collect();
    entries.sort();
    assert!(!entries.is_empty(), "corpus has no .crv inputs");

    let mut checked_blocks = 0usize;
    let mut checked_footnote_blocks = 0usize;
    let mut checked_inline_text = 0usize;
    let mut wrong: Vec<String> = Vec::new();

    for path in entries {
        let src = std::fs::read_to_string(&path).expect("read input");
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let codepoints: Vec<char> = src.chars().collect();
        let options = carve::Options {
            positions: true,
            ..Default::default()
        };
        let doc = carve::parse_with_options(&src, &options);
        for block in &doc.children {
            check(
                block,
                &codepoints,
                &name,
                &mut checked_blocks,
                &mut checked_inline_text,
                &mut wrong,
            );
        }
        for body in doc.footnote_defs.values() {
            for block in body {
                let before = checked_blocks;
                check(
                    block,
                    &codepoints,
                    &name,
                    &mut checked_blocks,
                    &mut checked_inline_text,
                    &mut wrong,
                );
                checked_footnote_blocks += checked_blocks - before;
            }
        }
    }

    assert!(
        checked_blocks > 400,
        "only {checked_blocks} positioned paragraphs seen - the walk stopped finding them"
    );
    assert!(
        checked_inline_text > 400,
        "only {checked_inline_text} positioned inline text nodes seen - the walk stopped finding them"
    );
    assert!(
        checked_footnote_blocks >= 10,
        "only {checked_footnote_blocks} positioned footnote body blocks seen - the walk stopped finding them"
    );
    assert!(
        wrong.is_empty(),
        "{} span(s) do not contain their own text:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

/// The first few characters of the paragraph's own text, or `None` when the
/// text is not a verbatim slice of the source.
fn anchor(nodes: &[InlineNode]) -> Option<String> {
    for node in nodes {
        let found = match node {
            // A node with NO span makes no claim about where it came from, so
            // it cannot anchor one. PART 12 §1a merges adjacent text runs, and
            // a run joined across a gap in the source -- the `<`/`>` of an
            // autolink unwrapped inside a link label, the delimiter between two
            // halves of a wrapped table cell -- deliberately publishes no
            // position, because its value is not a verbatim slice. Using it as
            // an anchor asked the source to contain text that was never
            // contiguous in it.
            InlineNode::Text(text) if text.pos.is_some() => {
                let trimmed = text.value.trim();
                if trimmed.is_empty()
                    || trimmed.chars().any(|c| BLOCK_ANCHOR_SENTINELS.contains(&c))
                {
                    None
                } else {
                    Some(trimmed.chars().take(10).collect::<String>())
                }
            }
            InlineNode::Emphasis(e) => anchor(&e.children),
            InlineNode::Link(l) => anchor(&l.children),
            _ => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

fn check(
    block: &BlockNode,
    source: &[char],
    file: &str,
    checked_blocks: &mut usize,
    checked_inline_text: &mut usize,
    wrong: &mut Vec<String>,
) {
    if let Some(pos) = block_pos(block) {
        *checked_blocks += 1;
        let (start, end) = (pos.start_offset, pos.end_offset);
        if start > end || end > source.len() {
            wrong.push(format!(
                "{file}: span {start}..{end} is outside the {}-codepoint document",
                source.len()
            ));
        } else if let BlockNode::Paragraph(paragraph) = block {
            if let Some(want) = anchor(&paragraph.children) {
                let slice: String = source[start..end].iter().collect();
                if !slice.contains(&want) {
                    wrong.push(format!(
                        "{file}: span {start}..{end} is {slice:?}, which does not contain {want:?}"
                    ));
                }
            }
        }
    }
    check_inline_children(
        block_inline_children(block),
        source,
        file,
        checked_inline_text,
        wrong,
    );
    match block {
        BlockNode::BlockQuote(b) => b
            .children
            .iter()
            .for_each(|c| check(c, source, file, checked_blocks, checked_inline_text, wrong)),
        BlockNode::Admonition(a) => a
            .children
            .iter()
            .for_each(|c| check(c, source, file, checked_blocks, checked_inline_text, wrong)),
        BlockNode::Div(d) => d
            .children
            .iter()
            .for_each(|c| check(c, source, file, checked_blocks, checked_inline_text, wrong)),
        BlockNode::LineBlock(l) => l
            .children
            .iter()
            .for_each(|c| check(c, source, file, checked_blocks, checked_inline_text, wrong)),
        BlockNode::List(list) => {
            for item in &list.items {
                item.children.iter().for_each(|c| {
                    check(c, source, file, checked_blocks, checked_inline_text, wrong)
                });
            }
        }
        BlockNode::DefinitionList(defs) => {
            for item in &defs.items {
                for term in &item.terms {
                    check_inline_nodes(&term.children, source, file, checked_inline_text, wrong);
                }
                for definition in &item.definitions {
                    definition.iter().for_each(|c| {
                        check(c, source, file, checked_blocks, checked_inline_text, wrong)
                    });
                }
            }
        }
        BlockNode::Figure(figure) => {
            check_inline_nodes(&figure.caption, source, file, checked_inline_text, wrong);
        }
        _ => {}
    }
}

fn block_pos(block: &BlockNode) -> Option<&Pos> {
    match block {
        BlockNode::Heading(n) => n.pos.as_ref(),
        BlockNode::Paragraph(n) => n.pos.as_ref(),
        BlockNode::CodeBlock(n) => n.pos.as_ref(),
        BlockNode::List(n) => n.pos.as_ref(),
        BlockNode::BlockQuote(n) => n.pos.as_ref(),
        BlockNode::Table(n) => n.pos.as_ref(),
        BlockNode::Admonition(n) => n.pos.as_ref(),
        BlockNode::Div(n) => n.pos.as_ref(),
        BlockNode::LineBlock(n) => n.pos.as_ref(),
        BlockNode::DefinitionList(n) => n.pos.as_ref(),
        BlockNode::Figure(n) => n.pos.as_ref(),
        BlockNode::LinkReferenceDefinition(n) => n.pos.as_ref(),
        BlockNode::AbbreviationDef(n) => n.pos.as_ref(),
        BlockNode::RawBlock(n) => n.pos.as_ref(),
        BlockNode::Comment(n) => n.pos.as_ref(),
        BlockNode::Extension(n) => n.pos.as_ref(),
        BlockNode::BlockImage(n) => n.pos.as_ref(),
        BlockNode::ThematicBreak(n) => n.pos.as_ref(),
    }
}

fn block_inline_children(block: &BlockNode) -> &[InlineNode] {
    match block {
        BlockNode::Heading(h) => &h.children,
        BlockNode::Paragraph(p) => &p.children,
        _ => &[],
    }
}

fn check_inline_children(
    nodes: &[InlineNode],
    source: &[char],
    file: &str,
    checked_inline_text: &mut usize,
    wrong: &mut Vec<String>,
) {
    check_inline_nodes(nodes, source, file, checked_inline_text, wrong);
}

fn check_inline_nodes(
    nodes: &[InlineNode],
    source: &[char],
    file: &str,
    checked_inline_text: &mut usize,
    wrong: &mut Vec<String>,
) {
    for node in nodes {
        if let InlineNode::Text(text) = node {
            if let Some(pos) = text.pos {
                let (start, end) = (pos.start_offset, pos.end_offset);
                if start > end || end > source.len() {
                    wrong.push(format!(
                        "{file}: inline text span {start}..{end} is outside the {}-codepoint document",
                        source.len()
                    ));
                } else {
                    let slice: String = source[start..end].iter().collect();
                    if !text.value.contains(LINE_BLOCK_INDENT) && !slice.contains('\\') {
                        *checked_inline_text += 1;
                        if slice != text.value {
                            wrong.push(format!(
                                "{file}: inline text span {start}..{end} is {slice:?}, want {:?}",
                                text.value
                            ));
                        }
                    }
                }
            }
        }
        match node {
            InlineNode::Emphasis(e) => {
                check_inline_nodes(&e.children, source, file, checked_inline_text, wrong)
            }
            InlineNode::Link(l) => {
                check_inline_nodes(&l.children, source, file, checked_inline_text, wrong)
            }
            InlineNode::Span(s) => {
                check_inline_nodes(&s.children, source, file, checked_inline_text, wrong)
            }
            InlineNode::Extension(e) => {
                check_inline_nodes(&e.children, source, file, checked_inline_text, wrong)
            }
            InlineNode::Footnote(f) => {
                if let Some(inline) = &f.inline {
                    check_inline_nodes(inline, source, file, checked_inline_text, wrong);
                }
            }
            InlineNode::CriticInsert(c) => {
                check_inline_nodes(&c.children, source, file, checked_inline_text, wrong)
            }
            InlineNode::CriticDelete(c) => {
                check_inline_nodes(&c.children, source, file, checked_inline_text, wrong)
            }
            _ => {}
        }
    }
}
