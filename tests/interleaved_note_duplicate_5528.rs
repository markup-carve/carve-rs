//! carve#5528: a later winning note keeps its body through losing definitions.

use carve::to_html;

fn owner(source: &str) -> String {
    let html = to_html(source);
    let first = html.find("<li id=\"fn1\">").expect("first note");
    let second = html.find("<li id=\"fn2\">").expect("second note");
    assert!(!html[..first].contains("<p>para</p>"), "{source}\n{html}");
    assert!(!html[first..second].contains("<p>para"), "{source}\n{html}");
    assert!(
        html[second..].contains("<p>para<a href=\"#fnref2\""),
        "{source}\n{html}"
    );
    html
}

#[test]
fn later_note_owns_the_indented_body_with_or_without_losing_duplicates() {
    for duplicates in ["", "[^n]: +\n", "[^n]: +\n[^n]: +\n"] {
        let source = format!("[^m]: +\n[^n]: +\n{duplicates}\n  para\n\nsee[^m][^n]\n");
        owner(&source);
    }
}

#[test]
fn later_note_owns_a_list_after_the_losing_duplicate() {
    let html = to_html("[^m]: +\n[^n]: +\n[^n]: +\n\n  - item\n\nsee[^m][^n]\n");
    assert!(!html.starts_with("<ul>"), "{html}");
    assert!(
        html.contains("<li id=\"fn2\">\n      <ul>\n        <li>item</li>"),
        "{html}"
    );
}

#[test]
fn an_indented_note_shaped_line_does_not_panic() {
    let source = "[^m]: +\n[^n]: +\n  [^n]: +\n\n  para\n\nsee[^m][^n]\n";
    let html = to_html(source);
    assert!(html.contains("<p>para<a href=\"#fnref2\""), "{html}");
}

#[test]
fn a_list_or_description_host_passes_the_later_note_its_body() {
    for (source, host) in [
        (
            "- [^m]: +\n  [^n]: +\n  [^n]: +\n\n    para\n\nsee[^m][^n]\n",
            "<li></li>",
        ),
        (
            ":: term\n: [^m]: +\n  [^n]: +\n  [^n]: +\n\n    para\n\nsee[^m][^n]\n",
            "<dd></dd>",
        ),
    ] {
        let html = to_html(source);
        assert!(html.contains(host), "{source}\n{html}");
        assert!(
            html.contains("<p>para<a href=\"#fnref2\""),
            "{source}\n{html}"
        );
    }
}

#[test]
fn a_quoted_note_keeps_an_authored_indented_block() {
    let inside =
        to_html("> [^m]: +\n> [^n]: +\n> [^n]: +\n> [^n]: +\n>\n>   - item\n\nsee[^m][^n]\n");
    assert!(
        inside.starts_with("<blockquote>\n\n</blockquote>"),
        "{inside}"
    );
    assert!(inside.contains("<li id=\"fn2\">\n      <ul>"), "{inside}");

    let outside =
        to_html("> [^m]: +\n> [^n]: +\n> [^n]: +\n> [^n]: +\n>\n> - item\n\nsee[^m][^n]\n");
    assert!(outside.starts_with("<blockquote>\n  <ul>"), "{outside}");
}

#[test]
fn the_winning_note_keeps_an_attached_blocks_continuation() {
    for source in [
        "[^m]: +\n[^n]: +\n[^n]: +\n\n  - item\n    continuation\n\nsee[^m][^n]\n",
        "> [^m]: +\n> [^n]: +\n> [^n]: +\n> [^n]: +\n>\n>   - item\n>     continuation\n\nsee[^m][^n]\n",
    ] {
        let html = to_html(source);
        assert!(html.contains("<li>item\ncontinuation</li>"), "{source}\n{html}");
        assert!(!html.starts_with("<ul>"), "{source}\n{html}");
    }
}

#[test]
fn the_winning_note_keeps_blank_payload_inside_a_closed_fence() {
    let html =
        to_html("[^m]: +\n[^n]: +\n[^n]: +\n\n  ```x\n  code\n\n  more\n  ```\n\nsee[^m][^n]\n");
    assert!(
        html.contains("<pre><code class=\"language-x\">code\n\nmore\n</code></pre>"),
        "{html}"
    );
    assert!(html.starts_with("<p>see"), "{html}");
}
