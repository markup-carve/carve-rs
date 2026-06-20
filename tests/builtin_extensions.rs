//! Unit and golden-parity tests for the built-in extensions.
//!
//! Golden strings were captured from carve-js (`dist/index.js`) for the same
//! input + extension config; the comment above each golden records the command.
//! For TOC / heading-permalinks the carve-js default slug is lowercase, so the
//! renderer is run with `with_lowercase_heading_ids(true)` and the extension's
//! `lowercase_ids` flag set to match.

use carve::{
    Autolink, AutolinkOptions, ExternalLinks, ExternalLinksOptions, HeadingPermalinks,
    HeadingPermalinksOptions, ListType, MathBlock, Mermaid, Options, Position, TabNormalize,
    TableOfContents, TableOfContentsOptions, Wikilinks, WikilinksOptions,
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

// ---------------------------------------------------------------------------
// mermaid
// ---------------------------------------------------------------------------

#[test]
fn mermaid_diagram_golden() {
    let ext = Mermaid::new();
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
    let ext = Mermaid::new();
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
    let ext = Mermaid::new();
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
fn math_block_does_not_copy_fence_attributes() {
    let ext = MathBlock::new();
    let opts = Options::new().with_extension(&ext);
    // Author attributes on the fence are dropped (only the fixed `math display`
    // class is emitted), so they cannot bypass safe-mode attribute filtering.
    assert_eq!(
        carve::to_html_with_options(
            "{#eq .big onclick=\"alert(1)\"}\n``` math\nx^2\n```\n",
            &opts
        ),
        "<div class=\"math display\">\\[x^2\\]</div>"
    );
}

// ---------------------------------------------------------------------------
// table-of-contents
// ---------------------------------------------------------------------------

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
        "<nav class=\"toc\"><ul><li><a href=\"#intro\">Intro</a><ul><li><a href=\"#details\">Details</a></li></ul></li></ul></nav>\n<section id=\"intro\">\n  <h1>Intro</h1>\n  <p>Text.</p>\n  <section id=\"details\">\n    <h2>Details</h2>\n    <p>More.</p>\n  </section>\n</section>"
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
        "<nav class=\"toc\"><ol><li><a href=\"#alpha\">Alpha</a><ol><li><a href=\"#beta\">Beta</a></li></ol></li></ol></nav>"
    ));
    assert_eq!(
        html,
        "<section id=\"alpha\">\n  <h1>Alpha</h1>\n  <section id=\"beta\">\n    <h2>Beta</h2>\n    <nav class=\"toc\"><ol><li><a href=\"#alpha\">Alpha</a><ol><li><a href=\"#beta\">Beta</a></li></ol></li></ol></nav>\n  </section>\n</section>"
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
