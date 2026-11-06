//! PART 9 §24 C3, CARVE-P9-083: the gap after a task box is the item's
//! authored gap, so a `>` after any gap width opens a block quote on the
//! marker line, as a heading or list marker there already did (carve-rs#4015).

use carve::to_html;

const QUOTE: &str = "<ul class=\"task-list\">\n  <li><input type=\"checkbox\" disabled> \n    <blockquote><p>q</p></blockquote>\n  </li>\n</ul>";

#[test]
fn a_quote_after_a_wide_task_gap_is_a_quote() {
    for gap in 1..=5 {
        let src = format!("- [ ]{}> q\n", " ".repeat(gap));
        assert_eq!(to_html(&src), QUOTE, "gap {gap}");
    }
}

#[test]
fn a_plus_quote_after_a_wide_task_gap_takes_its_body() {
    let html = to_html("- [ ]  > +\n       p\n");
    assert!(html.contains("<blockquote><p>p</p></blockquote>"), "{html}");
    assert!(!html.contains("&gt;"), "{html}");
}

#[test]
fn text_after_a_wide_task_gap_stays_text() {
    let html = to_html("- [x]   two  words\n");
    assert!(html.contains("checked disabled aria-label=\"two words\"> two  words</li>"), "{html}");
}
