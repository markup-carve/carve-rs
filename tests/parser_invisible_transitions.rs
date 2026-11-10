use carve::to_html;

#[test]
fn postblank_unclosed_comment_clears_outer_list_claim() {
    assert_eq!(
        to_html("- - a\n\n  %%%\nx\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>a</li>\n    </ul>\n  </li>\n</ul>\n<p>x</p>"
    );
}

#[test]
fn postblank_comment_span_keeps_the_next_sibling_loose() {
    assert_eq!(
        to_html("- - a\n\n  %%%\n  note\n  %%%\n- sib\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>a</li>\n    </ul>\n  </li>\n  <li><p>sib</p></li>\n</ul>"
    );
}

#[test]
fn reached_wrapped_attribute_closes_nested_claim_across_its_extent() {
    assert_eq!(
        // carve#5080: a closer at the brace's column refuses the run, so it is lazy item text.
        to_html("- - a\n   {c\n}\nx\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>a\n{c\n}\nx</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn below_column_wrapped_attribute_remains_nested_paragraph_text() {
    assert_eq!(
        // carve#5080: a closer at the brace's column refuses the run, so it is lazy item text.
        to_html("- - a\n {c\n}\nx\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>a\n{c\n}\nx</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn task_marker_wrapped_attribute_uses_the_item_column() {
    assert_eq!(
        // carve#5080: a closer at the brace's column refuses the run, so it is lazy item text.
        to_html("- [x] - a\n  {c\n}\nx\n"),
        "<ul class=\"task-list\">\n  <li data-task-state=\"x\"><input type=\"checkbox\" checked disabled> \n    <ul>\n      <li>a\n{c\n}\nx</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn matched_comment_payload_crosses_the_task_item_column() {
    assert_eq!(
        to_html("- [x] - a\n\n      %%%\n  note\n%%%\nx\n"),
        "<ul class=\"task-list\">\n  <li data-task-state=\"x\"><input type=\"checkbox\" checked disabled> \n    <ul>\n      <li>a</li>\n    </ul>\n  </li>\n</ul>\n<p>x</p>"
    );
}

#[test]
fn ordered_item_matched_comment_payload_is_opaque_across_indent() {
    // PART 0 O1 makes the matched span one source extent across authored
    // indentation. The current spec model gap is tracked in carve#3881.
    assert_eq!(
        to_html("1. 1. a\n\n   %%%\n  note\n%%%\nx\n"),
        // carve#4907: the column-0 `%%%` opens a span that hides `x`.
        "<ol>\n  <li>\n    <ol>\n      <li>a</li>\n    </ol>\n  </li>\n</ol>\n<p>note</p>"
    );
}

#[test]
fn nested_comment_blank_does_not_loosen_an_ancestor_item() {
    assert_eq!(
        to_html("- host\n  - - a\n\n    %% note\n  x\n"),
        "<ul>\n  <li><p>host</p>\n    <ul>\n      <li>\n        <ul>\n          <li>a</li>\n        </ul>\n      </li>\n    </ul>\n    <p>x</p>\n  </li>\n</ul>"
    );
}

#[test]
fn nested_braced_comment_preserves_blank_before_outer_sibling() {
    let expected = "<ul>\n  <li><p>a</p>\n    <ul>\n      <li>x</li>\n    </ul>\n  </li>\n  <li><p>s</p></li>\n</ul>";
    for invisible in ["{% a z %}", "{% a\n     z %}", "%% c"] {
        let source = format!("- a\n  - x\n\n    {invisible}\n- s\n");
        assert_eq!(to_html(&source), expected, "{source}");
    }
}

#[test]
fn braced_comment_matches_line_comment_at_nested_blank_boundary() {
    let mut checked = 0;
    for (outer, sibling) in [
        ("- a", "- s"),
        ("* a", "* s"),
        ("1. a", "2. s"),
        ("- [ ] a", "- [ ] s"),
    ] {
        for (inner, width) in [("- x", 2), ("1. x", 3), ("- [ ] x", 6)] {
            for depth in [1, 2] {
                for tail_kind in ["", "y", "- y"] {
                    for shift in 0..3 {
                        for brace in [
                            "{% a z %}",
                            "{% a\n{}z %}",
                            "{% a %}{% z %}",
                            "{% a %}",
                            "{% z %}",
                        ] {
                            let indent =
                                " ".repeat(2 + width + shift + if depth == 2 { 2 } else { 0 });
                            let inner_line = if depth == 2 {
                                format!("  - b\n    {inner}")
                            } else {
                                format!("  {inner}")
                            };
                            let tail = if tail_kind.is_empty() {
                                String::new()
                            } else {
                                format!(
                                    "\n{}{tail_kind}",
                                    " ".repeat(2 + width + if depth == 2 { 2 } else { 0 })
                                )
                            };
                            let payload = if brace.contains("{}") {
                                brace.replace("{}", &" ".repeat(indent.len() + 1))
                            } else {
                                brace.to_string()
                            };
                            let body = format!("{outer}\n{inner_line}{tail}\n\n{indent}");
                            let a = format!("{body}{payload}\n{sibling}\n");
                            let b = format!("{body}%% c\n{sibling}\n");
                            assert_eq!(to_html(&a), to_html(&b), "brace={a:?}, twin={b:?}");
                            checked += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(checked, 1080);
}
