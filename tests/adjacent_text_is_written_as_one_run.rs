//! PART 11 §2: adjacent text nodes are written as one run, so where a tree
//! splits text cannot decide which character carries an escape. The same
//! shapes are pinned in carve-js and carve-php.

use carve::{from_json, render_carve};

fn written(inlines: &str) -> String {
    let json = format!(
        r#"{{"type":"document","children":[{{"type":"paragraph","children":[{inlines}]}}],"srcByteLength":0}}"#
    );
    render_carve(&from_json(&json).expect("decode AST")).expect("write")
}

#[test]
fn a_split_opener_is_escaped_at_its_opener() {
    for (inlines, expected) in [
        (
            r#"{"type":"text","value":"x (r"},{"type":"text","value":") y"}"#,
            "x \\(r) y\n",
        ),
        (
            r#"{"type":"text","value":"x "},{"type":"text","value":"("},{"type":"text","value":"r"},{"type":"text","value":")"},{"type":"text","value":" y"}"#,
            "x \\(r) y\n",
        ),
        (
            r#"{"type":"text","value":"x -"},{"type":"text","value":"- y"}"#,
            "x \\-\\- y\n",
        ),
    ] {
        assert_eq!(written(inlines), expected, "{inlines}");
        let whole = inlines.replace(r#""},{"type":"text","value":""#, "");
        assert_eq!(written(&whole), expected, "{whole}");
    }
}

#[test]
fn a_flattened_ruby_is_written_as_the_text_it_flattens_to() {
    let ruby = r#"{"type":"ruby","pairs":[{"base":[{"type":"text","value":"["}],"annotation":[{"type":"text","value":"r"}]}]}"#;
    assert_eq!(
        written(&format!(
            r#"{{"type":"span","attrs":{{"classes":["c"]}},"children":[{ruby}]}}"#
        )),
        "[\\[\\(r)]{.c}\n"
    );
    assert_eq!(
        written(&format!(
            r#"{{"type":"span","attrs":{{"classes":["c"]}},"children":[{{"type":"emphasis","children":[{ruby}]}}]}}"#
        )),
        "[/\\[\\(r)/]{.c}\n"
    );
}
