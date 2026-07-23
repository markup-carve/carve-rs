//! Port of carve-js `test/svg-sanitize.test.ts`: the hand-rolled SVG sanitizer.
//!
//! Behavioral assertions mirror the carve-js `toContain` / `not.toContain`
//! checks; several cases also pin the exact byte output captured from carve-js
//! (`dist/svg-sanitize.js`) so the two implementations cannot silently diverge.

use carve::{sanitize_svg, SanitizeSvgOptions};

fn opts() -> SanitizeSvgOptions {
    SanitizeSvgOptions::default()
}

fn san(src: &str) -> String {
    sanitize_svg(src, &opts()).svg
}

fn san_with(src: &str, o: SanitizeSvgOptions) -> String {
    sanitize_svg(src, &o).svg
}

fn wrap(inner: &str) -> String {
    format!("<svg viewBox=\"0 0 10 10\">{inner}</svg>")
}

const NS: &str = "xmlns=\"http://www.w3.org/2000/svg\"";

// ---------------------------------------------------------------------------
// element filtering
// ---------------------------------------------------------------------------

#[test]
fn keeps_a_clean_presentational_svg() {
    let src = wrap("<path d=\"M0 0L10 10\" fill=\"currentColor\"/>");
    let r = sanitize_svg(&src, &opts());
    assert!(r.ok);
    assert_eq!(
        r.svg,
        format!(
            "<svg {NS} viewBox=\"0 0 10 10\"><path d=\"M0 0L10 10\" fill=\"currentColor\"/></svg>"
        )
    );
}

#[test]
fn drops_script_and_its_content() {
    let r = sanitize_svg(&wrap("<script>alert(1)</script><circle r=\"5\"/>"), &opts());
    assert!(r.ok);
    assert!(!r.svg.contains("script"));
    assert!(!r.svg.contains("alert"));
    assert!(r.svg.contains("<circle r=\"5\""));
}

#[test]
fn drops_foreign_object_subtree() {
    let svg = san(&wrap(
        "<foreignObject><body xmlns=\"http://www.w3.org/1999/xhtml\"><img src=x onerror=alert(1)></body></foreignObject>",
    ));
    assert!(!svg.contains("foreignObject"));
    assert!(!svg.contains("onerror"));
    assert!(!svg.contains("<img"));
}

#[test]
fn drops_smil_animation_by_default() {
    let svg = san(&wrap(
        "<rect width=\"10\" height=\"10\"><animate onbegin=\"alert(1)\" attributeName=\"x\"/></rect>",
    ));
    assert!(svg.contains("<rect"));
    assert!(!svg.contains("animate"));
    assert!(!svg.contains("onbegin"));
}

#[test]
fn drops_comments_cdata_pi_and_doctype() {
    let src = format!(
        "<!DOCTYPE svg><?xml-stylesheet href=\"x\"?>{}",
        wrap("<!-- c --><![CDATA[ x ]]><path d=\"M0 0\"/>")
    );
    let svg = san(&src);
    assert!(!svg.contains("<!--"));
    assert!(!svg.contains("CDATA"));
    assert!(!svg.contains("DOCTYPE"));
    assert!(!svg.contains("xml-stylesheet"));
    assert!(svg.contains("<path d=\"M0 0\""));
}

#[test]
fn keeps_nested_allowed_tags() {
    let inner =
        "<defs><linearGradient id=\"g\"><stop offset=\"0\" stop-color=\"red\"/></linearGradient>\
<filter id=\"f\"><feGaussianBlur stdDeviation=\"2\"/></filter></defs>\
<g transform=\"translate(1,1)\"><rect width=\"8\" height=\"8\" fill=\"url(#g)\"/></g>";
    let r = sanitize_svg(&wrap(inner), &opts());
    assert!(r.ok);
    assert!(r.svg.contains("<linearGradient"));
    assert!(r.svg.contains("<feGaussianBlur"));
    assert!(r.svg.contains("<g transform=\"translate(1,1)\""));
}

// ---------------------------------------------------------------------------
// attribute filtering
// ---------------------------------------------------------------------------

#[test]
fn strips_every_on_handler() {
    let svg = san(&wrap("<circle r=\"5\" onclick=\"x()\" onload=\"y()\"/>"));
    assert!(svg.contains("<circle r=\"5\""));
    assert!(!svg.contains("onclick"));
    assert!(!svg.contains("onload"));
}

#[test]
fn blocks_entity_encoded_schemes_in_href() {
    let links = SanitizeSvgOptions {
        allow_links: true,
        ..Default::default()
    };
    let num = san_with(
        &wrap("<a href=\"jav&#x61;script:alert(1)\"><rect width=\"1\" height=\"1\"/></a>"),
        links,
    );
    assert!(!num.contains("href="));
    let named = san_with(
        &wrap("<a href=\"javascript&colon;alert(1)\"><rect width=\"1\" height=\"1\"/></a>"),
        links,
    );
    assert!(!named.contains("href="));
}

#[test]
fn accepts_leading_whitespace_after_dropped_declaration() {
    let src = "<?xml version=\"1.0\"?>\n<!DOCTYPE svg>\n<svg viewBox=\"0 0 1 1\"><rect width=\"1\" height=\"1\"/></svg>";
    let r = sanitize_svg(src, &opts());
    assert!(r.ok);
    assert!(r.svg.starts_with("<svg"));
    assert!(r.svg.contains("<rect width=\"1\" height=\"1\""));
}

#[test]
fn drops_dangerous_href_keeps_local_frag() {
    let bad = san(&wrap("<use href=\"javascript:alert(1)\"/>"));
    assert!(!bad.contains("javascript"));
    let ext = san(&wrap("<use href=\"https://evil.example/x.svg#a\"/>"));
    assert!(!ext.contains("https://evil"));
    let local = san(&wrap("<use href=\"#icon\"/>"));
    assert!(local.contains("href=\"#icon\""));
}

#[test]
fn drops_style_by_default() {
    let svg = san(&wrap(
        "<rect style=\"fill:red\" width=\"10\" height=\"10\"/>",
    ));
    assert!(!svg.contains("style"));
}

#[test]
fn allow_style_keeps_benign_scrubs_dangerous() {
    let style = SanitizeSvgOptions {
        allow_style: true,
        ..Default::default()
    };
    let ok = san_with(
        &wrap("<rect style=\"fill:red\" width=\"1\" height=\"1\"/>"),
        style,
    );
    assert!(ok.contains("style=\"fill:red\""));
    let bad = san_with(
        &wrap("<rect style=\"background:url(javascript:alert(1))\" width=\"1\" height=\"1\"/>"),
        style,
    );
    assert!(!bad.contains("url("));
    assert!(!bad.contains("javascript"));
}

#[test]
fn always_drops_style_element_even_with_allow_style() {
    let style = SanitizeSvgOptions {
        allow_style: true,
        ..Default::default()
    };
    let svg = san_with(
        &wrap("<style>@import url(https://attacker.example/x.css)</style><rect width=\"1\" height=\"1\"/>"),
        style,
    );
    assert!(!svg.contains("style"));
    assert!(!svg.contains("@import"));
    assert!(!svg.contains("attacker"));
    assert!(svg.contains("<rect width=\"1\" height=\"1\""));
}

#[test]
fn allow_links_does_not_widen_external_href_onto_use() {
    let links = SanitizeSvgOptions {
        allow_links: true,
        ..Default::default()
    };
    let on = san_with(&wrap("<use href=\"https://evil.example/x.svg#a\"/>"), links);
    assert!(!on.contains("https://evil"));
    assert!(san_with(&wrap("<use href=\"#i\"/>"), links).contains("href=\"#i\""));
    let link = san_with(
        &wrap("<a href=\"https://ok.example/\"><rect width=\"1\" height=\"1\"/></a>"),
        links,
    );
    assert!(link.contains("href=\"https://ok.example/\""));
}

#[test]
fn blocks_os_handler_schemes_on_links_even_with_allow_links() {
    let links = SanitizeSvgOptions {
        allow_links: true,
        ..Default::default()
    };
    for scheme in ["ms-msdt:x", "shell:x", "vscode:x", "jar:x", "search-ms:x"] {
        let svg = san_with(
            &wrap(&format!(
                "<a href=\"{scheme}\"><rect width=\"1\" height=\"1\"/></a>"
            )),
            links,
        );
        assert!(!svg.contains("href="), "{scheme} must not survive");
    }
}

#[test]
fn allow_external_images_keeps_image_href() {
    let images = SanitizeSvgOptions {
        allow_external_images: true,
        ..Default::default()
    };
    let src = wrap("<image href=\"https://cdn.example/logo.png\" width=\"10\" height=\"10\"/>");
    assert!(!san(&src).contains("<image"));
    let on = san_with(&src, images);
    assert!(on.contains("<image"));
    assert!(on.contains("href=\"https://cdn.example/logo.png\""));
    let bad = san_with(
        &wrap("<image href=\"javascript:alert(1)\" width=\"1\" height=\"1\"/>"),
        images,
    );
    assert!(!bad.contains("javascript"));
}

#[test]
fn drops_presentation_attrs_with_external_url_keeps_local() {
    let ext = san(&wrap(
        "<rect width=\"1\" height=\"1\" fill=\"url(https://attacker.example/p.svg#x)\"/>",
    ));
    assert!(!ext.contains("attacker"));
    assert!(!ext.contains("url(http"));
    let filt = san(&wrap(
        "<rect width=\"1\" height=\"1\" filter=\"url(//evil/x)\"/>",
    ));
    assert!(!filt.contains("filter="));
    let local = san(&wrap(
        "<rect width=\"1\" height=\"1\" fill=\"url(#grad)\"/>",
    ));
    assert!(local.contains("fill=\"url(#grad)\""));
}

#[test]
fn rejects_quoted_url_whose_target_contains_paren() {
    let svg = san(&wrap(
        "<rect width=\"1\" height=\"1\" fill='url(\"https://attacker.example/a)b.svg#x\")'/>",
    ));
    assert!(!svg.contains("attacker"));
    assert!(!svg.contains("fill="));
}

#[test]
fn validates_each_smil_values_entry() {
    let anim = SanitizeSvgOptions {
        allow_animation: true,
        ..Default::default()
    };
    let svg = san_with(
        &wrap("<use href=\"#i\"><animate attributeName=\"href\" values=\"#i;https://attacker.example/x.svg#j\"/></use>"),
        anim,
    );
    assert!(!svg.contains("attacker"));
    assert!(!svg.contains("values="));
    let rel = san_with(
        &wrap("<rect width=\"1\" height=\"1\"><animate attributeName=\"fill\" values=\"#i;//evil.example/x.svg#j\"/></rect>"),
        anim,
    );
    assert!(!rel.contains("evil"));
    assert!(!rel.contains("values="));
    let clean = san_with(
        &wrap("<rect width=\"1\" height=\"1\"><animate attributeName=\"fill\" values=\"#a;#b\"/></rect>"),
        anim,
    );
    assert!(clean.contains("values=\"#a;#b\""));
}

#[test]
fn allow_style_rejects_css_escaped_url() {
    let style = SanitizeSvgOptions {
        allow_style: true,
        ..Default::default()
    };
    let svg = san_with(
        &wrap("<rect width=\"1\" height=\"1\" style=\"fill:u\\72l(https://attacker.example/x.svg#p)\"/>"),
        style,
    );
    assert!(!svg.contains("style"));
    assert!(!svg.contains("attacker"));
}

#[test]
fn drops_unknown_attributes() {
    let svg = san(&wrap("<path d=\"M0 0\" formaction=\"x\" srcdoc=\"y\"/>"));
    assert!(svg.contains("d=\"M0 0\""));
    assert!(!svg.contains("formaction"));
    assert!(!svg.contains("srcdoc"));
}

#[test]
fn preserves_existing_xml_entities_without_double_escaping() {
    let t = san(&wrap("<text>A &amp; B</text>"));
    assert!(t.contains("A &amp; B"));
    assert!(!t.contains("&amp;amp;"));
    let a = san(&wrap("<text aria-label=\"A &quot; B\">x</text>"));
    assert!(a.contains("aria-label=\"A &quot; B\""));
    assert!(!a.contains("&amp;quot;"));
    assert!(san(&wrap("<text>a & b</text>")).contains("a &amp; b"));
}

// ---------------------------------------------------------------------------
// root guard + xmlns
// ---------------------------------------------------------------------------

#[test]
fn rejects_non_svg_root() {
    assert!(!sanitize_svg("<div>not svg</div>", &opts()).ok);
    assert!(!sanitize_svg("hello", &opts()).ok);
}

#[test]
fn rejects_unclosed_root() {
    assert!(!sanitize_svg("<svg><path d=\"M0 0\"", &opts()).ok);
}

#[test]
fn rejects_non_whitespace_text_before_root() {
    assert!(!sanitize_svg("caption<svg><rect/></svg>", &opts()).ok);
    assert!(
        sanitize_svg(
            "  \n<svg viewBox=\"0 0 1 1\"><rect width=\"1\" height=\"1\"/></svg>",
            &opts()
        )
        .ok
    );
}

#[test]
fn deduplicates_repeated_attributes_keeps_first() {
    let r = sanitize_svg(
        "<svg viewBox=\"0 0 1 1\" viewBox=\"0 0 2 2\"><rect id=\"a\" id=\"b\" width=\"1\" height=\"1\"/></svg>",
        &opts(),
    );
    assert!(r.ok);
    assert_eq!(r.svg.matches("viewBox=").count(), 1);
    assert!(r.svg.contains("viewBox=\"0 0 1 1\""));
    // exact byte golden from carve-js
    assert_eq!(
        r.svg,
        format!("<svg {NS} viewBox=\"0 0 1 1\"><rect id=\"a\" width=\"1\" height=\"1\"/></svg>")
    );
}

#[test]
fn escapes_non_xml_named_entities() {
    let svg = san(&wrap("<text>a&nbsp;b &copy; c</text>"));
    assert!(svg.contains("&amp;nbsp;"));
    assert!(svg.contains("&amp;copy;"));
    let keep = san(&wrap("<text>a &amp; b &#160; c</text>"));
    assert!(keep.contains("&amp; "));
    assert!(keep.contains("&#160;"));
}

#[test]
fn rejects_multiple_top_level_roots() {
    assert!(!sanitize_svg("<svg></svg><svg></svg>", &opts()).ok);
    let src = format!("{}<svg></svg>", wrap("<rect width=\"1\" height=\"1\"/>"));
    assert!(!sanitize_svg(&src, &opts()).ok);
}

#[test]
fn rejects_mismatched_closing_tags() {
    assert!(!sanitize_svg("<svg><path></rect></svg>", &opts()).ok);
    assert!(!sanitize_svg("<svg><g></svg>", &opts()).ok);
}

#[test]
fn rejects_case_mismatched_tag_names() {
    assert!(!sanitize_svg("<svg><g></G></svg>", &opts()).ok);
    assert!(!sanitize_svg("<SVG><rect/></SVG>", &opts()).ok);
}

#[test]
fn does_not_exit_dropped_subtree_on_mismatched_close() {
    assert!(
        !sanitize_svg(
            "<svg><script></svg><rect width=\"1\" height=\"1\"/></svg>",
            &opts()
        )
        .ok
    );
}

#[test]
fn drops_well_formed_disallowed_subtree_keeps_siblings() {
    let r = sanitize_svg(
        "<svg><script>x</script><rect width=\"1\" height=\"1\"/></svg>",
        &opts(),
    );
    assert!(r.ok);
    assert!(!r.svg.contains("script"));
    assert!(r.svg.contains("<rect width=\"1\" height=\"1\""));
}

#[test]
fn injects_xmlns_on_root_when_missing() {
    let svg = san("<svg viewBox=\"0 0 1 1\"><path d=\"M0 0\"/></svg>");
    assert!(svg.contains("xmlns=\"http://www.w3.org/2000/svg\""));
}

#[test]
fn forces_canonical_xmlns_even_when_author_is_wrong() {
    let wrong = san("<svg xmlns=\"https://example.com\" viewBox=\"0 0 1 1\"><rect width=\"1\" height=\"1\"/></svg>");
    assert!(wrong.contains("xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(!wrong.contains("example.com"));
    assert_eq!(wrong.matches("xmlns=").count(), 1);
    let danger = san(
        "<svg xmlns=\"javascript:x\" viewBox=\"0 0 1 1\"><rect width=\"1\" height=\"1\"/></svg>",
    );
    assert!(danger.contains("xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(!danger.contains("javascript"));
}

#[test]
fn accepts_self_closing_empty_root() {
    let r = sanitize_svg("<svg viewBox=\"0 0 1 1\"/>", &opts());
    assert!(r.ok);
    assert_eq!(r.svg, format!("<svg {NS} viewBox=\"0 0 1 1\"/>"));
    assert!(r.svg.contains("xmlns=\"http://www.w3.org/2000/svg\""));
}

// ---------------------------------------------------------------------------
// idempotence
// ---------------------------------------------------------------------------

#[test]
fn idempotent() {
    let src = wrap(
        "<script>x</script><g><rect style=\"fill:red\" width=\"5\" height=\"5\" onclick=\"e\"/></g>",
    );
    let once = san(&src);
    let twice = san(&once);
    assert_eq!(twice, once);
}

// ---------------------------------------------------------------------------
// byte-exact goldens captured from carve-js dist/svg-sanitize.js
// ---------------------------------------------------------------------------

#[test]
fn byte_goldens_match_carve_js() {
    let cases: &[(SanitizeSvgOptions, &str, &str)] = &[
        (
            opts(),
            "<svg viewBox=\"0 0 10 10\"><path d=\"M0 0L10 10\" fill=\"currentColor\"/></svg>",
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\"><path d=\"M0 0L10 10\" fill=\"currentColor\"/></svg>",
        ),
        (
            opts(),
            "<svg viewBox=\"0 0 10 10\"><script>alert(1)</script><circle r=\"5\"/></svg>",
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\"><circle r=\"5\"/></svg>",
        ),
        (
            opts(),
            "<svg viewBox=\"0 0 10 10\"><text>a&nbsp;b &copy; c &amp; d &#160; e</text></svg>",
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\"><text>a&amp;nbsp;b &amp;copy; c &amp; d &#160; e</text></svg>",
        ),
        (
            opts(),
            "<svg viewBox=\"0 0 10 10\"><rect width=\"1\" height=\"1\" fill=\"url(#grad)\"/></svg>",
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\"><rect width=\"1\" height=\"1\" fill=\"url(#grad)\"/></svg>",
        ),
    ];
    for (o, input, expected) in cases {
        assert_eq!(&san_with(input, *o), expected, "input: {input}");
    }
}
