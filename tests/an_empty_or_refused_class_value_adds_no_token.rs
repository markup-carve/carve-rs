//! `CARVE-P4-007`: an empty or refused class value claims the slot and
//! contributes no token. The fold that put `class=VALUE` into `attrs.classes`
//! lost two things the slot already did - an empty entry stopped being dropped,
//! so the join leaked a space, and a value stopped passing the sanitizer every
//! other key-value passes, so a refused one reached HTML live (carve-rs#2060).
//!
//! The filter runs per AUTHORED ENTRY, not per whitespace-separated name: the
//! sanitizer normalizes a scheme across whitespace, so a per-name probe would
//! pass `java script:alert(1)` and deny less than the slot did before the fold
//! (carve-js#1164).
//!
//! Every expected reading below comes from the executable oracle in
//! markup-carve/carve, cross-checked against its contract test.

fn opener(attrs: &str, fence: &str) -> String {
    let source = format!("{{{attrs}}}\n{fence}\ny\n:::\n");
    carve::to_html(&source).lines().next().unwrap().to_string()
}

#[test]
fn plain_div_keeps_only_surviving_entries() {
    for (attrs, expected) in [
        ("class=a class", r#"<div class="a">"#),
        ("class .b", r#"<div class="b">"#),
        ("class class=a", r#"<div class="a">"#),
        // Control: the empty slot stays claimed.
        ("class", r#"<div class="">"#),
        (r#"class="""#, r#"<div class="">"#),
        (r#"class="javascript:alert(1)""#, r#"<div class="">"#),
        (r#"class="javascript:alert(1)" .b"#, r#"<div class="b">"#),
        // Control: other key-value names already passed through the sanitizer.
        (r#"k="javascript:alert(1)""#, r#"<div k="">"#),
        // Control: a colon alone is allowed.
        (r#"class="a:b""#, r#"<div class="a:b">"#),
        (r#"class="DATA:text/html,x""#, r#"<div class="">"#),
        // The carve-js#1164 shape: a per-name probe passes this, a per-entry one refuses it.
        (r#"class="java script:alert(1)""#, r#"<div class="">"#),
        // The whole entry goes, the neighbour stays.
        (r#"class="java script:alert(1)" .b"#, r#"<div class="b">"#),
        // An accepted entry stays WHOLE, which a per-name filter could not do.
        (r#"class="a  b" .c"#, r#"<div class="a  b c">"#),
        // Unchanged by this fix, below: dedup, source order and slot position.
        ("class=a .a", r#"<div class="a">"#),
        ("class=a .b", r#"<div class="a b">"#),
        (".b class=a", r#"<div class="b a">"#),
        ("#i class=a k=v .b", r#"<div id="i" class="a b" k="v">"#),
    ] {
        assert_eq!(opener(attrs, ":::"), expected, "{attrs}");
    }
}

#[test]
fn typed_div_keeps_its_base_class() {
    for (attrs, expected) in [
        // The claimed empty slot leaves the structural class alone.
        ("class", r#"<div class="sidebar">"#),
        ("class .b", r#"<div class="sidebar b">"#),
        ("class=b", r#"<div class="sidebar b">"#),
        (r#"class="javascript:alert(1)""#, r#"<div class="sidebar">"#),
        (
            r#"class="javascript:alert(1)" .b"#,
            r#"<div class="sidebar b">"#,
        ),
    ] {
        assert_eq!(opener(attrs, "::: sidebar"), expected, "{attrs}");
    }
}

#[test]
fn admonition_keeps_its_base_classes() {
    for (attrs, expected) in [
        // The claimed empty slot leaves the structural classes alone.
        (
            "class",
            r#"<aside class="admonition note" aria-label="Note">"#,
        ),
        (
            "class .b",
            r#"<aside class="admonition note b" aria-label="Note">"#,
        ),
        (
            r#"class="javascript:alert(1)""#,
            r#"<aside class="admonition note" aria-label="Note">"#,
        ),
        (
            r#"class="javascript:alert(1)" .b"#,
            r#"<aside class="admonition note b" aria-label="Note">"#,
        ),
    ] {
        assert_eq!(opener(attrs, "::: note"), expected, "{attrs}");
    }
}
