//! Lint resolves references of an included file against the including files (C2-N-054, C2-N-055).
use carve::include::{Cause, Includes, Request};
use carve::lint::{LintOptions, lint};
use std::sync::Arc;

fn options(files: &'static [(&'static str, &'static str)]) -> LintOptions {
    let resolver = Arc::new(move |request: &Request<'_>| {
        files
            .iter()
            .find(|(name, _)| *name == request.path)
            .map(|(name, text)| (text.as_bytes().to_vec(), (*name).into()))
            .ok_or(Cause::NotFound)
    });
    LintOptions {
        includes: Some(Includes::new(resolver)),
        ..LintOptions::default()
    }
}

fn findings(source: &str, files: &'static [(&'static str, &'static str)]) -> Vec<String> {
    lint(source, &options(files))
        .iter()
        .map(|d| {
            let pos = d.pos.expect("position");
            format!(
                "{}:{}:{} {}",
                d.file.as_deref().unwrap_or("main.crv"),
                pos.start_line,
                pos.start_column,
                d.code
            )
        })
        .collect()
}

#[test]
fn includes_19_child_links_resolve_against_the_parent() {
    let found = findings(
        "Parent [p][ref].\n\n[ref]: https://parent.example\n[up]: https://up.example\n\n{{ child.crv }}",
        &[(
            "child.crv",
            "Child [c][ref] and [up][].\n\n[ref]: https://child.example\n",
        )],
    );
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn includes_20_child_footnotes_resolve_against_the_parent() {
    let found = findings(
        "See [^n].\n\n{{ c.crv }}\n\n[^p]: Parent note.\n[^n]: Parent n.",
        &[("c.crv", "Child sees [^p] and [^n].\n\n[^n]: Child note.\n")],
    );
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn a_grandchild_reaches_the_top_level_definitions() {
    let found = findings(
        "Top.\n\n{{ c.crv }}\n\n[k]: /k\n[^n]: Note.\n",
        &[
            ("c.crv", "Child.\n\n{{ g.crv }}\n"),
            ("g.crv", "Grand [t][k] and [^n].\n"),
        ],
    );
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn a_definition_never_reaches_inward() {
    let found = findings(
        "Top [t][k] and [^n].\n\n{{ c.crv }}\n",
        &[("c.crv", "Child.\n\n[k]: /k\n[^n]: Note.\n")],
    );
    assert_eq!(
        found,
        [
            "main.crv:1:5 unresolved-reference-link",
            "main.crv:1:16 unresolved-footnote",
            "c.crv:3:1 unused-link-reference-definition",
            "c.crv:4:1 unused-footnote-definition",
        ]
    );
}

#[test]
fn the_nearest_definition_wins_and_the_outer_one_stays_unused() {
    let found = findings(
        "Top.\n\n{{ c.crv }}\n\n[k]: /outer\n",
        &[("c.crv", "Child [t][k].\n\n[k]: /inner\n")],
    );
    assert_eq!(found, ["main.crv:5:1 unused-link-reference-definition"]);
}
