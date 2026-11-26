//! C2-X-078 (carve#6835): an empty value of `header-rows`, `footer-rows` or
//! `header-cols` has no effect, and a row item that holds anything beside its
//! one list leaves the div a core div.

use carve::api::extensions;
use carve::api::{Carve, Options};
use std::fmt::Write as _;

fn html(source: &str) -> String {
    let mut carve = Carve::new();
    carve.register(extensions::list_table()).unwrap();
    carve.render(source, &Options::default()).unwrap()
}

const ROWS: &str = "- - a\n  - b\n- - c\n  - d\n";
const PLAIN: &str = "<table>\n  <tbody>\n    <tr><td>a</td><td>b</td></tr>\n    <tr><td>c</td><td>d</td></tr>\n  </tbody>\n</table>";

#[test]
fn c2_x_078_an_empty_or_invalid_count_has_no_effect() {
    for key in ["header-rows", "footer-rows", "header-cols"] {
        for value in [
            String::new(),
            "=\"\"".into(),
            "=\" \"".into(),
            "=0".into(),
            "=-1".into(),
            "=abc".into(),
            "=1.5".into(),
        ] {
            let src = format!("{{{key}{value}}}\n::: list-table\n{ROWS}:::\n");
            assert_eq!(html(&src), PLAIN, "{src}");
        }
    }
}

#[test]
fn c2_x_078_an_empty_key_beside_a_valid_one() {
    let head = "<table>\n  <thead>\n    <tr><th scope=\"col\">a</th><th scope=\"col\">b</th></tr>\n  </thead>\n  <tbody>\n    <tr><td>c</td><td>d</td></tr>\n  </tbody>\n</table>";
    for attrs in [
        "header-rows=1 header-cols",
        "header-cols header-rows=1",
        "header-rows=1 footer-rows=\"\"",
    ] {
        let src = format!("{{{attrs}}}\n::: list-table\n{ROWS}:::\n");
        assert_eq!(html(&src), head, "{src}");
    }
    let foot = "<table>\n  <tbody>\n    <tr><th scope=\"row\">a</th><td>b</td></tr>\n  </tbody>\n  <tfoot>\n    <tr><th scope=\"row\">c</th><td>d</td></tr>\n  </tfoot>\n</table>";
    assert_eq!(
        html(&format!(
            "{{header-rows footer-rows=1 header-cols=1}}\n::: list-table\n{ROWS}:::\n"
        )),
        foot
    );
}

#[test]
fn c2_x_078_a_row_item_with_more_than_its_list_is_no_row() {
    for extra in [
        "\n  extra\n",
        "\n  1. e\n",
        "\n  * e\n",
        "\n  > e\n",
        "\n  ``` \n  e\n  ```\n",
    ] {
        for row in 0..3 {
            let mut body = String::new();
            for r in 0..3 {
                let _ = write!(body, "- - a{r}\n  - b{r}\n");
                if r == row {
                    body.push_str(extra);
                }
            }
            let src = format!("::: list-table\n{body}:::\n");
            let out = html(&src);
            assert!(
                out.starts_with("<div class=\"list-table\">"),
                "{src}\n{out}"
            );
            assert!(!out.contains("<table"), "{src}\n{out}");
        }
    }
}

#[test]
fn c2_x_078_a_block_before_the_list_is_no_row() {
    let out = html("::: list-table\n- - a\n- para\n\n  - b\n:::\n");
    assert!(out.starts_with("<div class=\"list-table\">"), "{out}");
}

#[test]
fn c2_x_078_blocks_inside_a_cell_keep_the_table() {
    assert_eq!(
        html("::: list-table\n- - a\n\n    extra\n  - b\n:::\n"),
        "<table>\n  <tbody>\n    <tr><td><p>a</p>\n<p>extra</p></td><td>b</td></tr>\n  </tbody>\n</table>"
    );
    assert_eq!(
        html("::: list-table\n1. - a\n2. - b\n:::\n"),
        "<table>\n  <tbody>\n    <tr><td>a</td></tr>\n    <tr><td>b</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn c2_x_078_a_nested_list_table_follows_the_same_rule() {
    let out = html("> ::: list-table\n> - - a\n>\n>   extra\n> :::\n");
    assert!(
        out.contains("<div class=\"list-table\">") && !out.contains("<table"),
        "{out}"
    );
    let out = html("- item\n\n  ::: list-table\n  - - a\n  :::\n");
    assert!(out.contains("<table>"), "{out}");
}
