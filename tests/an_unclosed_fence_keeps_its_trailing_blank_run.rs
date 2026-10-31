//! PART 0 S2 and corpus 291: a blank run reaches every frame, so an unclosed
//! fence that ends with its container keeps every trailing blank line, at any
//! depth and in a description or note body (carve#4033, #4047, #4437).

use carve::to_html;

fn code(source: &str) -> String {
    let html = to_html(source);
    let start = html.find("<pre><code>").expect("a code block") + "<pre><code>".len();
    let end = html[start..].find("</code></pre>").expect("closed") + start;
    html[start..end].to_string()
}

#[test]
fn a_nested_fence_keeps_every_blank_line() {
    assert_eq!(code("- - - ```\n      x\n\n\n    q\n"), "x\n\n\n");
    assert_eq!(code("- - - ```\n      x\n\n\n      y\n"), "x\n\n\ny\n");
    assert_eq!(code("- - ```\n    x\n\n\n  q\n"), "x\n\n\n");
}

#[test]
fn the_blank_run_still_separates_the_outer_item() {
    let html = to_html("- - ```\n    x\n\n  q\n");
    assert!(html.contains("<p>q</p>"), "{html}");
}

#[test]
fn a_description_body_keeps_the_blank() {
    assert_eq!(code(":: t\n: ```\n  code\n\nafter\n"), "code\n\n");
    assert_eq!(code("- :: t\n  : ```\n    code\n\nafter\n"), "code\n\n");
}

#[test]
fn a_note_body_keeps_the_blank() {
    assert_eq!(code("x[^1]\n\n[^1]: ```\n  code\n\nafter\n"), "code\n\n");
    assert_eq!(
        code("x[^1]\n\n- [^1]: ```\n    code\n\nafter\n"),
        "code\n\n"
    );
    assert_eq!(code("x[^1]\n\n- [^1]: ```\n\nafter\n"), "\n");
    assert_eq!(code("[^a]: note\n  ```\n   code\n\nsee[^a]\n"), " code\n\n");
}

#[test]
fn a_plus_attached_fence_ends_at_the_blank() {
    assert_eq!(code("[^a]: +\n```\nx\n\nsee[^a]\n"), "x\n");
    assert_eq!(code("[^f]: n\n+\n```\na\n\nb\n\nsee[^f]\n"), "a\n");
}
