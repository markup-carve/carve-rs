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
