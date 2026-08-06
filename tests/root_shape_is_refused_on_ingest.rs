//! PART 12 §12: an ingest refuses a root shape that deviates from §7.
//!
//! This engine was already conformant on all three rows the clause names, and
//! is the one the other two were measured against. Nothing pinned the two
//! missing-field rows, though: `tests/ast_json.rs` covers an unknown node type
//! and a foreign root, and the root's REQUIRED fields only through the encoder
//! side (`root_keys`). A regression that made either field optional would have
//! left every test in this repo green.
//!
//! Each payload is built by mutating a document this engine itself serialized,
//! so a refusal is about the mutation rather than about whatever else a
//! hand-written tree was missing.

/// Serialize `source`, then remove one root field by name.
///
/// String surgery rather than a JSON library: this crate deliberately has no
/// serde dependency, and the encoder's output shape is fixed enough that the
/// removal is exact - which the `is_ok` control below proves.
fn without_root_field(source: &str, field: &str) -> String {
    let json = carve::to_json(&carve::parse(source));
    let needle = format!("\"{field}\":");
    let start = json
        .find(&needle)
        .expect("field present in this engine's own output");
    // The value ends at the next top-level comma or at the closing brace. Every
    // root field but `children` is a scalar, and `children` is removed by
    // taking the balanced array that follows it.
    let after = start + needle.len();
    let rest = &json[after..];
    let end = if rest.starts_with('[') {
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        let mut index = 0usize;
        for (offset, ch) in rest.char_indices() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    in_string = false;
                }
                continue;
            }
            match ch {
                '"' => in_string = true,
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        index = offset + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        after + index
    } else {
        after + rest.find([',', '}']).expect("scalar value ends")
    };
    let mut out = String::with_capacity(json.len());
    out.push_str(&json[..start]);
    out.push_str(&json[end..]);
    // One separator is now stray, whichever side the removed field sat on.
    out.replace(",,", ",").replace("{,", "{").replace(",}", "}")
}

#[test]
fn its_own_output_still_decodes() {
    // §9(a), and the control on everything below: without it, every assertion
    // here is satisfied by a decoder that refuses outright.
    let json = carve::to_json(&carve::parse("hi"));
    assert!(carve::from_json(&json).is_ok());
    // And the surgery above really does produce valid JSON when nothing is
    // asked of it that the decoder would reject for a second reason.
    let stripped = without_root_field("hi", "srcByteLength");
    assert!(
        stripped.starts_with('{') && stripped.ends_with('}'),
        "{stripped}"
    );
    assert!(!stripped.contains("srcByteLength"), "{stripped}");
    assert!(stripped.contains("\"children\""), "{stripped}");
}

#[test]
fn a_root_with_no_src_byte_length_is_refused_and_the_field_is_named() {
    let err = carve::from_json(&without_root_field("hi", "srcByteLength"))
        .expect_err("§12(a): a root missing srcByteLength is refused");
    // The MESSAGE, not just the Err: this decoder refuses malformed input for
    // several reasons, so `is_err` alone would pass on the wrong one.
    assert!(err.to_string().contains("srcByteLength"), "{err}");
}

#[test]
fn a_root_with_no_children_is_refused_and_the_field_is_named() {
    let err = carve::from_json(&without_root_field("hi", "children"))
        .expect_err("§12(a): a root missing children is refused");
    assert!(err.to_string().contains("children"), "{err}");
}

#[test]
fn a_root_with_no_type_is_refused() {
    let err = carve::from_json(&without_root_field("hi", "type"))
        .expect_err("§12(a): a root missing type is refused");
    assert!(err.to_string().contains("type"), "{err}");
}

#[test]
fn the_value_of_src_byte_length_is_not_checked() {
    // §12(a) is about PRESENCE. The value is derivable and nothing depends on
    // it, so all three engines ignore it - deliberately, not by oversight.
    let json = carve::to_json(&carve::parse("hi"))
        .replace("\"srcByteLength\":2", "\"srcByteLength\":99999");
    assert!(
        json.contains("99999"),
        "the fixture failed to substitute: {json}"
    );
    assert!(carve::from_json(&json).is_ok());
}

#[test]
fn an_unknown_node_type_is_refused_at_decode_nested_as_well_as_at_the_top() {
    // §12(c). `tests/ast_json.rs` covers the block case; an engine can turn a
    // foreign BLOCK away at the top of its child loop and still walk a foreign
    // INLINE into the tree, which is what carve-js did.
    let block = r#"{"type":"document","srcByteLength":0,"children":[{"type":"zzNotInTheSchema"}]}"#;
    let inline = r#"{"type":"document","srcByteLength":0,"children":[{"type":"paragraph","children":[{"type":"zzNotInTheSchema"}]}]}"#;
    for json in [block, inline] {
        let err = carve::from_json(json).expect_err("§12(c): refused at decode");
        assert!(err.to_string().contains("zzNotInTheSchema"), "{err}");
    }
}

#[test]
fn an_unexpected_root_field_is_refused() {
    // §12(b), which §11 already covers. Pinned so the root stays covered by it.
    let json = carve::to_json(&carve::parse("hi")).replace(
        "{\"type\":\"document\"",
        "{\"zzRootFieldNotInTheSchema\":1,\"type\":\"document\"",
    );
    let err = carve::from_json(&json).expect_err("§12(b): refused");
    assert!(
        err.to_string().contains("zzRootFieldNotInTheSchema"),
        "{err}"
    );
}

#[test]
fn an_attribute_named_type_still_decodes() {
    // THE TRAP under §12(c). Attribute names are ordinary identifiers and
    // `type` is one, so this document puts an object literally shaped
    // {"type":"widget"} in the tree. An unknown-type check that walked every
    // object would refuse a document this build just parsed, which §9(a)
    // forbids. This decoder reads field by field and is not exposed to it - the
    // row is here so a future rewrite to a generic walk is caught.
    let json = carve::to_json(&carve::parse("[x](/u){type=widget}"));
    assert!(
        json.contains("\"keyValues\":{\"type\":\"widget\"}"),
        "{json}"
    );
    assert!(carve::from_json(&json).is_ok());
}
