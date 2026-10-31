use carve::to_html;

#[test]
fn nested_wrapped_attributes_keep_their_container_boundary() {
    let mut failures = Vec::new();
    for (source, expected) in [
        (
            ":: t\n: - a\n    {.c\n}\npara\n",
            "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>a</li>\n    </ul>\n  </dd>\n</dl>\n<p>para</p>",
        ),
        (
            ":: t\n: d\n\n  - a\n    {.c\n}\npara\n",
            "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>d</p>\n    <ul>\n      <li>a</li>\n    </ul>\n  </dd>\n</dl>\n<p>para</p>",
        ),
        (
            "> - a\n>   {.c\n> }\npara\n",
            "<blockquote>\n  <ul>\n    <li>a</li>\n  </ul>\n</blockquote>\n<p>para</p>",
        ),
        (
            "> - a\n>   {.c\n}\npara\n",
            "<blockquote>\n  <ul>\n    <li>a\n{.c\n}\npara</li>\n  </ul>\n</blockquote>",
        ),
    ] {
        let actual = to_html(source);
        if actual != expected {
            failures.push(format!("{source:?}: {actual:?} != {expected:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn quoted_braces_outside_the_nested_item_column_remain_text() {
    let after_blank = to_html("> - a\n>\n>   {x\n>   y\n");
    assert!(
        after_blank.contains("<li><p>a</p>\n      <p>{x\ny</p>"),
        "{after_blank}"
    );

    // §15 A5: an indented wrapped block is an attribute line and leaves no
    // paragraph open, so the unprefixed line after it ends the quote.
    let overindented = to_html("> - a\n>    {.c\n>   }\npara\n");
    assert!(
        overindented.ends_with("</blockquote>\n<p>para</p>"),
        "{overindented}"
    );
    assert!(!overindented.contains("class=\"c\""), "{overindented}");

    let top_level = to_html("> - a\n>\n> {.c\n> }\n  cont\n");
    assert!(
        top_level.ends_with("</blockquote>\n<p>cont</p>"),
        "{top_level}"
    );
}

#[test]
fn explicit_continuation_still_attaches_after_the_nested_attribute() {
    let html = to_html(":: t\n: - a\n    {.c\n}\n+\npara\n");
    assert!(html.contains("    <p>para</p>\n  </dd>"), "{html}");
}

#[test]
fn wrapped_and_single_line_nested_attributes_have_the_same_owner() {
    let mut failures = Vec::new();
    for (single, wrapped) in [
        (
            ":: t\n: - a\n    - b\n      {.c}\npara\n",
            ":: t\n: - a\n    - b\n      {.c\n}\npara\n",
        ),
        (
            ":: t\n: - a\n    - b\n    {.c}\npara\n",
            ":: t\n: - a\n    - b\n    {.c\n}\npara\n",
        ),
        (
            "> - a\n>   - b\n>     {.c}\npara\n",
            "> - a\n>   - b\n>     {.c\n> }\npara\n",
        ),
        (
            "> - a\n>   - b\n>   {.c}\npara\n",
            "> - a\n>   - b\n>   {.c\n> }\npara\n",
        ),
        (
            "> - a\n>   - b\n>\n>   p\n>   {.c}\npara\n",
            "> - a\n>   - b\n>\n>   p\n>   {.c\n> }\npara\n",
        ),
        (
            "> - a\n>   - b\n>\n>   # H\n>   {.c}\npara\n",
            "> - a\n>   - b\n>\n>   # H\n>   {.c\n> }\npara\n",
        ),
        (
            ":: t\n: - a\n\n    {.c}\nmore\n",
            ":: t\n: - a\n\n    {.c\n    }\nmore\n",
        ),
        (
            "> - a\n>\n>   {.c}\n    cont\n",
            "> - a\n>\n>   {.c\n>   }\n    cont\n",
        ),
    ] {
        let one = to_html(single);
        let multi = to_html(wrapped);
        if one != multi {
            failures.push(format!("{wrapped:?}: {multi:?} != {one:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));

    let overindented = to_html(":: t\n: - a\n      {.c\n}\npara\n");
    assert!(
        overindented.ends_with("</dl>\n<p>para</p>"),
        "{overindented}"
    );
}

#[test]
fn a_line_at_the_definition_column_releases_the_nested_item() {
    for (source, expected) in [
        (
            ":: t\n: - a\n  p\n    {.c\n}\ntail\n",
            "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>a\np</li>\n    </ul>\n  </dd>\n</dl>\n<p>tail</p>",
        ),
        (
            ":: t\n: - a\n\n  p\n    {.c\n}\ntail\n",
            "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>a</li>\n    </ul>\n    <p>p</p>\n  </dd>\n</dl>\n<p>tail</p>",
        ),
    ] {
        assert_eq!(to_html(source), expected, "{source:?}");
    }
}
