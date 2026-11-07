use carve::to_html;

#[test]
fn a_blank_ends_the_list_attached_by_a_quote() {
    let source = "> +\n- t\n\n  > r\n";
    assert_eq!(
        to_html(source).trim(),
        "<blockquote>\n  <ul>\n    <li>t</li>\n  </ul>\n</blockquote>\n<blockquote><p>r</p></blockquote>"
    );
    assert_eq!(to_html(source), to_html("> - t\n\n  > r\n"));
}

#[test]
fn a_blank_ends_the_description_attached_by_a_quote() {
    let source = "> +\n:: t\n: u\n\n  # r\n";
    assert_eq!(
        to_html(source).trim(),
        "<blockquote>\n  <dl>\n    <dt>t</dt>\n    <dd>u</dd>\n  </dl>\n</blockquote>\n<section id=\"r\">\n  <h1>r</h1>\n</section>"
    );
    assert_eq!(to_html(source), to_html("> :: t\n> : u\n\n  # r\n"));
}

#[test]
fn a_blank_ends_the_attached_quote_without_ending_its_item() {
    let source = "- a\n  > +\n  :: t\n  : u\n\n    > r\n";
    assert_eq!(
        to_html(source),
        to_html("- a\n  > :: t\n  > : u\n\n    > r\n")
    );
}

#[test]
fn an_interior_code_payload_blank_keeps_the_attachment() {
    let attached = "> +\n```\na\n\nb\n```\n\n  > r\n";
    let prefixed = "> ```\n> a\n>\n> b\n> ```\n\n  > r\n";
    assert_eq!(to_html(attached), to_html(prefixed));
}

#[test]
fn an_interior_colon_payload_blank_keeps_the_attachment() {
    let attached = "> +\n::: note\na\n\nb\n:::\n\n  > r\n";
    let prefixed = "> ::: note\n> a\n>\n> b\n> :::\n\n  > r\n";
    assert_eq!(to_html(attached), to_html(prefixed));
}

// The attached extent stops at a blank that no leading fence holds (model `oneBlockEnd`).
#[test]
fn a_blank_in_an_item_fence_bounds_the_attached_extent() {
    let source = "> a\n+\n- t\n  ```\n  a\n\n  b\n  ```\n\n  # r\n";
    assert_eq!(
        to_html(source).trim(),
        "<blockquote>\n  <p>a</p>\n  <ul>\n    <li>t\n      <pre><code>a\n</code></pre>\n    </li>\n  </ul>\n</blockquote>\n<p>b\n<code></code></p>\n<p># r</p>"
    );
}

#[test]
fn a_blank_in_an_item_fence_keeps_a_following_quote_literal() {
    let source = "> +\n- t\n  ```\n  a\n\n  b\n  ```\n\n  > r\n";
    assert!(to_html(source).trim_end().ends_with("<p>&gt; r</p>"));
}
