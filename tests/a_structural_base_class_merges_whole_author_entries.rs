//! A structural base class merges with the author's class slot by WHOLE
//! ENTRIES. The merge used to join the base and the slot into one string and
//! run `split_whitespace` over it, which both put the base in the author's
//! dedup pool and collapsed an entry's internal run (carve-rs#2064).
//!
//! The typed-div merge appends the base and dedups nothing across the boundary,
//! so `{.note}` on `::: note` opens `class="admonition note note"`. The
//! block-attribute merge prepends the base to the author's list instead, so a
//! figure group's own class collides with it and is dropped. Neither splits an
//! entry.
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
        (".sidebar", r#"<div class="sidebar sidebar">"#),
        ("class=sidebar", r#"<div class="sidebar sidebar">"#),
        (r#"class="sidebar x""#, r#"<div class="sidebar sidebar x">"#),
        // An accepted entry keeps its internal run.
        (r#"class="a  b" .c"#, r#"<div class="sidebar a  b c">"#),
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
            r#"<aside class="admonition note note" aria-label="Note">"#,
        ),
        (
            "class=note",
            r#"<aside class="admonition note note" aria-label="Note">"#,
        ),
        (
            ".admonition",
            r#"<aside class="admonition note admonition" aria-label="Note">"#,
        ),
        (
            r#"class="admonition note""#,
            r#"<aside class="admonition note admonition note" aria-label="Note">"#,
        ),
        (
            r#"class="a  b" .c"#,
            r#"<aside class="admonition note a  b c" aria-label="Note">"#,
        ),
    ] {
        assert_eq!(container(attrs, "::: note"), expected, "{attrs}");
    }
}

#[test]
fn a_figure_group_dedups_its_base_as_one_entry_without_splitting() {
    for (attrs, expected) in [
        // The base is ONE entry of the author's list here, so an equal entry drops.
        (
            ".carve-figure-group .x",
            r#"<figure class="carve-figure-group x">"#,
        ),
        // Not equal to the base as a whole, so nothing drops and the run stays.
        (
            r#"class="carve-figure-group  y""#,
            r#"<figure class="carve-figure-group carve-figure-group  y">"#,
        ),
        (
            r#"class="a  b" .c"#,
            r#"<figure class="carve-figure-group a  b c">"#,
        ),
    ] {
        assert_eq!(figure_group(attrs), expected, "{attrs}");
    }
}

#[test]
fn an_inline_base_class_dedups_as_one_entry_without_splitting() {
    for (source, expected) in [
        (
            "$`x`{class=\"math inline\"}",
            r#"<p><span class="math inline" role="math">\(x\)</span></p>"#,
        ),
        (
            "$`x`{class=\"math inline  z\"}",
            r#"<p><span class="math inline math inline  z" role="math">\(x\)</span></p>"#,
        ),
        // Neither name alone equals the base, so both are added.
        (
            "$`x`{.math .inline}",
            r#"<p><span class="math inline math inline" role="math">\(x\)</span></p>"#,
        ),
    ] {
        assert_eq!(opener(source), expected, "{source}");
    }
}
