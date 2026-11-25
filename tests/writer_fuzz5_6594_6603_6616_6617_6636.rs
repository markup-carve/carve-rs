use carve::api::{Carve, Options, WriteOptions};

fn semantic_tree(tree: &carve::api::Tree) -> serde_json::Value {
    fn strip(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                for key in ["pos", "srcByteLength", "stages"] {
                    map.remove(key);
                }
                for child in map.values_mut() {
                    strip(child);
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    strip(item);
                }
            }
            _ => {}
        }
    }
    let mut value = serde_json::from_str(&carve::api::encode_tree(tree)).unwrap();
    strip(&mut value);
    value
}

fn check_formats(cases: &[(&str, &str)]) {
    let carve = Carve::new();
    let opts = WriteOptions::default();
    for &(source, expected) in cases {
        let written = carve.format_with_diagnostics(source, &opts).unwrap();
        assert_eq!(written.output, expected, "{source:?}");
        assert!(written.diagnostics.is_empty(), "{source:?}");
        let tree = carve.parse(source, &Options::default()).unwrap();
        let back = carve.parse(expected, &Options::default()).unwrap();
        assert_eq!(semantic_tree(&back), semantic_tree(&tree), "{source:?}");
        let again = carve.format_with_diagnostics(expected, &opts).unwrap();
        assert_eq!(again.output, expected, "{source:?}");
    }
}

// carve#6594, C2-W-066: the opener of a table stands behind its attribute and comment lines,
// whatever column those start in; the source is a pipe table, so the output is one.
#[test]
fn pipe_table_below_shifted_attribute_line_stays_pipe() {
    check_formats(&[
        (" {.wide}\n| a | b |\n", "{.wide}\n| a | b |\n"),
        ("\t{s=50}\n| a | b | c |\n", "{s=50}\n| a | b | c |\n"),
        ("{.a}\n%% c\n| a | b |\n", "%% c\n\n{.a}\n| a | b |\n"),
        (
            "- x\n\n{footer-rows=0}\n%% #region s\n| b |\n",
            "- x\n\n{footer-rows=0}\n%% #region s\n| b |\n",
        ),
        ("- {s}\n > :\n", "- {s}\n  > :\n"),
    ]);
}

// carve#6617, C2-W-058: the quotes of an include option come from the include's own line,
// not from the attribute line above it.
#[test]
fn include_option_quotes_below_attribute_line() {
    check_formats(&[(
        "{.c}\n{{ v.crv @lines:\"1-2\" }}\n",
        "{.c}\n{{ v.crv @lines:\"1-2\" }}\n",
    )]);
}

// carve#6616, C2-B-010: the CR of a CRLF break inside a reference key is no content.
#[test]
fn crlf_break_in_reference_key() {
    check_formats(&[
        ("x [a][b\r\nc] y\r\n", "x [a][b\nc] y\n"),
        ("> x [a][b\r\n> c] y\r\n", "> x [a][b\n> c] y\n"),
    ]);
}

// carve#6603, C2-W-007, C2-L-020, C2-L-021: a continuation piece keeps its line whatever
// inline starts it or stands before it.
#[test]
fn continuation_piece_keeps_its_line() {
    check_formats(&[
        ("||\n+|---JSON c |\n", "| | |\n+ | ---JSON c |\n"),
        ("| a |\n+ | *b* c |\n", "| a | |\n+ | *b* c |\n"),
        (
            "| a |\n+ | b |\n+ | *c* |\n",
            "| a | |\n+ | b |\n+ | *c* |\n",
        ),
        (
            "| x a |\n+ | b *c* d |\n+ | e |\n",
            "| x a | |\n+ | b *c* d |\n+ | e |\n",
        ),
    ]);
}

// carve#6636, C2-B-017, C2-B-024: a caption line the quote does not own is a lazy line of an
// open stanza, not the quote's caption.
#[test]
fn lazy_caret_line_in_open_stanza() {
    let carve = Carve::new();
    let html = carve
        .render("> ::: |\n> a\n^ A\n", &Options::default())
        .unwrap();
    assert_eq!(
        html,
        "<blockquote>\n  <div class=\"line-block\">\n    <p>a<br>\n^ A</p>\n  </div>\n</blockquote>"
    );
    let html = carve.render("> a\n^ A\n", &Options::default()).unwrap();
    assert!(html.contains("<figcaption>A</figcaption>"), "{html}");
}
