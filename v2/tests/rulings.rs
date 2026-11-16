use carve2::{Options, render};

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
