//! Positions index the ORIGINAL file, not the normalized copy (carve#876).
//!
//! Parsing normalizes its input first: a leading U+FEFF is stripped and CRLF /
//! lone CR collapse to LF. The offset table was built from the RESULT, so it
//! was short by one codepoint per removed character and every offset in the
//! document landed before the text it named - `<BOM># T` reported the space
//! where the node said `T`.
//!
//! All three engines did it the same way, and nothing could see it: the HTML is
//! identical either way, and no corpus document carries a BOM or a CR
//! (carve#872).

use carve::ast::BlockNode;
use carve::Options;

/// The text a node's span actually names in the ORIGINAL source.
fn sliced(src: &str, index: usize) -> String {
    let doc = carve::parse_with_options(src, &Options::default().with_positions(true));
    let BlockNode::Paragraph(p) = &doc.children[index] else {
        panic!("expected a paragraph at {index}");
    };
    let pos = p.pos.as_ref().expect("positions are on");

    src.chars()
        .skip(pos.start_offset)
        .take(pos.end_offset - pos.start_offset)
        .collect()
}

#[test]
fn a_leading_byte_order_mark_does_not_shift_offsets() {
    assert_eq!(sliced("\u{feff}# T\n\nabc\n", 1), "abc");
}

#[test]
fn crlf_line_endings_do_not_shift_offsets() {
    // The same defect through the other half of the normalization: every CR
    // removed before parsing shortened the text the offsets were measured
    // against.
    assert_eq!(sliced("# T\r\n\r\nabc\r\n", 1), "abc");
}

#[test]
fn a_lone_carriage_return_does_not_shift_offsets() {
    // `newline` admits a lone '\r' too, so it collapses like the pair.
    assert_eq!(sliced("# T\r\rabc\r", 1), "abc");
}

#[test]
fn a_document_needing_no_normalization_is_unshifted() {
    // The control: adding an offset unconditionally would move every document.
    assert_eq!(sliced("# T\n\nabc\n", 1), "abc");
}

#[test]
fn the_mark_is_still_stripped_for_parsing() {
    // The boundary the offset fix must not undo.
    assert_eq!(carve::to_html("\u{feff}# T\n"), carve::to_html("# T\n"));
}

#[test]
fn byte_length_counts_the_input_before_normalization() {
    for (source, normalized, bytes) in [
        ("alpha\r\n", "alpha\n", 7),
        ("🙂\r\n", "🙂\n", 6),
        ("\u{feff}a\n", "a\n", 5),
        ("a\0b\n", "a\u{fffd}b\n", 4),
        ("a\rb\r\n", "a\nb\n", 5),
        ("\u{feff}🙂\0\r\n", "🙂\u{fffd}\n", 10),
    ] {
        assert_eq!(carve::to_html(source), carve::to_html(normalized));
        for positions in [true, false] {
            let doc =
                carve::parse_with_options(source, &Options::default().with_positions(positions));
            assert_eq!(doc.source_len, bytes, "{source:?}");
            assert!(
                carve::authored_provenance(&doc, source, None).is_ok(),
                "{source:?}"
            );
            let wire: serde_json::Value = serde_json::from_str(&carve::to_json(&doc)).unwrap();
            assert_eq!(wire["srcByteLength"], bytes, "{source:?}");
        }
    }
}
