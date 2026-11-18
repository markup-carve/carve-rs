//! C2-W-138: duplicate IDs identify the authored attribute token.

use carve2::Options;
use carve2::include::{Cause, Includes, Request};
use std::sync::Arc;

#[test]
fn duplicate_id_range_skips_equal_text_and_prefixes() {
    for (input, line, start, end) in [
        ("{#a}\n= A\n\n[#a]{#a}\n", 4, 6, 8),
        ("{#a}\n= A\n\n[x]{#ab #a}\n", 4, 9, 11),
        ("{#a}\n= A\n\n[x]{id=a}\n", 4, 5, 9),
        ("{id='a b'}\n= A\n\n[x]{id='a b'}\n", 4, 5, 13),
        ("{id=\"a b\"}\n= A\n\n[x]{id=\"a b\"}\n", 4, 5, 13),
    ] {
        let out = carve2::render_with_diagnostics(input, &Options::default());
        let finding = out
            .diagnostics
            .iter()
            .find(|d| d.code == "duplicate-id")
            .unwrap_or_else(|| panic!("missing duplicate-id for {input:?}"));
        let pos = finding.pos.unwrap();
        assert_eq!(
            (
                pos.start_line,
                pos.start_column,
                pos.end_line,
                pos.end_column
            ),
            (line, start, line, end),
            "{input:?}"
        );
    }
}

#[test]
fn duplicate_id_in_included_file_uses_its_attribute_range() {
    let source = "{#a}\n= A\n\n[x]{#a}\n";
    let resolver = Arc::new(move |request: &Request<'_>| {
        if request.path == "a.crv" {
            Ok((source.as_bytes().to_vec(), "a.crv".to_string()))
        } else {
            Err(Cause::NotFound)
        }
    });
    let options = Options {
        includes: Some(Includes::new(resolver)),
        ..Options::default()
    };
    let out = carve2::render_with_diagnostics("{{ a.crv }}", &options);
    let finding = out
        .diagnostics
        .iter()
        .find(|d| d.code == "duplicate-id")
        .expect("duplicate ID in included file");
    let pos = finding.pos.unwrap();
    assert_eq!(finding.file.as_deref(), Some("a.crv"));
    assert_eq!(
        (
            pos.start_line,
            pos.start_column,
            pos.end_line,
            pos.end_column
        ),
        (4, 5, 4, 7)
    );
}

#[test]
fn duplicate_ids_on_a_container_and_its_child_keep_distinct_ranges() {
    let input = "{#a}\n= A\n\n{#a}\n::: note\n[x]{#a}\n:::\n";
    let out = carve2::render_with_diagnostics(input, &Options::default());
    let found: Vec<_> = out
        .diagnostics
        .iter()
        .filter(|d| d.code == "duplicate-id")
        .filter_map(|d| d.pos)
        .map(|p| (p.start_line, p.start_column, p.end_line, p.end_column))
        .collect();
    assert_eq!(found, [(4, 2, 4, 4), (6, 5, 6, 7)]);
}

#[test]
fn duplicate_footnote_range_skips_escaped_closing_bracket() {
    let input = "[^a\\]:b]: first\n\n[^a\\]:b]: second\n";
    let out = carve2::render_with_diagnostics(input, &Options::default());
    let finding = out
        .diagnostics
        .iter()
        .find(|d| d.code == "duplicate-footnote-definition")
        .expect("duplicate footnote");
    let pos = finding.pos.unwrap();
    assert_eq!(
        (
            pos.start_line,
            pos.start_column,
            pos.end_line,
            pos.end_column
        ),
        (3, 1, 3, 10)
    );
}
