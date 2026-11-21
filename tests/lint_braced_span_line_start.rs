//! C2-B-090: a line that opens with a braced inline form is no failed attribute line, so
//! `block-marker-as-text` does not report it.

use carve::lint::{LintOptions, lint};

fn reports(source: &str) -> usize {
    lint(source, &LintOptions::default())
        .into_iter()
        .filter(|finding| finding.code == "block-marker-as-text")
        .count()
}

const SPANS: [&str; 7] = [
    "{_underline_}",
    "{_a b_}",
    "{_{*a*}_}",
    "{_x_}{.c}",
    "{~x~}",
    "{=x=}",
    "{+x+}",
];

#[test]
fn a_leading_braced_span_is_not_reported() {
    for host in ["", "> ", "- ", "1. ", "a\n", "> a\n> ", ":: t\n: "] {
        for span in SPANS {
            for rest in ["", " text", " {_y_}"] {
                let source = format!("{host}{span}{rest}\n");
                assert_eq!(reports(&source), 0, "{source:?}");
            }
        }
    }
}

#[test]
fn an_attribute_line_shape_that_did_not_open_is_still_reported() {
    for host in ["", "a\n"] {
        for shape in ["{#id", "{.x", "{_x", "{key=v", "{_x_ y", "{a="] {
            let source = format!("{host}{shape} {{_y_}}\n");
            assert_eq!(reports(&source), 1, "{source:?}");
        }
    }
}
