//! C2-W-144: the formatter keeps a next-line control comment directly above
//! the line it suppresses (C2-B-216).

use carve::api::{self, Carve, EncodeOptions, LintOptions, Options, WriteOptions};
use serde_json::Value;
use std::collections::BTreeMap;

const CONTROL: &str = "%% carve-lint-disable-next-line";
const FENCE: &str = "```";

fn format(src: &str) -> String {
    Carve::new()
        .format(src, &WriteOptions::default())
        .unwrap_or_else(|e| panic!("{src:?}: {e}"))
}

/// The tree of `src` without positions, as C2-A-161 compares it.
fn bare_tree(src: &str) -> Value {
    let tree = api::parse(src, &Options::default()).unwrap_or_else(|e| panic!("{src:?}: {e}"));
    let mut v: Value = serde_json::from_str(&api::encode_tree_with(
        &tree,
        EncodeOptions { positions: false },
    ))
    .unwrap_or_default();
    strip(&mut v);
    v
}

fn strip(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.remove("pos");
            m.remove("segments");
            m.remove("srcByteLength");
            m.values_mut().for_each(strip);
        }
        Value::Array(a) => a.iter_mut().for_each(strip),
        _ => {}
    }
}

/// The lint findings of `src` by rule, outside the parse channel.
fn findings(src: &str) -> BTreeMap<String, usize> {
    let mut found = BTreeMap::new();
    for entry in api::lint(src, &LintOptions::default()).unwrap_or_default() {
        if entry.channel != "parse" {
            *found.entry(entry.code).or_insert(0) += 1;
        }
    }
    found
}

#[test]
fn no_blank_line_between_the_comment_and_the_block_it_suppresses() {
    let c = CONTROL;
    assert_eq!(
        format(&format!("para\n\n{c}\n## Heading\n")),
        format!("para\n\n{c}\n== Heading\n")
    );
    assert_eq!(
        format(&format!("x\n\n{c}\n|a|b|\n")),
        format!("x\n\n{c}\n| a | b |\n")
    );
    assert_eq!(
        format(&format!("> x\n>\n> {c}\n> > q\n")),
        format!("> x\n>\n> {c}\n> > q\n")
    );
    assert_eq!(
        format(&format!("- a\n\n{c}\n- b\n")),
        format!("- a\n\n  {c}\n- b\n")
    );
}

#[test]
fn a_comment_that_suppresses_nothing_keeps_its_blank_line() {
    let c = CONTROL;
    assert_eq!(
        format(&format!("x\n\n{c}\n\n## See [a]()\n")),
        format!("x\n\n{c}\n\n== See [a]()\n")
    );
    assert_eq!(
        format(&format!("- a\n\n{c}\n\n- b\n")),
        format!("- a\n\n  {c}\n\n- b\n")
    );
    // C2-W-114 writes no blank line between two descriptions; this one stays, or the comment would suppress `: [a]()`.
    assert_eq!(
        format(&format!(":: t\n: d\n\n{c}\n\n: [a]()\n")),
        format!(":: t\n: d\n\n  {c}\n\n: [a]()\n")
    );
}

#[test]
fn the_blank_line_stays_where_leaving_it_out_changes_the_tree() {
    let c = CONTROL;
    assert_eq!(format(&format!("a {c}\n\nb\n")), format!("a {c}\n\nb\n"));
    assert_eq!(
        format(&format!("{FENCE}\n{c}\n\nx\n{FENCE}\n")),
        format!("{FENCE}\n{c}\n\nx\n{FENCE}\n")
    );
}

#[test]
fn a_supplied_tree_has_the_comment_directly_above_the_next_block() {
    let c = CONTROL;
    let tree = api::parse(
        format!("x\n\n{c}\n\n## See [a]()\n").as_str(),
        &Options::default(),
    )
    .unwrap();
    let written = Carve::new().write(&tree, &WriteOptions::default()).unwrap();
    assert_eq!(written, format!("x\n\n{c}\n== See [a]()\n"));
}

#[test]
fn fmt_then_lint_reports_what_lint_reports() {
    let finding = "[a]()";
    let targets = [
        format!("para {finding}"),
        format!("== H {finding}"),
        format!("| {finding} | b |"),
        format!("- {finding}"),
        format!("> {finding}"),
        format!(": {finding}"),
        format!("::: note\n{finding}\n:::"),
        format!("1. {finding}"),
        format!("{{.c}}\npara {finding}"),
    ];
    let befores = [
        String::new(),
        "x\n\n".into(),
        "== T\n\n".into(),
        "- a\n\n".into(),
        "| a | b |\n\n".into(),
        format!("{FENCE}\nc\n{FENCE}\n\n"),
        "> q\n\n".into(),
        "%% other\n".into(),
        ":: t\n: d\n\n".into(),
    ];
    let wraps: [fn(&str) -> String; 4] = [
        str::to_string,
        |s| {
            s.split('\n')
                .map(|l| {
                    if l.is_empty() {
                        ">".into()
                    } else {
                        format!("> {l}")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        },
        |s| {
            s.split('\n')
                .enumerate()
                .map(|(i, l)| match (l.is_empty(), i) {
                    (true, _) => String::new(),
                    (false, 0) => format!("- {l}"),
                    (false, _) => format!("  {l}"),
                })
                .collect::<Vec<_>>()
                .join("\n")
        },
        |s| format!(":::: w\n{s}\n::::"),
    ];
    let mut checked = 0;
    for wrap in wraps {
        for before in &befores {
            for target in &targets {
                for blank in ["", "\n"] {
                    let src = format!("{}\n", wrap(&format!("{before}{CONTROL}\n{blank}{target}")));
                    let out = format(&src);
                    assert_eq!(bare_tree(&out), bare_tree(&src), "{src:?}");
                    assert_eq!(format(&out), out, "{src:?}");
                    assert_eq!(findings(&out), findings(&src), "{src:?}");
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 600);
}

#[test]
fn a_crlf_blank_line_below_the_comment_stays() {
    let src = format!("x\r\n\r\n{CONTROL}\r\n\r\n## See [a]()\r\n");
    assert_eq!(format(&src), format!("x\n\n{CONTROL}\n\n== See [a]()\n"));
}

#[test]
fn a_comment_that_only_holds_the_keyword_later_is_no_control() {
    let src = format!("x\n\n%% foo {CONTROL}\n## H\n");
    assert_eq!(format(&src), format!("x\n\n%% foo {CONTROL}\n\n== H\n"));
}

#[test]
fn a_base64_layout_counts_as_no_source() {
    let bytes: &[u8] = b"x\n\n%% carve-lint-disable-next-line\n## H \xff\n";
    let parsed = api::parse_with_layout(bytes, &Options::default()).unwrap();
    assert_eq!(parsed.layout["encoding"], "base64");
    let opts = WriteOptions {
        layout: Some(parsed.layout.clone()),
        ..WriteOptions::default()
    };
    let written = Carve::new().write(&parsed.tree, &opts).unwrap();
    assert_eq!(written, format!("x\n\n{CONTROL}\n== H \u{FFFD}\n"));
}
