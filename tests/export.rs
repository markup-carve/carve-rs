//! The Markdown, plain-text and ANSI exports (`carve::export`), with the
//! cases of carve-js `test/export.test.ts`.

use carve::api::{self, Carve, Error, Options, UrlPolicy, WriteOptions, Writer, Written};
use carve::export::{self, ExportOptions, ExportWriter, Target};
use std::sync::Arc;

const ESC: &str = "\x1b";

fn tree(source: &str) -> api::Tree {
    api::parse(source, &Options::default()).unwrap()
}

fn md(source: &str) -> String {
    export::to_markdown(&tree(source), &ExportOptions::default())
        .unwrap()
        .output
}

fn plain(source: &str) -> String {
    export::to_plain_text(&tree(source), &ExportOptions::default())
        .unwrap()
        .output
}

fn ansi(source: &str) -> String {
    export::to_ansi(&tree(source), &ExportOptions::default())
        .unwrap()
        .output
}

/// The ANSI output with its SGR sequences taken out, which is the text a terminal shows.
fn shown(source: &str) -> String {
    let out = ansi(source);
    let mut text = String::new();
    let mut rest = out.as_str();
    while let Some(at) = rest.find("\x1b[") {
        text.push_str(&rest[..at]);
        let after = &rest[at + 2..];
        let end = after.find('m').unwrap();
        rest = &after[end + 1..];
    }
    text.push_str(rest);
    text
}

fn codes(written: &Written) -> Vec<&str> {
    written
        .diagnostics
        .iter()
        .map(|e| e.code.as_str())
        .collect()
}

#[test]
fn an_export_returns_the_text_and_the_losses_in_one_result() {
    let result = export::to_markdown(&tree("::: x\nz\n:::\n"), &ExportOptions::default()).unwrap();
    assert_eq!(result.output, "z\n");
    assert_eq!(codes(&result), ["markdown-container-flattened"]);
    let entry = &result.diagnostics[0];
    assert_eq!(entry.channel, "render-loss");
    assert_eq!(entry.severity, "warning");
    assert_eq!(entry.path.as_deref(), Some("/children/0"));
    assert!(entry.range.is_some());
}

#[test]
fn strict_losses_refuses_a_run_that_lost_something() {
    let strict = ExportOptions {
        strict_losses: true,
        ..ExportOptions::default()
    };
    let err = export::to_markdown(&tree("```=latex\n\\x\n```\n"), &strict).unwrap_err();
    assert!(
        matches!(&err, Error::Export { diagnostics } if diagnostics[0].code == "raw-format-dropped")
    );
    assert_eq!(err.spec_name(), "CarveExportError");
    assert_eq!(err.category(), "render-loss");
    assert_eq!(
        export::to_markdown(&tree("a\n"), &strict).unwrap().output,
        "a\n"
    );
}

#[test]
fn smart_punctuation_is_the_glyph_or_as_typed() {
    let t = tree("a -- b...\n");
    assert_eq!(
        export::to_plain_text(&t, &ExportOptions::default())
            .unwrap()
            .output,
        "a – b…\n"
    );
    let typed = ExportOptions {
        smart_typography: false,
        ..ExportOptions::default()
    };
    assert_eq!(
        export::to_plain_text(&t, &typed).unwrap().output,
        "a -- b...\n"
    );
}

#[test]
fn a_denied_destination_is_blanked_and_reported() {
    let by_default = export::to_markdown(
        &tree("[a](javascript:x) [b](ftp://h/f)\n"),
        &ExportOptions::default(),
    )
    .unwrap();
    assert_eq!(by_default.output, "[a]() [b](ftp://h/f)\n");
    assert_eq!(codes(&by_default), ["destination-denied"]);
    assert_eq!(by_default.diagnostics[0].severity, "error");
    let deny_ftp = ExportOptions {
        url_policy: UrlPolicy::Deny(vec!["ftp".into()]),
        ..ExportOptions::default()
    };
    assert_eq!(
        export::to_markdown(&tree("[b](ftp://h/f)\n"), &deny_ftp)
            .unwrap()
            .output,
        "[b]()\n"
    );
}

#[test]
fn the_registry_holds_the_exports_under_their_reserved_targets() {
    let host = Carve::new();
    assert_eq!(
        host.writers().targets(),
        ["ansi", "carve", "html", "markdown", "plain"]
    );
    let t = tree("= T\n\n*b* text\n");
    let w = WriteOptions::default();
    let opts = ExportOptions::default();
    assert_eq!(
        host.write_to("markdown", &t, &w).unwrap(),
        export::to_markdown(&t, &opts).unwrap()
    );
    assert_eq!(
        host.write_to("plain", &t, &w).unwrap(),
        export::to_plain_text(&t, &opts).unwrap()
    );
    assert_eq!(
        host.write_to("ansi", &t, &w).unwrap(),
        export::to_ansi(&t, &opts).unwrap()
    );
    let names: Vec<&str> = export::EXPORT_TARGETS.iter().map(|t| t.name()).collect();
    assert_eq!(names, ["markdown", "plain", "ansi"]);
    let mut other = Carve::new();
    let taken = other
        .register_writer(Arc::new(ExportWriter(Target::Plain)))
        .unwrap_err();
    assert!(taken.to_string().contains("reserved"), "{taken}");
}

#[test]
fn an_unknown_target_lists_the_known_ones() {
    let err = Carve::new()
        .write_to("pdf", &tree("a"), &WriteOptions::default())
        .unwrap_err();
    assert!(matches!(err, Error::Option { .. }));
    assert!(
        err.to_string()
            .contains("known targets: ansi, carve, html, markdown, plain"),
        "{err}"
    );
    assert_eq!(
        ExportWriter(Target::Plain)
            .write(&tree("*a*"), &WriteOptions::default())
            .unwrap()
            .output,
        "a\n"
    );
}

#[test]
fn markdown_writes_each_construct() {
    let cases = [
        ("= Head", "# Head\n"),
        ("a *b* /c/ `d`", "a **b** *c* `d`\n"),
        ("- [x] done\n- [ ] todo", "- [x] done\n- [ ] todo\n"),
        ("1. one\n2. two", "1. one\n2. two\n"),
        ("> q", "> q\n"),
        ("```js\nx\n```", "```js\nx\n```\n"),
        ("a\n\n***\n\nb", "a\n\n---\n\nb\n"),
        (
            "|= A |= B |\n| 1 | 2 |",
            "| A | B |\n| --- | --- |\n| 1 | 2 |\n",
        ),
        (
            "{=mark=} {+ins+} {-del-}",
            "<mark>mark</mark> <ins>ins</ins> <del class=\"critic-delete\">del</del>\n",
        ),
        ("[t](https://e.x \"T\")", "[t](https://e.x \"T\")\n"),
        ("![a](i.png)", "![a](i.png)\n"),
        ("<https://a.b>", "<https://a.b>\n"),
        ("x[^n]\n\n[^n]: note", "x[^n]\n\n[^n]: note\n"),
        ("::: note\nbody\n:::", "body\n"),
        ("::: |\nl1\nl2\n:::", "l1\\\nl2\n"),
        ("$`x`", "$x$\n"),
        ("[x]{abbr=Y}", "<abbr title=\"Y\">x</abbr>\n"),
        (":: t\n: d", "**t**\n\nd\n"),
        ("```=html\n<b>\n```", "&lt;b&gt;\n"),
        ("a\\\nb", "a\\\nb\n"),
        ("snake_case and _x", "snake_case and \\_x\n"),
        ("= H\n\nSee </#H>.", "# H\n\nSee [H](#h).\n"),
        ("- a\n\n%% #region x\n\n\n\n- b", "- a\n\n* b\n"),
        ("- a\n  - b", "- a\n  - b\n"),
        ("x {_y_}", "x <u>y</u>\n"),
        ("``a`b``", "``a`b``\n"),
        ("x{*\"y\"*}", "x<strong>“y”</strong>\n"),
    ];
    for (source, expected) in cases {
        assert_eq!(md(source), expected, "{source:?}");
    }
}

#[test]
fn markdown_keeps_text_from_reading_as_a_block() {
    let json = r##"{"type":"document","astVersion":"2.0","children":[{"type":"paragraph","children":[
        {"type":"text","value":"a"},{"type":"soft_break"},{"type":"text","value":"- b"},{"type":"soft_break"},
        {"type":"text","value":"# c"},{"type":"soft_break"},{"type":"text","value":"1. d"},{"type":"soft_break"},
        {"type":"text","value":"***"}]}]}"##;
    let t = api::decode_tree(json).unwrap();
    assert_eq!(
        export::to_markdown(&t, &ExportOptions::default())
            .unwrap()
            .output,
        "a\n\\- b\n\\# c\n1\\. d\n\\*\\*\\*\n"
    );
}

#[test]
fn markdown_writes_a_caption_after_its_host() {
    let result = export::to_markdown(
        &tree("![p](p.png)\n^ Figure #: q\n"),
        &ExportOptions::default(),
    )
    .unwrap();
    assert_eq!(result.output, "![p](p.png)\nFigure 1: q\n");
    assert_eq!(codes(&result), ["markdown-caption-flattened"]);
}

#[test]
fn plain_text_writes_each_construct() {
    let cases = [
        ("= Head", "Head\n"),
        ("a *b* /c/ `d`", "a b c d\n"),
        ("- [x] done\n- [ ] todo", "- done\n- todo\n"),
        ("> q", "\"q\"\n"),
        ("|= A |= B |\n| 1 | 2 |", "A | B\n1 | 2\n"),
        ("{-del-} {+ins+}", "~del~ ins\n"),
        ("x[^n]\n\n[^n]: note", "x[n]\n\n[^n]: note\n"),
        (
            "*[HTML]: Hyper Text\n\nHTML here",
            "HTML (Hyper Text) here\n",
        ),
        ("@user #tag :smile:", "@user #tag :smile:\n"),
        ("::: |\na \\\nb\n:::", "a \nb\n"),
        ("a\n%% c\nb", "a b\n"),
    ];
    for (source, expected) in cases {
        assert_eq!(plain(source), expected, "{source:?}");
    }
    let raw =
        export::to_plain_text(&tree("```=html\n<b>\n```\n"), &ExportOptions::default()).unwrap();
    assert_eq!(raw.output, "\n");
    assert_eq!(codes(&raw), ["raw-format-dropped"]);
}

#[test]
fn ansi_styles_inline_markup_and_resets_after_each_run() {
    assert_eq!(
        ansi("a *b* /c/ `d`"),
        format!("a {ESC}[1mb{ESC}[0m {ESC}[3mc{ESC}[0m {ESC}[93md{ESC}[0m\n")
    );
}

#[test]
fn ansi_underlines_the_first_two_heading_levels_across_their_width() {
    assert_eq!(shown("= 日本"), "日本\n════\n");
    assert_eq!(shown("=== x"), "x\n");
}

#[test]
fn ansi_draws_a_table_in_a_box() {
    assert_eq!(
        shown("|= A |= B |\n| 1 | 22 |"),
        "┌───┬────┐\n│ A │ B  │\n├───┼────┤\n│ 1 │ 22 │\n└───┴────┘\n"
    );
}

#[test]
fn ansi_marks_quote_lines_and_task_states() {
    assert_eq!(shown("> a\n>\n> b"), "│ a\n\n│ b\n");
    assert_eq!(shown("- [x] a\n- [ ] b"), "☑ a\n☐ b\n");
}

#[test]
fn ansi_drops_control_characters_of_the_author() {
    let json = r#"{"type":"document","astVersion":"2.0","children":[{"type":"paragraph","children":[
        {"type":"text","value":"a\u001b]0;x\u0007b"}]}]}"#;
    let t = api::decode_tree(json).unwrap();
    assert_eq!(
        export::to_ansi(&t, &ExportOptions::default())
            .unwrap()
            .output,
        "a]0;xb\n"
    );
}

fn examples() -> serde_json::Value {
    let text = std::fs::read_to_string("spec-conformance/examples.json").unwrap();
    let mut de = serde_json::Deserializer::from_str(&text);
    de.disable_recursion_limit();
    serde::Deserialize::deserialize(&mut de).unwrap()
}

fn example_tree(id: &str) -> api::Tree {
    let all = examples();
    let example = all["examples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap();
    api::decode_tree(&example["expected"]["tree"].to_string()).unwrap()
}

#[test]
fn an_expanded_include_is_exported_with_its_own_definitions() {
    let t = example_tree("includes-40");
    let opts = ExportOptions::default();
    assert_eq!(
        export::to_markdown(&t, &opts).unwrap().output,
        "See [s](/u) and <abbr title=\"Hyper\">HTML</abbr>.\n\n*[HTML]: Hyper\n"
    );
    assert_eq!(
        export::to_plain_text(&t, &opts).unwrap().output,
        "See s and HTML (Hyper).\n"
    );
}

#[test]
fn every_specification_example_tree_has_an_output_in_each_export() {
    let examples = examples();
    let mut written = 0;
    for example in examples["examples"].as_array().unwrap() {
        let Some(json) = example.pointer("/expected/tree") else {
            continue;
        };
        let t = api::decode_tree(&json.to_string()).unwrap();
        for target in export::EXPORT_TARGETS {
            let first = export::export_to(target, &t, &ExportOptions::default()).unwrap();
            let again = export::export_to(target, &t, &ExportOptions::default()).unwrap();
            assert_eq!(first, again, "{}", example["id"]);
        }
        written += 1;
    }
    assert!(written > 500, "{written}");
}

#[test]
fn the_json_boundary_writes_the_export_targets() {
    let call = |target: &str, options: &str| {
        api::json::call(&format!(
            r#"{{"call":"writeWithDiagnostics","target":"{target}","options":{options},"tree":{}}}"#,
            api::encode_tree(&tree("a -- *b*\n"))
        ))
    };
    assert_eq!(
        call("markdown", "{}"),
        r#"{"ok":{"diagnostics":[],"output":"a – **b**\n"}}"#
    );
    assert_eq!(
        call("plain", r#"{"smartTypography":false}"#),
        r#"{"ok":{"diagnostics":[],"output":"a -- b\n"}}"#
    );
    assert!(call("ansi", "{}").contains(r"\u001b[1mb\u001b[0m"));
    let unknown = call("pdf", "{}");
    assert!(
        unknown.contains("known targets: ansi, carve, html, markdown, plain"),
        "{unknown}"
    );
    let refused = call("carve", r#"{"smartTypography":false}"#);
    assert!(refused.contains("does not read it"), "{refused}");
}
