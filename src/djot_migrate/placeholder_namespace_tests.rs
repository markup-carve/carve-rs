#[test]
fn reserves_exact_namespaces_without_growing_the_prefix() {
    for base in [
        "\0DJOTSTRONG",
        "\0DJOTLITERAL\0",
        "\0DJOTORPHAN\0",
        "\0DJOTEMPTYTERM\0",
        "\0DJOTALT\0",
    ] {
        let cases = [
            (format!("{base}0\0{}1\0", &base[1..]), 2),
            (format!("{base}00\0"), 0),
            (format!("{base}{}", "\0".repeat(32768)), 0),
            (
                (0..128).map(|n| format!("{base}{n}\0")).collect::<String>(),
                128,
            ),
        ];
        for (source, namespace) in cases {
            assert_eq!(
                super::djot_placeholder_prefix(&source, base),
                format!("{base}{namespace}\0")
            );
        }
    }
}
