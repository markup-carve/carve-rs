//! Golden-parity tests for the `Details` disclosure extension.
//!
//! Goldens captured from carve-js (`dist/index.js`) with
//! `carveToHtml(src, { extensions: [details()] }).trim()` - mirror of
//! carve-js `test/details.test.ts`.

use carve::{Details, Options};

/// Render `src` with the details extension, trimmed (matching the carve-js
/// test harness `.trim()`).
fn h(src: &str) -> String {
    let ext = Details::new();
    let opts = Options::new().with_extension(&ext);
    carve::to_html_with_options(src, &opts).trim().to_string()
}

#[test]
fn quoted_title_becomes_summary() {
    assert_eq!(
        h("::: details \"More info\"\nHidden _here_.\n:::"),
        [
            "<details>",
            "  <summary>More info</summary>",
            "  <p>Hidden <u>here</u>.</p>",
            "</details>",
        ]
        .join("\n")
    );
}

#[test]
fn default_summary_when_no_title() {
    assert_eq!(
        h("::: details\nBody.\n:::"),
        [
            "<details>",
            "  <summary>Details</summary>",
            "  <p>Body.</p>",
            "</details>",
        ]
        .join("\n")
    );
}

#[test]
fn escapes_html_special_chars_in_summary() {
    assert!(h("::: details \"Tom & Jerry\"\nx\n:::").contains("<summary>Tom &amp; Jerry</summary>"));
}

#[test]
fn flattens_inline_markup_in_title() {
    assert!(h("::: details \"see /here/\"\nx\n:::").contains("<summary>see here</summary>"));
}

#[test]
fn hardens_authored_attributes() {
    let out =
        h("{onclick=alert(1) style=\"background:url(javascript:alert(1))\"}\n::: details\nx\n:::");
    assert!(!out.contains("onclick="), "{out}");
    assert!(!out.contains("javascript:"), "{out}");
    assert!(out.contains("style=\"\""), "{out}");
}

#[test]
fn summary_keeps_code_link_emphasis_text() {
    // carve-js collects code `value` and emphasis/link `children`.
    assert!(h("::: details \"a `code` b\"\nx\n:::").contains("<summary>a code b</summary>"));
    assert!(h("::: details \"a [link](x) b\"\nx\n:::").contains("<summary>a link b</summary>"));
    assert!(h("::: details \"a *bold* b\"\nx\n:::").contains("<summary>a bold b</summary>"));
}

#[test]
fn summary_drops_image_alt_matching_carve_js() {
    // carve-js drops image alt (the image node carries no `value`/children
    // array its walk picks up): `a ![alt](x) b` -> `a  b`.
    assert!(h("::: details \"a ![alt text](x.png) b\"\nx\n:::").contains("<summary>a  b</summary>"));
}

#[test]
fn keeps_multiple_block_children() {
    assert_eq!(
        h("::: details \"T\"\nOne.\n\nTwo.\n:::"),
        [
            "<details>",
            "  <summary>T</summary>",
            "  <p>One.</p>",
            "  <p>Two.</p>",
            "</details>",
        ]
        .join("\n")
    );
}

#[test]
fn heading_child_carries_slug_id() {
    // A heading inside the container still carries its slug id on the <h*>
    // (carve-php parity); only the top-level <section> pass is skipped.
    assert_eq!(
        h("::: details \"T\"\n# H\n\nx\n:::"),
        [
            "<details>",
            "  <summary>T</summary>",
            "  <h1 id=\"H\">H</h1>",
            "  <p>x</p>",
            "</details>",
        ]
        .join("\n")
    );
}

#[test]
fn heading_id_counter_continues_across_details_boundary() {
    // A duplicate slug inside a details block gets its `-2` suffix from the
    // shared document heading counter (carve-js parity), not a fresh counter.
    assert_eq!(
        h("# H\n\n::: details \"T\"\n# H\n\nx\n:::"),
        [
            "<section id=\"H\">",
            "  <h1>H</h1>",
            "  <details>",
            "    <summary>T</summary>",
            "    <h1 id=\"H-2\">H</h1>",
            "    <p>x</p>",
            "  </details>",
            "</section>",
        ]
        .join("\n")
    );
}

#[test]
fn details_inside_list_item_nests_with_p_wrappers() {
    assert_eq!(
        h("- item\n\n  ::: details \"T\"\n  x\n  :::"),
        [
            "<ul>",
            "  <li>item",
            "    <details>",
            "      <summary>T</summary>",
            "      <p>x</p>",
            "    </details>",
            "  </li>",
            "</ul>",
        ]
        .join("\n")
    );
}

#[test]
fn nested_details_blocks() {
    assert_eq!(
        h(":::: details \"Outer\"\n::: details \"Inner\"\ndeep\n:::\n::::"),
        [
            "<details>",
            "  <summary>Outer</summary>",
            "  <details>",
            "    <summary>Inner</summary>",
            "    <p>deep</p>",
            "  </details>",
            "</details>",
        ]
        .join("\n")
    );
}

#[test]
fn carries_id_and_class_attrs_onto_details_tag() {
    assert_eq!(
        h("{#faq .open}\n::: details \"Q\"\na\n:::"),
        [
            "<details id=\"faq\" class=\"open\">",
            "  <summary>Q</summary>",
            "  <p>a</p>",
            "</details>",
        ]
        .join("\n")
    );
}

#[test]
fn carries_open_boolean_attr() {
    assert_eq!(
        h("{#faq open}\n::: details \"FAQ\"\nA.\n:::"),
        [
            "<details id=\"faq\" open=\"\">",
            "  <summary>FAQ</summary>",
            "  <p>A.</p>",
            "</details>",
        ]
        .join("\n")
    );
}

#[test]
fn preserves_explicit_empty_id() {
    assert!(h("{id}\n::: details \"T\"\nx\n:::").contains("<details id=\"\">"));
    assert!(h("{#foo}\n::: details \"T\"\nx\n:::").contains("<details id=\"foo\">"));
}

#[test]
fn keeps_attrs_with_stale_order_list() {
    // An extension that appends a class without touching `attrs.order`; the
    // details renderer must still emit it (not silently drop it). Mirrors the
    // carve-js `keeps attrs another extension adds with a stale order list`
    // test.
    use carve::ast::{Attrs, BlockNode, Document};
    use carve::{BeforeRenderContext, CarveExtension, RenderContext};

    struct AddClass;
    impl CarveExtension for AddClass {
        fn name(&self) -> &'static str {
            "add"
        }
        fn before_render(&self, mut doc: Document, _ctx: &BeforeRenderContext<'_>) -> Document {
            fn walk(blocks: &mut [BlockNode]) {
                for block in blocks.iter_mut() {
                    if let BlockNode::Admonition(a) = block {
                        if a.kind == "details" {
                            let attrs = a.attrs.get_or_insert_with(Attrs::default);
                            attrs.classes.push("added".to_string());
                        }
                        walk(&mut a.children);
                    }
                }
            }
            walk(&mut doc.children);
            doc
        }
        fn render_block_extension(
            &self,
            _node: &carve::ast::BlockExtension,
            _ctx: &RenderContext<'_>,
        ) -> Option<String> {
            None
        }
    }

    // AddClass runs before Details so the appended class is in place when the
    // admonition is rewritten.
    let add = AddClass;
    let det = Details::new();
    let opts = Options::new().with_extension(&add).with_extension(&det);
    let out = carve::to_html_with_options("{#x}\n::: details \"Q\"\na\n:::", &opts);
    assert!(
        out.contains("<details id=\"x\" class=\"added\">"),
        "got: {out}"
    );
}

#[test]
fn leaves_canonical_admonitions_untouched() {
    assert!(h("::: note\nhi\n:::").contains("<aside class=\"admonition note\">"));
}

#[test]
fn leaves_other_custom_admonition_types_as_plain_divs() {
    assert!(h("::: aside-note\nhi\n:::").contains("<div class=\"aside-note\">"));
}

#[test]
fn restrictive_profile_still_gates_details_as_a_div() {
    // The details rewrite happens before profile filtering, but the carrier is
    // gated as a `div` (its origin), so a profile that denies custom containers
    // strips the disclosure exactly as it would the underlying admonition - it
    // must NOT slip through gated as an inline extension. Parity with carve-js,
    // which gates the un-rewritten admonition as a div.
    use carve::Profile;
    let src = "::: details \"T\"\nhi\n:::";
    let ext = Details::new();
    let no_ext = carve::to_html_with_options(src, &Options::new().with_profile(Profile::comment()));
    let with_ext = carve::to_html_with_options(
        src,
        &Options::new()
            .with_extension(&ext)
            .with_profile(Profile::comment()),
    );
    assert_eq!(no_ext, "<p>hi</p>");
    assert_eq!(
        with_ext, no_ext,
        "details must not bypass the div restriction"
    );
}

#[test]
fn without_extension_details_stays_plain_div() {
    assert!(
        carve::to_html("::: details \"More\"\nHidden.\n:::").contains("<div class=\"details\">")
    );
}
