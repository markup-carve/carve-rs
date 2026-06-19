//! Links never nest. A link's label is inline-parsed, so a link, a core
//! `<url>` autolink, or a bare-URL autolink (extension) can appear inside the
//! label. The inner link is replaced by its text content; only the outermost
//! link's destination applies. Golden strings match carve-js.

use carve::{Autolink, Options};

#[test]
fn explicit_nested_brackets_unwrap_inner_link() {
    // [[x](y)](z) -> only the outer destination survives.
    assert_eq!(carve::to_html("[[x](y)](z)"), "<p><a href=\"z\">x</a></p>");
}

#[test]
fn core_angle_autolink_in_label_becomes_text() {
    // [pre <http://h> post](/u)
    assert_eq!(
        carve::to_html("[pre <http://h> post](/u)"),
        "<p><a href=\"/u\">pre http://h post</a></p>"
    );
}

#[test]
fn mailto_autolink_in_label_strips_scheme() {
    // [mail <a@b.com> here](/u) -> display text without the mailto: scheme.
    assert_eq!(
        carve::to_html("[mail <a@b.com> here](/u)"),
        "<p><a href=\"/u\">mail a@b.com here</a></p>"
    );
}

#[test]
fn literal_brackets_in_label_unchanged() {
    // [a [b] c](/u) -> literal brackets, no inner link.
    assert_eq!(
        carve::to_html("[a [b] c](/u)"),
        "<p><a href=\"/u\">a [b] c</a></p>"
    );
}

#[test]
fn bare_url_autolink_extension_in_label_becomes_text() {
    let ext = Autolink::new();
    let opts = Options::new().with_extension(&ext);
    // [https://x.com](https://y.com) with the autolink extension.
    assert_eq!(
        carve::to_html_with_options("[https://x.com](https://y.com)", &opts),
        "<p><a href=\"https://y.com\">https://x.com</a></p>"
    );
}

#[test]
fn top_level_autolink_extension_still_links() {
    let ext = Autolink::new();
    let opts = Options::new().with_extension(&ext);
    // A top-level autolink (not inside a label) is unchanged: it still links.
    assert_eq!(
        carve::to_html_with_options("plain https://x.com here", &opts),
        "<p>plain <a href=\"https://x.com\">https://x.com</a> here</p>"
    );
}

#[test]
fn unresolved_reference_link_in_label_keeps_literal_source() {
    // [[x][missing]](/z): the inner reference link has no matching definition,
    // so it never resolves and reverts to its literal source rather than its
    // parsed children. Only the outer destination links.
    assert_eq!(
        carve::to_html("[[x][missing]](/z)"),
        "<p><a href=\"/z\">[x][missing]</a></p>"
    );
}

#[test]
fn link_in_footnote_body_in_label_survives() {
    // [x ^[see [y](/inner)]](/outer): a footnote body renders in the endnotes
    // section, outside the outer anchor, so the link inside the footnote is not
    // a nested anchor and must survive untouched.
    let html = carve::to_html("[x ^[see [y](/inner)]](/outer)");
    assert!(
        html.contains("<a href=\"/inner\">y</a>"),
        "footnote body should keep its link, got: {html}"
    );
}

#[test]
fn resolved_reference_link_in_label_unwraps_to_display_text() {
    // [good]: /g  then  [[x][good]](/z): the inner reference link DOES resolve,
    // so at post-resolution it is a real Link node and is unwrapped to its
    // display text. Only the outer destination links.
    assert_eq!(
        carve::to_html("[good]: /g\n\n[[x][good]](/z)"),
        "<p><a href=\"/z\">x</a></p>"
    );
}

#[test]
fn crossref_in_label_becomes_text_no_nested_anchor() {
    // # H  then  [see </#H>](/outer): the crossref resolves to a Link only
    // during resolution, so the post-resolution pass flattens it to its display
    // text. carve-rs default heading ids are case-preserving, so the would-be
    // href is "#H"; what matters is that NO nested anchor survives and the inner
    // crossref became text inside the outer link.
    let html = carve::to_html("# H\n\n[see </#H>](/outer)");
    assert!(
        html.contains("<a href=\"/outer\">see H</a>"),
        "crossref in label should flatten to text, got: {html}"
    );
    assert!(
        !html.contains("href=\"#H\""),
        "no nested anchor to the heading should survive, got: {html}"
    );
}

#[test]
fn nested_link_in_admonition_title_flattens() {
    // A nested link in an admonition title must be flattened: the inner link
    // is replaced by its text, only the outer destination links. The enforce
    // pass must descend into the title inline array, matching carve-js
    // walkBlock coverage.
    let html = carve::to_html("::: note \"[[x](y)](z)\"\nbody\n:::");
    assert!(
        html.contains("<a href=\"z\">x</a>"),
        "outer link should survive in title, got: {html}"
    );
    assert!(
        !html.contains("href=\"y\""),
        "inner link in title should be flattened, got: {html}"
    );
}

#[test]
fn nested_link_in_table_caption_flattens() {
    // A nested link in a table caption must be flattened the same way.
    let html = carve::to_html("| a |\n|---|\n| b |\n^ [[x](y)](z)");
    assert!(
        html.contains("<a href=\"z\">x</a>"),
        "outer link should survive in caption, got: {html}"
    );
    assert!(
        !html.contains("href=\"y\""),
        "inner link in caption should be flattened, got: {html}"
    );
}

#[test]
fn autolink_nested_in_emphasis_in_label_unwraps() {
    let ext = Autolink::new();
    let opts = Options::new().with_extension(&ext);
    // [*em https://x.com*](/u): the link nested inside emphasis inside the
    // label is unwrapped via recursion through child nodes.
    assert_eq!(
        carve::to_html_with_options("[*em https://x.com*](/u)", &opts),
        "<p><a href=\"/u\"><strong>em https://x.com</strong></a></p>"
    );
}
