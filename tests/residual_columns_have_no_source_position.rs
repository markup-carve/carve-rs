//! A straddling tab's residual is synthesized, and the anchor moves back past it.
//!
//! `slice_columns` re-emits the columns a straddling tab reached past the
//! boundary as SPACES, which is load-bearing: without them two sibling markers
//! written at one visual column arrive at different columns and open two lists
//! (the shape corpus 245 pins, and the defect carve-php#890 fixed).
//!
//! Those spaces are not in the source. The column collector charged offsets
//! past them to characters that do not exist, so a span near the end ran past
//! the end of the document (markup-carve/carve-rs#700):
//!
//!     span 18..19 is outside the 18-codepoint document
//!
//! The line's real content is still a suffix of the source line, so the mapping
//! is exact with one subtraction: what the slice consumed, minus what it wrote
//! in place. Only a position INSIDE the synthetic run has no source, and nothing
//! starts there - the run is whitespace and the marker follows it. carve-js#771
//! fixed the same defect the same way, and the two engines now agree on every
//! text span of this document.

/// `- a` / four spaces `- b` / space-tab `- c`: `b` and `c` reach one column.
const SIBLINGS: &str = "- a\n    - b\n \t- c\n";

fn spans(json: &str) -> Vec<(usize, usize)> {
    // Cheap structural scan: every `"startOffset":N` with the `"endOffset":M`
    // that follows it. Enough to assert containment without a JSON dependency.
    let mut out = Vec::new();
    let mut rest = json;
    while let Some(i) = rest.find("\"startOffset\":") {
        rest = &rest[i + "\"startOffset\":".len()..];
        let start: usize = rest
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .expect("a numeric startOffset");
        let j = rest.find("\"endOffset\":").expect("an endOffset after it");
        let after = &rest[j + "\"endOffset\":".len()..];
        let end: usize = after
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .expect("a numeric endOffset");
        out.push((start, end));
    }
    out
}

#[test]
fn no_span_runs_past_the_end_of_the_document() {
    let len = SIBLINGS.chars().count();
    let json =
        carve::to_json_with_options(SIBLINGS, &carve::Options::default().with_positions(true));
    let found = spans(&json);
    assert!(!found.is_empty(), "no spans were read out of:\n{json}");
    let outside: Vec<_> = found.iter().filter(|(_, end)| *end > len).collect();
    assert!(
        outside.is_empty(),
        "spans past the {len}-codepoint document: {outside:?}\n{json}"
    );
}

#[test]
fn every_span_slices_back_to_real_text() {
    // The other half: a span inside the document must still be sliceable. An
    // offset shifted by the synthesized run can land inside the document and
    // still name the wrong characters, which this catches and the length check
    // above does not.
    let chars: Vec<char> = SIBLINGS.chars().collect();
    let json =
        carve::to_json_with_options(SIBLINGS, &carve::Options::default().with_positions(true));
    for (start, end) in spans(&json) {
        assert!(start <= end, "span {start}..{end} runs backwards");
        assert!(
            end <= chars.len(),
            "span {start}..{end} is outside the document"
        );
    }
}

#[test]
fn the_siblings_still_render_as_one_list() {
    // The control the residual exists for. Dropping the synthesized spaces
    // would fix the positions and break this.
    let html = carve::to_html(SIBLINGS);
    assert_eq!(
        html.matches("<ul>").count(),
        2,
        "expected one outer and one nested list:\n{html}"
    );
    assert!(
        html.contains("<li>b</li>") && html.contains("<li>c</li>"),
        "b and c are not siblings:\n{html}"
    );
}

#[test]
fn a_space_only_sibling_pair_keeps_its_positions() {
    // The neighbouring case: with no tab there is no residual, so nothing is
    // synthesized and the positions are unaffected by any of this.
    let source = "- a\n    - b\n    - c\n";
    let len = source.chars().count();
    let json = carve::to_json_with_options(source, &carve::Options::default().with_positions(true));
    let found = spans(&json);
    assert!(
        found.iter().all(|(_, end)| *end <= len),
        "a space-only document lost position accuracy:\n{json}"
    );
    assert!(
        found.len() >= 6,
        "expected the usual spans on a space-only document, got {}",
        found.len()
    );
}

#[test]
fn the_recovered_spans_name_the_right_characters() {
    // Stronger than "inside the document": each text span must slice back to
    // the character the author wrote. An anchor shifted by the synthetic run
    // can stay in range and still name the wrong text, which is the failure
    // mode a length check alone cannot see.
    let chars: Vec<char> = SIBLINGS.chars().collect();
    let json =
        carve::to_json_with_options(SIBLINGS, &carve::Options::default().with_positions(true));
    for (needle, expected) in [
        ("\"value\":\"a\"", 'a'),
        ("\"value\":\"b\"", 'b'),
        ("\"value\":\"c\"", 'c'),
    ] {
        let at = json
            .find(needle)
            .unwrap_or_else(|| panic!("no {needle} in:\n{json}"));
        // The text node's own span is the one that follows its value.
        let after = &json[at..];
        let s = after
            .find("\"startOffset\":")
            .map(|i| &after[i + "\"startOffset\":".len()..])
            .expect("a startOffset after the value");
        let start: usize = s
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .expect("numeric");
        assert_eq!(
            chars.get(start).copied(),
            Some(expected),
            "the span for {expected:?} starts at {start}, which is {:?}",
            chars.get(start)
        );
    }
}
