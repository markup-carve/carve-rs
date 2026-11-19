use carve::include::{Cause, Includes, Request};
use carve::lint::{LintOptions, lint};
use std::sync::Arc;

fn options(text: &'static str) -> LintOptions {
    let resolver = Arc::new(move |request: &Request<'_>| {
        if request.path == "a.crv" {
            Ok((text.as_bytes().to_vec(), "a.crv".into()))
        } else {
            Err(Cause::NotFound)
        }
    });
    LintOptions {
        includes: Some(Includes::new(resolver)),
        ..LintOptions::default()
    }
}

#[test]
fn c2_n_085_to_087_lints_accepted_file_once_with_file_position_and_path() {
    let findings = lint("{{ a.crv }}\n\n{{ a.crv }}", &options("**bold**\n\nsafe\n"));
    let hits: Vec<_> = findings
        .iter()
        .filter(|d| d.code == "markdown-strong-double-star")
        .collect();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].file.as_deref(), Some("a.crv"));
    assert_eq!(hits[0].pos.map(|p| p.start_line), Some(1));
    assert_eq!(hits[0].included_by.len(), 1);
    assert_eq!(hits[0].included_by[0].line, 1);
    assert!(
        hits[0]
            .path
            .as_deref()
            .is_some_and(|path| path.starts_with("/children/0/children/"))
    );
}

#[test]
fn c2_n_086_lints_only_the_selected_lines_or_section() {
    let source = "# A\n\n**bold**\n\n# B\n\n**other**\n";
    let opts = options(source);
    let lines = lint("{{ a.crv @lines:1-3 }}", &opts);
    let hits: Vec<_> = lines
        .iter()
        .filter(|d| d.code == "markdown-strong-double-star")
        .collect();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].pos.map(|p| p.start_line), Some(3));
    let section = lint("{{ a.crv#A }}", &opts);
    let hits: Vec<_> = section
        .iter()
        .filter(|d| d.code == "markdown-strong-double-star")
        .collect();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].pos.map(|p| p.start_line), Some(3));
}

#[test]
fn c2_n_085_no_resolver_and_missing_target_are_reported() {
    let no_resolver = lint("{{ a.crv }}", &LintOptions::default());
    assert!(
        no_resolver
            .iter()
            .any(|finding| finding.code == "include-no-resolver")
    );
    let missing = lint("{{ b.crv }}", &options("ready"));
    assert!(
        missing
            .iter()
            .any(|finding| finding.code.starts_with("include-"))
    );
}
