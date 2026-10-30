//! An unclosed fence a quote's first-block `+` attaches ends with its last
//! payload line: the spec model and carve-js add no empty line after it
//! (markup-carve/carve#4018).

use carve::to_html;

#[test]
fn the_fence_payload_ends_with_its_last_line() {
    for (source, code) in [
        ("> +\n```\nx\n", "x\n"),
        ("> +\n```\nx\n\npara\n", "x\n"),
        ("- > +\n  ```\n  x\n- b\n", "x\n"),
        ("- a\n  > +\n  ```\nx\n```\n", ""),
        ("> > +\n> ```\n> x\n", "x\n"),
        ("- > +\n  ```\n  x\n- b", "x\n"),
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
