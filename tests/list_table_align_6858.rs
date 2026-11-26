//! C2-X-078 (carve#6858): `aligns`, `valigns` and `widths` of a list table
//! follow the pipe-table rules (C2-B-211, C2-B-212, C2-H-152).

use carve::api::extensions;
use carve::api::{Carve, Options};

fn html(source: &str) -> String {
    let mut carve = Carve::new();
    carve.register(extensions::list_table()).unwrap();
    carve.render(source, &Options::default()).unwrap()
}

#[test]
fn c2_b_211_valigns_baseline_is_invalid_on_a_pipe_table() {
    assert_eq!(
        html("{valigns=baseline}\n| a |\n"),
        "<table>\n  <tbody>\n    <tr><td>a</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn c2_h_152_widths_give_one_col_per_column() {
    let cols = |src: &str| {
        let out = html(src);
        out.lines()
            .filter(|l| l.trim_start().starts_with("<col"))
            .map(str::trim)
            .collect::<Vec<_>>()
            .join(" ")
    };
    let rows = "::: list-table\n- - a\n  - b\n:::\n";
    assert_eq!(
        cols(&format!("{{widths=25,25,50}}\n{rows}")),
        r#"<colgroup> <col style="width: 25%;"> <col style="width: 25%;">"#
    );
    assert_eq!(
        cols(&format!("{{widths=40}}\n{rows}")),
        r#"<colgroup> <col style="width: 40%;"> <col>"#
    );
    assert_eq!(
        cols(&format!("{{widths=\"30.50, 20\"}}\n{rows}")),
        r#"<colgroup> <col style="width: 30.5%;"> <col style="width: 20%;">"#
    );
    for bad in ["0,50", "101", "50,", "5.", ".5"] {
        assert_eq!(cols(&format!("{{widths=\"{bad}\"}}\n{rows}")), "", "{bad}");
    }
}
