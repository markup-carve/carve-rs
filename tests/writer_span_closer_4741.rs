use carve::api::{Carve, Options, WriteOptions};

#[test]
fn container_span_closer_leaves_harmless_following_text_bare() {
    for input in [
        "- a \\%%{,a\n  ,}`\n",
        "> a \\%%{,a\n> ,}`\n",
        "- a \\%%{^a\n  ^}*\n",
    ] {
        let tree = Carve::new().parse(input, &Options::default()).unwrap();
        let written = Carve::new()
            .write_with_diagnostics(&tree, &WriteOptions::default())
            .unwrap();
        assert_eq!(written.output, input, "{input:?}");
    }
}

#[test]
fn container_span_closer_variants() {
    let carve = Carve::new();
    let mut checked = 0;
    for (head, continuation) in [("-", "  "), (">", "> ")] {
        for lead in ["a", "ab", "word", "text", "longer"] {
            for width in 1..=10 {
                for extra in 0..10 {
                    let inside = "a".repeat(width + extra);
                    let after = if extra % 2 == 0 { '`' } else { '*' };
                    let input =
                        format!("{head} {lead} \\%%{{,{inside}\n{continuation},}}{after}\n");
                    let tree = carve.parse(&input, &Options::default()).unwrap();
                    let written = carve
                        .write_with_diagnostics(&tree, &WriteOptions::default())
                        .unwrap();
                    assert_eq!(written.output, input, "{input:?}");
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 1_000);
}
