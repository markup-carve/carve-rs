//! A definition registers behind a prefix that alternates quote and list
//! markers (markup-carve/carve-rs#1082).
//!
//! The definition pre-passes strip container prefixes off a line before asking
//! whether it is a definition. They asked the column question - "is there a live
//! item content column here, with a `>` at it?" - once, and about the RAW line.
//! On a line that starts with a `>` the indent before it is zero, so no column
//! ever matched and the inner quote was never stripped: `> - a` /
//! `>   > [r]: /u` left the definition as paragraph text inside the quote, where
//! the block parser publishes it and every other engine registers it.
//!
//! THE COLUMN IS THE WHOLE RULE, and it is not being relaxed here. An indented
//! `>` opens a quote only AT a live item's content column, which is what the
//! block parser already does. `> a` / `>   > b` has no item, so nothing matches,
//! the `>` stays ordinary text, and the definition twin of that shape still does
//! not register - pinned below, because a fix that simply stripped any indented
//! `>` would pass every positive case here and break that one.
//!
//! Every expectation in this file was taken from the executable spec oracle at
//! spec `7666027`, not from another engine.

use carve::to_html;

fn registers(src: &str) -> bool {
    to_html(src).contains("href=\"/u\"")
}

fn note_registers(src: &str) -> bool {
    to_html(src).contains("doc-noteref")
}

#[test]
fn a_quote_inside_an_item_inside_a_quote_registers() {
    // The reported shape. The item's content column is measured INSIDE the
    // quote, so it is 2 and not 4 - which is why the question has to be asked
    // after the quote markers rather than about the raw line.
    let src = "> - a\n>   > [r]: /u\n\nSee [r][].\n";
    assert!(registers(src), "{}", to_html(src));
    assert!(
        !to_html(src).contains("[r]: /u"),
        "the definition stayed visible: {}",
        to_html(src)
    );
}

#[test]
fn the_same_shape_with_the_quote_on_the_marker_line() {
    let src = "> - > x\n>   > [r]: /u\n\nSee [r][].\n";
    assert!(registers(src), "{}", to_html(src));
    assert!(!to_html(src).contains("[r]: /u"), "{}", to_html(src));
}

#[test]
fn the_footnote_kind_moves_with_it() {
    // Both pre-passes share the prefix strip, so both kinds move together. A
    // fix that touched only one would sort definitions by kind, which is the
    // tell this family keeps producing.
    let src = "> - a\n>   > [^f]: note\n\nSee [^f].\n";
    assert!(note_registers(src), "{}", to_html(src));
}

#[test]
fn the_question_is_asked_again_at_each_depth() {
    // The prefixes alternate, so one hop is not enough: `- > - > x` puts an item
    // inside a quote inside an item inside a quote, and the continuation line
    // carries an indent-then-quote twice.
    let src = "- > - > x\n  >   > [r]: /u\n\nSee [r][].\n";
    assert!(registers(src), "{}", to_html(src));
}

#[test]
fn an_indented_quote_with_no_item_holding_it_is_still_text() {
    // THE CONTROL, and the one a looser fix breaks. There is no list here, so
    // no content column matches the indent and the `>` is ordinary text - the
    // block parser renders it as `&gt; ...` inside the quote, and nothing may
    // register from it.
    let src = "> a\n>   > [r]: /u\n\nSee [r][].\n";
    assert!(
        !registers(src),
        "registered from lazy text: {}",
        to_html(src)
    );
    assert!(
        to_html(src).contains("&gt; [r]: /u"),
        "the line stopped being text: {}",
        to_html(src)
    );

    let note = "> a\n>   > [^f]: note\n\nSee [^f].\n";
    assert!(!note_registers(note), "{}", to_html(note));
}

#[test]
fn the_unquoted_twin_did_not_move() {
    // carve-rs#588 fixed this one, and it is the shape the column question was
    // added for. It must answer exactly as before.
    let src = "- a\n  > [r]: /u\n\nSee [r][].\n";
    assert!(registers(src), "{}", to_html(src));
}

#[test]
fn a_definition_at_the_top_level_of_a_quote_did_not_move() {
    // The plain spelling, which never needed a column at all.
    assert!(registers("> [r]: /u\n\nSee [r][].\n"));
    assert!(registers("> > [r]: /u\n\nSee [r][].\n"));
    assert!(note_registers("> [^f]: note\n\nSee [^f].\n"));
}

#[test]
fn a_comment_fence_still_defers_the_definition_at_these_prefixes() {
    // The other half of this seam, from markup-carve/carve#1341: reaching the
    // definition is not the same as registering it. A comment fence at the same
    // prefix must still hide it, and widening the strip must not walk past that.
    for src in [
        "> - a\n>   > %%%\n>   > [r]: /u\n>   > %%%\n\nSee [r][].\n",
        "- > - > x\n  >   > %%%\n  >   > [r]: /u\n  >   > %%%\n\nSee [r][].\n",
    ] {
        assert!(
            !registers(src),
            "a commented-out definition registered: {}",
            to_html(src)
        );
    }
}
