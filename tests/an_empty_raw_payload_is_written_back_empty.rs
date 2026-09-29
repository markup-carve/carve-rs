//! A raw block with no payload line is written back with none.
//!
//! `markup-carve/carve#2574` made a zero-line raw payload and a one-blank one
//! different blocks, and carve-rs#2159 gave the parser and the renderer that
//! distinction. The writer kept one spelling for both: it wrote the separator
//! that ends the payload's last line even when there was no line to end, so the
//! zero-line payload came back as the one-blank one and the round trip changed
//! the document (carve-rs#2166).

use carve::{to_carve, to_html};

/// Every host, because the writer is one function and the hosts differ only in
/// the prefix each line carries.
#[test]
fn a_zero_line_raw_payload_round_trips() {
    for source in [
        "```=html\n```\n",
        "- a\n\n  ```=html\n  ```\n\n- s\n",
        "> ```=html\n> ```\n",
        "::: note\n```=html\n```\n:::\n",
    ] {
        assert_eq!(to_carve(source), source, "{source:?}");
        assert_eq!(to_html(&to_carve(source)), to_html(source), "{source:?}");
    }
}

/// The one-blank payload is the spelling the writer used to reach for, and it
/// still round trips - the fix has to keep both, not swap them.
#[test]
fn a_one_blank_raw_payload_round_trips() {
    for source in [
        "```=html\n\n```\n",
        "- a\n\n  ```=html\n\n  ```\n\n- s\n",
        "> ```=html\n>\n> ```\n",
    ] {
        assert_eq!(to_carve(source), source, "{source:?}");
        assert_eq!(to_html(&to_carve(source)), to_html(source), "{source:?}");
    }
}

/// The two do not collapse into each other on the way out either, which is the
/// property the round trip above cannot state on its own: it would hold just as
/// well if the writer spelled both the same way and the parser read that one
/// spelling back.
#[test]
fn the_two_payloads_are_written_differently() {
    let zero = to_carve("```=html\n```\n");
    let one_blank = to_carve("```=html\n\n```\n");
    assert_ne!(zero, one_blank);
    assert_ne!(to_html(&zero), to_html(&one_blank));
}

/// A payload with content is unaffected, including one whose last line is blank.
#[test]
fn a_raw_payload_with_content_round_trips() {
    for source in [
        "```=html\n<b>x</b>\n```\n",
        "```=html\n<b>x</b>\n\n```\n",
        "```=html\n\n<b>x</b>\n```\n",
    ] {
        assert_eq!(to_carve(source), source, "{source:?}");
    }
}

/// Corpus 521's base document, which is what fails the formatter sweep once the
/// spec pin carries it. Written out here so the reading does not depend on a pin.
#[test]
fn the_corpus_shape_keeps_its_html() {
    let source = "- a\n\n  ```=html\n  ```\n\n- s\n";
    assert_eq!(to_carve(source), source);
    assert_eq!(
        to_html(source).trim(),
        "<ul>\n  <li><p>a</p>\n    \n  </li>\n  <li><p>s</p></li>\n</ul>"
    );
    assert_eq!(to_html(&to_carve(source)), to_html(source));
}
