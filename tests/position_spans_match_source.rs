//! Every block span the parser emits must slice back to the block it belongs
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

/// Text nodes whose value is not a verbatim slice of the source: the parser
/// substitutes a private-use sentinel for a no-break space and for line-block
/// indentation, so the value cannot be compared against the source directly.
const SENTINELS: [char; 3] = ['\u{e000}', '\u{e001}', '\u{e002}'];

#[test]
fn every_block_span_slices_back_to_its_own_text() {
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

    let mut checked = 0usize;
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
            check(block, &codepoints, &name, &mut checked, &mut wrong);
        }
    }

    assert!(
        checked > 400,
        "only {checked} positioned paragraphs seen - the walk stopped finding them"
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
            InlineNode::Text(text) => {
                let trimmed = text.trim();
                if trimmed.is_empty() || trimmed.chars().any(|c| SENTINELS.contains(&c)) {
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
    checked: &mut usize,
    wrong: &mut Vec<String>,
) {
    if let BlockNode::Paragraph(paragraph) = block {
        if let Some(pos) = paragraph.pos {
            *checked += 1;
            let (start, end) = (pos.start_offset, pos.end_offset);
            if start > end || end > source.len() {
                wrong.push(format!(
                    "{file}: span {start}..{end} is outside the {}-codepoint document",
                    source.len()
                ));
            } else if let Some(want) = anchor(&paragraph.children) {
                let slice: String = source[start..end].iter().collect();
                if !slice.contains(&want) {
                    wrong.push(format!(
                        "{file}: span {start}..{end} is {slice:?}, which does not contain {want:?}"
                    ));
                }
            }
        }
    }
    match block {
        BlockNode::BlockQuote(b) => b
            .children
            .iter()
            .for_each(|c| check(c, source, file, checked, wrong)),
        BlockNode::Admonition(a) => a
            .children
            .iter()
            .for_each(|c| check(c, source, file, checked, wrong)),
        BlockNode::Div(d) => d
            .children
            .iter()
            .for_each(|c| check(c, source, file, checked, wrong)),
        BlockNode::LineBlock(l) => l
            .children
            .iter()
            .for_each(|c| check(c, source, file, checked, wrong)),
        BlockNode::List(list) => {
            for item in &list.items {
                item.children
                    .iter()
                    .for_each(|c| check(c, source, file, checked, wrong));
            }
        }
        BlockNode::DefinitionList(defs) => {
            for item in &defs.items {
                for definition in &item.definitions {
                    definition
                        .iter()
                        .for_each(|c| check(c, source, file, checked, wrong));
                }
            }
        }
        BlockNode::Figure(figure) => {
            if let FigureTarget::BlockQuote(quote) = &figure.target {
                quote
                    .children
                    .iter()
                    .for_each(|c| check(c, source, file, checked, wrong));
            }
        }
        _ => {}
    }
}
