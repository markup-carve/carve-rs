//! carve#6788: revision keys on insert, delete and editorial comment
//! (C2-I-063, C2-H-210, C2-M-044, C2-M-113), beyond the pinned clause cases.

use carve::api::{self, ImportOptions};

fn import(html: &str) -> (String, Vec<String>) {
    let got = api::import_document(html, &ImportOptions::default()).expect("import");
    let codes = got.diagnostics.iter().map(|d| d.code.clone()).collect();
    (got.source, codes)
}

fn html(src: &str) -> String {
    carve::render(src, &carve::Options::default())
}

#[test]
fn comment_takes_id_lang_and_keys() {
    assert_eq!(
        html("{#n#}{#i .k lang=de author=A} x\n"),
        "<p><span id=\"i\" class=\"critic-comment k\" lang=\"de\" data-author=\"A\">n</span> x</p>"
    );
}

#[test]
fn authored_data_datetime_wins_on_the_comment() {
    assert_eq!(
        html("{#n#}{data-datetime=y datetime=x}\n"),
        "<p><span class=\"critic-comment\" data-datetime=\"y\">n</span></p>"
    );
}

#[test]
fn upper_case_key_is_ordinary() {
    assert_eq!(
        html("{+x+}{Author=A}\n"),
        "<p><ins Author=\"A\">x</ins></p>"
    );
}

#[test]
fn import_keeps_data_author_beside_an_authored_author() {
    let (src, _) = import("<ins author=\"A\" data-author=\"B\">x</ins>");
    assert_eq!(src, "{+x+}{author=A data-author=B}\n");
}

#[test]
fn ranged_comment_without_end_loses_nothing() {
    let (src, codes) =
        import("<p>a <span class=\"comment-start\" data-comment-id=\"1\">n</span>b</p>");
    assert_eq!(src, "a {#n#}b\n");
    assert!(codes.is_empty(), "{codes:?}");
}

#[test]
fn ranged_comment_pairs_by_id_across_paragraphs() {
    let (src, codes) = import(concat!(
        "<p>a<span class=\"comment-start\" data-comment-id=\"1\">n</span></p>",
        "<p>b<span class=\"comment-end\" data-comment-id=\"2\">e</span>",
        "<span class=\"comment-end\" data-comment-id=\"1\"></span></p>",
    ));
    assert_eq!(src, "a{#n#}\n\nb[e]{.comment-end data-comment-id=2}\n");
    assert_eq!(codes, ["import-structure-repaired"]);
}

#[test]
fn ranged_comment_without_ids_keeps_the_end_span() {
    let (src, codes) = import(
        "<p>a <span class=\"comment-start\">n</span>b<span class=\"comment-end\">e</span></p>",
    );
    assert_eq!(src, "a {#n#}b[e]{.comment-end}\n");
    assert!(codes.is_empty(), "{codes:?}");
}
