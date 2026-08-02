//! An `escaped_text` node published U+E002 where the reference publishes the
//! caret itself (carve-rs#408).
//!
//! It was the only escape treated that way - every other one already stored the
//! plain character. The marker existed so a `\^` would not be read as a caption
//! marker, but nothing downstream ever read it for that: every consumer in the
//! tree mapped it straight back to `^`, and the caption decision is made from
//! source lines, not from a node's value.
//!
//! The node type already carries what the marker was distinguishing. An
//! `escaped_text` node IS an escape, so the writer emits a backslash plus the
//! value without the value needing to say so again.

const MARKER: char = '\u{e002}';

fn escaped_values(source: &str) -> Vec<String> {
    let doc = carve::parse(source);
    let mut out = Vec::new();
    fn walk(nodes: &[carve::ast::InlineNode], out: &mut Vec<String>) {
        for node in nodes {
            match node {
                carve::ast::InlineNode::EscapedText(e) => out.push(e.value.clone()),
                carve::ast::InlineNode::Emphasis(e) => walk(&e.children, out),
                carve::ast::InlineNode::Link(l) => walk(&l.children, out),
                _ => {}
            }
        }
    }
    for block in &doc.children {
        if let carve::ast::BlockNode::Paragraph(p) = block {
            walk(&p.children, out.as_mut());
        }
    }
    out
}

#[test]
fn an_escaped_caret_publishes_the_caret() {
    let source = format!("a {}^ b\n", '\\');
    assert_eq!(escaped_values(&source), vec!["^".to_string()]);
}

#[test]
fn no_escape_publishes_a_private_use_character() {
    let bs = '\\';
    let source = format!("{}^ {}* {}_ {}~ {}[ {}\n", bs, bs, bs, bs, bs, bs);
    for value in escaped_values(&source) {
        assert!(
            !value
                .chars()
                .any(|c| ('\u{e000}'..='\u{f8ff}').contains(&c)),
            "a private-use character reached a published value: {value:?}"
        );
    }
}

/// The writer must still reproduce the escape. It does so from the node TYPE, so
/// this passes without the value carrying a marker - which is the whole point.
#[test]
fn the_writer_still_reproduces_the_escape() {
    let source = format!("a {}^ b\n", '\\');
    assert_eq!(carve::to_carve(&source), source);
}

/// The reason the marker was introduced. A caret line after an image promotes it
/// to a figure; an ESCAPED caret must not. That decision is made from source
/// lines, so removing the marker cannot affect it - and this pins that it does
/// not, because the claim is easy to assert and easy to get wrong.
#[test]
fn an_escaped_caret_line_still_does_not_become_a_caption() {
    let source = format!("![alt](x.png)\n{}^ not a caption\n", '\\');
    let html = carve::to_html(&source);

    assert!(
        !html.contains("<figure"),
        "an escaped caret was read as a caption marker: {html}"
    );
    assert!(html.contains('^'), "the caret itself disappeared: {html}");
    assert!(
        !html.contains(MARKER),
        "the marker leaked into rendered output: {html}"
    );
}
