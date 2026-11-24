use carve::api::{LintOptions, json::call};
use serde_json::{Value, json};

fn invoke(request: &Value) -> Value {
    serde_json::from_str(&call(&request.to_string())).unwrap()
}

#[test]
fn paired_brackets_inside_labels_stay_bare() {
    let tree = json!({"type":"document","astVersion":"2.0","children":[
        {"type":"paragraph","children":[{"type":"link","href":"#x","children":[
            {"type":"span","attrs":{"classes":["c"]},"children":[
                {"type":"text","value":"[1]"}
            ]}
        ]}]}
    ]});
    let written = invoke(&json!({"call":"writeWithDiagnostics","tree":tree}));
    assert_eq!(written["ok"]["output"], "[[[1]]{.c}](#x)\n");
    assert_eq!(written["ok"]["diagnostics"], json!([]));

    for source in ["[[1]](#x)\n", "[a [b] c](#x)\n", "[[[1]]{.c}](#x)\n"] {
        let formatted = invoke(&json!({"call":"formatWithDiagnostics","source":source}));
        assert_eq!(formatted["ok"]["source"], source, "{source}");
    }
}

#[test]
fn bare_text_pairs_take_escaped_openers() {
    for (value, expected) in [
        ("~~s~~", "\\~\\~s~~\n"),
        ("~s~", "\\~s~\n"),
        ("=x=", "\\=x=\n"),
        ("_u_", "\\_u_\n"),
        ("**b**", "\\*\\*b**\n"),
        ("~é~", "\\~é~\n"),
        ("~a~\u{200d}b~", "\\~a~\u{200d}b~\n"),
    ] {
        let tree = json!({"type":"document","astVersion":"2.0","children":[
            {"type":"paragraph","children":[{"type":"text","value":value}]}]});
        let result = invoke(&json!({"call":"writeWithDiagnostics","tree":tree}));
        assert_eq!(result["ok"]["output"], expected, "{value}");
        assert_eq!(result["ok"]["diagnostics"], json!([]), "{value}");
        let lint = invoke(&json!({"call":"lint","source":expected}));
        assert!(
            lint["ok"]["diagnostics"]
                .as_array()
                .is_none_or(Vec::is_empty),
            "{value}: {lint}"
        );
    }
}

/// C2-W-156: the `deprecated-bare-*` rows run under `upgrade`.
fn upgrade() -> LintOptions {
    LintOptions {
        upgrade: true,
        ..LintOptions::default()
    }
}

#[test]
fn joined_candidates_cannot_open_or_close() {
    for mark in ['~', '_', '='] {
        for join in ['\u{301}', '\u{fe0f}', '\u{200d}'] {
            let opener = format!("x {mark}{join}a{mark} y\n");
            assert!(
                !carve::api::lint(opener.as_str(), &upgrade())
                    .unwrap()
                    .iter()
                    .any(|e| e.code.starts_with("deprecated-bare-")),
                "{opener}"
            );
            let closer = format!("{mark}a{mark}{join}b{mark}\n");
            let entries = carve::api::lint(closer.as_str(), &upgrade()).unwrap();
            assert!(
                entries
                    .iter()
                    .any(|e| e.code.starts_with("deprecated-bare-")),
                "{closer}"
            );
        }
    }
}

#[test]
fn escaped_first_mark_does_not_consume_a_later_pair() {
    let entries = carve::api::lint("\\~foo ~bar~", &upgrade()).unwrap();
    let pair = entries
        .iter()
        .find(|e| e.code == "deprecated-bare-strike")
        .unwrap();
    let range = pair.range.as_ref().unwrap();
    assert_eq!((range.start_offset, range.end_offset), (6, 11));
}

#[test]
fn legacy_pair_cannot_start_inside_a_longer_run() {
    let entries = carve::api::lint("~~a~", &upgrade()).unwrap();
    assert!(!entries.iter().any(|e| e.code == "deprecated-bare-strike"));
}
