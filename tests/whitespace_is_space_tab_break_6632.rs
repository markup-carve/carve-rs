//! carve#6632, carve#6634: whitespace is a space, a tab or a line break; a vertical tab or U+2028 is an ordinary character of a column value or a caption label.

use carve::api::{Carve, Options};

fn html(source: &str) -> String {
    Carve::new()
        .render(source, &Options::default())
        .unwrap()
        .trim_end()
        .to_owned()
}

#[test]
fn a_vertical_tab_is_part_of_a_column_value() {
    let out = html("{aligns=\u{b}left}\n|a|\n");
    assert!(!out.contains("text-align"), "{out}");
}

#[test]
fn a_line_separator_stays_in_the_caption_label() {
    let out = html("![](png)\n^ Figure #: Y\n::: figure\n:::\n^ Figure\u{2028} #: Both\n");
    assert!(out.contains("Figure\u{2028} 1: Both"), "{out}");
}
