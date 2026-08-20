//! AN EMPTY BRACE PAIR IS NOT A CONSTRUCT, and the string the empty deletion
//! freed is the braced en dash (markup-carve/carve#1447).
//!
//! Every content slot involved is a one-or-more repetition -- `forced_content`
//! and `inline_content` both -- so an opener that meets its own closer opened
//! nothing and its characters are text. This engine already read the forced
//! spans that way; the editorial family is what moves.
//!
//! The dash half exists because the bare run carries a flanking guard
//! (carve#1443): a run with whitespace before it and a non-whitespace character
//! after it is flag-shaped and stays literal, which is right for a long CLI flag
//! and wrong for the author who meant a dash there. `{--}` is the way to say it,
//! and it cost nothing -- the string it took was an empty `<del>`.

fn html(source: &str) -> String {
    carve::to_html(source)
}

#[test]
fn every_empty_forced_span_is_literal() {
    for pair in ["{//}", "{**}", "{__}", "{~~}", "{^^}", "{,,}", "{==}"] {
        assert_eq!(html(&format!("{pair}\n")), format!("<p>{pair}</p>"));
    }
}

#[test]
fn the_empty_addition_and_comment_are_literal() {
    assert_eq!(html("{++}\n"), "<p>{++}</p>");
    assert_eq!(html("{##}\n"), "<p>{##}</p>");
}

#[test]
fn an_empty_pair_does_not_swallow_the_next_construct() {
    // The empty pair being text has to mean it STOPS there. carve-js's lazy
    // forced-span run and carve-php's closer scan both grew past their own
    // closer and took the next construct with them, so `{//} x {/y/}` came back
    // as one `<em>` holding `/} x {/y`. This engine refuses at the closer; the
    // case is pinned so it stays that way.
    assert_eq!(html("{//} x {/y/}\n"), "<p>{//} x <em>y</em></p>");
    assert_eq!(html("{**} x {*b*}\n"), "<p>{**} x <strong>b</strong></p>");
    assert_eq!(html("{~~} x {~s~}\n"), "<p>{~~} x <s>s</s></p>");
    assert_eq!(html("{==} x {=h=}\n"), "<p>{==} x <mark>h</mark></p>");
    assert_eq!(html("{++} x {+y+}\n"), "<p>{++} x <ins>y</ins></p>");
    assert_eq!(html("{--} x {-y-}\n"), "<p>\u{2013} x <del>y</del></p>");
}

#[test]
fn a_pair_holding_something_is_still_the_construct() {
    assert_eq!(html("{/i/}\n"), "<p><em>i</em></p>");
    assert_eq!(html("{*b*}\n"), "<p><strong>b</strong></p>");
    assert_eq!(html("{~s~}\n"), "<p><s>s</s></p>");
    assert_eq!(html("{+ins+}\n"), "<p><ins>ins</ins></p>");
    assert_eq!(html("{-del-}\n"), "<p><del>del</del></p>");
    assert_eq!(
        html("{# c #}\n"),
        "<p><span class=\"critic-comment\"> c </span></p>"
    );
}

#[test]
fn a_fully_empty_substitution_is_left_alone() {
    // Its halves are independent, and a half-empty substitution is an ordinary
    // edit -- a deletion with no replacement, an insertion replacing nothing --
    // so requiring content per half would refuse real documents.
    assert_eq!(html("{~a~>~}\n"), "<p><del>a</del><ins></ins></p>");
    assert_eq!(html("{~~>b~}\n"), "<p><del></del><ins>b</ins></p>");
    assert_eq!(html("{~~>~}\n"), "<p><del></del><ins></ins></p>");
}

#[test]
fn a_deletion_holding_a_hyphen_is_untouched() {
    // The one string that moved is the EMPTY deletion, which is also why there
    // is no braced em dash: a three-hyphen brace deletes a hyphen, and that is
    // a thing an author writes.
    assert_eq!(html("{---}\n"), "<p><del>-</del></p>");
    assert_eq!(html("{-x-}\n"), "<p><del>x</del></p>");
}

#[test]
fn the_braced_pair_converts_where_the_bare_run_is_refused() {
    assert_eq!(html("a ---(p) b\n"), "<p>a ---(p) b</p>");
    assert_eq!(html("a {--}(p) b\n"), "<p>a \u{2013}(p) b</p>");
    assert_eq!(html("x {--}verbose y\n"), "<p>x \u{2013}verbose y</p>");
}

#[test]
fn the_braced_pair_consumes_its_braces_wherever_it_stands() {
    assert_eq!(html("x{--}y\n"), "<p>x\u{2013}y</p>");
    assert_eq!(html("{--}start\n"), "<p>\u{2013}start</p>");
    assert_eq!(html("{--}{--}\n"), "<p>\u{2013}\u{2013}</p>");
}

#[test]
fn the_braced_pair_is_inline_content() {
    assert_eq!(html("*a {--} b*\n"), "<p><strong>a \u{2013} b</strong></p>");
    assert_eq!(
        html("[a {--} b](u)\n"),
        "<p><a href=\"u\">a \u{2013} b</a></p>"
    );
}

#[test]
fn the_braced_pair_is_not_read_inside_a_code_span() {
    assert_eq!(html("`{--}`\n"), "<p><code>{--}</code></p>");
}

#[test]
fn the_writer_round_trips() {
    for source in ["a {--} b\n", "{--}start\n", "{---} and {-x-}\n"] {
        assert_eq!(html(&carve::to_carve(source)), html(source));
    }
}
