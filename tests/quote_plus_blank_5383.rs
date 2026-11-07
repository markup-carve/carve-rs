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
