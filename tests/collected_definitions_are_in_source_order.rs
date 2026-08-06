//! PART 12 §7: "Definitions appear in DOCUMENT ORDER by source position."
//!
//! Collection moves a definition to the document and §4 keeps the `pos` it was
//! written at, so the published order follows that `pos`. This engine wrote
//! `doc.children` first and the footnote map afterwards, so a link definition
//! preceded a footnote whatever the author wrote and `pos` ran backwards
//! between two adjacent siblings. carve#746.
//!
//! The measurement that hides it is a single document whose footnote happens to
//! be written first, where kind order and source order agree.

/// The `type` of every DIRECT child of the serialized document, in order.
///
/// Hand-rolled rather than pulled from `parse()`: footnote definitions are a
/// map on the runtime document and only become siblings on the wire, so the
/// order under test exists nowhere else. Every node writes `type` first, so the
/// value sits immediately after the child's opening brace.
fn kinds(source: &str) -> Vec<String> {
    // Positions are opt-in in this engine (PART 12 §4 permits that), and the
    // order under test is BY position, so the probe has to ask for them - the
    // same thing `carve --json` does.
    let options = carve::Options::new().with_positions(true);
    let json = carve::to_json_with_options(source, &options);
    let bytes = json.as_bytes();
    let start = json.find("\"children\":[").expect("a children array") + "\"children\":[".len();

    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut i = start;
    while i < bytes.len() {
        let ch = bytes[i] as char;
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' | '[' => {
                if depth == 0 && ch == '{' {
                    let rest = &json[i + 1..];
                    let prefix = "\"type\":\"";
                    assert!(
                        rest.starts_with(prefix),
                        "a child that does not lead with type"
                    );
                    let name = &rest[prefix.len()..];
                    out.push(name[..name.find('"').expect("a closed type")].to_string());
                }
                depth += 1;
            }
            '}' | ']' => {
                if depth == 0 {
                    break; // the children array closed
                }
                depth -= 1;
            }
            _ => {}
        }
        i += 1;
    }
    out
}

#[test]
fn the_scanner_sees_every_direct_child() {
    assert_eq!(kinds("# h\n\ntext\n"), vec!["heading", "paragraph"]);
}

#[test]
fn a_footnote_written_first_precedes_a_link_definition() {
    assert_eq!(
        kinds("[^a]: note\n[r]: /u\n\nsee[^a] and [t][r]\n"),
        vec!["paragraph", "footnote", "link_reference_definition"]
    );
}

#[test]
fn a_link_definition_written_first_precedes_a_footnote() {
    assert_eq!(
        kinds("[r]: /u\n[^a]: note\n\nsee[^a] and [t][r]\n"),
        vec!["paragraph", "link_reference_definition", "footnote"]
    );
}

#[test]
fn three_definitions_of_two_kinds_follow_source_position() {
    assert_eq!(
        kinds("[r]: /u\n[^a]: note\n[s]: /v\n\nsee[^a] and [t][r] and [u][s]\n"),
        vec![
            "paragraph",
            "link_reference_definition",
            "footnote",
            "link_reference_definition"
        ]
    );
}

#[test]
fn an_abbreviation_definition_keeps_its_authored_position() {
    // An `abbreviation_def` is NOT collected out of the document - §7 refuses
    // that specifically, since hoisting it would empty the line rather than
    // relocate visible output - so it stays where it was written and is not
    // drawn into the collected tail.
    assert_eq!(
        kinds(
            "*[HTML]: HyperText Markup Language\n[r]: /u\n[^a]: note\n\nsee[^a] and [t][r] and HTML\n"
        ),
        vec![
            "abbreviation_def",
            "paragraph",
            "link_reference_definition",
            "footnote"
        ]
    );
}
