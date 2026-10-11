use carve::{parse, to_carve, to_html, to_json};
use serde_json::Value;

fn semantic(source: &str) -> Value {
    fn strip_positions(value: &mut Value) {
        match value {
            Value::Object(fields) => {
                fields.remove("pos");
                fields.remove("srcByteLength");
                for value in fields.values_mut() {
                    strip_positions(value);
                }
            }
            Value::Array(values) => values.iter_mut().for_each(strip_positions),
            _ => {}
        }
    }

    let mut tree = serde_json::from_str(&to_json(&parse(source))).unwrap();
    strip_positions(&mut tree);
    tree
}

fn assert_keeps(source: &str) {
    let written = to_carve(source);
    assert_eq!(written, source);
    assert_eq!(semantic(&written), semantic(source));
    assert_eq!(to_html(&written), to_html(source));
    assert_eq!(to_carve(&written), written);
}

macro_rules! delimiter_cases {
    ($($name:ident: $delimiter:expr,)*) => {
        $(mod $name {
            use super::*;

            #[test]
            fn only_an_escaped_space() {
                assert_keeps(&format!("{{{}\\ {}}}\n", $delimiter, $delimiter));
            }

            #[test]
            fn text_before_the_space() {
                assert_keeps(&format!("{{{}x\\ {}}}\n", $delimiter, $delimiter));
            }

            #[test]
            fn text_around_the_span() {
                assert_keeps(&format!("a {{{}b\\ {}}} c\n", $delimiter, $delimiter));
            }

            #[test]
            fn an_authored_private_use_character_moves_the_sentinel() {
                assert_keeps(&format!("\u{e010} {{{}x\\ {}}}\n", $delimiter, $delimiter));
            }

            #[test]
            fn text_after_the_escape_keeps_the_bare_spelling() {
                for content in ["\\ x", "\\!"] {
                    assert_eq!(
                        to_carve(&format!("{{{}{content}{}}}\n", $delimiter, $delimiter)),
                        format!("{}{content}{}\n", $delimiter, $delimiter),
                    );
                }
            }

            #[test]
            fn an_authored_private_use_character_keeps_the_bare_spelling() {
                assert_eq!(
                    to_carve(&format!("{{{}x\u{e010}{}}}\n", $delimiter, $delimiter)),
                    format!("{}x\u{e010}{}\n", $delimiter, $delimiter),
                );
            }
        })*
    };
}

delimiter_cases! {
    strong: "*",
    emphasis: "/",
    underline: "_",
    strike: "~",
    highlight: "=",
}

#[test]
fn an_insertion_keeps_its_braces() {
    assert_keeps("{+\\ +}\n");
}

#[test]
fn a_deletion_keeps_its_braces() {
    assert_keeps("{-\\ -}\n");
}
