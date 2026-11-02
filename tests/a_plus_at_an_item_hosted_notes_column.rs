//! PART 9 §17 L3: a lone `+` at the marker column of a note hosted on a list
//! marker line gives the note its next block (carve#4359).

use carve::to_html;

#[test]
fn the_note_takes_the_next_block() {
    for (source, body, item) in [
        (
            "- [^m]: x\n  +\n  > q\n\nr[^m]\n",
            "<p>x</p>\n      <blockquote>",
            "<li></li>",
        ),
        (
            "- [^m]: x\n  +\n  p\n\nr[^m]\n",
            "<p>x</p>\n      <p>p",
            "<li></li>",
        ),
        (
            "- [^m]: x\n  y\n  +\n  # h\nt\n\nr[^m]\n",
            "<h1 id=\"h\">h</h1>",
            "<li></li>",
        ),
        (
            "1. [^long]: x\n   +\n   %% c\n   p\n\nr[^long]\n",
            "<p>x</p>\n      <p>p",
            "<li></li>",
        ),
        (
            "- [^m]: x\n  +\n  [r]: /u\n  p\n\nr[^m]\n",
            "<p>x</p>\n      <p>p",
            "<li></li>",
        ),
        (
            "- [^m]: ```\n  +\n  p\n\nr[^m]\n",
            "<pre><code></code></pre>\n      <p>p",
            "<li></li>",
        ),
        (
            "- [^m]: +\n  +\n  p\n\nr[^m]\n",
            "<li id=\"fn1\">\n      <p>p",
            "<li></li>",
        ),
    ] {
        let html = to_html(source);
        assert!(html.contains(body), "{source}\n{html}");
        assert!(html.contains(item), "{source}\n{html}");
    }
}

#[test]
fn a_plus_inside_the_attached_fence_is_code() {
    let html = to_html("- [^m]: x\n  +\n  ```\n  +\n  ```\n\nr[^m]\n");
    assert!(html.contains("<pre><code>+\n</code></pre>"), "{html}");
}

#[test]
fn a_plus_with_no_block_stays_text() {
    let html = to_html("- [^m]: x\n  +\n\nr[^m]\n");
    assert!(html.contains("<p>x\n+"), "{html}");
}

#[test]
fn a_task_box_keeps_the_note_at_its_literal_column() {
    let html = to_html("- [ ] [^m]: x\n      +\n      p\n\nr[^m]\n");
    assert!(html.contains("<p>x\n+\np"), "{html}");
}
