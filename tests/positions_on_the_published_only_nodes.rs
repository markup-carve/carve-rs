//! Two nodes #517 started publishing went out with no honest span.
//!
//! PART 12 §4 requires a position on every node but the root, and allows it to
//! be omitted only where a node was REASSEMBLED and has none. Both of these
//! have one: the definition is a line the author wrote, and the emphasis
//! covers the content between `/*` and `*/`. carve-js publishes both.
//!
//! The `abbreviation_def` half then failed the second way this engine keeps
//! failing - a correct line and column with offsets of `0..0`, present and
//! selecting nothing, because `fill_offsets` had a `_ => None` arm. That is
//! the fourth family after figure captions, footnote definition bodies and
//! definition terms (carve-rs#333), so that match is exhaustive now.

use carve::ast::BlockNode;

fn positioned(src: &str) -> String {
    carve::to_json_with_options(
        src,
        &carve::Options {
            positions: true,
            ..Default::default()
        },
    )
}

/// The `pos` of the node whose `"type":"<name>"` comes first.
///
/// Brace-matched, and deliberately not "the next `pos` after the type name":
/// that finds the first CHILD's position, which happens to equal the parent's
/// for a single-run emphasis and does not for a multi-line one. Three of these
/// tests passed against that version while asserting the wrong node.
fn pos_of(json: &str, node_type: &str) -> String {
    try_pos_of(json, node_type).unwrap_or_else(|| panic!("{node_type} carries no pos of its own"))
}

/// `None` when the node has no `pos` at its own level.
fn try_pos_of(json: &str, node_type: &str) -> Option<String> {
    let needle = format!("\"type\":\"{node_type}\"");
    let at = json
        .find(&needle)
        .unwrap_or_else(|| panic!("no {node_type} in {json}"));
    // The object opens at the `{` immediately before its first field.
    let start = json[..at].rfind('{').expect("an opening brace");
    let bytes = json.as_bytes();
    let mut depth = 0usize;
    let mut end = start;
    let mut in_string = false;
    let mut escaped = false;
    for (i, b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escaped {
                escaped = false;
            } else if *b == b'\\' {
                escaped = true;
            } else if *b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                depth -= 1;
                if depth == 0 {
                    end = i;
                    break;
                }
            }
            _ => {}
        }
    }
    let object = &json[start..=end];
    // The `pos` at the node's OWN level, not one belonging to a child.
    //
    // Taking the last `"pos":` in the object is not the same thing: with no
    // pos of its own, a node falls through to its final child's, which equals
    // the parent's for a single-run emphasis. Two of these tests passed
    // against that version whether or not the fix was present - the multi-line
    // one was the only one that could tell.
    let bytes = object.as_bytes();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match b {
            b'{' | b'[' => depth += 1,
            b'}' | b']' => depth -= 1,
            b'"' => {
                if depth == 1 && object[i..].starts_with("\"pos\":") {
                    let tail = &object[i + 6..];
                    let close = tail.find('}').expect("a closed pos object");
                    return Some(tail[..=close].to_string());
                }
                in_string = true;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

#[test]
fn an_abbreviation_definition_carries_its_own_line() {
    let json = positioned("*[HTML]: Hyper Text\n\nThe HTML spec.\n");
    assert_eq!(
        pos_of(&json, "abbreviation_def"),
        // Matches carve-js for the same input, checked rather than assumed.
        "{\"startLine\":1,\"endLine\":1,\"startColumn\":1,\"endColumn\":20,\"startOffset\":0,\"endOffset\":19}"
    );
}

#[test]
fn the_definitions_span_selects_the_definition() {
    // The half that regressed silently: a line and column with `0..0` offsets
    // reads as present while selecting nothing.
    let source = "*[HTML]: Hyper Text\n\nThe HTML spec.\n";
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options(source, &options);
    let BlockNode::AbbreviationDef(def) = doc.children.first().expect("a block") else {
        panic!(
            "expected an abbreviation_def, got {:?}",
            doc.children.first()
        );
    };
    let pos = def.pos.as_ref().expect("a position");
    assert_eq!(
        &source[pos.start_offset..pos.end_offset],
        "*[HTML]: Hyper Text"
    );
}

#[test]
fn the_materialised_emphasis_spans_the_content_between_the_delimiters() {
    // `/*` and `*/` are two ASCII characters each, so the inner span is the
    // strong's with two trimmed off each end. carve-js publishes exactly this.
    let json = positioned("/*bold italic*/\n");
    assert_eq!(
        pos_of(&json, "emphasis"),
        "{\"startLine\":1,\"endLine\":1,\"startColumn\":3,\"endColumn\":14,\"startOffset\":2,\"endOffset\":13}"
    );
}

#[test]
fn the_materialised_emphasis_spans_a_multi_line_run() {
    // Lines are unchanged by the trim: the delimiters sit on the run's first
    // and last line, so only the columns and offsets move.
    let json = positioned("/*multi\nline*/\n");
    assert_eq!(
        pos_of(&json, "emphasis"),
        "{\"startLine\":1,\"endLine\":2,\"startColumn\":3,\"endColumn\":5,\"startOffset\":2,\"endOffset\":12}"
    );
}

#[test]
fn the_materialised_emphasis_is_inside_its_strong() {
    // PART 12 requires a span to contain its children's. Mid-paragraph, so the
    // strong does not start at column 1 and an off-by-two shows up.
    let json = positioned("x /*y*/ z\n");
    assert_eq!(
        pos_of(&json, "emphasis"),
        "{\"startLine\":1,\"endLine\":1,\"startColumn\":5,\"endColumn\":6,\"startOffset\":4,\"endOffset\":5}"
    );
}

#[test]
fn an_attributed_bold_italic_omits_the_inner_span_rather_than_inventing_one() {
    // This engine's span for an attributed inline covers the attribute block,
    // so `/*x*/{#id}` ends at the `}`. Trimming two off that lands inside the
    // attributes - the inner node would claim `x*/{#`. PART 12 §4 allows
    // omission where there is no honest span, and a wrong span is worse than
    // none. The outer span is the real defect (carve-rs#521), and it is not
    // specific to the combined form: `*x*{#i}` has it too.
    let json = positioned("/*x*/{#id}\n");
    assert_eq!(try_pos_of(&json, "emphasis"), None, "{json}");
    // The unattributed form still gets one, so this is a guard and not a
    // silent loss of the fix.
    assert!(try_pos_of(&positioned("/*x*/\n"), "emphasis").is_some());
}
