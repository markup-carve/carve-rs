//! carve-rs#4918: the unclosed `[` of text before a span fills the reader's stack too, so past the C2-I-118 cap
//! the `[` in the span's content is text and takes no backslash (C2-W-088).
use carve::api::{self, Carve, WriteOptions};
use carve::options::Options;

fn write(src: &str, normalize: bool) -> String {
    let parsed = api::parse_with_layout(src, &Options::default()).unwrap();
    let opts = WriteOptions {
        layout: Some(parsed.layout),
        normalize,
        ..WriteOptions::default()
    };
    Carve::new().write(&parsed.tree, &opts).unwrap()
}

fn plain(src: &str) -> String {
    Carve::new()
        .format_with_diagnostics(src, &WriteOptions::default())
        .unwrap()
        .output
}

#[test]
fn span_bracket_past_the_cap_stays_bare() {
    for n in [199, 200, 206, 230, 260] {
        for tail in ["[x][][[]()[[]{.c}[", "[[]{.c}", "[a[]{.c}"] {
            for lead in ["", "a "] {
                let src = format!("{lead}{}{tail}\n", "[".repeat(n));
                assert_eq!(plain(&src), src, "{n} {lead:?} {tail}");
                assert_eq!(write(&src, false), src, "{n} {lead:?} {tail}");
                assert_eq!(write(&src, true), src, "{n} {lead:?} {tail} normalize");
            }
        }
    }
}

#[test]
fn span_bracket_below_the_cap_keeps_its_backslash() {
    for src in [
        format!("{}[\\[]{{.c}}\n", "[".repeat(197)),
        format!("{}[\\[]{{.c}}\n", "[a] ".repeat(150)),
    ] {
        assert_eq!(plain(&src), src);
        assert_eq!(write(&src, true), src);
    }
}
