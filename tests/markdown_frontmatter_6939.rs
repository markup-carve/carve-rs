//! carve#6939: a leading Markdown rule only becomes frontmatter for a mapping body.

use carve::api::json::call;
use serde_json::{Value, json};

fn import(from: &str, input: &str) -> Value {
    let reply: Value = serde_json::from_str(&call(
        &json!({"call": "importDocument", "from": from, "source": input}).to_string(),
    ))
    .unwrap();
    reply["ok"]
        .as_object()
        .unwrap_or_else(|| panic!("{from}: {input:?}: {reply}"));
    reply["ok"].clone()
}

#[test]
fn commonmark_and_gfm_examples_96_and_98() {
    let cases = [
        (
            "---\nFoo\n---\nBar\n---\nBaz\n",
            "***\n\n== Foo\n\n== Bar\n\nBaz\n",
            &["thematic_break", "heading", "heading", "paragraph"][..],
        ),
        (
            "---\n---\n",
            "***\n\n***\n",
            &["thematic_break", "thematic_break"][..],
        ),
    ];
    for from in ["markdown", "gfm"] {
        for (input, source, kinds) in cases {
            let result = import(from, input);
            assert_eq!(result["source"], source, "{from} {input:?}");
            let got: Vec<_> = result["tree"]["children"]
                .as_array()
                .unwrap()
                .iter()
                .map(|node| node["type"].as_str().unwrap())
                .collect();
            assert_eq!(got, kinds, "{from} {input:?}");
        }
    }
}

#[test]
fn leading_rule_mapping_shape_variants() {
    let openers = ["---", "--- ", "---\t", "***", "___"];
    let bodies = [
        ("Foo", false),
        ("title: Hi", true),
        ("# comment\ntitle: Hi", true),
        ("", false),
        ("# only", false),
        ("[table]", false),
        ("name = \"v\"", false),
        ("a:b", false),
        ("a:\tb", true),
        ("'quoted': v", true),
        ("- list", false),
        ("Baz", false),
    ];
    let endings = ["\n", "\r\n", "\r"];
    let closers = ["---", "--- ", "___"];
    let tails = ["", "\nBar\n---\nBaz\n"];
    let mut cases = 0;
    for from in ["markdown", "gfm"] {
        for opener in openers {
            for (body, mapping) in bodies {
                for nl in endings {
                    for closer in closers {
                        for tail in tails {
                            let input = format!("{opener}{nl}{body}{nl}{closer}{nl}{tail}");
                            let result = import(from, &input);
                            let first = result["tree"]["children"][0]["type"].as_str();
                            let frontmatter = opener.starts_with("---")
                                && mapping
                                && (closer.starts_with("---") || tail.contains("\n---\n"));
                            assert_eq!(
                                first,
                                Some(if frontmatter {
                                    "frontmatter"
                                } else {
                                    "thematic_break"
                                }),
                                "{from} {input:?}: {}",
                                result["source"]
                            );
                            if !frontmatter {
                                assert!(
                                    result["source"].as_str().unwrap().starts_with("***\n"),
                                    "{from} {input:?}: {}",
                                    result["source"]
                                );
                            }
                            cases += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 2160);
}

#[test]
fn toml_mapping_shape_still_keeps_tables_and_keys() {
    for from in ["markdown", "gfm"] {
        for (body, mapping) in [
            ("[site]", true),
            ("title = \"Hi\"", true),
            ("# note\ntitle = \"Hi\"", true),
            ("plain", false),
            ("", false),
            ("# note", false),
        ] {
            let input = format!("+++\n{body}\n+++\n");
            let result = import(from, &input);
            assert_eq!(
                result["tree"]["children"][0]["type"],
                if mapping { "frontmatter" } else { "paragraph" },
                "{from} {input:?}"
            );
        }
    }
}

#[test]
fn a_plain_yaml_key_with_a_colon_is_not_mapping_shape() {
    // The refined #2799 shape test excludes a plain key that holds any ':'.
    for from in ["markdown", "gfm"] {
        let result = import(from, "---\nfoo:bar: baz\n---\n");
        assert_eq!(result["tree"]["children"][0]["type"], "thematic_break");
    }
}
