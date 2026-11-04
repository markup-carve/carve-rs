// carve#5113: the blank after a fence that a consumed `+` ended is no fence
// payload, so it loosens the outer item (PART 9 §17 L1, PART 0 S1).

#[test]
fn a_blank_after_a_consumed_plus_loosens_the_outer_item() {
    let source = "- a\n  - ```\n  +\n\n  > q\n";
    let expected = "<ul>\n  <li><p>a</p>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn the_same_shape_in_a_quote_and_before_a_heading_is_loose() {
    let source = "> - a\n>   - ```\n>   +\n>\n>   # h\n";
    let expected = "<blockquote>\n  <ul>\n    <li><p>a</p>\n      <ul>\n        <li>\n          <pre><code></code></pre>\n        </li>\n      </ul>\n      <h1 id=\"h\">h</h1>\n    </li>\n  </ul>\n</blockquote>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn a_plus_inside_the_fence_payload_still_lets_its_blank_separate() {
    let source = "- a\n  - ```\n    +\n\n  text\n";
    let expected = "<ul>\n  <li><p>a</p>\n    <ul>\n      <li>\n        <pre><code>+\n\n</code></pre>\n      </li>\n    </ul>\n    <p>text</p>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn a_dedented_line_ending_the_fence_keeps_the_list_loose() {
    let source = "- a\n  - ```\n   c\n\n  # h\n";
    let expected = "<ul>\n  <li><p>a</p>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n    <p>c</p>\n    <h1 id=\"h\">h</h1>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}
