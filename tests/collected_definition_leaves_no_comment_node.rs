//! A definition collected out of a container leaves no comment NODE behind
//! (PART 12, carve-rs#602, markup-carve/carve#620).
//!
//! Removing a definition from a container cannot leave a blank line: inside a
//! quoted list item the leftover structural prefix IS blank, and a blank there
//! loosens the list (§17 L1) even though the definition rendered nothing. So the
//! line is replaced with `%%`, the one construct invisible at any column that
//! closes nothing (§24 C3). Sound device.
//!
//! It reached the AST as an authored comment, which cost two things:
//!
//!   - `fmt` wrote a `%%` nobody typed (corpus 194, and only in `quote > item` -
//!     the one nesting where the leftover prefix is blank).
//!   - an item holding one is not EMPTY, so the writer's emptied-item branch
//!     never fired and it wrote `- %%` where carve-js and carve-php write `- +`.
//!     That divergence was open as a spec question about which spelling PART 11
//!     should pin; it was this bug seen from the other end.
//!
//! The placeholder now carries a private-use suffix that the block parser's `%%`
//! arm recognizes and drops. Everything the placeholder existed for happens
//! before that point, so the tightness it protects is unchanged.

use carve::{parse, to_carve, to_html};

fn ast(src: &str) -> String {
    format!("{:?}", parse(src))
}

#[test]
fn an_emptied_item_is_written_with_the_continuation_marker() {
    // Corpus 16. carve-js and carve-php both write `- +`; carve-rs wrote `- %%`
    // only because the placeholder made the item look non-empty.
    assert_eq!(
        to_carve("- [ref]: /url\n\nSee [it][ref].\n"),
        // The reference is no longer inlined and the definition line is written
        // back, hoisted to the document (PART 12 §10, carve-rs#631). Byte-identical
        // to carve-js. The `- +` continuation marker - what this test is about -
        // is unchanged.
        "- +\n\nSee [it][ref].\n\n[ref]: /url\n"
    );
}

#[test]
fn an_item_that_keeps_its_content_gets_no_filler() {
    // Corpus 194. The item still holds `a`, so nothing needs to stand in for the
    // hoisted definition - there was never a hole to fill.
    assert_eq!(
        to_carve("> - a\n\n[r]: /u\n\nsee [t][r]\n"),
        // Same change; the point here is that the item keeps `a` and gains no
        // filler, which still holds.
        "> - a\n\nsee [t][r]\n\n[r]: /u\n"
    );
}

#[test]
fn no_comment_node_reaches_the_ast() {
    // The wire shape, which is the actual defect - the writer was faithfully
    // serializing a node that should not have existed.
    assert!(
        !ast("- [ref]: /url\n\nSee [it][ref].\n").contains("Comment"),
        "a placeholder comment reached the AST"
    );
    assert!(
        !ast("> - a\n>   [r]: /u\n\nsee [t][r]\n").contains("Comment"),
        "a placeholder comment reached the AST"
    );
}

#[test]
fn an_authored_comment_is_still_a_comment() {
    // The control that decides the fix has to live in the parser: an authored
    // bare `%%` and the placeholder are otherwise the same node, so a writer-side
    // "skip empty comments" would fix the cases above by breaking these. All
    // three engines write each of them back unchanged.
    assert_eq!(to_carve("- a\n+\n%%\n"), "- a\n+\n%%\n");
    assert_eq!(to_carve("- a\n+\n%% note\n"), "- a\n+\n%% note\n");
    assert_eq!(to_carve("- %%\n"), "- %%\n");
    assert!(ast("- a\n+\n%%\n").contains("Comment"));
}

#[test]
fn the_list_does_not_loosen() {
    // What the placeholder is FOR. The definition renders nothing, so it is not
    // the item's second block and the item stays tight (§17 L1, L2). Dropping
    // the line outright instead of neutralizing it would leave a blank here.
    assert_eq!(
        to_html("> - a\n\nsee [t][r]\n\n[r]: /u\n"),
        "<blockquote>\n  <ul>\n    <li>a</li>\n  </ul>\n</blockquote>\n<p>see <a href=\"/u\">t</a></p>"
    );
    // The same shape one container out, which was already right and must stay.
    assert_eq!(
        to_html("- a\n\nsee [t][r]\n\n[r]: /u\n"),
        "<ul>\n  <li>a</li>\n</ul>\n<p>see <a href=\"/u\">t</a></p>"
    );
}

#[test]
fn the_definition_still_registers() {
    // The other half: neutralizing the line must not un-collect the definition.
    for src in [
        "- [ref]: /url\n\nSee [it][ref].\n",
        "> - a\n\n[r]: /u\n\nsee [t][r]\n",
        "- a\n\n[^f]: note\n\nsee[^f]\n",
    ] {
        let out = to_html(src);
        assert!(
            !out.contains("]: "),
            "the definition leaked as text for {src:?}: {out}"
        );
    }
}

#[test]
fn the_sentinel_never_reaches_output() {
    // A private-use character that escaped into a document would be invisible in
    // review and corrupt the file. It is consumed by the block parser and has no
    // other reader.
    for src in [
        "- [ref]: /url\n\nSee [it][ref].\n",
        "> - a\n>   [r]: /u\n\nsee [t][r]\n",
    ] {
        assert!(
            !to_carve(src).contains('\u{E005}'),
            "sentinel in fmt output"
        );
        assert!(
            !to_html(src).contains('\u{E005}'),
            "sentinel in html output"
        );
        assert!(!ast(src).contains('\u{E005}'), "sentinel in the AST");
    }
}

#[test]
fn an_authored_line_matching_the_sentinel_is_dropped() {
    // The collision, pinned rather than hidden. A `%%` line whose entire content
    // is the sentinel character is indistinguishable from the placeholder, so it
    // is dropped. Reviewers find this; it is a deliberate trade, not an
    // oversight.
    //
    // What makes it acceptable: the construct is a COMMENT, so the document
    // renders identically either way and only fmt output loses a line nobody can
    // see. Compare the two sentinels already in the writer - an authored U+E003
    // inside a CODE BLOCK is restored as an empty line, which changes the
    // rendered document (markup-carve/carve#678). This one cannot do that,
    // because a comment has no rendering to change.
    let src = "%%\u{E005}\n\nafter\n";
    assert_eq!(to_html(src), to_html("after\n"));
    assert!(!to_carve(src).contains('\u{E005}'));

    // The sentinel is only a placeholder as a WHOLE comment line. Inside
    // verbatim content, where authored bytes must survive, it is untouched.
    let code = "```\na\n\u{E005}\nb\n```\n";
    assert_eq!(to_html(&to_carve(code)), to_html(code));
    assert!(to_html(code).contains('\u{E005}'));
}

#[test]
fn fmt_still_round_trips() {
    for src in [
        "- [ref]: /url\n\nSee [it][ref].\n",
        "> - a\n>   [r]: /u\n\nsee [t][r]\n",
        "- a\n  %%\n",
    ] {
        let once = to_carve(src);
        assert_eq!(to_html(&once), to_html(src), "fmt changed the document");
        assert_eq!(to_carve(&once), once, "fmt is not idempotent");
    }
}
