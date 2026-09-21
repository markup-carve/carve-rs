//! PART 9 §19: a node an include pulled in keeps the coordinates of its own
//! file and carries that file's canonical id in `pos.file`. A `@lines` slice
//! is cut from that file, so its nodes report where they sit in the file and
//! not where they sit in the slice (markup-carve/carve-rs#1779).

use std::collections::HashMap;

use carve::{
    expand_includes, parse_with_options, to_json, Document, IncludeDenial, IncludeOptions,
    IncludeResolved, IncludeResolver, Options,
};

/// Eight lines of padding, 20 codepoints.
const PAD: &str = "pad\n\npad\n\npad\n\npad\n\n";

struct Files(HashMap<String, String>);

impl IncludeResolver for Files {
    fn resolve(
        &self,
        path: &str,
        _ctx: &carve::IncludeContext<'_>,
    ) -> Result<IncludeResolved, IncludeDenial> {
        self.0
            .get(path)
            .map(|source| IncludeResolved::with_id(source.clone(), path))
            .ok_or(IncludeDenial::NotFound)
    }
}

fn expand(source: &str, files: &[(&str, &str)]) -> Document {
    let resolver = Files(
        files
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect(),
    );
    let doc = parse_with_options(source, &Options::default().with_positions(true));
    expand_includes(doc, source, &IncludeOptions::new().with_resolver(&resolver)).doc
}

/// Every `pos` object in the encoded tree, in document order.
fn positions(source: &str, files: &[(&str, &str)]) -> Vec<String> {
    let json = to_json(&expand(source, files));
    let mut out = Vec::new();
    let mut rest = json.as_str();
    while let Some(at) = rest.find("\"pos\":{") {
        rest = &rest[at + "\"pos\":".len()..];
        let end = rest.find('}').expect("a pos object closes");
        out.push(rest[..=end].to_string());
        rest = &rest[end + 1..];
    }
    out
}

#[test]
fn a_heading_on_line_9_reports_line_9_not_line_1() {
    let files = [("child.crv", &*format!("{PAD}## Deep heading\n\nBody.\n"))];
    assert_eq!(
        positions("{{ child.crv @lines:9-11 }}\n", &files),
        [
            // Children are encoded ahead of the node that holds them, so the
            // heading's text comes first.
            "{\"startLine\":9,\"endLine\":9,\"startColumn\":4,\"endColumn\":16,\"startOffset\":23,\"endOffset\":35,\"file\":\"child.crv\"}",
            "{\"startLine\":9,\"endLine\":9,\"startColumn\":1,\"endColumn\":16,\"startOffset\":20,\"endOffset\":35,\"file\":\"child.crv\"}",
            "{\"startLine\":11,\"endLine\":11,\"startColumn\":1,\"endColumn\":6,\"startOffset\":37,\"endOffset\":42,\"file\":\"child.crv\"}",
            "{\"startLine\":11,\"endLine\":11,\"startColumn\":1,\"endColumn\":6,\"startOffset\":37,\"endOffset\":42,\"file\":\"child.crv\"}",
        ]
    );
}

#[test]
fn the_sliced_reading_equals_the_unsliced_control_node_for_node() {
    let child = format!(
        "{PAD}## Deep heading\n\n| a | b |\n|---|---|\n| one | two |\n\nTail /em/.\n\n[^fn]: note\n"
    );
    let files = [("child.crv", &*child)];
    let control = positions("{{ child.crv }}\n", &files);
    let sliced = positions("{{ child.crv @lines:9-17 }}\n", &files);
    // Four padding paragraphs, one paragraph and one text node each.
    assert_eq!(sliced, control[8..], "control: {control:#?}");
}

#[test]
fn a_sliced_grandchild_carries_its_own_base_and_not_the_one_above_it() {
    // grand.crv's own base, so line 9 and offset 20. child.crv's base is eight
    // lines and 20 codepoints as well; adding it on top would report line 17.
    let child = format!("{PAD}{{{{ grand.crv @lines:9-9 }}}}\n");
    let grand = format!("{PAD}Grandchild body.\n");
    assert_eq!(
        positions(
            "{{ child.crv @lines:9-9 }}\n",
            &[("child.crv", &*child), ("grand.crv", &*grand)],
        ),
        [
            "{\"startLine\":9,\"endLine\":9,\"startColumn\":1,\"endColumn\":17,\"startOffset\":20,\"endOffset\":36,\"file\":\"grand.crv\"}",
            "{\"startLine\":9,\"endLine\":9,\"startColumn\":1,\"endColumn\":17,\"startOffset\":20,\"endOffset\":36,\"file\":\"grand.crv\"}",
        ]
    );
}

#[test]
fn a_crlf_child_is_rebased_over_the_endings_it_really_has() {
    // Offsets index the raw source, so the three CRLF lines ahead of line 4
    // occupy 9 codepoints, not 6.
    let files = [("child.crv", "a\r\nb\r\nc\r\nHeading here\r\n")];
    let control = positions("{{ child.crv }}\n", &files);
    assert_eq!(
        positions("{{ child.crv @lines:4-4 }}\n", &files),
        [
            "{\"startLine\":4,\"endLine\":4,\"startColumn\":1,\"endColumn\":13,\"startOffset\":9,\"endOffset\":21,\"file\":\"child.crv\"}",
            "{\"startLine\":4,\"endLine\":4,\"startColumn\":1,\"endColumn\":13,\"startOffset\":9,\"endOffset\":21,\"file\":\"child.crv\"}",
        ],
        "control: {control:#?}"
    );
}

#[test]
fn a_multi_line_crlf_slice_matches_its_control() {
    // The slice keeps each line's `\r`, so one base covers the whole range
    // rather than drifting by an ending per line.
    let files = [("child.crv", "a\r\nb\r\n\r\nOne.\r\n\r\nTwo.\r\n")];
    let control = positions("{{ child.crv }}\n", &files);
    assert_eq!(
        positions("{{ child.crv @lines:4-6 }}\n", &files),
        control[4..]
    );
}

#[test]
fn a_slice_starting_at_line_1_moves_nothing() {
    // CONTROL: no base, so the rebase is a no-op and the unsliced reading stands.
    let files = [("child.crv", "One\n\nTwo\n")];
    assert_eq!(
        positions("{{ child.crv @lines:1-1 }}\n", &files),
        [
            "{\"startLine\":1,\"endLine\":1,\"startColumn\":1,\"endColumn\":4,\"startOffset\":0,\"endOffset\":3,\"file\":\"child.crv\"}",
            "{\"startLine\":1,\"endLine\":1,\"startColumn\":1,\"endColumn\":4,\"startOffset\":0,\"endOffset\":3,\"file\":\"child.crv\"}",
        ]
    );
}

#[test]
fn every_part_an_include_pulled_in_names_its_file() {
    // List items, table rows and cells, definition terms and bodies, and
    // citation items carry a `pos` without being nodes of their own. A walk
    // that reached only nodes left them unstamped, and under a slice they kept
    // the slice's coordinates as well.
    let citations = carve::Citations::new();
    let child =
        "- item\n\n:: term\n:  def\n\n| a | b |\n|---|---|\n| one | two |\n\nSee [@doe, p. 3].\n";
    let resolver = Files(HashMap::from([(
        "child.crv".to_string(),
        child.to_string(),
    )]));
    let source = "{{ child.crv }}\n";
    let options = Options::default()
        .with_positions(true)
        .with_extension(&citations);
    let doc = parse_with_options(source, &options);
    let json = to_json(
        &expand_includes(
            doc,
            source,
            &IncludeOptions::new()
                .with_resolver(&resolver)
                .with_extension(&citations),
        )
        .doc,
    );
    for part in [
        "list_item",
        "definition_term",
        "definition_description",
        "table_row",
        "table_cell",
        "citation",
    ] {
        assert!(json.contains(&format!("\"{part}\"")), "no {part} in {json}");
    }
    let unstamped =
        json.matches("\"pos\":{").count() - json.matches("\"file\":\"child.crv\"").count();
    assert_eq!(unstamped, 0, "{json}");
}
