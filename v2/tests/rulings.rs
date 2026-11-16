use carve2::{Options, render};

fn tree(src: &str) -> serde_json::Value {
    let arena = carve2::Arena::new();
    let document = carve2::parse_resolved_in(src, &Options::default(), &arena);
    serde_json::from_str(&carve2::to_json(&document)).unwrap()
}

#[test]
fn c2_b_105_admonition_glued_attrs_are_reported_and_dropped() {
    assert_eq!(
        render("::: note{.x}\nn\n:::", &Options::default()),
        "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p>n</p>\n</aside>"
    );
}

#[test]
fn c2_b_119_title_and_label_need_space_behind_div_run() {
    assert_eq!(render(":::[x] y", &Options::default()), "<p>:::[x] y</p>");
    assert!(render("::: [x]\ny\n:::", &Options::default()).contains("class=\"div-label\""));
}

#[test]
fn c2_b_205_shift_has_at_most_nine_digits() {
    assert_eq!(
        render(
            "{{ a.crv @shift:-123456789 }}\n\n{{ a.crv @shift:1234567890 }}",
            &Options::default()
        ),
        "<p>{{ a.crv @shift:-123456789 }}</p>\n<p>{{ a.crv <span class=\"mention\"><strong>@shift</strong></span>:1234567890 }}</p>"
    );
}

#[test]
fn c2_i_117_closing_mark_is_not_a_plus_minus_neighbor() {
    assert_eq!(
        render("{++-+} and {++-1+} and a{%x%}+-b", &Options::default()),
        "<p><ins>±</ins> and <ins>±1</ins> and a±b</p>"
    );
    assert_eq!(render("+-{%x%}-", &Options::default()), "<p>+--</p>");
}

#[test]
fn c2_b_211_invalid_column_lists_have_no_effect_and_stay_in_attrs() {
    let src = "{aligns=right,x widths=25,x}\n| a | b |";
    let t = tree(src);
    let table = &t["children"][0];
    assert_eq!(
        table["attrs"]["keyValues"],
        serde_json::json!([["aligns", "right,x"], ["widths", "25,x"]])
    );
    assert!(
        table["columns"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c.get("align").is_none() && c.get("width").is_none())
    );
    assert!(!render(src, &Options::default()).contains("<colgroup>"));
}

#[test]
fn c2_b_227_invalid_and_inapplicable_counts_stay_in_attrs() {
    let src = "{header-rows=2 footer-rows=x}\n| a | b |\n| c | d |";
    let t = tree(src);
    assert_eq!(
        t["children"][0]["attrs"]["keyValues"],
        serde_json::json!([["header-rows", "2"], ["footer-rows", "x"]])
    );
    let html = render(src, &Options::default());
    assert!(!html.contains("header-rows=") && !html.contains("footer-rows="));
}

#[test]
fn c2_b_211_short_valid_list_applies_and_invalid_tail_cancels_all() {
    let valid = tree("{aligns=right widths=25}\n| a | b |");
    let columns = &valid["children"][0]["columns"];
    assert_eq!(columns[0]["align"], "right");
    assert_eq!(columns[0]["width"], 0.25);
    assert!(columns[1].get("align").is_none());
    assert!(valid["children"][0].get("attrs").is_none());

    let invalid = tree("{aligns=right,left,x widths=25,50,0}\n| a | b |");
    let columns = invalid["children"][0]["columns"].as_array().unwrap();
    assert!(
        columns
            .iter()
            .all(|c| c.get("align").is_none() && c.get("width").is_none())
    );
}

#[test]
fn c2_i_084_heading_lookup_reads_markup_in_the_key() {
    let src = "# A bold title\n\n[A *bold* title][] and [x][A {*bold*} title]";
    let html = render(src, &Options::default());
    assert!(html.contains("<a href=\"#A-bold-title\">A <strong>bold</strong> title</a>"));
    assert!(html.contains("<a href=\"#A-bold-title\">x</a>"));
    let t = tree(src);
    assert!(t.to_string().contains("#A-bold-title"));
}

#[test]
fn c2_h_020_attributed_block_image_keeps_paragraph() {
    let src = "{.c #i}\n![a](x.png)\n\n{:de}\n![b](y.png)\n\n![c](z.png)";
    assert_eq!(
        render(src, &Options::default()),
        "<p id=\"i\" class=\"c\"><img src=\"x.png\" alt=\"a\"></p>\n<p lang=\"de\"><img src=\"y.png\" alt=\"b\"></p>\n<img src=\"z.png\" alt=\"c\">"
    );
}

#[test]
fn c2_h_064_table_panel_has_figure_wrapper() {
    let src = ":::: figure\n![a](a.png)\n^ First\n\n| t |\n^ Second\n::::";
    let html = render(src, &Options::default());
    assert!(html.contains("  <figure class=\"carve-figure-panel\">\n    <table>"));
    assert!(html.contains("      <caption>Second</caption>"));
    assert!(html.contains("    </table>\n  </figure>"));
}

#[test]
fn c2_h_165_degraded_table_keeps_caption_number() {
    let mut options = Options::default();
    options.profile = carve2::options::Profile::Comment;
    let src = "| t |\n^ Table #: tc";
    assert_eq!(render(src, &options), "<p>t</p>\n<p>Table 1: tc</p>");
}
