//! carve#6827: a ranged comment reports a lost range only when an inline the
//! import keeps stands between its start and end (C2-M-113).

use carve::api::{self, ImportOptions};

fn import_with(html: &str, raw_html: bool) -> (String, Vec<String>) {
    let options = ImportOptions {
        raw_html,
        ..ImportOptions::default()
    };
    let got = api::import_document(html, &options).expect("import");
    let codes = got.diagnostics.iter().map(|d| d.code.clone()).collect();
    (got.source, codes)
}

fn import(html: &str) -> (String, Vec<String>) {
    import_with(html, false)
}

const START: &str = "<p><span class=\"comment-start\" data-comment-id=\"1\">n</span>";
const END: &str = "<span class=\"comment-end\" data-comment-id=\"1\"></span>y</p>";

fn ranged(inner: &str) -> String {
    format!("{START}{inner}{END}")
}

#[test]
fn dropped_script_leaves_no_inline() {
    let (src, codes) = import(&ranged("<script>x()</script>"));
    assert_eq!(src, "{#n#}y\n");
    assert_eq!(codes, ["import-element-dropped"]);
}

#[test]
fn empty_nested_comment_leaves_no_inline() {
    let (src, codes) = import(&ranged(concat!(
        "<span class=\"comment-start\" data-comment-id=\"2\"></span>",
        "<span class=\"comment-end\" data-comment-id=\"2\"></span>",
    )));
    assert_eq!(src, "{#n#}y\n");
    assert_eq!(codes, ["import-element-dropped"]);
}

#[test]
fn empty_inline_container_leaves_no_inline() {
    let (src, codes) = import(&ranged("<em></em>"));
    assert_eq!(src, "{#n#}y\n");
    assert!(codes.is_empty(), "{codes:?}");
}

#[test]
fn nested_comment_with_text_is_an_inline() {
    let (_, codes) = import(&ranged(concat!(
        "<span class=\"comment-start\" data-comment-id=\"2\">m</span>",
        "<span class=\"comment-end\" data-comment-id=\"2\"></span>",
    )));
    assert_eq!(codes, ["import-structure-repaired"]);
}

#[test]
fn kept_text_is_an_inline() {
    let (src, codes) = import(&ranged("<em>x</em>"));
    assert_eq!(src, "{#n#}{/x/}y\n");
    assert_eq!(codes, ["import-structure-repaired"]);
}

#[test]
fn kept_raw_element_is_an_inline() {
    let (_, codes) = import_with(&ranged("<script>x()</script>"), true);
    assert_eq!(codes, ["import-structure-repaired"]);
}

#[test]
fn kept_leaf_is_an_inline() {
    let (_, codes) = import(&ranged("<br>"));
    assert_eq!(codes, ["import-structure-repaired"]);
}
