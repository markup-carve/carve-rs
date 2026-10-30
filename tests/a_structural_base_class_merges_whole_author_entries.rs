//! A structural base class and the author's classes are one list, each value
//! once (CARVE-P4-002, carve#4143). A `class=V` value is a list of class tokens
//! (CARVE-P4-007, carve#4205).
//!
//! Every expected reading below was measured against the executable oracle in
//! markup-carve/carve (`scripts/spec/layout.mjs` plus `scripts/spec/html.mjs`)
//! and against carve-js, both run rather than read.

fn opener(source: &str) -> String {
    carve::to_html(source)
        .lines()
        .next()
        .unwrap()
        .trim_start()
        .to_string()
}

fn container(attrs: &str, fence: &str) -> String {
    opener(&format!("{{{attrs}}}\n{fence}\ny\n:::\n"))
}

fn figure_group(attrs: &str) -> String {
    opener(&format!(
        "{{{attrs}}}\n::: figure\n![a](a.png)\n^ p\n\n^ g\n:::\n"
    ))
}

#[test]
fn a_typed_div_appends_the_author_s_entries_after_its_type_class() {
    for (attrs, expected) in [
        (".sidebar", r#"<div class="sidebar">"#),
        ("class=sidebar", r#"<div class="sidebar">"#),
        (r#"class="sidebar x""#, r#"<div class="sidebar x">"#),
        // A value splits into class tokens (CARVE-P4-007).
        (r#"class="a  b" .c"#, r#"<div class="sidebar a b c">"#),
        // Control: the dedup WITHIN the author's slot still runs (§15).
        ("class=a .a", r#"<div class="sidebar a">"#),
        // Controls from carve-rs#2065: a refused value adds no token.
        (r#"class="javascript:alert(1)""#, r#"<div class="sidebar">"#),
        (
            r#"class="javascript:alert(1)" .b"#,
            r#"<div class="sidebar b">"#,
        ),
        (r#"class="a:b""#, r#"<div class="sidebar a:b">"#),
    ] {
        assert_eq!(container(attrs, "::: sidebar"), expected, "{attrs}");
    }
}

#[test]
fn an_admonition_appends_the_author_s_entries_after_both_base_classes() {
    for (attrs, expected) in [
        (
            ".note",
            r#"<aside class="admonition note" aria-label="Note">"#,
        ),
        (
            "class=note",
            r#"<aside class="admonition note" aria-label="Note">"#,
        ),
        (
            ".admonition",
            r#"<aside class="admonition note" aria-label="Note">"#,
        ),
        (
            r#"class="admonition note""#,
            r#"<aside class="admonition note" aria-label="Note">"#,
        ),
        (
            r#"class="a  b" .c"#,
            r#"<aside class="admonition note a b c" aria-label="Note">"#,
        ),
    ] {
        assert_eq!(container(attrs, "::: note"), expected, "{attrs}");
    }
}

#[test]
fn a_figure_group_dedups_its_base() {
    for (attrs, expected) in [
        // The base is ONE entry of the author's list here, so an equal entry drops.
        (
            ".carve-figure-group .x",
            r#"<figure class="carve-figure-group x">"#,
        ),
        (
            r#"class="carve-figure-group  y""#,
            r#"<figure class="carve-figure-group y">"#,
        ),
        (
            r#"class="a  b" .c"#,
            r#"<figure class="carve-figure-group a b c">"#,
        ),
    ] {
        assert_eq!(figure_group(attrs), expected, "{attrs}");
    }
}

#[test]
fn an_inline_base_class_dedups() {
    for (source, expected) in [
        (
            "$`x`{class=\"math inline\"}",
            r#"<p><span class="math inline" role="math">\(x\)</span></p>"#,
        ),
        (
            "$`x`{class=\"math inline  z\"}",
            r#"<p><span class="math inline z" role="math">\(x\)</span></p>"#,
        ),
        (
            "$`x`{.math .inline}",
            r#"<p><span class="math inline" role="math">\(x\)</span></p>"#,
        ),
    ] {
        assert_eq!(opener(source), expected, "{source}");
    }
}
