//! An unclosed fence a quote's first-block `+` attaches keeps an empty last
//! line, as the spec model and carve-js read it, unless the document ends on
//! its last line (markup-carve/carve#3421).

use carve::to_html;

#[test]
fn the_fence_payload_ends_with_an_empty_line() {
    for (source, code) in [
        ("> +\n```\nx\n", "x\n\n"),
        ("> +\n```\nx\n\npara\n", "x\n\n"),
        ("- > +\n  ```\n  x\n- b\n", "x\n\n"),
        ("- a\n  > +\n  ```\nx\n```\n", "x\n```\n\n"),
        ("> > +\n> ```\n> x\n", "x\n\n"),
        ("- > +\n  ```\n  x\n- b", "x\n\n"),
        ("> +\n```\nx", "x"),
        ("> +\n```\nx\n```\n", "x\n"),
        ("> ```\n> x\n\npara\n", "x\n"),
    ] {
        let html = to_html(source);
        assert!(
            html.contains(&format!("<code>{code}</code>")),
            "{source:?}: {html}"
        );
    }
}
