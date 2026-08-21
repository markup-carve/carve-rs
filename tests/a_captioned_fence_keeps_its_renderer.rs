//! A CAPTIONED DIAGRAM FENCE KEEPS ITS PRESET (markup-carve/carve-rs#1151).
//!
//! `FencedRender` and `ImgFence` claim a fence by swapping its `CodeBlock` for a
//! `RawBlock`. A captioned fence is not a block in a list, though - it is a
//! `Figure`'s `target`, and PART 12 pins the five target types with no raw-HTML
//! spelling among them. So the walk ended at `_ => {}` and a caption silently
//! turned the diagram back into a highlighted code block: the author saw the
//! caption appear and the drawing disappear, with no diagnostic.
//!
//! `Figure::rendered_target` carries the rendered HTML beside the target. The
//! bytes below are carve-js's, measured on the same documents.

use carve::{FencedRender, ImgFence, Mode, Options, StaticRenderers};

fn html(src: &str, ext: &dyn carve::CarveExtension) -> String {
    carve::to_html_with_options(src, &Options::new().with_extension(ext))
        .trim()
        .to_string()
}

const CHART: &str = "``` chart\n{\"type\":\"bar\"}\n```";
const ELEMENT: &str =
    "<div class=\"chart\" role=\"img\" aria-label=\"chart\"><script type=\"application/json\">{\"type\":\"bar\"}</script></div>";

#[test]
fn a_captioned_fence_renders_the_element_inside_the_figure() {
    let ext = FencedRender::chart();
    assert_eq!(
        html(&format!("{CHART}\n^ A caption."), &ext),
        format!("<figure>\n  {ELEMENT}\n  <figcaption>A caption.</figcaption>\n</figure>")
    );
}

#[test]
fn an_uncaptioned_fence_is_unchanged() {
    // The control: the plain fence always worked, and must keep working.
    let ext = FencedRender::chart();
    assert_eq!(html(CHART, &ext), ELEMENT);
}

#[test]
fn a_panel_inside_a_figure_group_keeps_its_renderer() {
    let ext = FencedRender::chart();
    let src = format!("::: figure\n{CHART}\n^ Panel.\n:::\n^ Group.");
    let out = html(&src, &ext);

    assert!(out.contains(ELEMENT), "the panel lost its preset: {out}");
    assert!(out.contains("<figcaption>Panel.</figcaption>"));
    assert!(out.contains("<figcaption>Group.</figcaption>"));
    assert!(!out.contains("<pre"), "fell back to a code block: {out}");
}

#[test]
fn a_captioned_fence_inside_a_quoted_figure_is_reached_too() {
    // A figure whose target is a BLOCK QUOTE holds blocks of its own, so the
    // ordinary recursion applies there rather than the target swap.
    let ext = FencedRender::chart();
    let src = "> ``` chart\n> {\"type\":\"bar\"}\n> ```\n^ A caption.";
    let out = html(src, &ext);

    assert!(out.contains("<figure"), "not a figure: {out}");
    assert!(out.contains("<blockquote>"), "not a quoted figure: {out}");
    assert!(out.contains(ELEMENT), "the quote lost its preset: {out}");
    assert!(out.contains("<figcaption>A caption.</figcaption>"), "{out}");
}

#[test]
fn the_static_path_reaches_a_captioned_fence() {
    // Static mode with a build renderer SSR-renders the source; the figure has
    // to take that output too, not only the hydration element.
    let ext = FencedRender::chart();
    let opts = Options::new()
        .with_extension(&ext)
        .with_mode(Mode::Static)
        .with_renderers(StaticRenderers::new().diagram("chart", |_src: &str| "<svg></svg>".into()));
    let out = carve::to_html_with_options(&format!("{CHART}\n^ A caption."), &opts);

    assert!(out.contains("<svg></svg>"), "no SSR output: {out}");
    assert!(out.contains("<figcaption>A caption.</figcaption>"));
}

#[test]
fn the_static_path_degrades_inside_the_figure_without_a_renderer() {
    // No renderer supplied: the static path degrades to an escaped source block,
    // and it must do that INSIDE the figure rather than by losing the caption.
    let ext = FencedRender::chart();
    let opts = Options::new().with_extension(&ext).with_mode(Mode::Static);
    let out = carve::to_html_with_options(&format!("{CHART}\n^ A caption."), &opts);

    assert!(out.contains("<figure"), "lost the figure: {out}");
    assert!(out.contains("<pre"), "expected the source fallback: {out}");
    assert!(out.contains("<figcaption>A caption.</figcaption>"));
}

#[test]
fn raw_html_off_escapes_the_rendered_target() {
    // The replacement is an ordinary RawBlock, so the host's raw-HTML switch
    // reaches it exactly as it reaches a claimed fence standing on its own.
    let ext = FencedRender::chart();
    let opts = Options::new().with_extension(&ext).with_raw_html(false);
    let out = carve::to_html_with_options(&format!("{CHART}\n^ A caption."), &opts);

    assert!(
        !out.contains("<div class=\"chart\""),
        "raw HTML leaked: {out}"
    );
    assert!(
        out.contains("&lt;div class=&quot;chart&quot;") || out.contains("&lt;div"),
        "{out}"
    );
}

#[test]
fn an_img_fence_reaches_a_caption_too() {
    // The same model change closes the gap ImgFence recorded (its parity test is
    // no longer ignored).
    let ext = ImgFence::new();
    let src = "``` img\n<svg xmlns=\"http://www.w3.org/2000/svg\"><title>T</title></svg>\n```\n^ A caption.";
    let out = html(src, &ext);

    assert!(out.contains("<figure"), "{out}");
    assert!(out.contains("<img src=\"data:image/svg+xml,"), "{out}");
    assert!(out.contains("<figcaption>A caption.</figcaption>"), "{out}");
}
