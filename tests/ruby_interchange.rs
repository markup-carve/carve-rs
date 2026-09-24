use carve::{CheckedRenderOptions, InlineNode, RenderTarget};
use std::io::Write;
use std::process::{Command, Stdio};

const RUBY: &str = r#"{"type":"document","srcByteLength":0,"children":[{"type":"paragraph","children":[{"type":"ruby","pairs":[{"base":[{"type":"text","value":"漢"}],"annotation":[{"type":"text","value":"かん"}]},{"base":[{"type":"text","value":"字"}],"annotation":[]}],"attrs":{"id":"reading"}}]}]}"#;

fn document() -> carve::Document {
    carve::from_json(RUBY).unwrap()
}

#[test]
fn ruby_pairs_survive_json_and_reject_missing_structure() {
    let doc = document();
    let written = carve::to_json(&doc);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&written).unwrap(),
        serde_json::from_str::<serde_json::Value>(RUBY).unwrap()
    );
    assert_eq!(carve::from_json(&written).unwrap(), doc);
    let carve::BlockNode::Paragraph(paragraph) = &doc.children[0] else {
        panic!("paragraph lost");
    };
    let InlineNode::Ruby(ruby) = &paragraph.children[0] else {
        panic!("ruby node lost");
    };
    assert_eq!(ruby.pairs.len(), 2);
    assert!(ruby.pairs[1].annotation.is_empty());

    for bad in [
        RUBY.replace(r#""pairs":[{"base""#, r#""pairs":[{"extra":1,"base""#),
        RUBY.replace(r#""pairs":[{"base""#, r#""pairs":[{"base":[],"x""#),
        RUBY.replace(r#""pairs":[{"base""#, r#""pairs":[], "unused":[{"base""#),
    ] {
        assert!(carve::from_json(&bad).is_err(), "{bad}");
    }
}

#[test]
fn native_targets_keep_pairs_and_text_targets_report_one_loss() {
    let doc = document();
    assert_eq!(
        carve::render_html(&doc).unwrap(),
        "<p><ruby id=\"reading\">漢<rp>(</rp><rt>かん</rt><rp>)</rp>字<rp>(</rp><rt></rt><rp>)</rp></ruby></p>"
    );
    assert_eq!(
        carve::render_markdown(&doc).unwrap(),
        "<ruby id=\"reading\">漢<rp>(</rp><rt>かん</rt><rp>)</rp>字<rp>(</rp><rt></rt><rp>)</rp></ruby>\n"
    );
    for (target, expected) in [
        (RenderTarget::Plain, "漢(かん)字()\n"),
        (RenderTarget::Ansi, "漢(かん)字()\n"),
        (RenderTarget::Carve, "[漢(かん)字()]{#reading}\n"),
    ] {
        let result = carve::with_render_loss_report(
            target,
            CheckedRenderOptions::default(),
            || match target {
                RenderTarget::Plain => carve::render_plain_text(&doc).unwrap(),
                RenderTarget::Ansi => carve::render_ansi(&doc).unwrap(),
                RenderTarget::Carve => carve::render_carve(&doc).unwrap(),
                _ => unreachable!(),
            },
        )
        .unwrap();
        assert_eq!(result.value, expected);
        assert_eq!(result.total_losses, 1);
        assert_eq!(result.losses[0].code, "ruby-flattened");
        assert_eq!(result.losses[0].format, None);
    }
}

fn cli(args: &[&str], source: &str) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn cli_allows_only_named_losses_even_when_report_is_truncated() {
    let refused = cli(&["--from-json", "--plain", "--strict-losses"], RUBY);
    assert!(!refused.status.success());
    assert!(refused.stdout.is_empty());

    let allowed = cli(
        &[
            "--from-json",
            "--plain",
            "--strict-losses",
            "--allow-loss",
            "ruby-flattened",
            "--max-render-losses",
            "0",
        ],
        RUBY,
    );
    assert!(allowed.status.success(), "{:?}", allowed.stderr);
    assert_eq!(allowed.stdout, "漢(かん)字()\n".as_bytes());

    let mixed = RUBY.replace(
        r#""type":"ruby""#,
        r#"{"type":"raw_inline","format":"latex","content":"x"},{"type":"ruby""#,
    );
    let refused_mixed = cli(
        &[
            "--from-json",
            "--plain",
            "--strict-losses",
            "--allow-loss",
            "ruby-flattened",
        ],
        &mixed,
    );
    assert!(!refused_mixed.status.success());
    assert!(refused_mixed.stdout.is_empty());
}

#[test]
fn nested_ruby_reports_each_node_once_in_reading_order() {
    let mut payload: serde_json::Value = serde_json::from_str(RUBY).unwrap();
    payload["children"][0]["children"][0]["pairs"][0]["base"] = serde_json::json!([
        {
            "type": "ruby",
            "pairs": [{
                "base": [{"type": "text", "value": "日"}],
                "annotation": [{"type": "text", "value": "にち"}]
            }]
        }
    ]);
    let doc = carve::from_json(&payload.to_string()).unwrap();
    assert_eq!(carve::from_json(&carve::to_json(&doc)).unwrap(), doc);
    let result = carve::with_render_loss_report(
        RenderTarget::Carve,
        CheckedRenderOptions::default(),
        || carve::render_carve(&doc).unwrap(),
    )
    .unwrap();
    assert_eq!(result.total_losses, 2);
    assert_eq!(result.losses.len(), 2);
    assert_eq!(result.value, "[日(にち)(かん)字()]{#reading}\n");
}

#[test]
fn profile_rules_reach_links_in_base_and_annotation() {
    use carve::{apply_profile, DisallowedAction, Profile};

    let mut payload: serde_json::Value = serde_json::from_str(RUBY).unwrap();
    for (field, href, text) in [
        ("base", "https://base.test", "base"),
        ("annotation", "https://annotation.test", "reading"),
    ] {
        payload["children"][0]["children"][0]["pairs"][0][field] = serde_json::json!([
            {"type":"link","href":href,"children":[{"type":"text","value":text}]}
        ]);
    }
    let doc = carve::from_json(&payload.to_string()).unwrap();
    let html = carve::render_html(&doc).unwrap();
    assert!(html.contains("href=\"https://base.test\""), "{html}");
    assert!(html.contains("href=\"https://annotation.test\""), "{html}");

    let filtered = apply_profile(
        doc,
        &Profile::default()
            .deny_inline(&["link"])
            .on_disallowed(DisallowedAction::ToText),
        None,
    )
    .unwrap();
    let wire = carve::to_json(&filtered.doc);
    assert!(wire.contains(r#""type":"ruby""#), "{wire}");
    assert!(!wire.contains(r#""type":"link""#), "{wire}");
    assert!(wire.contains("base") && wire.contains("reading"), "{wire}");
}

#[test]
fn profiles_reach_both_sides_and_can_flatten_the_ruby_wrapper() {
    use carve::{apply_profile, DisallowedAction, Profile};

    let flattened = apply_profile(
        document(),
        &Profile::default()
            .deny_inline(&["ruby"])
            .on_disallowed(DisallowedAction::ToText),
        None,
    )
    .unwrap();
    assert!(!carve::to_json(&flattened.doc).contains(r#""type":"ruby""#));
    assert_eq!(
        carve::render_plain_text(&flattened.doc).unwrap(),
        "漢(かん)字()\n"
    );

    let stripped = apply_profile(
        document(),
        &Profile::default()
            .deny_inline(&["ruby"])
            .on_disallowed(DisallowedAction::Strip),
        None,
    )
    .unwrap();
    assert!(!carve::to_json(&stripped.doc).contains("ruby"));
}

#[test]
fn carve_writer_escapes_text_against_flattened_ruby_neighbors() {
    let mut payload: serde_json::Value = serde_json::from_str(RUBY).unwrap();
    payload["children"][0]["children"] = serde_json::json!([
        {"type":"text","value":"^"},
        {"type":"ruby","attrs":{"id":"r"},"pairs":[{
            "base":[{"type":"text","value":"B"}],
            "annotation":[{"type":"text","value":"a"}]
        }]}
    ]);
    let footnote_neighbor = carve::from_json(&payload.to_string()).unwrap();
    assert_eq!(
        carve::render_carve(&footnote_neighbor).unwrap(),
        "\\^[B(a)]{#r}\n"
    );

    payload["children"][0]["children"] = serde_json::json!([
        {"type":"text","value":"a $"},
        {"type":"ruby","pairs":[{
            "base":[{"type":"code","value":"x"}],
            "annotation":[]
        }]}
    ]);
    let code_neighbor = carve::from_json(&payload.to_string()).unwrap();
    assert_eq!(carve::render_carve(&code_neighbor).unwrap(), "a \\$`x`()\n");
}

#[test]
fn carve_loss_report_skips_citation_fields_the_writer_does_not_emit() {
    let mut payload: serde_json::Value = serde_json::from_str(RUBY).unwrap();
    let ruby = payload["children"][0]["children"][0].clone();
    payload["children"][0]["children"] = serde_json::json!([{
        "type":"citation_group","raw":"[see @a]","items":[{
            "type":"citation","key":"a","suppressAuthor":false,
            "prefix":[ruby]
        }]
    }]);
    let document = carve::from_json(&payload.to_string()).unwrap();
    let result = carve::with_render_loss_report(
        RenderTarget::Carve,
        CheckedRenderOptions::default(),
        || carve::render_carve(&document).unwrap(),
    )
    .unwrap();
    assert_eq!(result.value, "[see @a]\n");
    assert_eq!(result.total_losses, 0);
}
