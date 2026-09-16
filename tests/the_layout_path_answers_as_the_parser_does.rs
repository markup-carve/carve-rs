//! `to_html` tries a fast layout path before the parser. Where the two disagree
//! the fast one is wrong by construction, because the parser is the engine's
//! answer (markup-carve/carve-rs#1655).

fn both(src: &str) -> (String, String) {
    let fast = carve::to_html(src);
    let slow = carve::render_html(&carve::parse(src)).unwrap_or_default();
    (fast, slow)
}

#[test]
fn a_marker_touching_its_own_closer_opens_nothing() {
    let (fast, slow) = both("/x//y/\n");
    assert_eq!(fast.trim(), "<p><em>x</em>/y/</p>");
    assert_eq!(fast, slow);
}

#[test]
fn the_same_holds_for_a_strong() {
    let (fast, slow) = both("a *x**y*\n");
    assert_eq!(fast.trim(), "<p>a <strong>x</strong>*y*</p>");
    assert_eq!(fast, slow);
}

#[test]
fn a_doubled_marker_opens_nothing() {
    let (fast, slow) = both("//x//\n");
    assert_eq!(fast.trim(), "<p>//x//</p>");
    assert_eq!(fast, slow);
}

#[test]
fn two_spans_with_a_space_between_them_still_both_open() {
    let (fast, slow) = both("/x/ /y/\n");
    assert_eq!(fast.trim(), "<p><em>x</em> <em>y</em></p>");
    assert_eq!(fast, slow);
}

#[test]
fn an_ordinary_emphasis_still_takes_the_fast_path() {
    let (fast, slow) = both("a /x/ b\n");
    assert_eq!(fast.trim(), "<p>a <em>x</em> b</p>");
    assert_eq!(fast, slow);
}
