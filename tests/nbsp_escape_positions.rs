use carve::ast::{BlockNode, InlineNode};

fn spans(source: &str) -> Vec<(usize, usize)> {
    let doc = carve::parse_with_options(
        source,
        &carve::Options {
            positions: true,
            ..Default::default()
        },
    );
    let BlockNode::Paragraph(p) = &doc.children[0] else {
        panic!("paragraph")
    };
    p.children
        .iter()
        .map(|n| {
            let pos = match n {
                InlineNode::Text(t) => t.pos.as_ref(),
                InlineNode::NonBreakingSpace(n) => n.pos.as_ref(),
                _ => None,
            }
            .expect("source span");
            (pos.start_offset, pos.end_offset)
        })
        .collect()
}

#[test]
fn an_escaped_space_owns_its_backslash_and_space() {
    assert_eq!(spans("say\\ x\n"), [(0, 3), (3, 5), (5, 6)]);
}

#[test]
fn two_escapes_have_separate_exact_spans() {
    assert_eq!(
        spans("a\\ b\\ c\n"),
        [(0, 1), (1, 3), (3, 4), (4, 6), (6, 7)]
    );
}

/// A run with no escape must be unaffected - the delta has to reset per run, or
/// the next span inherits the previous one's correction.
#[test]
fn a_plain_run_after_an_escaped_one_is_still_exact() {
    let source = format!("a{} b\n\nplain text\n", '\\');
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options(&source, &options);
    let BlockNode::Paragraph(para) = &doc.children[1] else {
        panic!("expected the second paragraph");
    };
    let InlineNode::Text(text) = &para.children[0] else {
        panic!("expected a text node");
    };
    let pos = text
        .pos
        .clone()
        .expect("the plain run must carry a position");

    let slice: String = source
        .chars()
        .skip(pos.start_offset)
        .take(pos.end_offset - pos.start_offset)
        .collect();
    assert_eq!(slice, text.value, "the delta leaked into the next run");
}
