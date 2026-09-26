//! PART 11 §5's lone-bracket scan reads through the inline nodes that write no
//! brackets of their own, as carve-js does: small caps without attributes,
//! critic insert, delete and substitution, abbreviation text, and ruby written
//! flattened. Small caps with attributes are a bracketed run of their own.
//! Every expected line is carve-js's output for the same tree.

use carve::{from_json, render_carve};

fn written(inline: &str) -> String {
    let json = format!(
        r#"{{"type":"document","children":[{{"type":"paragraph","children":[{{"type":"span","attrs":{{"classes":["c"]}},"children":[{inline}]}}]}}],"srcByteLength":0}}"#
    );
    render_carve(&from_json(&json).expect("decode AST")).expect("write")
}

#[test]
fn every_transparent_node_is_scanned() {
    for (inline, expected) in [
        (
            r#"{"type":"small_caps","children":[{"type":"text","value":"a ["}]},{"type":"text","value":" b"}"#,
            "[a \\[ b]{.c}\n",
        ),
        (
            r#"{"type":"small_caps","attrs":{"classes":["sc"]},"children":[{"type":"text","value":"["}]}"#,
            "[[\\[]{.sc}]{.c}\n",
        ),
        (
            r#"{"type":"substitution","old":[{"type":"text","value":"["}],"new":[{"type":"text","value":"x"}]}"#,
            "[{~\\[~>x~}]{.c}\n",
        ),
        (
            r#"{"type":"insert","children":[{"type":"text","value":"["}]}"#,
            "[{+\\[+}]{.c}\n",
        ),
        (
            r#"{"type":"delete","children":[{"type":"text","value":"a ]"}]}"#,
            "[{-a \\]-}]{.c}\n",
        ),
        (
            r#"{"type":"abbreviation","abbr":"[a","expansion":"x"}"#,
            "[\\[a]{.c}\n",
        ),
    ] {
        assert_eq!(written(inline), expected, "{inline}");
    }
}
