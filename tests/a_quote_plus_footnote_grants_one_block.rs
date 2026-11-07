//! PART 9 §17 L3: a quote's `+` grants an invisible definition as one block.
//! A later visible opener outside that grant keeps its own reading.

#[test]
fn a_fence_after_the_granted_definition_stays_visible() {
    assert_eq!(
        carve::to_html("> +\n[^n]: x\n  ```\n  a\n  ```\n"),
        "<blockquote>\n\n</blockquote>\n<pre><code>a\n</code></pre>"
    );
}

#[test]
fn a_heading_after_the_granted_definition_stays_visible() {
    assert_eq!(
        carve::to_html("> +\n[^n]: x\n  # H\n"),
        "<blockquote>\n\n</blockquote>\n<section id=\"H\">\n  <h1>H</h1>\n</section>"
    );
}

#[test]
fn a_following_quote_is_a_separate_block() {
    assert_eq!(
        carve::to_html("> +\n[^n]: x\n  > a\n"),
        "<blockquote>\n\n</blockquote>\n<blockquote><p>a</p></blockquote>"
    );
}

#[test]
fn a_note_body_line_does_not_split_later_list_siblings() {
    assert_eq!(
        carve::to_html("> +\n[^n]: x\n  y\n  - a\n- end\n"),
        "<blockquote>\n\n</blockquote>\n<ul>\n  <li>a</li>\n  <li>end</li>\n</ul>"
    );
}
