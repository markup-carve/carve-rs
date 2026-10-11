use carve::{djot_to_carve, to_html};

#[test]
fn rejected_block_comments_keep_visible_text_and_heading_ownership() {
    for source in [
        "{% x\ny %}\n",
        "# a\n{% x\ny %}\n# b\n",
        "> {% x\n> y %}\n",
        "- {% x\n  y %}\n",
    ] {
        let html = to_html(&djot_to_carve(source));
        assert!(html.contains('x') && html.contains('y'), "{html}");
        assert!(!html.contains("<h1>b</h1>"), "{html}");
        if source.starts_with('#') {
            assert!(html.contains("<h1>a</h1>"), "{html}");
        }
    }
}

#[test]
fn valid_comment_shapes_still_disappear() {
    for source in [
        ": def\n  {% x\n  y %} z\n",
        "- a\n {% x\n y %}\n",
        "para\n# not a heading\n{% x\ny %}\n",
        "para\n- not an item\n{% x\ny %}\n",
        "p\n\na\n{% x\ny %} z\n",
        "- a\n\n  b\n  {% x\n  y %} z\n",
        "> p\n>\n> a\n> {% x\n> y %} z\n",
        "Hi. there\n{% x\ny %} z\n",
        "a\n{% x\ny %} z\n",
        "- a\n  {% x\n  y %} z\n",
        "{% x %}\n",
        "{% x\n  y %}\n",
    ] {
        assert!(!djot_to_carve(source).contains('x'));
    }
    assert_eq!(
        to_html(&djot_to_carve("a {% x\ny %} z\n")).trim(),
        "<p>a  z</p>"
    );
}

#[test]
fn an_unindented_rejected_comment_ends_the_list() {
    let html = to_html(&djot_to_carve("- a\n{% x\ny %}\n"));
    assert!(html.contains("<li>a</li>"), "{html}");
    assert!(html.contains("<p>{% x\ny %}</p>"), "{html}");
}

#[test]
fn a_comment_in_a_footnote_body_is_inline() {
    let html = to_html(&djot_to_carve("a[^n]\n\n[^n]: text\n  {% x\n  y %} z\n"));
    assert!(!html.contains("{% x"), "{html}");
    assert!(html.contains("text") && html.contains('z'));
}

#[test]
fn consecutive_block_comments_do_not_recurse_through_their_context() {
    assert!(to_html(&djot_to_carve(&"{% x\n  y %}\n".repeat(64)))
        .trim()
        .is_empty());
}

#[test]
fn a_comment_that_fails_on_its_first_line_continues_the_heading() {
    let html = to_html(&djot_to_carve("# a\n{% 50% off %}\n"));
    assert!(html.contains("<h1>a {% 50% off %}</h1>"), "{html}");
    assert!(!html.contains("<p>"));
}
