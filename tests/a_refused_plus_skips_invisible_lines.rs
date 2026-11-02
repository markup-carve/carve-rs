//! The `+` gates read the first visible line past invisible ones. A blank, the
//! end of input or a sibling there refuses it, and it stays a letter
//! (CARVE-P9-067, carve#4449, carve#4870).

fn flat(source: &str) -> String {
    carve::to_html(source)
        .split('\n')
        .map(str::trim)
        .collect::<String>()
}

#[test]
fn a_comment_fence_crossing_a_quote_end_refuses_the_plus() {
    assert_eq!(
        flat("> - +\n> %%%\n\n> %%%\n> text\n"),
        // carve#4907: the second quote's unclosed span hides `text`.
        "<blockquote><ul><li>+</li></ul></blockquote><blockquote></blockquote>"
    );
}

#[test]
fn nothing_visible_after_the_invisible_lines_refuses_the_plus() {
    for invisible in ["%% c", "%%%\nc\n%%%", "[r]: /u", "[^q]: n", "*[A]: b"] {
        for (tail, expected) in [
            ("", "<ul><li>+</li></ul>"),
            ("\ntext\n", "<ul><li>+</li></ul><p>text</p>"),
        ] {
            let source = format!("- +\n{invisible}\n{tail}");
            assert_eq!(flat(&source), expected, "{source:?}");
        }
    }
}

#[test]
fn a_sibling_after_the_invisible_line_refuses_the_plus() {
    assert_eq!(flat("- +\n%% c\n- b\n"), "<ul><li>+</li><li>b</li></ul>");
}

#[test]
fn a_visible_block_after_the_invisible_line_still_attaches() {
    assert_eq!(flat("- +\n%% c\ntext\n"), "<ul><li>text</li></ul>");
}

#[test]
fn a_comment_span_or_comment_paragraph_before_the_block_is_skipped() {
    for invisible in ["%%%\nc\n%%%", "{% c %}"] {
        for (tail, expected) in [
            ("text", "<ul><li>text</li></ul>"),
            ("> q", "<ul><li><blockquote><p>q</p></blockquote></li></ul>"),
        ] {
            let source = format!("- +\n{invisible}\n{tail}\n");
            assert_eq!(flat(&source), expected, "{source:?}");
        }
    }
}

#[test]
fn a_comment_paragraph_with_nothing_after_it_leaves_the_plus_a_letter() {
    assert_eq!(flat("- a\n+\n{% c %}\n"), "<ul><li>a+</li></ul>");
}

#[test]
fn a_skipped_comment_stays_in_the_item() {
    use carve::ast::BlockNode;
    let doc = carve::parse("- +\n%%%\nc\n%%%\ntext\n");
    let BlockNode::List(list) = &doc.children[0] else {
        panic!("expected a list, got {:?}", doc.children[0]);
    };
    let kinds: Vec<_> = list.items[0]
        .children
        .iter()
        .map(|block| matches!(block, BlockNode::Comment(_)))
        .collect();
    assert_eq!(kinds, [true, false]);
}
