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
