use carve::api::json::call;
use serde_json::{Value, json};

fn inlines(values: &[&str]) -> Vec<Value> {
    values
        .iter()
        .map(|&v| match v {
            "\n" => json!({"type":"soft_break"}),
            v => json!({"type":"text","value":v}),
        })
        .collect()
}

fn written(block: &Value) -> String {
    let tree = json!({"type":"document","astVersion":"2.0","children":[block]});
    let out: Value = serde_json::from_str(&call(
        &json!({"call":"writeWithDiagnostics","tree":tree}).to_string(),
    ))
    .unwrap();
    assert_eq!(out["ok"]["diagnostics"], json!([]), "{tree}");
    out["ok"]["output"].as_str().unwrap().to_owned()
}

fn paragraph(values: &[&str]) -> String {
    written(&json!({"type":"paragraph","children":inlines(values)}))
}

#[test]
fn hyphen_runs_split_over_text_nodes_escape_the_second_hyphen() {
    for (values, expected) in [
        (&["a -", "-:"][..], "a -\\-:\n"),
        (&["-", "-"][..], "-\\-\n"),
        (&["-", "--"][..], "-\\--\n"),
        (&["--", "-"][..], "-\\--\n"),
        (&["a --:"][..], "a -\\-:\n"),
        (&["x", "\n", "-", "--"][..], "x\n-\\--\n"),
        (&["1", ". a"][..], "1\\. a\n"),
    ] {
        assert_eq!(paragraph(values), expected, "{values:?}");
    }
    let heading = json!({"type":"heading","level":2,"children":[
        {"type":"link","href":"u","children":inlines(&["a -", "-}"])}]});
    assert_eq!(written(&heading), "== [a -\\-}](u)\n");
}

#[test]
fn split_text_is_written_like_one_text_node() {
    for value in ["a --:", "---", "----", "x --- y", "$---", "a'-->:", "1. a"] {
        let whole = paragraph(&[value]);
        let cuts: Vec<usize> = (1..value.len()).collect();
        for &i in &cuts {
            assert_eq!(
                paragraph(&[&value[..i], &value[i..]]),
                whole,
                "{value:?} at {i}"
            );
            for &j in cuts.iter().filter(|&&j| j > i) {
                let parts = [&value[..i], &value[i..j], &value[j..]];
                assert_eq!(paragraph(&parts), whole, "{parts:?}");
            }
        }
    }
}
