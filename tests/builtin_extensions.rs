//! Unit and golden-parity tests for the built-in extensions.
//!
//! Golden strings were captured from carve-js (`dist/index.js`) for the same
//! input + extension config; the comment above each golden records the command.
//! For TOC / heading-permalinks the carve-js default slug is lowercase, so the
//! renderer is run with `with_lowercase_heading_ids(true)` and the extension's
//! `lowercase_ids` flag set to match.

use carve::{
    Autolink, AutolinkOptions, Citations, ColorSwatch, ExternalLinks, ExternalLinksOptions,
    FencedRender, HeadingPermalinks, HeadingPermalinksOptions, ListType, MathBlock, Options,
    Position, Spoiler, SwatchPosition, SwatchShape, TabNormalize, TableOfContents,
    TableOfContentsOptions, TocPlacement, Wikilinks, WikilinksOptions,
};

// ---------------------------------------------------------------------------
// autolink
// ---------------------------------------------------------------------------

#[test]
fn autolink_url_golden() {
    let ext = Autolink::new();
    let opts = Options::new().with_extension(&ext);
    // carveToHtml("Visit https://example.com today.", {extensions:[autolink()]})
    assert_eq!(
        carve::to_html_with_options("Visit https://example.com today.", &opts),
        "<p>Visit <a href=\"https://example.com\">https://example.com</a> today.</p>"
    );
}

#[test]
fn autolink_trailing_dot_left_outside() {
    let ext = Autolink::new();
    let opts = Options::new().with_extension(&ext);
    // carveToHtml("See https://x.com.", {extensions:[autolink()]})
    assert_eq!(
        carve::to_html_with_options("See https://x.com.", &opts),
        "<p>See <a href=\"https://x.com\">https://x.com</a>.</p>"
    );
}

#[test]
fn autolink_bare_email_golden() {
    let ext = Autolink::new();
    let opts = Options::new().with_extension(&ext);
    // carveToHtml("Mail a@b.com now.", {extensions:[autolink()]})
    assert_eq!(
        carve::to_html_with_options("Mail a@b.com now.", &opts),
        "<p>Mail <a href=\"mailto:a@b.com\">a@b.com</a> now.</p>"
    );
}

#[test]
fn autolink_mailto_strips_prefix_in_text() {
    let ext = Autolink::new();
    let opts = Options::new().with_extension(&ext);
    // carveToHtml("Mail mailto:a@b.com now.", {extensions:[autolink()]})
    assert_eq!(
        carve::to_html_with_options("Mail mailto:a@b.com now.", &opts),
        "<p>Mail <a href=\"mailto:a@b.com\">a@b.com</a> now.</p>"
    );
}

#[test]
fn autolink_http_only_disables_mailto() {
    let ext = Autolink::with_options(AutolinkOptions {
        allowed_schemes: vec!["http".into()],
    });
    let opts = Options::new().with_extension(&ext);
    // carveToHtml("a@b.com and http://x.io", {extensions:[autolink({allowedSchemes:["http"]})]})
    assert_eq!(
        carve::to_html_with_options("a@b.com and http://x.io", &opts),
        "<p>a@b.com and <a href=\"http://x.io\">http://x.io</a></p>"
    );
}

// ---------------------------------------------------------------------------
// wikilinks
// ---------------------------------------------------------------------------

#[test]
fn wikilinks_simple_golden() {
    let ext = Wikilinks::new();
    let opts = Options::new().with_extension(&ext);
    // carveToHtml("See [[Tigers]].", {extensions:[wikilinks()]})
    assert_eq!(
        carve::to_html_with_options("See [[Tigers]].", &opts),
        "<p>See <a href=\"tigers\" class=\"wikilink\" data-wikilink=\"Tigers\">Tigers</a>.</p>"
    );
}

#[test]
fn wikilinks_display_golden() {
    let ext = Wikilinks::new();
    let opts = Options::new().with_extension(&ext);
    // carveToHtml("See [[home|Home Page]].", {extensions:[wikilinks()]})
    assert_eq!(
        carve::to_html_with_options("See [[home|Home Page]].", &opts),
        "<p>See <a href=\"home\" class=\"wikilink\" data-wikilink=\"home\">Home Page</a>.</p>"
    );
}

#[test]
fn wikilinks_anchor_golden() {
    let ext = Wikilinks::new();
    let opts = Options::new().with_extension(&ext);
    // carveToHtml("See [[Guide#setup]].", {extensions:[wikilinks()]})
    assert_eq!(
        carve::to_html_with_options("See [[Guide#setup]].", &opts),
        "<p>See <a href=\"guide#setup\" class=\"wikilink\" data-wikilink=\"Guide\">Guide</a>.</p>"
    );
}

#[test]
fn wikilinks_new_window_attr_order_golden() {
    let ext = Wikilinks::with_options(WikilinksOptions {
        new_window: true,
        ..Default::default()
    });
    let opts = Options::new().with_extension(&ext);
    // carveToHtml("[[Foo]]", {extensions:[wikilinks({newWindow:true})]})
    assert_eq!(
        carve::to_html_with_options("[[Foo]]", &opts),
        "<p><a href=\"foo\" class=\"wikilink\" data-wikilink=\"Foo\" target=\"_blank\" rel=\"noopener\">Foo</a></p>"
    );
}

#[test]
fn wikilinks_empty_stays_literal_anchor_only_links() {
    let ext = Wikilinks::new();
    let opts = Options::new().with_extension(&ext);
    // carveToHtml("[[ ]] and [[#sec]]", {extensions:[wikilinks()]})
    assert_eq!(
        carve::to_html_with_options("[[ ]] and [[#sec]]", &opts),
        "<p>[[ ]] and <a href=\"#sec\" class=\"wikilink\" data-wikilink=\"\">#sec</a></p>"
    );
}

// ---------------------------------------------------------------------------
// external-links
// ---------------------------------------------------------------------------

#[test]
fn external_links_explicit_golden() {
    let ext = ExternalLinks::new();
    let opts = Options::new().with_extension(&ext);
    // carveToHtml("[docs](https://example.com)", {extensions:[externalLinks()]})
    assert_eq!(
        carve::to_html_with_options("[docs](https://example.com)", &opts),
        "<p><a href=\"https://example.com\" target=\"_blank\" rel=\"noopener noreferrer\">docs</a></p>"
    );
}

#[test]
fn external_links_internal_untouched() {
    let ext = ExternalLinks::new();
    let opts = Options::new().with_extension(&ext);
    // carveToHtml("[page](/local)", {extensions:[externalLinks()]})
    assert_eq!(
        carve::to_html_with_options("[page](/local)", &opts),
        "<p><a href=\"/local\">page</a></p>"
    );
}

#[test]
fn external_links_nofollow_golden() {
    let ext = ExternalLinks::with_options(ExternalLinksOptions {
        nofollow: true,
        ..Default::default()
    });
    let opts = Options::new().with_extension(&ext);
    // carveToHtml("[docs](https://example.com)", {extensions:[externalLinks({nofollow:true})]})
    assert_eq!(
        carve::to_html_with_options("[docs](https://example.com)", &opts),
        "<p><a href=\"https://example.com\" target=\"_blank\" rel=\"noopener noreferrer nofollow\">docs</a></p>"
    );
}

#[test]
fn external_links_angle_autolink_golden() {
    let ext = ExternalLinks::new();
    let opts = Options::new().with_extension(&ext);
    // carveToHtml("<https://e.com>", {extensions:[externalLinks()]})
    assert_eq!(
        carve::to_html_with_options("<https://e.com>", &opts),
        "<p><a href=\"https://e.com\" target=\"_blank\" rel=\"noopener noreferrer\">https://e.com</a></p>"
    );
}

// ---------------------------------------------------------------------------
// heading-permalinks
// ---------------------------------------------------------------------------

fn permalinks_lc() -> HeadingPermalinks {
    HeadingPermalinks::with_options(HeadingPermalinksOptions {
        lowercase_ids: true,
        ..Default::default()
    })
}

#[test]
fn heading_permalinks_append_golden() {
    let ext = permalinks_lc();
    let opts = Options::new()
        .with_extension(&ext)
        .with_lowercase_heading_ids(true);
    // carveToHtml("# My Heading", {extensions:[headingPermalinks()]})
    assert_eq!(
        carve::to_html_with_options("# My Heading", &opts),
        "<section id=\"my-heading\">\n  <h1>My Heading <a href=\"#my-heading\" class=\"permalink\" aria-label=\"Permalink\">¶</a></h1>\n</section>"
    );
}

#[test]
fn heading_permalinks_prepend_golden() {
    let ext = HeadingPermalinks::with_options(HeadingPermalinksOptions {
        lowercase_ids: true,
        prepend: true,
        ..Default::default()
    });
    let opts = Options::new()
        .with_extension(&ext)
        .with_lowercase_heading_ids(true);
    // carveToHtml("## Sub Title", {extensions:[headingPermalinks({prepend:true})]})
    assert_eq!(
        carve::to_html_with_options("## Sub Title", &opts),
        "<section id=\"sub-title\">\n  <h2><a href=\"#sub-title\" class=\"permalink\" aria-label=\"Permalink\">¶</a> Sub Title</h2>\n</section>"
    );
}

#[test]
fn heading_permalinks_levels_filter_golden() {
    let ext = HeadingPermalinks::with_options(HeadingPermalinksOptions {
        lowercase_ids: true,
        levels: vec![2],
        ..Default::default()
    });
    let opts = Options::new()
        .with_extension(&ext)
        .with_lowercase_heading_ids(true);
    // carveToHtml("# One\n\n## Two", {extensions:[headingPermalinks({levels:[2]})]})
    assert_eq!(
        carve::to_html_with_options("# One\n\n## Two", &opts),
        "<section id=\"one\">\n  <h1>One</h1>\n  <section id=\"two\">\n    <h2>Two <a href=\"#two\" class=\"permalink\" aria-label=\"Permalink\">¶</a></h2>\n  </section>\n</section>"
    );
}

/// Pull the heading's own id (the `<section id="...">` the core emits) and the
/// permalink anchor's `href` (the `<a ... class="permalink">`), so a test can
/// assert the invariant that the two are equal for any heading content.
fn id_and_permalink_href(html: &str) -> (String, String) {
    let id = {
        let i = html.find("<section id=\"").expect("section id present");
        let rest = &html[i + "<section id=\"".len()..];
        rest[..rest.find('"').unwrap()].to_string()
    };
    let href = {
        let cls = html
            .find("class=\"permalink\"")
            .expect("permalink anchor present");
        let before = &html[..cls];
        let h = before.rfind("href=\"#").expect("permalink href present");
        let rest = &before[h + "href=\"#".len()..];
        rest[..rest.find('"').unwrap()].to_string()
    };
    (id, href)
}

/// Render `input` with Citations + HeadingPermalinks both enabled and assert
/// the permalink anchor `href` equals the heading's own id.
fn assert_href_equals_id(input: &str) {
    let cit = Citations::default();
    let perma = HeadingPermalinks::with_options(HeadingPermalinksOptions::default());
    let opts = Options::new().with_extension(&cit).with_extension(&perma);
    let html = carve::to_html_with_options(input, &opts);
    let (id, href) = id_and_permalink_href(&html);
    assert_eq!(
        href, id,
        "permalink href (#{href}) must equal heading id ({id}) for input: {input:?}\n{html}"
    );
}

/// Regression: a heading permalink's `href` must always point at the heading's
/// own id, for any inline content. Previously the permalink slug pass ignored
/// `CitationGroup` nodes (which the core section-id pass includes), so a heading
/// like `# Research [@doe]` produced a dead anchor: `href="#Research"` against
/// `id="Research-doe"`. The fix shares one flattening function between the two
/// passes, so these cases pin href == id for every inline node type.
#[test]
fn heading_permalink_href_always_equals_id() {
    // The original bug: citation group in the heading.
    assert_href_equals_id("# Research [@doe]\n\n[@doe]: John Doe, 2020\n");
    // Other inline node types that were already consistent - guard against
    // future drift in either pass.
    assert_href_equals_id("# Hi @bob there\n");
    assert_href_equals_id("# Energy $E=mc^2$ done\n");
    assert_href_equals_id("# Smile :smile: here\n");
    assert_href_equals_id("# Note[^a] here\n\n[^a]: body\n");
    assert_href_equals_id("# See [](#target) here\n");
    assert_href_equals_id("# A [span text]{.cls} b\n");
    assert_href_equals_id("# Tag #foo here\n");
    assert_href_equals_id("# Plain heading\n");
}

// ---------------------------------------------------------------------------
// mermaid
// ---------------------------------------------------------------------------

#[test]
fn mermaid_diagram_golden() {
    let ext = FencedRender::mermaid();
    let opts = Options::new().with_extension(&ext);
    // carveToHtml("``` mermaid\ngraph TD; A-->B\n```\n", {extensions:[mermaid()]})
    assert_eq!(
        carve::to_html_with_options("``` mermaid\ngraph TD; A-->B\n```\n", &opts),
        "<pre class=\"mermaid\">graph TD; A-->B</pre>"
    );
}

#[test]
fn mermaid_inside_footnote_is_transformed() {
    // A mermaid block inside a footnote def is rendered (from footnote_defs,
    // outside the tree), so it must be transformed too -- matching carve-js.
    let ext = FencedRender::mermaid();
    let opts = Options::new().with_extension(&ext);
    let out =
        carve::to_html_with_options("see[^a]\n\n[^a]: ``` mermaid\n    graph\n    ```\n", &opts);
    assert!(
        out.contains("<pre class=\"mermaid\">"),
        "footnote mermaid not transformed: {out}"
    );
    assert!(
        !out.contains("language-mermaid"),
        "left as code block: {out}"
    );
}

#[test]
fn mermaid_non_mermaid_defers_golden() {
    let ext = FencedRender::mermaid();
    let opts = Options::new().with_extension(&ext);
    // carveToHtml("``` js\nlet x = 1 < 2;\n```\n", {extensions:[mermaid()]})
    assert_eq!(
        carve::to_html_with_options("``` js\nlet x = 1 < 2;\n```\n", &opts),
        "<pre><code class=\"language-js\">let x = 1 &lt; 2;\n</code></pre>"
    );
}

// ---------------------------------------------------------------------------
// math-block
// ---------------------------------------------------------------------------

#[test]
fn math_block_integral_golden() {
    let ext = MathBlock::new();
    let opts = Options::new().with_extension(&ext);
    // carveToHtml("``` math\n\\int_0^1 x^2 \\, dx\n```\n", {extensions:[mathBlock()]})
    assert_eq!(
        carve::to_html_with_options("``` math\n\\int_0^1 x^2 \\, dx\n```\n", &opts),
        "<div class=\"math display\">\\[\\int_0^1 x^2 \\, dx\\]</div>"
    );
}

#[test]
fn math_block_escapes_amp_lt_gt() {
    let ext = MathBlock::new();
    let opts = Options::new().with_extension(&ext);
    // `>` is escaped too (unlike Mermaid), matching the core math renderer.
    assert_eq!(
        carve::to_html_with_options("``` math\na < b & c > d\n```\n", &opts),
        "<div class=\"math display\">\\[a &lt; b &amp; c &gt; d\\]</div>"
    );
}

#[test]
fn math_block_single_line_no_trailing_newline() {
    let ext = MathBlock::new();
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options("``` math\nx^2\n```\n", &opts),
        "<div class=\"math display\">\\[x^2\\]</div>"
    );
}

#[test]
fn math_block_non_math_defers_golden() {
    let ext = MathBlock::new();
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options("``` js\nlet x = 1 < 2;\n```\n", &opts),
        "<pre><code class=\"language-js\">let x = 1 &lt; 2;\n</code></pre>"
    );
}

#[test]
fn math_block_inert_without_extension() {
    let opts = Options::new();
    // Without the extension, a ```math block stays a plain code block.
    assert_eq!(
        carve::to_html_with_options("``` math\nx^2\n```\n", &opts),
        "<pre><code class=\"language-math\">x^2\n</code></pre>"
    );
}

#[test]
fn math_block_merges_classes_and_copies_attributes() {
    let ext = MathBlock::new();
    let opts = Options::new().with_extension(&ext);
    // Author classes merge after the `math display` base; id and other attrs
    // follow in source order, mirroring core display `$$` math (class-first).
    assert_eq!(
        carve::to_html_with_options("{#eq .big data-ref=x}\n``` math\nx^2\n```\n", &opts),
        "<div class=\"math display big\" id=\"eq\" data-ref=\"x\">\\[x^2\\]</div>"
    );
}

#[test]
fn math_block_strips_event_handler_attributes() {
    let ext = MathBlock::new();
    let opts = Options::new().with_extension(&ext);
    // Always-on attribute hardening strips event handlers regardless of options,
    // while safe author attributes (id, classes) survive.
    assert_eq!(
        carve::to_html_with_options(
            "{#eq .big onclick=\"alert(1)\"}\n``` math\nx^2\n```\n",
            &opts
        ),
        "<div class=\"math display big\" id=\"eq\">\\[x^2\\]</div>"
    );
}

// ---------------------------------------------------------------------------
// spoiler
// ---------------------------------------------------------------------------

#[test]
fn spoiler_inline_renders_span() {
    let ext = Spoiler::new();
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options("Plot: :spoiler[the butler did it].", &opts),
        "<p>Plot: <span class=\"spoiler\">the butler did it</span>.</p>"
    );
}

#[test]
fn spoiler_inline_merges_classes_and_strips_event_handler() {
    let ext = Spoiler::new();
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options(":spoiler[x]{#s .big onclick=\"y\"}", &opts),
        "<p><span class=\"spoiler big\" id=\"s\">x</span></p>"
    );
}

#[test]
fn spoiler_inline_falls_back_to_ext_span_without_extension() {
    assert_eq!(
        carve::to_html("Plot: :spoiler[x]."),
        "<p>Plot: <span class=\"ext-spoiler\">x</span>.</p>"
    );
}

#[test]
fn spoiler_block_renders_details_disclosure() {
    let ext = Spoiler::new();
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options("::: spoiler \"Ending\"\nEveryone lives.\n:::", &opts),
        "<details class=\"spoiler\">\n  <summary>Ending</summary>\n  <p>Everyone lives.</p>\n</details>"
    );
}

#[test]
fn spoiler_block_defaults_summary() {
    let ext = Spoiler::new();
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options("::: spoiler\nHidden.\n:::", &opts),
        "<details class=\"spoiler\">\n  <summary>Spoiler</summary>\n  <p>Hidden.</p>\n</details>"
    );
}

#[test]
fn spoiler_block_falls_back_to_div_without_extension() {
    assert_eq!(
        carve::to_html("::: spoiler\nHidden.\n:::"),
        "<div class=\"spoiler\">\n  <p>Hidden.</p>\n</div>"
    );
}

// ---------------------------------------------------------------------------
// color swatch
// ---------------------------------------------------------------------------

#[test]
fn color_swatch_hex_renders_chip_and_label() {
    let ext = ColorSwatch::new();
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options(":color[#ff8800]", &opts),
        "<p><span class=\"swatch\"><span class=\"swatch-chip\" style=\"background-color:#ff8800\"></span> #ff8800</span></p>"
    );
}

#[test]
fn color_swatch_named_color_renders_chip_and_label() {
    let ext = ColorSwatch::new();
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options(":color[rebeccapurple]", &opts),
        "<p><span class=\"swatch\"><span class=\"swatch-chip\" style=\"background-color:rebeccapurple\"></span> rebeccapurple</span></p>"
    );
}

#[test]
fn color_swatch_function_color_renders_chip_and_label() {
    let ext = ColorSwatch::new();
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options(":color[rgb(248,81,73)]", &opts),
        "<p><span class=\"swatch\"><span class=\"swatch-chip\" style=\"background-color:rgb(248,81,73)\"></span> rgb(248,81,73)</span></p>"
    );
}

#[test]
fn color_swatch_contrast_dark_hex_uses_white_text() {
    let ext = ColorSwatch::new();
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options(":color[#0d1117]{contrast}", &opts),
        "<p><span class=\"swatch-label\" style=\"background:#0d1117;color:#fff\">#0d1117</span></p>"
    );
}

#[test]
fn color_swatch_contrast_mid_hex_uses_black_text() {
    let ext = ColorSwatch::new();
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options(":color[#58a6ff]{contrast}", &opts),
        "<p><span class=\"swatch-label\" style=\"background:#58a6ff;color:#000\">#58a6ff</span></p>"
    );
}

#[test]
fn color_swatch_contrast_light_hex_uses_black_text() {
    let ext = ColorSwatch::new();
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options(":color[#f0f6fc]{contrast}", &opts),
        "<p><span class=\"swatch-label\" style=\"background:#f0f6fc;color:#000\">#f0f6fc</span></p>"
    );
}

#[test]
fn color_swatch_contrast_rgb_integer_uses_black_text() {
    let ext = ColorSwatch::new();
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options(":color[rgb(240,246,252)]{contrast}", &opts),
        "<p><span class=\"swatch-label\" style=\"background:rgb(240,246,252);color:#000\">rgb(240,246,252)</span></p>"
    );
}

#[test]
fn color_swatch_contrast_merges_author_attrs_and_consumes_flag() {
    let ext = ColorSwatch::new();
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options(":color[#fff]{contrast .x #y}", &opts),
        "<p><span class=\"swatch-label x\" id=\"y\" style=\"background:#fff;color:#000\">#fff</span></p>"
    );
}

#[test]
fn color_swatch_contrast_declines_fully_transparent_color() {
    let ext = ColorSwatch::new();
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options(":color[#00000000]{contrast}", &opts),
        "<p><span class=\"swatch\"><span class=\"swatch-chip\" style=\"background-color:#00000000\"></span> #00000000</span></p>"
    );
}

#[test]
fn color_swatch_contrast_author_style_wins_without_duplicate() {
    let ext = ColorSwatch::new();
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options(":color[#fff]{contrast style=\"opacity:0.5\"}", &opts),
        "<p><span class=\"swatch-label\" style=\"opacity:0.5\">#fff</span></p>"
    );
}

#[test]
fn color_swatch_contrast_named_color_falls_back_to_normal_swatch() {
    let ext = ColorSwatch::new();
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options(":color[rebeccapurple]{contrast}", &opts),
        "<p><span class=\"swatch\"><span class=\"swatch-chip\" style=\"background-color:rebeccapurple\"></span> rebeccapurple</span></p>"
    );
}

#[test]
fn color_swatch_merges_author_attrs_on_outer_span() {
    let ext = ColorSwatch::new();
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options(":color[#fff]{#x .y onclick=\"z\"}", &opts),
        "<p><span class=\"swatch y\" id=\"x\"><span class=\"swatch-chip\" style=\"background-color:#fff\"></span> #fff</span></p>"
    );
}

#[test]
fn color_swatch_invalid_value_defers_to_generic_fallback() {
    let ext = ColorSwatch::new();
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options(":color[nope!]", &opts),
        "<p><span class=\"ext-color\">nope!</span></p>"
    );
    assert_eq!(
        carve::to_html_with_options(":color[red;}x{}]", &opts),
        "<p><span class=\"ext-color\">red;}x{}</span></p>"
    );
}

#[test]
fn color_swatch_bareword_that_is_not_a_named_color_defers_to_fallback() {
    let ext = ColorSwatch::new();
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options(":color[banana]", &opts),
        "<p><span class=\"ext-color\">banana</span></p>"
    );
}

#[test]
fn color_swatch_named_color_matches_case_insensitively() {
    let ext = ColorSwatch::new();
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options(":color[DarkSlateGray]", &opts),
        "<p><span class=\"swatch\"><span class=\"swatch-chip\" style=\"background-color:DarkSlateGray\"></span> DarkSlateGray</span></p>"
    );
}

#[test]
fn color_swatch_position_after_puts_chip_after_value() {
    let ext = ColorSwatch::new().position(SwatchPosition::After);
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options(":color[#3b82f6]", &opts),
        "<p><span class=\"swatch\">#3b82f6 <span class=\"swatch-chip\" style=\"background-color:#3b82f6\"></span></span></p>"
    );
}

#[test]
fn color_swatch_position_none_renders_chip_only_with_title() {
    let ext = ColorSwatch::new().position(SwatchPosition::None);
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options(":color[#3b82f6]", &opts),
        "<p><span class=\"swatch swatch-chip-only\" title=\"#3b82f6\"><span class=\"swatch-chip\" style=\"background-color:#3b82f6\"></span></span></p>"
    );
}

#[test]
fn color_swatch_shape_round_adds_modifier_class() {
    let ext = ColorSwatch::new().shape(SwatchShape::Round);
    let opts = Options::new().with_extension(&ext);
    assert!(
        carve::to_html_with_options(":color[#3b82f6]", &opts).contains(
            "<span class=\"swatch-chip swatch-chip-round\" style=\"background-color:#3b82f6\">"
        )
    );
}

#[test]
fn color_swatch_shape_ring_uses_border_color() {
    let ext = ColorSwatch::new().shape(SwatchShape::Ring);
    let opts = Options::new().with_extension(&ext);
    let html = carve::to_html_with_options(":color[#3b82f6]", &opts);
    assert!(html.contains("swatch-chip-ring"));
    assert!(html.contains("style=\"border-color:#3b82f6\""));
    assert!(!html.contains("background-color:#3b82f6"));
}

#[test]
fn color_swatch_tint_paints_color_mix_behind_swatch() {
    let ext = ColorSwatch::new().tint(true);
    let opts = Options::new().with_extension(&ext);
    let html = carve::to_html_with_options(":color[#3b82f6]", &opts);
    assert!(html.contains("class=\"swatch swatch-tint\""));
    assert!(
        html.contains("style=\"background-color:color-mix(in srgb, #3b82f6 12%, transparent)\"")
    );
}

#[test]
fn color_swatch_reveal_wraps_value_and_makes_swatch_focusable() {
    let ext = ColorSwatch::new().reveal(true);
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options(":color[#3b82f6]", &opts),
        "<p><span class=\"swatch swatch-reveal\" tabindex=\"0\"><span class=\"swatch-chip\" style=\"background-color:#3b82f6\"></span> <span class=\"swatch-val\">#3b82f6</span></span></p>"
    );
}

#[test]
fn color_swatch_reveal_with_position_after_wraps_value_before_chip() {
    let ext = ColorSwatch::new()
        .position(SwatchPosition::After)
        .reveal(true);
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options(":color[#3b82f6]", &opts),
        "<p><span class=\"swatch swatch-reveal\" tabindex=\"0\"><span class=\"swatch-val\">#3b82f6</span> <span class=\"swatch-chip\" style=\"background-color:#3b82f6\"></span></span></p>"
    );
}

#[test]
fn color_swatch_reveal_is_ignored_when_position_is_none() {
    // `none` already hides the value (surfaced via title); reveal must be a no-op.
    let ext = ColorSwatch::new()
        .position(SwatchPosition::None)
        .reveal(true);
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options(":color[#3b82f6]", &opts),
        "<p><span class=\"swatch swatch-chip-only\" title=\"#3b82f6\"><span class=\"swatch-chip\" style=\"background-color:#3b82f6\"></span></span></p>"
    );
}

// ---------------------------------------------------------------------------
// fenced-render
// ---------------------------------------------------------------------------

#[test]
fn fenced_render_text_mode_escapes_amp_lt_keeps_gt() {
    let ext = FencedRender::d2();
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options("``` d2\na -> b & <c\n```\n", &opts),
        "<pre class=\"d2\">a -> b &amp; &lt;c</pre>"
    );
}

#[test]
fn fenced_render_graphviz_claims_dot_and_graphviz() {
    let ext = FencedRender::graphviz();
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options("``` dot\na -> b\n```\n", &opts),
        "<pre class=\"graphviz\">a -> b</pre>"
    );
    assert_eq!(
        carve::to_html_with_options("``` graphviz\na -> b\n```\n", &opts),
        "<pre class=\"graphviz\">a -> b</pre>"
    );
}

#[test]
fn fenced_render_json_mode_wraps_in_script_inside_div() {
    let ext = FencedRender::vega_lite();
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options("``` vega-lite\n{\"mark\": \"bar\"}\n```\n", &opts),
        "<div class=\"vega-lite\"><script type=\"application/json\">{\"mark\": \"bar\"}</script></div>"
    );
}

#[test]
fn fenced_render_json_mode_guards_script_close() {
    let ext = FencedRender::vega_lite();
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options("``` vega-lite\n{\"x\": \"</script>\"}\n```\n", &opts),
        "<div class=\"vega-lite\"><script type=\"application/json\">{\"x\": \"<\\/script>\"}</script></div>"
    );
}

#[test]
fn fenced_render_strips_event_handler_attributes() {
    let ext = FencedRender::d2();
    let opts = Options::new().with_extension(&ext);
    // Always-on hardening strips on* while safe attributes survive.
    assert_eq!(
        carve::to_html_with_options("{#c1 .tall onclick=\"x\"}\n``` d2\na\n```\n", &opts),
        "<pre id=\"c1\" class=\"d2 tall\">a</pre>"
    );
}

#[test]
fn fenced_render_defers_unclaimed_language() {
    let ext = FencedRender::d2();
    let opts = Options::new().with_extension(&ext);
    let html = carve::to_html_with_options("``` python\nprint(1)\n```\n", &opts);
    assert!(html.contains("class=\"language-python\""));
    assert!(!html.contains("class=\"d2\""));
}

#[test]
fn fenced_render_custom_tag_and_css_class() {
    use carve::ContentMode;
    let ext = FencedRender::with_options(carve::FencedRenderOptions::new(
        vec!["d2".into()],
        Some("diagram".into()),
        Some("div".into()),
        ContentMode::Text,
    ));
    let opts = Options::new().with_extension(&ext);
    assert_eq!(
        carve::to_html_with_options("``` d2\na -> b\n```\n", &opts),
        "<div class=\"diagram\">a -> b</div>"
    );
}

#[test]
fn fenced_render_mermaid_preset_matches_manual_instance() {
    let fr = FencedRender::new("mermaid");
    let mm = FencedRender::mermaid();
    let src = "``` mermaid\ngraph TD; A-->B\n```\n";
    assert_eq!(
        carve::to_html_with_options(src, &Options::new().with_extension(&fr)),
        carve::to_html_with_options(src, &Options::new().with_extension(&mm))
    );
}

#[test]
fn fenced_render_presets_register_all_languages() {
    let presets = FencedRender::presets();
    assert_eq!(presets.len(), 7);
    let mut opts = Options::new();
    for ext in &presets {
        opts = opts.with_extension(ext);
    }
    assert_eq!(
        carve::to_html_with_options("``` mermaid\ngraph TD; A-->B\n```\n", &opts),
        "<pre class=\"mermaid\">graph TD; A-->B</pre>"
    );
    assert!(
        carve::to_html_with_options("``` dot\ndigraph { a -> b }\n```\n", &opts)
            .contains("<pre class=\"graphviz\">")
    );
    assert!(
        carve::to_html_with_options("``` chart\n{\"type\":\"bar\"}\n```\n", &opts)
            .contains("<div class=\"chart\">")
    );
}

// ---------------------------------------------------------------------------
// table-of-contents
// ---------------------------------------------------------------------------

/// Collect every `<section id="...">` value in document order.
fn section_ids(html: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find("<section id=\"") {
        rest = &rest[i + "<section id=\"".len()..];
        let end = rest.find('"').expect("section id closing quote");
        ids.push(rest[..end].to_string());
        rest = &rest[end..];
    }
    ids
}

/// Collect every TOC anchor target (`<a href="#...">` inside the `<nav>`).
fn toc_anchor_targets(html: &str) -> Vec<String> {
    let nav_start = html.find("<nav").expect("nav present");
    let nav_end = html[nav_start..].find("</nav>").expect("nav closed") + nav_start;
    let nav = &html[nav_start..nav_end];
    let mut targets = Vec::new();
    let mut rest = nav;
    while let Some(i) = rest.find("href=\"#") {
        rest = &rest[i + "href=\"#".len()..];
        let end = rest.find('"').expect("href closing quote");
        targets.push(rest[..end].to_string());
        rest = &rest[end..];
    }
    targets
}

/// Render `input` with the TOC + Citations extensions enabled and assert every
/// TOC anchor target resolves to an actual heading section id.
fn assert_toc_anchors_match_heading_ids(input: &str) {
    let cit = Citations::default();
    let toc = TableOfContents::with_options(TableOfContentsOptions {
        lowercase_ids: true,
        ..Default::default()
    });
    let opts = Options::new()
        .with_extension(&cit)
        .with_extension(&toc)
        .with_lowercase_heading_ids(true);
    let html = carve::to_html_with_options(input, &opts);
    let ids = section_ids(&html);
    let targets = toc_anchor_targets(&html);
    assert!(
        !targets.is_empty(),
        "expected at least one TOC anchor for input: {input:?}\n{html}"
    );
    for target in &targets {
        assert!(
            ids.contains(target),
            "TOC anchor (#{target}) has no matching heading id ({ids:?}) for input: {input:?}\n{html}"
        );
    }
}

/// Regression: a TOC entry's anchor must point at the heading's own id, for any
/// inline content. The TOC previously kept its own flattening that ignored
/// `CitationGroup` nodes (which the core section-id pass includes), so a heading
/// like `# Research [@doe]` produced a dead TOC link: `href="#research"` against
/// `id="research-doe"`. The fix shares the core's `plain_inlines` between the
/// TOC and the renderer, so these cases pin TOC anchor == heading id for every
/// inline node type.
#[test]
fn toc_anchor_always_equals_heading_id() {
    // The original bug: citation group in the heading.
    assert_toc_anchors_match_heading_ids("# Research [@doe]\n\n[@doe]: John Doe, 2020\n");
    // Other inline node types - guard against future drift in either pass.
    assert_toc_anchors_match_heading_ids("# Hi @bob there\n");
    assert_toc_anchors_match_heading_ids("# Energy $E=mc^2$ done\n");
    assert_toc_anchors_match_heading_ids("# Smile :smile: here\n");
    assert_toc_anchors_match_heading_ids("# Note[^a] here\n\n[^a]: body\n");
    assert_toc_anchors_match_heading_ids("# See [](#target) here\n");
    assert_toc_anchors_match_heading_ids("# A [span text]{.cls} b\n");
    assert_toc_anchors_match_heading_ids("# Tag #foo here\n");
    assert_toc_anchors_match_heading_ids("# Plain heading\n");
}

#[test]
fn toc_top_nested_golden() {
    let ext = TableOfContents::with_options(TableOfContentsOptions {
        lowercase_ids: true,
        ..Default::default()
    });
    let opts = Options::new()
        .with_extension(&ext)
        .with_lowercase_heading_ids(true);
    // carveToHtml("# Intro\n\nText.\n\n## Details\n\nMore.\n", {extensions:[tableOfContents()]})
    assert_eq!(
        carve::to_html_with_options("# Intro\n\nText.\n\n## Details\n\nMore.\n", &opts),
        "<nav class=\"toc\">\n<ul>\n<li><a href=\"#intro\">Intro</a>\n<ul>\n<li><a href=\"#details\">Details</a></li>\n</ul>\n</li>\n</ul>\n</nav>\n<section id=\"intro\">\n  <h1>Intro</h1>\n  <p>Text.</p>\n  <section id=\"details\">\n    <h2>Details</h2>\n    <p>More.</p>\n  </section>\n</section>"
    );
}

#[test]
fn toc_bottom_ol_golden() {
    let ext = TableOfContents::with_options(TableOfContentsOptions {
        lowercase_ids: true,
        position: Position::Bottom,
        list_type: ListType::Ol,
        ..Default::default()
    });
    let opts = Options::new()
        .with_extension(&ext)
        .with_lowercase_heading_ids(true);
    // The generated <nav> HTML is byte-identical to carve-js; the ONLY
    // difference is leading indentation. carve-js renders the appended
    // bottom-position raw block as a flat top-level child (no indent), while
    // carve-rs's core `render_section` absorbs a trailing top-level block into
    // the last open <section> and indents it. That section-absorption is a
    // pre-existing CORE rendering trait, unrelated to this extension, so the
    // assertion encodes the carve-rs output (nav contents match js exactly).
    let html = carve::to_html_with_options("# Alpha\n\n## Beta\n", &opts);
    assert!(html.contains(
        "<nav class=\"toc\">\n<ol>\n<li><a href=\"#alpha\">Alpha</a>\n<ol>\n<li><a href=\"#beta\">Beta</a></li>\n</ol>\n</li>\n</ol>\n</nav>"
    ));
    assert_eq!(
        html,
        "<section id=\"alpha\">\n  <h1>Alpha</h1>\n  <section id=\"beta\">\n    <h2>Beta</h2>\n    <nav class=\"toc\">\n<ol>\n<li><a href=\"#alpha\">Alpha</a>\n<ol>\n<li><a href=\"#beta\">Beta</a></li>\n</ol>\n</li>\n</ol>\n</nav>\n  </section>\n</section>"
    );
}

// ---------------------------------------------------------------------------
// tab-normalize
// ---------------------------------------------------------------------------

#[test]
fn tab_normalize_code_block_default_width_golden() {
    let ext = TabNormalize::new();
    let opts = Options::new().with_extension(&ext);
    // carveToHtml("```\n\tindented\n```\n", {extensions:[tabNormalize()]})
    assert_eq!(
        carve::to_html_with_options("```\n\tindented\n```\n", &opts),
        "<pre><code>  indented\n</code></pre>"
    );
}

#[test]
fn tab_normalize_inline_code_width_four_golden() {
    let ext = TabNormalize::with_width(4);
    let opts = Options::new().with_extension(&ext);
    // carveToHtml("Code: `a\tb`", {extensions:[tabNormalize(4)]})
    assert_eq!(
        carve::to_html_with_options("Code: `a\tb`", &opts),
        "<p>Code: <code>a    b</code></p>"
    );
}

// ---------------------------------------------------------------------------
// ::: toc placement directive
// ---------------------------------------------------------------------------

#[test]
fn toc_placement_renders_nested_nav_in_place() {
    let ext = TocPlacement::new();
    let opts = Options::new().with_extension(&ext);
    let html = carve::to_html_with_options(
        "::: toc\n:::\n\n# Intro\n\n## Setup\n\n### Details\n\n## Usage\n",
        &opts,
    );
    assert!(html.contains(
        "<nav class=\"toc\">\n<ul>\n<li><a href=\"#Intro\">Intro</a>\n<ul>\n\
<li><a href=\"#Setup\">Setup</a>\n<ul>\n<li><a href=\"#Details\">Details</a></li>\n</ul>\n</li>\n\
<li><a href=\"#Usage\">Usage</a></li>\n</ul>\n</li>\n</ul>\n</nav>"
    ));
    // Placed before the first heading section.
    assert!(html.find("<nav").expect("nav") < html.find("<h1").expect("h1"));
}

#[test]
fn toc_placement_depth_limits_levels() {
    let ext = TocPlacement::new();
    let opts = Options::new().with_extension(&ext);
    let html = carve::to_html_with_options(
        "# A\n\n{depth=2}\n::: toc\n:::\n\n## B\n\n### C\n\n## D\n",
        &opts,
    );
    assert!(html.contains("<a href=\"#B\">B</a>"));
    assert!(html.contains("<a href=\"#D\">D</a>"));
    assert!(!html.contains("href=\"#C\""));
}

#[test]
fn toc_placement_from_to_window() {
    let ext = TocPlacement::new();
    let opts = Options::new().with_extension(&ext);
    let html = carve::to_html_with_options(
        "# A\n\n{from=2 to=2}\n::: toc\n:::\n\n## B\n\n### C\n\n## D\n",
        &opts,
    );
    assert!(html.contains("<a href=\"#B\">B</a>"));
    assert!(html.contains("<a href=\"#D\">D</a>"));
    assert!(!html.contains("href=\"#A\""));
    assert!(!html.contains("href=\"#C\""));
}

#[test]
fn toc_placement_carries_author_attrs_and_strips_window_keys() {
    let ext = TocPlacement::new();
    let opts = Options::new().with_extension(&ext);
    let html =
        carve::to_html_with_options("# A\n\n{#nav .side depth=1}\n::: toc\n:::\n\n## B\n", &opts);
    assert!(html.contains("<nav id=\"nav\" class=\"toc side\">"));
    assert!(!html.contains("depth="));
}

#[test]
fn toc_placement_empty_window_renders_empty_nav() {
    let ext = TocPlacement::new();
    let opts = Options::new().with_extension(&ext);
    let html = carve::to_html_with_options("::: toc\n:::\n\nplain\n", &opts);
    assert!(html.contains("<nav class=\"toc\"></nav>"));
}

#[test]
fn toc_placement_inert_without_extension() {
    let html = carve::to_html("# A\n\n::: toc\n:::\n");
    assert!(html.contains("class=\"toc\""));
    assert!(!html.contains("<nav"));
}

// ---------------------------------------------------------------------------
// ::: footnotes placement directive (core, no extension)
// ---------------------------------------------------------------------------

#[test]
fn footnotes_placement_flushes_at_marker() {
    let html = carve::to_html("Intro[^a].\n\n::: footnotes\n:::\n\n## After\n\n[^a]: note a\n");
    assert!(html.find("role=\"doc-endnotes\"").expect("endnotes") < html.find("<h2").expect("h2"));
    assert!(html.contains("<li id=\"fn1\">"));
}

#[test]
fn footnotes_placement_includes_later_references() {
    // Flush is "all footnotes", including those referenced after the marker.
    let html = carve::to_html(
        "A[^a].\n\n::: footnotes\n:::\n\n## After\n\nB[^b].\n\n[^a]: a\n\n[^b]: b\n",
    );
    assert!(html.contains("<li id=\"fn1\">"));
    assert!(html.contains("<li id=\"fn2\">"));
    assert_eq!(html.matches("role=\"doc-endnotes\"").count(), 1);
}

#[test]
fn footnotes_no_marker_renders_at_end() {
    let html = carve::to_html("Intro[^a].\n\n## After\n\n[^a]: note a\n");
    assert!(html.find("<h2").expect("h2") < html.find("role=\"doc-endnotes\"").expect("endnotes"));
}

#[test]
fn footnotes_placement_degrades_without_footnotes() {
    let html = carve::to_html("Plain.\n\n::: footnotes\n:::\n");
    assert!(html.contains("<div class=\"footnotes\"></div>"));
    assert!(!html.contains("doc-endnotes"));
}

#[test]
fn footnotes_placement_second_marker_no_duplicate() {
    let html = carve::to_html("X[^a].\n\n::: footnotes\n:::\n\n::: footnotes\n:::\n\n[^a]: a\n");
    assert_eq!(html.matches("role=\"doc-endnotes\"").count(), 1);
}

#[test]
fn footnotes_placement_nested_in_definition_never_leaks_sentinel() {
    // A `::: footnotes` inside a footnote definition renders as an ordinary div,
    // never the internal placement sentinel.
    let html = carve::to_html("X[^a].\n\n[^a]: ::: footnotes\n    :::\n");
    assert!(!html.contains('\u{0}'));
    assert!(!html.contains("footnotes-placement"));
    assert!(html.contains("<div class=\"footnotes\">"));
}

#[test]
fn toc_placement_includes_nested_container_headings() {
    // Headings inside ::: note, blockquotes, lists render with id anchors, so
    // the placed TOC includes them.
    let ext = TocPlacement::new();
    let opts = Options::new().with_extension(&ext);
    let html = carve::to_html_with_options(
        "::: toc\n:::\n\n# Top\n\n::: note\n## InNote\n:::\n\n> ## InQuote\n",
        &opts,
    );
    assert!(html.contains("<a href=\"#InNote\">InNote</a>"));
    assert!(html.contains("<a href=\"#InQuote\">InQuote</a>"));
}
