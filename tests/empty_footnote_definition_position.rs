//! A footnote definition with an EMPTY body carries a position.
//!
//! A definition's extent was derived from its body alone, and `[^f]: {empty}`
//! parses to no blocks - so there was nothing to derive from and the node
//! reached the wire with no `pos`. PART 12 §4 permits omitting a position only
//! for a node that CANNOT be placed; this one is written on a line of its own,
//! so the definition line is its extent, and it is what the reference
//! publishes. markup-carve/carve#1023.
//!
//! The measurement that hides it is any document whose definitions all have
//! content: each of those derives a span from its first block and looks placed,
//! so a probe over them reports a conformant engine.

/// The published tree, which is where the defect was visible: a definition is a
/// map on the runtime document and only becomes a node on the wire.
///
/// Positions are OPT-IN (§4 permits that), so every probe here asks for them.
/// Without the flag no node carries a span, and a check that only compared the
/// spans it found would pass against the engine this file covers.
fn published(source: &str) -> String {
    carve::to_json_with_options(
        source,
        &carve::Options {
            positions: true,
            ..Default::default()
        },
    )
}

/// Every `footnote` object in the published tree, in the order it was written.
///
/// The needle carries its own closing quote, so `footnote_ref` - which shares
/// the prefix and is far more common - is not collected by accident.
fn footnote_objects(json: &str) -> Vec<String> {
    let bytes = json.as_bytes();
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(hit) = json[from..].find("\"type\":\"footnote\"") {
        let at = from + hit;
        // The object opens at the `{` immediately before its first field.
        let start = json[..at].rfind('{').expect("an opening brace");
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        let mut end = start;
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
        out.push(json[start..=end].to_string());
        from = end + 1;
    }

    out
}

/// A definition's OWN `pos`, asserted PRESENT before anything is compared.
///
/// An absent `pos` is the defect itself, so a helper that returned a default
/// here would let every assertion below pass against the unfixed engine.
///
/// The LAST `"pos":` in the object is the node's own: the writer emits
/// `children` before it, so any child's position comes earlier in the text.
fn pos_of(object: &str) -> &str {
    let at = object
        .rfind("\"pos\":")
        .unwrap_or_else(|| panic!("the footnote definition carries no position: {object}"));

    &object[at + "\"pos\":".len()..object.len() - 1]
}

fn field(pos: &str, name: &str) -> usize {
    let needle = format!("\"{name}\":");
    let at = pos
        .find(&needle)
        .unwrap_or_else(|| panic!("no {name} in {pos}"));
    let rest = &pos[at + needle.len()..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());

    rest[..end].parse().expect("a number")
}

/// The source a span points at, sliced in CODEPOINTS - the unit §4 states and
/// the unit the offsets are published in.
fn slice_of(source: &str, pos: &str) -> String {
    let codepoints: Vec<char> = source.chars().collect();

    codepoints[field(pos, "startOffset")..field(pos, "endOffset")]
        .iter()
        .collect()
}

/// The spec corpus document, verbatim:
/// `283-an-empty-footnote-body-is-written-with-the-empty-sentinel`.
#[test]
fn an_empty_definition_spans_its_own_line() {
    let source = "See[^f]\n\n[^f]: {empty}\n";

    let defs = footnote_objects(&published(source));
    assert_eq!(defs.len(), 1);

    let pos = pos_of(&defs[0]);
    assert_eq!(slice_of(source, pos), "[^f]: {empty}");
    assert_eq!(field(pos, "startLine"), 3);
    assert_eq!(field(pos, "endLine"), 3);
    assert_eq!(field(pos, "startColumn"), 1);
}

/// A definition WITH content keeps the extent its body gives it.
///
/// The fallback is reached only where the body places nothing, and this pins
/// that: widening it to every definition would move the extent of every footnote
/// in the corpus, which is a separate question - the engines already disagree
/// about it and that disagreement is declared.
#[test]
fn a_definition_with_a_body_keeps_its_body_derived_extent() {
    let source = "See[^f]\n\n[^f]: body\n";

    let defs = footnote_objects(&published(source));
    assert_eq!(defs.len(), 1);
    assert_eq!(slice_of(source, pos_of(&defs[0])), "body");
}

/// §4 puts a span's start at the markup that OPENS the construct, not at the
/// container prefix that carried the line.
#[test]
fn an_empty_definition_inside_a_container_starts_at_its_own_column() {
    let source = "> See[^f]\n>\n> [^f]: {empty}\n";

    let defs = footnote_objects(&published(source));
    assert_eq!(defs.len(), 1);

    let pos = pos_of(&defs[0]);
    assert_eq!(slice_of(source, pos), "[^f]: {empty}");
    assert_eq!(
        field(pos, "startColumn"),
        3,
        "the span must skip the quote marker"
    );
}

/// The same at an item's content column, where the prefix is indentation rather
/// than a marker - a start derived from the marker alone would miss it.
#[test]
fn an_empty_definition_at_an_item_content_column_starts_at_that_column() {
    let source = "- See[^f]\n\n  [^f]: {empty}\n";

    let defs = footnote_objects(&published(source));
    assert_eq!(defs.len(), 1);

    let pos = pos_of(&defs[0]);
    assert_eq!(slice_of(source, pos), "[^f]: {empty}");
    assert_eq!(field(pos, "startColumn"), 3);
}

/// The last line of the document, with no trailing newline. A line length taken
/// from a terminator that is not there would clip the slice.
#[test]
fn an_empty_definition_on_an_unterminated_last_line_is_placed() {
    let source = "See[^f]\n\n[^f]: {empty}";

    let defs = footnote_objects(&published(source));
    assert_eq!(defs.len(), 1);
    assert_eq!(slice_of(source, pos_of(&defs[0])), "[^f]: {empty}");
}

/// PART 12 §7: "Definitions appear in DOCUMENT ORDER by source position."
///
/// The same gap in a second place. The order is taken from the published
/// position, and an empty definition had none - so it sorted last and came out
/// BELOW a definition the author wrote after it.
#[test]
fn an_empty_definition_written_first_is_published_first() {
    let source = "See[^a][^b]\n\n[^a]: {empty}\n\n[^b]: x\n";

    let defs = footnote_objects(&published(source));
    assert_eq!(defs.len(), 2);
    assert!(defs[0].contains("\"label\":\"a\""), "got {}", defs[0]);
    assert!(defs[1].contains("\"label\":\"b\""), "got {}", defs[1]);

    let first = field(pos_of(&defs[0]), "startOffset");
    let second = field(pos_of(&defs[1]), "startOffset");
    assert!(
        first < second,
        "published order must follow source position, got {first} then {second}"
    );
}

/// The AST-INGEST path builds a footnote separately from the parser, so a
/// parser-only fix leaves it publishing nothing. §6 makes
/// serialize(ingest(serialize(parse(x)))) equal to serialize(parse(x)).
/// THE PRESENCE ASSERTION IS THE POINT, not the equality. Comparing the two
/// serializations alone passes against the unfixed engine for the wrong reason:
/// the parse published no position and the ingest re-derived none, so both sides
/// are the same tree with the same gap and the check cannot fail. It is the
/// dead-check shape this project keeps re-finding, and it was live in this file
/// until the before-and-after run showed this test green without the fix.
#[test]
fn an_ingested_empty_definition_keeps_its_position() {
    let source = "See[^f]\n\n[^f]: {empty}\n";
    let wire = published(source);

    let ingested = carve::from_json(&wire).expect("the tree decodes");
    let again = carve::to_json(&ingested);

    let defs = footnote_objects(&again);
    assert_eq!(defs.len(), 1);
    assert_eq!(slice_of(source, pos_of(&defs[0])), "[^f]: {empty}");

    assert_eq!(
        again, wire,
        "a round trip through the codec must not drop the definition's position"
    );
}
