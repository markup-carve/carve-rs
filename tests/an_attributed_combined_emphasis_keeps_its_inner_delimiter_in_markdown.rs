//! An attributed combined `/*b*/` writes its attributes on `<strong>` and its
//! inner emphasis as `*b*`, which reads back there (CARVE-P11-045,
//! markup-carve/carve-rs#3874).

fn md(src: &str) -> String {
    carve::to_markdown(src)
}

#[test]
fn inside_an_outer_emphasis() {
    assert_eq!(
        md("{/a /*b*/{.k}{#i}/}\n"),
        "*a <strong class=\"k\" id=\"i\">*b*</strong>*\n"
    );
}

#[test]
fn on_its_own() {
    assert_eq!(md("/*b c*/{.k}\n"), "<strong class=\"k\">*b c*</strong>\n");
}

#[test]
fn matches_the_authored_strong_around_an_emphasis() {
    assert_eq!(md("/*b*/{.k}\n"), md("*/b/*{.k}\n"));
}
