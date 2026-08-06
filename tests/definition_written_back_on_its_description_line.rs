//! A definition collected from a definition list's description is written back
//! on that description's line (carve#805, carve-rs#681).
//!
//! Collecting empties the `dd`, and an empty description has no source
//! spelling: a bare `:` line re-parses into the term above it. So the writer
//! produced a document that says something else, and PART 11 §1's
//! `to_html(fmt(x)) == to_html(x)` failed.

use carve::{to_carve, to_html};

fn html(source: &str) -> String {
    to_html(source)
}

#[test]
fn a_link_definition_is_written_back_on_its_own_line() {
    let source = ":: term\n:  [r]: /u\n\nsee [t][r]\n";

    assert_eq!(to_carve(source), ":: term\n:  [r]: /u\n\nsee [t][r]\n");
}

#[test]
fn a_footnote_definition_is_written_back_on_its_own_line() {
    let source = ":: term\n:  [^f]: x\n\nsee[^f]\n";

    assert_eq!(to_carve(source), ":: term\n:  [^f]: x\n\nsee[^f]\n");
}

#[test]
fn the_document_still_says_the_same_thing() {
    // PART 11 §1 stated directly. This is the invariant the bug broke: the
    // written-back document rendered differently from the one the author wrote.
    for source in [
        ":: term\n:  [r]: /u\n\nsee [t][r]\n",
        ":: term\n:  [^f]: x\n\nsee[^f]\n",
    ] {
        assert_eq!(html(&to_carve(source)), html(source), "source: {source:?}");
    }
}

#[test]
fn the_definition_is_not_written_twice() {
    // Emitting it in BOTH places still round-trips through HTML, so the
    // invariant above would pass while the source grew a duplicate.
    let written = to_carve(":: term\n:  [r]: /u\n\nsee [t][r]\n");

    assert_eq!(written.matches("[r]: /u").count(), 1, "{written:?}");
}

#[test]
fn an_ordinary_description_still_round_trips() {
    // The control: a pass above could otherwise mean descriptions stopped
    // being written at all.
    let source = ":: term\n:  plain text\n";

    assert_eq!(to_carve(source), source);
}
