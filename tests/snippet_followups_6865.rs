//! Snippet follow-ups: the attribute line above a definition (carve#6865) and
//! `unused-snippet` across the files of a lint with includes (C2-N-096).
use carve::include::{Cause, Includes, Request};
use carve::lint::{LintOptions, lint};
use carve::options::Options;
use std::sync::Arc;

fn codes(source: &str, opts: &LintOptions) -> Vec<(String, Option<String>, u32)> {
    lint(source, opts)
        .into_iter()
        .map(|d| {
            (
                d.code.to_string(),
                d.file,
                d.pos.map_or(0, |p| p.start_line),
            )
        })
        .collect()
}

fn with_files(files: &'static [(&'static str, &'static str)]) -> LintOptions {
    let resolver = Arc::new(move |request: &Request<'_>| {
        files
            .iter()
            .find(|(name, _)| *name == request.path)
            .map(|(name, text)| (text.as_bytes().to_vec(), (*name).to_string()))
            .ok_or(Cause::NotFound)
    });
    LintOptions {
        includes: Some(Includes::new(resolver)),
        ..LintOptions::default()
    }
}

/// C2-B-236: an id in a key-value or in a later attribute line refuses the definition too.
#[test]
fn an_id_anywhere_in_the_attribute_lines_refuses_the_definition() {
    let opts = Options::default();
    assert_eq!(
        carve::render("{id=x}\n{$v}: X\n", &opts),
        "<p id=\"x\">{$v}: X</p>"
    );
    assert_eq!(
        carve::render("{#i}\n{.k}\n{$v}: X\n\n{{ $v }}\n", &opts),
        "<p id=\"i\" class=\"k\">{$v}: X</p>\n<p>{{ $v }}</p>"
    );
    let found = codes("{id=x}\n{$v}: X\n", &LintOptions::default());
    assert!(found.contains(&("snippet-definition-id".into(), None, 2)));
}

/// C2-B-236: a marker id under an attribute line without one is reported at the marker.
#[test]
fn a_marker_id_under_an_attribute_line_is_reported_at_the_marker() {
    let found = codes("{.k}\n{$v #x}: a\n", &LintOptions::default());
    assert!(found.contains(&("snippet-definition-id".into(), None, 2)));
}

/// C2-N-096: a use in an included file counts for a definition of the entry,
/// and a definition that loses to an earlier one takes no hint.
#[test]
fn unused_snippet_reads_the_uses_and_the_registry_of_every_file() {
    let opts = with_files(&[("part.crv", "{$b}: other\n{$c}: C\n\nUse {{ $a }}.\n")]);
    let found = codes("{$a}: A\n{$b}: B\n\n{{ part.crv }}\n", &opts);
    let unused: Vec<_> = found
        .iter()
        .filter(|(code, _, _)| code == "unused-snippet")
        .map(|(_, file, line)| (file.clone(), *line))
        .collect();
    assert_eq!(unused, [(None, 2), (Some("part.crv".into()), 2)]);
    assert!(found.contains(&("duplicate-snippet".into(), Some("part.crv".into()), 1)));
}

/// C2-N-086: an include the expansion refused is not linted.
#[test]
fn a_refused_include_is_not_linted() {
    let opts = with_files(&[("part.crv", "[r]: /u\n\n# a\n### b\n")]);
    let found = codes("{{ part.crv @lines:5-5 }}\n", &opts);
    assert_eq!(found, [("include-lines-out-of-range".into(), None, 1)]);
}

/// C2-N-090: the definitions of an include inside a container count where it stands.
#[test]
fn unused_snippet_reads_an_include_inside_a_container() {
    let opts = with_files(&[("part.crv", "{$a}: A\n{$b}: B\n")]);
    let found = codes("- {{ part.crv }}\n\n{$b}: again\n\nUse {{ $a }}.\n", &opts);
    let unused: Vec<_> = found
        .iter()
        .filter(|(code, _, _)| code == "unused-snippet")
        .map(|(_, file, line)| (file.clone(), *line))
        .collect();
    assert_eq!(unused, [(Some("part.crv".into()), 2)]);
}
