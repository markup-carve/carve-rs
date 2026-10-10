#[test]
fn user_placeholders_survive_namespace_selection() {
    for base in [
        "\0DJOTSTRONG",
        "\0DJOTWORD",
        "\0DJOTORPHAN\0",
        "\0DJOTEMPTYTERM\0",
        "\0DJOTALT\0",
        "\0DJOTLITERAL\0",
    ] {
        let tokens = [
            format!("{base}0\0{}1\0", &base[1..]),
            format!("{base}00\0"),
            format!("{base}{}", "\0".repeat(32768)),
            (0..128).map(|n| format!("{base}{n}\0")).collect::<String>(),
        ];
        for token in tokens {
            let source = format!(
                "{token} w{{x}}{{.c}} ![*alt*](u)\n\na {{.o}} b\n\n{{.orphan}}\n\n: ```\n  payload\n  ```\n\n{{+unclosed\n"
            );
            let converted = carve::djot_to_carve(&source);
            assert!(converted.contains(&token));
            assert!(!converted.replace(&token, "").contains("\0DJOT"));
            let html = carve::to_html(&converted);
            assert!(html.contains("class=\"c\""));
            assert!(html.contains("alt=\"alt\""));
            assert!(html.contains("<dd>"));
            assert!(html.contains("a  b"));
            assert!(html.contains("{+unclosed"));
        }
    }
}

#[test]
fn frontmatter_keeps_user_placeholder_text() {
    let prefix = "---\nlabel: \0DJOTSTRONG0\0\n---\n\n";
    let converted = carve::djot_to_carve(&format!("{prefix}\0DJOTSTRONG0\0 w{{x}}{{.c}}"));
    assert!(converted.starts_with(prefix));
    assert!(converted.contains("\0DJOTSTRONG0\0"));
    assert!(carve::to_html(&converted).contains("class=\"c\""));
}

#[test]
fn user_markers_formed_by_orphan_removal_are_preserved() {
    for index in [0, 7] {
        let token = format!("\0DJOTSTRONG0\0{index}\0");
        let source = format!("\0{{.a}}DJOTSTRONG0\0{index}\0 w{{.c}}");
        assert_eq!(carve::djot_to_carve(&source), format!("{token} [w]{{.c}}"));
    }
}

#[test]
fn empty_term_user_marker_stays_in_image_alt_text() {
    let token = "\0DJOTEMPTYTERM\00\0";
    assert!(carve::djot_to_carve(&format!("![{token}](x)")).contains(token));
}

#[test]
fn user_markers_survive_flattened_emphasis() {
    for base in ["DJOTSTRONG0", "DJOTALT\00", "DJOTLITERAL\00", "DJOTUSERNUL"] {
        let source = format!("{{*a \0{{*{base}\00\0*}} b*}} w{{.c}}");
        let converted = carve::djot_to_carve(&source);
        assert!(converted.contains(&format!("\0{base}\00\0")));
        assert_eq!(carve::to_html(&converted).matches("class=\"c\"").count(), 1);
    }
}

#[test]
fn user_text_matching_the_nul_shield_is_preserved() {
    let token = "\0U\0";
    assert!(carve::djot_to_carve(&format!("{token} w{{.c}}")).contains(token));
}

#[test]
fn long_backslash_run_stays_inside_the_attributed_word() {
    let source = format!("a{}b{{.c}}", "\\".repeat(32768));
    let converted = carve::djot_to_carve(&source);
    let expected = format!("<p><span class=\"c\">a{}b</span></p>", "\\".repeat(16384));
    assert_eq!(carve::to_html(&converted).trim(), expected);
}

#[test]
fn nul_characters_in_imported_data_are_restored() {
    for source in [
        "# a\0b",
        "[a](x\0y)",
        "![a](x\0y)",
        "[a]{key=\"x\0y\"}",
        "<x:a\0b>",
    ] {
        assert_eq!(carve::djot_to_carve(source), source);
    }
}

#[test]
fn original_nul_is_restored_before_flattening_a_formatted_image_label() {
    assert_eq!(carve::djot_to_carve("![*a*\0](x)"), "![a\u{FFFD}](x)");
}

#[test]
fn escaped_nul_stays_inside_its_attributed_word() {
    let converted = carve::djot_to_carve("\\\0{.c}");
    assert!(carve::to_html(&converted).contains("<span class=\"c\">"));
}

#[test]
fn single_index_user_markers_survive_syntax_removal() {
    for base in [
        "DJOTINVALIDATTR0",
        "DJOTINVALIDATTR\00",
        "DJOTNOTEATTR0",
        "DJOTNOTEATTR\00",
    ] {
        let token = format!("\0{base}\0");
        for source in [
            format!("\0{{.a}}{base}\0 {{x y}}"),
            format!("{{*a \0{{*{base}\0*}} b*}} {{x y}}"),
        ] {
            let converted =
                carve::djot_to_carve(&format!("{source}\n\n[^n]: note\n\n  {{.c}}\n\n[^n]"));
            assert!(converted.contains(&token));
            assert!(!converted.replace(&token, "").contains("\0DJOT"));
        }
    }
}
