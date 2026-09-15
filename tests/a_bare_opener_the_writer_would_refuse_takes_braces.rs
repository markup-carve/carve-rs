//! A bare emphasis whose opener the character just written would refuse
//! (CARVE-P3-013) takes the braced form; the span before it is untouched
//! (markup-carve/carve-rs#1649, markup-carve/carve#2045).

fn carve(src: &str) -> String {
    carve::to_carve(src)
}

#[test]
fn a_second_italic_after_an_italic_takes_braces() {
    assert_eq!(carve("{/x/}{/y/}\n"), "/x/{/y/}\n");
}

#[test]
fn a_second_strong_after_a_strong_takes_braces() {
    assert_eq!(carve("{*x*}{*y*}\n"), "*x*{*y*}\n");
}

#[test]
fn a_second_highlight_after_a_highlight_takes_braces() {
    assert_eq!(carve("{=x=}{=y=}\n"), "=x={=y=}\n");
}

#[test]
fn an_underline_after_a_slash_takes_braces() {
    assert_eq!(carve("{/x/}{_y_}\n"), "/x/{_y_}\n");
}

#[test]
fn an_emphasis_after_a_literal_slash_takes_braces() {
    assert_eq!(carve("a /{/x/}/b\n"), "a /{/x/}/b\n");
}

#[test]
fn a_different_marker_that_can_open_stays_bare() {
    assert_eq!(carve("{/x/}{*y*}\n"), "/x/*y*\n");
    assert_eq!(carve("{/x/}{~y~}\n"), "/x/~y~\n");
}

#[test]
fn attributes_stay_outside_the_braces() {
    assert_eq!(carve("{/x/}{/y/}{.c}\n"), "/x/{/y/}{.c}\n");
}

#[test]
fn the_corpus_463_shape_round_trips() {
    let src = "~{/x/}{/y~/}\n";
    let out = carve(src);
    assert_eq!(out, "~/x/{/y~/}\n");
    assert_eq!(carve::to_html(&out), carve::to_html(src));
}
