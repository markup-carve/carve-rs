use carve::{to_carve, to_html};

#[test]
fn structural_characters_require_quoted_values() {
    for value in [r"a|b", r"a\b", r"a\}b", r#"a"b"c"#, "a'b'c", r"a\"] {
        for source in [
            format!("*x*{{k={value}}}\n"),
            format!("{{k={value}}}\n\nx\n"),
        ] {
            let html = to_html(&source);
            assert!(!html.contains(" k="), "{source:?}: {html}");
            assert!(html.contains("{k="), "{source:?}: {html}");
        }
    }
}

#[test]
fn only_ascii_separators_end_unquoted_values() {
    for ch in [
        '\u{b}', '\u{c}', '\u{85}', '\u{a0}', '\u{1680}', '\u{2000}', '\u{2001}', '\u{2002}',
        '\u{2003}', '\u{2004}', '\u{2005}', '\u{2006}', '\u{2007}', '\u{2008}', '\u{2009}',
        '\u{200a}', '\u{2028}', '\u{2029}', '\u{202f}', '\u{205f}', '\u{3000}',
    ] {
        for value in [format!("a{ch}b"), format!("{ch}b"), ch.to_string()] {
            for (source, quoted) in [
                (
                    format!("*x*{{k={value}}}\n"),
                    format!("*x*{{k=\"{value}\"}}\n"),
                ),
                (
                    format!("{{k={value}}}\n\nx\n"),
                    format!("{{k=\"{value}\"}}\n\nx\n"),
                ),
            ] {
                let html = to_html(&source);
                assert!(html.contains(" k="), "{source:?}: {html}");
                assert_eq!(html, to_html(&quoted), "{source:?}");
                assert_eq!(html, to_html(&to_carve(&source)), "{source:?}");
            }
        }
    }
}

#[test]
fn other_punctuation_remains_valid_without_quotes() {
    for value in [
        "w-1/2", "a=b", "a{b", "v1.2", "xml:lang", "a@b", "a+b", "你好",
    ] {
        let source = format!("*x*{{k={value}}}\n");
        assert_eq!(
            to_html(&source),
            format!("<p><strong k=\"{value}\">x</strong></p>")
        );
    }
}

#[test]
fn quoted_values_must_end_at_a_separator_or_closing_brace() {
    for value in [r#""a"b"c""#, r#""a"b"#, "'a'b'c'", r#""a""b""#] {
        let source = format!("*x*{{k={value}}}\n");
        assert!(!to_html(&source).contains(" k="), "{source:?}");
    }
}

#[test]
fn quoted_escapes_survive_formatting_in_inline_block_and_table_attributes() {
    for value in [
        r#""a\\b""#,
        r#""a\"b""#,
        r#"'a\'b'"#,
        r#""a\|b""#,
        r#""a\}b""#,
    ] {
        for source in [
            format!("*x*{{k={value}}}\n"),
            format!("{{k={value}}}\n\nx\n"),
            format!("| *x*{{k={value}}} | y |\n"),
        ] {
            let formatted = to_carve(&source);
            assert!(to_html(&source).contains(" k="), "{source:?}");
            assert_eq!(
                to_html(&source),
                to_html(&formatted),
                "{source:?}: {formatted:?}"
            );
            assert_eq!(formatted, to_carve(&formatted), "{source:?}");
        }
    }
}
