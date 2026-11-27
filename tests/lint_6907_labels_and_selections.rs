//! Lint of comments in labels (C2-I-072, C2-I-077, C2-I-087) and of selected includes (C2-N-020, C2-N-086).
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
            let sites: Vec<String> = d
                .included_by
                .iter()
                .map(|s| format!(" <{}:{}", s.line, s.column))
                .collect();
            format!(
                "{}:{}:{} {}{}",
                d.file.as_deref().unwrap_or("main.crv"),
                pos.start_line,
                pos.start_column,
                d.code,
                sites.concat()
            )
        })
        .collect()
}

#[test]
fn a_spaced_bracket_pair_is_no_reference_label() {
    assert_eq!(findings("[x] [a %% b]\n", &[]), Vec::<String>::new());
}

#[test]
fn a_glued_reference_label_still_reports() {
    assert_eq!(
        findings("[x][a %% b]\n", &[]),
        ["main.crv:1:6 comment-in-reference-label"]
    );
}

#[test]
fn a_footnote_label_reports() {
    assert_eq!(
        findings("[^a %% b]\n", &[]),
        ["main.crv:1:4 comment-in-reference-label"]
    );
    assert_eq!(
        findings("Text [^a %% b].\n", &[]),
        ["main.crv:1:9 comment-in-reference-label"]
    );
}

#[test]
fn link_text_and_a_shortcut_take_no_label() {
    assert_eq!(
        findings("[a %% b][r]\n\n[r]: /u\n", &[]),
        ["main.crv:3:1 unused-link-reference-definition"]
    );
    assert_eq!(findings("[a %% b]\n", &[]), Vec::<String>::new());
    assert_eq!(findings("[a %% b](/u) c\n", &[]), Vec::<String>::new());
}

#[test]
fn a_comment_that_takes_no_closer_leaves_no_label_unclosed() {
    assert_eq!(findings("[x][a %% b\n", &[]), Vec::<String>::new());
}

#[test]
fn a_region_is_linted_without_the_definitions_appended_to_it() {
    assert_eq!(
        findings(
            "{{ b.crv @region:k }}\n",
            &[(
                "b.crv",
                "%% #region k\nSee nothing.\n%% #endregion k\n\n[s]: /u\n"
            )],
        ),
        Vec::<String>::new()
    );
}

#[test]
fn a_definition_inside_the_region_still_reports() {
    assert_eq!(
        findings(
            "{{ b.crv @region:k }}\n",
            &[(
                "b.crv",
                "See [t][].\n\n%% #region k\n[s]: /u\n%% #endregion k\n\n[t]: /t\n"
            )],
        ),
        ["b.crv:4:1 unused-link-reference-definition <1:1"]
    );
}

#[test]
fn a_duplicate_id_of_a_second_site_names_that_site() {
    assert_eq!(
        findings(
            "{{ a.crv }}\n\n{{ a.crv @lines:1-2 }}\n",
            &[("a.crv", "{#x}\n# H\n")],
        ),
        ["a.crv:1:2 duplicate-id <3:1"]
    );
}
