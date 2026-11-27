use carve::api;
use carve::include::{Cause, Includes, Request};
use carve::lint::{LintOptions, lint};
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Arc;

fn finding(source: &str, code: &str, upgrade: bool) -> Vec<carve::diagnostic::Diagnostic> {
    lint(
        source,
        &LintOptions {
            upgrade,
            ..LintOptions::default()
        },
    )
    .into_iter()
    .filter(|d| d.code == code)
    .collect()
}

#[test]
fn blocked_span_on_table_continuation_reports_once() {
    let source = "|= a |\n+ | ^ |";
    let hits = finding(source, "table-span-marker-blocked", false);
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(
        hits[0].pos.map(|p| (p.start_line, p.start_column)),
        Some((2, 4))
    );
    assert!(hits[0].message.contains("first row of its group"));
    let mut opts = LintOptions::default();
    opts.rules
        .insert("table-span-marker-blocked".into(), "off".into());
    assert!(
        lint(source, &opts)
            .iter()
            .all(|d| d.code != "table-span-marker-blocked")
    );
}

#[test]
fn blocked_span_continuation_family() {
    let words = [
        "a", "b", "cat", "dog", "one", "two", "red", "blue", "east", "west",
    ];
    let mut checked = 0;
    for word in words {
        for left in 0..10 {
            for right in 0..10 {
                let source = format!(
                    "|= {word} |\n+ |{}^{}|",
                    " ".repeat(left),
                    " ".repeat(right)
                );
                let hits = finding(&source, "table-span-marker-blocked", false);
                assert_eq!(hits.len(), 1, "{source:?}: {hits:?}");
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 1000);
}

#[test]
fn spaced_lone_less_than_cell_is_not_old_alignment() {
    assert!(finding("| a | < |", "deprecated-cell-alignment", true).is_empty());
    assert_eq!(finding("|< |", "deprecated-cell-alignment", true).len(), 1);
}

#[test]
fn unclosed_div_messages_keep_named_and_anonymous_templates() {
    let hits = finding(
        "- ::: note\n  text\n:::\nafter",
        "unclosed-container-fence",
        false,
    );
    assert!(
        hits.iter().any(|d| d.message.contains("admonition")),
        "{hits:?}"
    );
    assert!(
        hits.iter()
            .any(|d| d.message.contains("container has no closer")),
        "{hits:?}"
    );
    let named = finding("::: widget\nbody", "unclosed-container-fence", false);
    assert_eq!(named.len(), 1);
    assert!(named[0].message.contains("3-colon div has no closer"));
}

#[test]
fn duplicate_footnote_message_keeps_label() {
    let hits = finding(
        "x[^n]\n\n[^n]: a\n[^n]: [x][d]\n\n[d]: /u",
        "duplicate-footnote-definition",
        false,
    );
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert!(hits[0].message.contains("[^n]"), "{}", hits[0].message);
}

#[test]
fn code_fence_message_keeps_marker_after_attributes() {
    let hits = finding("{table}\n```csv\n \n`", "unclosed-code-fence", false);
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(
        hits[0].message.matches("```").count(),
        2,
        "{}",
        hits[0].message
    );
}

#[test]
fn include_failures_cap_at_100_and_count_suppressed() {
    let resolver = Arc::new(|_: &Request<'_>| Err(Cause::NotFound));
    let opts = LintOptions {
        includes: Some(Includes::new(resolver)),
        ..LintOptions::default()
    };
    let source = "{{ a.crv }}\n".repeat(101);
    let (entries, suppressed) = api::lint_with_suppressed(source.as_str(), &opts).unwrap();
    assert_eq!(
        entries
            .iter()
            .filter(|d| d.code == "include-not-found")
            .count(),
        100
    );
    assert_eq!(suppressed, 1);
}

#[test]
fn fix_without_an_edit_preserves_cr_and_crlf() {
    for source in [b"a\rb".as_slice(), b"a\r\nb".as_slice()] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
            .args(["fix", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(source).unwrap();
        let result = child.wait_with_output().unwrap();
        assert_eq!(result.stdout, source);
    }
}
