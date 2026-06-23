//! Static render-mode parity tests.
//!
//! Mirrors carve-js `test/static-render-mode.test.ts` (PR #242) and carve-php
//! `StaticRenderModeTest` (PR #240): interactive vs static for each interactive
//! extension carve-rs ships, the no-renderer source fallback and the
//! with-renderer SSR path, the caption floor, and escaping. carve-rs has no
//! Tabs / CodeGroup extension (those are carve-js / carve-php only), so this
//! battery covers Details, Spoiler, FencedRender (mermaid / chart), MathBlock,
//! and core inline / display math.

use carve::{Details, FencedRender, MathBlock, Mode, Options, Spoiler, StaticRenderers};

/// Render `src` to HTML with the given extensions in interactive mode.
fn interactive(src: &str, exts: &[&dyn carve::CarveExtension]) -> String {
    let mut opts = Options::new().with_mode(Mode::Interactive);
    for ext in exts {
        opts = opts.with_extension(*ext);
    }
    carve::to_html_with_options(src, &opts).trim().to_string()
}

/// Render `src` to HTML with the given extensions in static mode (no renderers).
fn static_html(src: &str, exts: &[&dyn carve::CarveExtension]) -> String {
    let mut opts = Options::new().with_mode(Mode::Static);
    for ext in exts {
        opts = opts.with_extension(*ext);
    }
    carve::to_html_with_options(src, &opts).trim().to_string()
}

// --- option plumbing ---------------------------------------------------------

#[test]
fn omitting_mode_is_interactive_non_breaking() {
    let ext = Details::new();
    let src = "::: details \"More\"\nBody.\n:::";
    // The default Options (no with_mode call) must match an explicit Interactive.
    let omitted = carve::to_html_with_options(src, &Options::new().with_extension(&ext));
    let explicit = carve::to_html_with_options(
        src,
        &Options::new()
            .with_mode(Mode::Interactive)
            .with_extension(&ext),
    );
    assert_eq!(omitted, explicit);
    assert!(omitted.contains("<details>"));
}

#[test]
fn mode_is_an_enum_so_unknown_values_are_unrepresentable() {
    // The mode is a closed Rust enum (Interactive | Static); an unknown mode
    // like "print" simply cannot be constructed, satisfying the spec's "MUST
    // reject an unknown mode value" by construction (no string parsing).
    assert_eq!(Mode::default(), Mode::Interactive);
}

// --- details -----------------------------------------------------------------

#[test]
fn details_interactive_is_disclosure_static_is_expanded_section() {
    let ext = Details::new();
    let src = "::: details \"More info\"\nHidden body.\n:::";

    assert_eq!(
        interactive(src, &[&ext]),
        [
            "<details>",
            "  <summary>More info</summary>",
            "  <p>Hidden body.</p>",
            "</details>",
        ]
        .join("\n")
    );

    assert_eq!(
        static_html(src, &[&ext]),
        [
            "<section class=\"details\">",
            "  <h3 class=\"details-title\">More info</h3>",
            "  <p>Hidden body.</p>",
            "</section>",
        ]
        .join("\n")
    );
}

#[test]
fn details_static_default_title_when_none() {
    let ext = Details::new();
    assert!(static_html("::: details\nBody.\n:::", &[&ext])
        .contains("<h3 class=\"details-title\">Details</h3>"));
}

#[test]
fn details_static_preserves_grouping_label_after_title() {
    let ext = Details::new();
    let html = static_html("::: details \"More\" [Build]\nBody.\n:::", &[&ext]);
    assert!(html.contains("<h3 class=\"details-title\">More</h3>"));
    assert!(html.contains("<p class=\"div-label\">Build</p>"));
    // Title first, then the label floor.
    assert!(html.find("details-title").unwrap() < html.find("div-label").unwrap());
}

#[test]
fn details_static_merges_author_classes_into_one_attribute() {
    let ext = Details::new();
    // Attributes attach via a preceding block-attribute line (strict djot).
    let html = static_html("{.wide}\n::: details \"More\"\nBody.\n:::", &[&ext]);
    assert!(html.contains("<section class=\"details wide\">"));
    // section + h3 = exactly two class attributes (no duplicate `class`).
    assert_eq!(html.matches("class=\"").count(), 2);
}

// --- spoiler -----------------------------------------------------------------

#[test]
fn spoiler_inline_static_reveals_content() {
    let ext = Spoiler::new();
    assert_eq!(
        interactive("Plot: :spoiler[the butler did it].", &[&ext]),
        "<p>Plot: <span class=\"spoiler\">the butler did it</span>.</p>"
    );
    assert_eq!(
        static_html("Plot: :spoiler[the butler did it].", &[&ext]),
        "<p>Plot: <span class=\"spoiler spoiler-revealed\">the butler did it</span>.</p>"
    );
}

#[test]
fn spoiler_block_static_is_revealed_section() {
    let ext = Spoiler::new();
    let src = "::: spoiler \"Ending\"\nEveryone lives.\n:::";
    assert!(interactive(src, &[&ext]).contains("<details class=\"spoiler\">"));

    assert_eq!(
        static_html(src, &[&ext]),
        [
            "<section class=\"spoiler spoiler-revealed\">",
            "  <h3 class=\"spoiler-title\">Ending</h3>",
            "  <p>Everyone lives.</p>",
            "</section>",
        ]
        .join("\n")
    );
}

#[test]
fn spoiler_block_static_preserves_label() {
    let ext = Spoiler::new();
    let html = static_html("::: spoiler \"End\" [Build]\nOver.\n:::", &[&ext]);
    assert!(html.contains("<h3 class=\"spoiler-title\">End</h3>"));
    assert!(html.contains("<p class=\"div-label\">Build</p>"));
}

// --- fenced-render: mermaid --------------------------------------------------

#[test]
fn mermaid_interactive_is_client_hydration_pre() {
    let ext = FencedRender::mermaid();
    assert_eq!(
        interactive("``` mermaid\ngraph TD; A --> B\n```\n", &[&ext]),
        "<pre class=\"mermaid\">graph TD; A --> B</pre>"
    );
}

#[test]
fn mermaid_static_no_renderer_degrades_to_escaped_source() {
    let ext = FencedRender::mermaid();
    // No renderers map -> source fallback. `escape_text` escapes `>` (unlike the
    // interactive text mode which keeps it for arrow syntax). Trailing `\n`
    // before `</code>`.
    assert_eq!(
        static_html("``` mermaid\ngraph TD; A --> B\n```\n", &[&ext]),
        "<pre class=\"mermaid\"><code class=\"language-mermaid\">graph TD; A --&gt; B\n</code></pre>"
    );
}

#[test]
fn mermaid_static_source_fallback_preserves_fence_attributes() {
    let ext = FencedRender::mermaid();
    let html = static_html("{#d1 .bordered}\n``` mermaid\nA --> B\n```\n", &[&ext]);
    assert!(html.contains("<pre id=\"d1\" class=\"mermaid bordered\">"));
}

#[test]
fn mermaid_static_with_renderer_emits_injected_svg() {
    let ext = FencedRender::mermaid();
    let opts = Options::new()
        .with_mode(Mode::Static)
        .with_extension(&ext)
        .with_renderers(StaticRenderers {
            mermaid: Some(Box::new(|src: &str| {
                format!("<svg data-src=\"{}\"><!--diagram--></svg>", src.len())
            })),
            ..Default::default()
        });
    // The closure receives the verbatim source "graph TD; A --> B" (17 bytes).
    let html = carve::to_html_with_options("``` mermaid\ngraph TD; A --> B\n```\n", &opts);
    assert_eq!(html.trim(), "<svg data-src=\"17\"><!--diagram--></svg>");
    assert!(!html.contains("<pre"));
}

// --- fenced-render: chart ----------------------------------------------------

#[test]
fn chart_static_no_renderer_keeps_json_source_as_pre_code() {
    let ext = FencedRender::chart();
    let html = static_html("``` chart\n{\"type\":\"bar\"}\n```\n", &[&ext]);
    assert!(html.contains("<pre class=\"chart\"><code class=\"language-chart\">"));
    assert!(html.contains("{\"type\":\"bar\"}"));
    // No live <script> in static mode (the interactive json mode would emit one).
    assert!(!html.contains("<script"));
}

#[test]
fn chart_static_with_renderer_emits_injected_image() {
    let ext = FencedRender::chart();
    let opts = Options::new()
        .with_mode(Mode::Static)
        .with_extension(&ext)
        .with_renderers(StaticRenderers {
            chart: Some(Box::new(|_src: &str| {
                "<img alt=\"chart\" src=\"chart.png\">".to_string()
            })),
            ..Default::default()
        });
    let html = carve::to_html_with_options("``` chart\n{\"type\":\"bar\"}\n```\n", &opts);
    assert_eq!(html.trim(), "<img alt=\"chart\" src=\"chart.png\">");
}

#[test]
fn other_presets_always_degrade_to_source_even_in_static() {
    // d2 has no static_renderer key, so even with a mermaid renderer supplied it
    // degrades to source (the renderer is not its build hook).
    let ext = FencedRender::d2();
    let opts = Options::new()
        .with_mode(Mode::Static)
        .with_extension(&ext)
        .with_renderers(StaticRenderers {
            mermaid: Some(Box::new(|_: &str| "<svg/>".to_string())),
            ..Default::default()
        });
    let html = carve::to_html_with_options("``` d2\na -> b\n```\n", &opts);
    // Source fallback uses escape_text (escapes `>`), unlike the interactive
    // text mode which keeps `>` for arrow syntax.
    assert_eq!(
        html.trim(),
        "<pre class=\"d2\"><code class=\"language-d2\">a -&gt; b\n</code></pre>"
    );
}

// --- math-block fence --------------------------------------------------------

#[test]
fn math_block_static_no_renderer_keeps_source() {
    let ext = MathBlock::new();
    // Identical to interactive when no math renderer is supplied.
    assert_eq!(
        static_html("``` math\n\\int_0^1 x^2\n```\n", &[&ext]),
        "<div class=\"math display\">\\[\\int_0^1 x^2\\]</div>"
    );
    assert_eq!(
        interactive("``` math\n\\int_0^1 x^2\n```\n", &[&ext]),
        "<div class=\"math display\">\\[\\int_0^1 x^2\\]</div>"
    );
}

#[test]
fn math_block_static_with_renderer_emits_ssr_in_div() {
    let ext = MathBlock::new();
    let opts = Options::new()
        .with_mode(Mode::Static)
        .with_extension(&ext)
        .with_renderers(StaticRenderers {
            math: Some(Box::new(|_tex: &str, display: bool| {
                format!("<math data-display=\"{display}\">SSR</math>")
            })),
            ..Default::default()
        });
    let html = carve::to_html_with_options("``` math\n\\int_0^1 x^2\n```\n", &opts);
    // Block math always passes display = true.
    assert_eq!(
        html.trim(),
        "<div class=\"math display\"><math data-display=\"true\">SSR</math></div>"
    );
}

// --- core inline / display math ----------------------------------------------

#[test]
fn core_inline_math_static_no_renderer_keeps_source() {
    // No extension needed - core math. Static without a renderer == interactive.
    let opts = Options::new().with_mode(Mode::Static);
    let html = carve::to_html_with_options("Euler: $`e^{i\\pi}`.", &opts);
    assert!(html.contains("<span class=\"math inline\">\\(e^{i\\pi}\\)</span>"));
}

#[test]
fn core_inline_math_static_with_renderer_emits_mathml() {
    let opts = Options::new()
        .with_mode(Mode::Static)
        .with_renderers(StaticRenderers {
            math: Some(Box::new(|tex: &str, display: bool| {
                format!("<math data-display=\"{display}\">{tex}</math>")
            })),
            ..Default::default()
        });
    let html = carve::to_html_with_options("Euler: $`e^{i\\pi}`.", &opts);
    assert!(html.contains(
        "<span class=\"math inline\"><math data-display=\"false\">e^{i\\pi}</math></span>"
    ));
}

#[test]
fn core_display_math_static_with_renderer_passes_display_true() {
    let opts = Options::new()
        .with_mode(Mode::Static)
        .with_renderers(StaticRenderers {
            math: Some(Box::new(|tex: &str, display: bool| {
                format!("<math data-display=\"{display}\">{tex}</math>")
            })),
            ..Default::default()
        });
    let html = carve::to_html_with_options("$$`\\frac{a}{b}`", &opts);
    assert!(html.contains(
        "<span class=\"math display\"><math data-display=\"true\">\\frac{a}{b}</math></span>"
    ));
}

#[test]
fn core_math_renderer_ignored_in_interactive_mode() {
    // The renderers map is consulted ONLY on the static path.
    let opts = Options::new()
        .with_mode(Mode::Interactive)
        .with_renderers(StaticRenderers {
            math: Some(Box::new(|_t: &str, _d: bool| {
                "<math>SSR</math>".to_string()
            })),
            ..Default::default()
        });
    let html = carve::to_html_with_options("Euler: $`e^{i\\pi}`.", &opts);
    assert!(html.contains("\\(e^{i\\pi}\\)"));
    assert!(!html.contains("<math>"));
}

// --- mode is HTML-only: non-HTML renderers ignore Mode::Static ---------------

#[test]
fn static_mode_does_not_change_markdown_output() {
    // A caller reusing one static Options across formats must get the SAME
    // Markdown as interactive - static rendering is an HTML-only concern; the
    // Markdown renderer flattens on its own. (FencedRender's HTML static path
    // would otherwise leak its <pre><code> fallback into the Markdown.)
    let ext = FencedRender::mermaid();
    let src = "``` mermaid\nA --> B\n```\n";
    let interactive_md = carve::to_markdown_with_options(src, &Options::new().with_extension(&ext));
    let static_md = carve::to_markdown_with_options(
        src,
        &Options::new().with_mode(Mode::Static).with_extension(&ext),
    );
    assert_eq!(interactive_md, static_md);
}

#[test]
fn static_mode_with_math_renderer_does_not_change_ansi_output() {
    // Even a supplied math renderer must not reach the ANSI output.
    let ext = MathBlock::new();
    let src = "``` math\n\\int x\n```\n";
    let opts = Options::new()
        .with_mode(Mode::Static)
        .with_extension(&ext)
        .with_renderers(StaticRenderers {
            math: Some(Box::new(|_t: &str, _d: bool| {
                "<math>SSR</math>".to_string()
            })),
            ..Default::default()
        });
    let interactive_ansi = carve::to_ansi_with_options(src, &Options::new().with_extension(&ext));
    let static_ansi = carve::to_ansi_with_options(src, &opts);
    assert_eq!(interactive_ansi, static_ansi);
    assert!(!static_ansi.contains("SSR"));
}

// --- caption floor (no extension active) -------------------------------------

#[test]
fn caption_floor_labeled_div_renders_div_label() {
    assert_eq!(
        carve::to_html("::: [Notes]\nBody.\n:::"),
        "<div>\n  <p class=\"div-label\">Notes</p>\n  <p>Body.</p>\n</div>"
    );
}

#[test]
fn caption_floor_escapes_label_text() {
    let html = carve::to_html("::: [<b>x</b>]\nBody.\n:::");
    assert!(html.contains("<p class=\"div-label\">&lt;b&gt;x&lt;/b&gt;</p>"));
}

#[test]
fn caption_floor_labeled_admonition_title_first() {
    let html = carve::to_html("::: tip \"Pro Tip\" [Build]\nSave often.\n:::");
    let title_idx = html.find("admonition-title").expect("title present");
    let label_idx = html.find("div-label").expect("label present");
    assert!(label_idx > title_idx);
    assert!(html.contains("<p class=\"div-label\">Build</p>"));
}

#[test]
fn caption_floor_holds_in_static_mode_without_group_extension() {
    // No group extension consumes the label, so the core floor surfaces it even
    // in static HTML.
    let opts = Options::new().with_mode(Mode::Static);
    assert_eq!(
        carve::to_html_with_options("::: [First]\nFirst panel.\n:::", &opts).trim(),
        "<div>\n  <p class=\"div-label\">First</p>\n  <p>First panel.</p>\n</div>"
    );
}
