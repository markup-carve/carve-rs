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
fn the_braced_pair_is_the_same_node_the_bare_run_produces() {
    // Not a glyph in a text run: `fmt` preserves `--` and `...` because they
    // are `smart_punctuation` carrying the authored spelling, and the braced
    // form is a second spelling of the same kind rather than a second
    // construct. Written as text it formatted to a literal en dash and the
    // author's `{--}` was gone.
    let doc = carve::parse_with_options("a {--} b\n", &carve::Options::new().with_positions(true));
    let json = carve::ast_json::to_json(&doc);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let node = &value["children"][0]["children"][1];
    assert_eq!(node["type"], "smart_punctuation");
    assert_eq!(node["kind"], "en_dash");
    assert_eq!(node["value"], "{--}");
    assert_eq!(node["pos"]["startOffset"], 2);
    assert_eq!(node["pos"]["endOffset"], 6);
}

#[test]
fn the_writer_round_trips() {
    assert_eq!(carve::to_carve("a {--} b\n"), "a {--} b\n");
    for source in ["a {--} b\n", "{--}start\n", "{---} and {-x-}\n"] {
        assert_eq!(html(&carve::to_carve(source)), html(source));
    }
}

/// THE WRITER MUST NOT ESCAPE WHAT OPENS NOTHING (PART 11 §2, corpus 388).
///
/// The pinned canonical form on spec `main` is the bare spelling. It is not
/// readable from `tests/spec` at this pin - `388-*.fmt` arrived after it - so
/// the bytes are stated here, which is where they bite on a regression whatever
/// the submodule is pointed at.
#[test]
fn the_writer_leaves_an_empty_pairs_carets_bare() {
    let source = "Empty pairs are text: {//} {**} {__} {~~} {^^} {,,} {==} {++} {##}.\n\n\
                  A pair that holds something is the construct: {/i/} {*b*} {~s~} {+ins+} {# c #}.\n";
    assert_eq!(
        carve::to_carve(source),
        "Empty pairs are text: {//} {**} {__} {~~} {^^} {,,} {==} {++} {##}.\n\n\
         A pair that holds something is the construct: /i/ *b* ~s~ {+ins+} {# c #}.\n"
    );
}

/// The decisive property, asserted rather than described: the two spellings
/// differ in nothing but escape bytes, which §1's EQUALITY IS MODULO ESCAPING
/// makes the same document. So PART 11 §4 asks for the bare one - §2 pins the
/// form, and where §2 and §5 differ, §2 wins.
///
/// Asserted on the RENDER, deliberately. The trees are not `==`: this engine
/// records an escape as an `escaped_text` node, so `{\^\^}` reaches four
/// children where `{^^}` reaches one. That difference IS the escaping, which is
/// exactly what the clause says to compare modulo - and the render is where
/// "the same document" is observable from outside the engine.
#[test]
fn the_bare_and_the_escaped_spelling_are_the_same_document() {
    assert_eq!(html("{^^}\n"), html("{\\^\\^}\n"));
    assert_eq!(html("{^^}\n"), "<p>{^^}</p>");
}

/// THE NEAR MISS, and the reason the test above cannot pass by the writer
/// having simply stopped escaping carets.
///
/// `{^x^}` holds something, so it IS a forced superscript - the construct
/// round-trips as itself. And a caret a re-parse WOULD read is still escaped:
/// an authored literal `{^x^}`, which reaches the writer as text, keeps every
/// escape it needs to stay text.
#[test]
fn a_pair_that_holds_something_keeps_its_escapes() {
    assert_eq!(carve::to_carve("a {^x^} b\n"), "a {^x^} b\n");
    assert_eq!(html(&carve::to_carve("a {^x^} b\n")), html("a {^x^} b\n"));

    let authored_literal = "lit \\{\\^x\\^} b\n";
    assert_eq!(carve::to_carve(authored_literal), authored_literal);
    assert_eq!(
        carve::parse(&carve::to_carve(authored_literal)).children,
        carve::parse(authored_literal).children
    );
}

/// OUT OF SCOPE, PINNED SO A LATER SWEEP CANNOT MOVE IT BY ACCIDENT. §2a's
/// `}^p` and `[^` over-escapes are open in all three engines and corpus 388
/// deliberately does not pin them, so this states what the engine does today
/// rather than what §2 would eventually ask for.
#[test]
fn an_authored_mid_prose_caret_escape_is_preserved() {
    for source in ["a \\^ b\n", "x \\^[a\n"] {
        assert_eq!(
            carve::parse(&carve::to_carve(source)).children,
            carve::parse(source).children,
            "parse(fmt(x)) != parse(x) for {source:?}"
        );
    }
}
