#[test]
fn formatting_preserves_rendered_content_and_is_idempotent() {
    for source in [
        "- - p `a\n > b` q\n",
        "- 1. p `a\n > b` q\n",
        "- p\n\n  - q `a\n > b` r\n",
        "r[^n]\n\n[^n]: p `a\n  [x]: y` q\n",
        "r[^n]\n\n[^n]: - p `a\n   > b` q\n",
        "> - - p `a\n>  > b` q\n",
        "- p `a\n > b` q\n",
        "- p `a\n >\nb` q\n",
        "- p `a\n [x]: y` q\n",
        "> - p `a\n>  > b` q\n",
        "`\n\t> x\n",
        "~``` x\n[d]: u ```\n",
        "`\n``\n",
        "`\n``x\n",
        "1. [d]: u\n",
        "- A\n{x}\n*[A]: }\n",
    ] {
        let formatted = carve::to_carve(source);
        assert_eq!(
            carve::to_html(&formatted),
            carve::to_html(source),
            "{source:?}"
        );
        assert_eq!(carve::to_carve(&formatted), formatted, "{source:?}");
    }
}

#[test]
fn a_sigil_after_emphasis_does_not_force_braces() {
    for source in ["~s~@t\n", "~s~#t\n", "~s~:name:\n"] {
        assert_eq!(carve::to_carve(source), source);
    }
}

#[test]
fn constructed_block_marker_code_refuses_in_terms_and_footnotes() {
    for source in [":: p `X` q\n", "r[^n]\n\n[^n]: p `X` q\n"] {
        for value in ["a\n> b", "a\n>\nb", "a\n[x]: y", "a\r> b"] {
            if source.starts_with("r[") && value.contains("[x]") {
                continue;
            }
            let json = carve::to_json(&carve::parse(source));
            let replacement = serde_json::to_string(value).unwrap();
            let doc = carve::from_json(&json.replace("\"X\"", &replacement)).unwrap();
            assert!(
                matches!(
                    carve::render_carve(&doc),
                    Err(carve::RenderCarveError::SourceUnspellable(_))
                ),
                "{source:?}: {value:?}"
            );
        }
    }
}

#[test]
fn a_constructed_carriage_return_is_normalized_before_guarding() {
    let json = carve::to_json(&carve::parse("`X`\n"));
    let doc = carve::from_json(&json.replace("\"X\"", r#""a\r> b""#)).unwrap();
    assert_eq!(carve::render_carve(&doc).unwrap(), "`a\n > b`\n");
}

#[test]
fn heading_suffixes_skip_ids_that_are_already_used() {
    for (source, ids) in [
        ("# A\n# A-2\n# A\n", vec!["A", "A-2", "A-3"]),
        (
            "# A\n# A\n# A-2\n# A-2\n",
            vec!["A", "A-2", "A-2-2", "A-2-3"],
        ),
        ("{#same}\n# A\n\n{#same}\n# B\n", vec!["same", "same"]),
    ] {
        let parsed = carve::parse(source);
        for html in [carve::to_html(source), carve::render_html(&parsed).unwrap()] {
            let actual: Vec<_> = html
                .split("<section id=\"")
                .skip(1)
                .map(|part| part.split('"').next().unwrap())
                .collect();
            assert_eq!(actual, ids, "{source:?}");
        }
    }
}
