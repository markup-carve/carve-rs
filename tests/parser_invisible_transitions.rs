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
        to_html("- - a\n   {c\n}\nx\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>a\n{c\n}</li>\n    </ul>\n  </li>\n</ul>\n<p>x</p>"
    );
}

#[test]
fn below_column_wrapped_attribute_remains_nested_paragraph_text() {
    assert_eq!(
        to_html("- - a\n {c\n}\nx\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>a\n{c\n}\nx</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn task_marker_wrapped_attribute_uses_the_item_column() {
    assert_eq!(
        to_html("- [x] - a\n  {c\n}\nx\n"),
        "<ul class=\"task-list\">\n  <li data-task-state=\"x\"><input type=\"checkbox\" checked disabled> \n    <ul>\n      <li>a</li>\n    </ul>\n  </li>\n</ul>\n<p>x</p>"
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
        "<ol>\n  <li>\n    <ol>\n      <li>a</li>\n    </ol>\n  </li>\n</ol>\n<p>x</p>"
    );
}

#[test]
fn nested_comment_blank_does_not_loosen_an_ancestor_item() {
    assert_eq!(
        to_html("- host\n  - - a\n\n    %% note\n  x\n"),
        "<ul>\n  <li>host\n    <ul>\n      <li>\n        <ul>\n          <li>a</li>\n        </ul>\n      </li>\n    </ul>\n    x\n  </li>\n</ul>"
    );
}
