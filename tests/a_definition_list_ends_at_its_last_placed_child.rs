//! A definition list ends at its last placed child too.
//!
//! It was the one container that answered the floating-attribute question the
//! other way: a floating attribute is SCOPED to the container that holds it, so
//! the attribute line is one the definition list consumed - and consuming it was
//! read as owning it, which ran the extent past the last description.
//!
//! Scope and extent are different questions. Scope decides which blocks an
//! attribute may reach and answers "not past this container"; extent decides
//! which source a node claims and answers "not past my last child". The bullet
//! list one construct over already separates them, and the attribute here
//! attaches to nothing, leaves no attributes on the node either, and is the
//! unattached attribute block PART 12 section 4 excludes by name
//! (markup-carve/carve#1530).
//!
//! The children section 4 means are the wire nodes - `definition_term` and
//! `definition_description` - and not the items, which are a grouping this
//! engine keeps in memory and does not publish.

use serde_json::Value;

fn ast(source: &str) -> Value {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    serde_json::from_str(&carve::to_json(&carve::parse_with_options(
        source, &options,
    )))
    .expect("the serializer emits JSON")
}

fn spans(node: &Value, out: &mut Vec<(String, Value)>) {
    match node {
        Value::Array(items) => items.iter().for_each(|item| spans(item, out)),
        Value::Object(fields) => {
            if let (Some(Value::String(ty)), Some(pos)) = (fields.get("type"), fields.get("pos")) {
                out.push((ty.clone(), pos.clone()));
            }
            for (key, value) in fields {
                if key != "pos" {
                    spans(value, out);
                }
            }
        }
        _ => {}
    }
}

/// The `nth` node of `ty` in serialization order, as `(startOffset, endOffset)`.
fn nth(source: &str, ty: &str, nth: usize) -> (u64, u64) {
    let mut found = Vec::new();
    spans(&ast(source), &mut found);
    let pos = found
        .into_iter()
        .filter(|(node_ty, _)| node_ty == ty)
        .map(|(_, pos)| pos)
        .nth(nth)
        .unwrap_or_else(|| panic!("no {ty} #{nth} in {source:?}"));
    (
        pos["startOffset"].as_u64().unwrap(),
        pos["endOffset"].as_u64().unwrap(),
    )
}

#[test]
fn it_stops_before_an_attribute_line_no_child_covers() {
    let source = ":: t\n:  d\n   {.k}\ntail\n";

    // The list used to end at 17, which is where the attribute line ends.
    assert_eq!(nth(source, "definition_list", 0), (0, 9));
    assert_eq!(nth(source, "definition_description", 0), (5, 9));
}

#[test]
fn it_stops_before_the_wrapped_spelling_of_the_same_block() {
    // Section 15 A5 lets one attribute block wrap, so the list consumed two
    // lines here rather than one. Neither of them is a child.
    let source = ":: t\n:  d\n   {.k\n   #x}\ntail\n";

    assert_eq!(nth(source, "definition_list", 0), (0, 9));
}

#[test]
fn it_stops_before_the_definition_hoisted_out_of_it() {
    // PART 12 section 7 hoists the definition to the document, so it is the
    // list's SIBLING - and the two used to claim 10..20 at once.
    let source = ":: t\n:  a\n   [r]: /u\ntail\n\n[r][]\n";

    assert_eq!(nth(source, "definition_list", 0), (0, 9));
    let (def_start, _) = nth(source, "link_reference_definition", 0);
    assert!(def_start >= 9, "the definition starts after the list ends");
}

#[test]
fn it_stops_before_trailing_whitespace_the_clause_excludes() {
    // Corpus 268-trailing-whitespace-on-a-content-line-is-dropped-5, where the
    // list used to end at 16 and its description at 15.
    let source = ":: term \n:  def \n";

    assert_eq!(nth(source, "definition_list", 0), (0, 15));
}

#[test]
fn a_list_ending_on_its_last_description_is_unchanged() {
    // The control: deriving the end from the children must not SHORTEN a list
    // that already ended on one.
    let source = ":: t\n:  d\n";

    assert_eq!(nth(source, "definition_list", 0), (0, 9));
    assert_eq!(nth(source, "definition_description", 0), (5, 9));
}

#[test]
fn a_term_with_no_description_still_places_the_list() {
    // The other control. An item whose description never arrived has only a
    // term to end at, and the list must reach it rather than fall back to the
    // emptied-container branch.
    let source = ":: t\n";

    assert_eq!(nth(source, "definition_list", 0), (0, 4));
    assert_eq!(nth(source, "definition_term", 0), (0, 4));
}

#[test]
fn the_rendered_list_is_unchanged() {
    // The extent moved; nothing the reader sees did.
    let html = carve::to_html(":: t\n:  d\n   {.k}\ntail\n");

    assert_eq!(html, "<dl>\n  <dt>t</dt>\n  <dd>d</dd>\n</dl>\n<p>tail</p>");
}
