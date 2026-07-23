//! Port of carve-js `test/img-fence.test.ts`: the SVG `img` fence extension.
//!
//! `carveToHtml(src, {extensions:[imgFence()]})` maps to
//! `to_html_with_options(src, Options::new().with_extension(&ImgFence::new()))`.
//! Behavioral checks mirror the carve-js assertions; the emitted HTML is also
//! pinned byte-for-byte against carve-js (`dist/index.js`).

use carve::{ImgFence, Options};

fn html(src: &str, ext: &ImgFence) -> String {
    carve::to_html_with_options(src, &Options::new().with_extension(ext))
}

fn html_plain(src: &str) -> String {
    carve::to_html(src)
}

/// A code fence carries no inline attributes: any `{…}` goes on the PRECEDING
/// block-attribute line, which lands in `code.attrs`.
fn fence(attrs: &str, body: &str) -> String {
    let prefix = if attrs.is_empty() {
        String::new()
    } else {
        format!("{}\n", attrs.trim())
    };
    format!("{prefix}```img\n{body}\n```")
}

/// Decode the `data:image/svg+xml,<encoded>` payload out of an `<img>`.
fn data_uri(out: &str) -> String {
    let marker = "src=\"data:image/svg+xml,";
    let start = out.find(marker).expect("data uri present") + marker.len();
    let rest = &out[start..];
    let end = rest.find('"').expect("closing quote");
    decode_uri_component(&rest[..end])
}

fn decode_uri_component(s: &str) -> String {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hi = (b[i + 1] as char).to_digit(16).unwrap();
            let lo = (b[i + 2] as char).to_digit(16).unwrap();
            out.push((hi * 16 + lo) as u8);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap()
}

const SB: &str = "<svg viewBox=\"0 0 1 1\"><rect width=\"1\" height=\"1\"/></svg>";

// The exact carve-js sandbox `<img>` for `SB` with an empty alt.
fn img_sb(alt: &str, extra: &str) -> String {
    format!(
        "<img src=\"data:image/svg+xml,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20viewBox%3D%220%200%201%201%22%3E%3Crect%20width%3D%221%22%20height%3D%221%22%2F%3E%3C%2Fsvg%3E\" alt=\"{alt}\"{extra}>"
    )
}

// ---------------------------------------------------------------------------
// sandbox mode (default)
// ---------------------------------------------------------------------------

#[test]
fn renders_clean_svg_as_data_uri_img_not_inline() {
    let ext = ImgFence::new();
    let out = html(&fence("", SB), &ext);
    assert_eq!(out, img_sb("", ""));
    assert!(out.contains("<img"));
    assert!(!out.contains("<svg ") && !out.contains("<svg>"));
    let decoded = data_uri(&out);
    assert!(decoded.contains("<rect width=\"1\" height=\"1\""));
    assert!(decoded.contains("xmlns=\"http://www.w3.org/2000/svg\""));
}

#[test]
fn sanitizes_injected_script_before_encoding() {
    let ext = ImgFence::new();
    let out = html(
        &fence(
            "",
            "<svg viewBox=\"0 0 1 1\"><script>alert(1)</script><rect width=\"1\" height=\"1\"/></svg>",
        ),
        &ext,
    );
    let decoded = data_uri(&out);
    assert!(!decoded.contains("script"));
    assert!(decoded.contains("<rect width=\"1\" height=\"1\""));
}

#[test]
fn sets_alt_and_does_not_leak_flag() {
    let ext = ImgFence::new();
    let out = html(&fence(" {alt=\"a map\"}", SB), &ext);
    assert_eq!(out, img_sb("a map", ""));
    assert!(out.contains("alt=\"a map\""));
    assert!(!out.contains("alt=\"\""));
}

#[test]
fn strips_src_srcset_overrides() {
    let ext = ImgFence::new();
    let out = html(
        &fence(" {srcset=\"https://attacker.example/x.svg 1x\"}", SB),
        &ext,
    );
    assert_eq!(out, img_sb("", ""));
    assert!(out.contains("src=\"data:image/svg+xml,"));
    assert!(!out.contains("srcset"));
    assert!(!out.contains("attacker"));
    assert_eq!(out.matches("src=").count(), 1);
}

#[test]
fn swallows_redundant_sandbox_marker() {
    let ext = ImgFence::new();
    let out = html(&fence(" {sandbox}", SB), &ext);
    assert_eq!(out, img_sb("", ""));
    assert!(!out.contains("sandbox"));
}

#[test]
fn merges_id_class_onto_the_img() {
    let ext = ImgFence::new();
    let out = html(&fence(" {#pic .thumb}", SB), &ext);
    assert_eq!(out, img_sb("", " id=\"pic\" class=\"thumb\""));
}

#[test]
fn claims_the_image_alias() {
    let ext = ImgFence::new();
    let out = html(
        "```image\n<svg viewBox=\"0 0 1 1\"><rect width=\"1\" height=\"1\"/></svg>\n```",
        &ext,
    );
    assert_eq!(out, img_sb("", ""));
}

// ---------------------------------------------------------------------------
// {inline} is gated by allow_inline (security)
// ---------------------------------------------------------------------------

#[test]
fn ignores_inline_when_host_did_not_opt_in() {
    let ext = ImgFence::new(); // allow_inline not set
    let out = html(&fence(" {inline}", SB), &ext);
    assert_eq!(out, img_sb("", ""));
    assert!(!out.contains("<svg ") && !out.contains("<svg>"));
}

#[test]
fn renders_inline_svg_only_with_allow_inline_and_flag() {
    let ext = ImgFence::new().allow_inline(true);
    let out = html(
        &fence(
            " {inline}",
            "<svg viewBox=\"0 0 10 10\"><circle cx=\"5\" cy=\"5\" r=\"4\" fill=\"currentColor\"/></svg>",
        ),
        &ext,
    );
    assert_eq!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\"><circle cx=\"5\" cy=\"5\" r=\"4\" fill=\"currentColor\"/></svg>"
    );
    assert!(!out.contains("data:image/svg+xml"));
}

#[test]
fn with_allow_inline_but_no_flag_stays_sandboxed() {
    let ext = ImgFence::new().allow_inline(true);
    let out = html(&fence("", SB), &ext);
    assert_eq!(out, img_sb("", ""));
    assert!(!out.contains("<svg ") && !out.contains("<svg>"));
}

#[test]
fn sanitizes_injected_script_in_inline_mode() {
    let ext = ImgFence::new().allow_inline(true);
    let out = html(
        &fence(
            " {inline}",
            "<svg viewBox=\"0 0 1 1\"><script>alert(1)</script><rect width=\"1\" height=\"1\"/></svg>",
        ),
        &ext,
    );
    assert!(!out.contains("<script"));
    assert!(!out.contains("alert"));
    assert!(out.contains("<rect width=\"1\" height=\"1\""));
}

// ---------------------------------------------------------------------------
// inline attribute merge (allow_inline)
// ---------------------------------------------------------------------------

#[test]
fn merges_fence_id_class_onto_root_svg() {
    let ext = ImgFence::new().allow_inline(true);
    let out = html(
        &fence(
            " {inline #logo .icon}",
            "<svg viewBox=\"0 0 1 1\"><path d=\"M0 0\"/></svg>",
        ),
        &ext,
    );
    assert_eq!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" id=\"logo\" class=\"icon\" viewBox=\"0 0 1 1\"><path d=\"M0 0\"/></svg>"
    );
}

#[test]
fn merges_onto_root_with_quoted_gt_in_value() {
    let ext = ImgFence::new().allow_inline(true);
    let out = html(
        &fence(
            " {inline #x}",
            "<svg aria-label=\"1&gt;2\" viewBox=\"0 0 1 1\"><rect width=\"1\" height=\"1\"/></svg>",
        ),
        &ext,
    );
    assert_eq!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" id=\"x\" aria-label=\"1&gt;2\" viewBox=\"0 0 1 1\"><rect width=\"1\" height=\"1\"/></svg>"
    );
}

#[test]
fn scrubs_dangerous_fence_attr_merged_onto_root() {
    let ext = ImgFence::new().allow_inline(true);
    let out = html(
        &fence(
            " {inline fill=\"url(https://attacker.example/p.svg#x)\"}",
            SB,
        ),
        &ext,
    );
    assert_eq!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1 1\"><rect width=\"1\" height=\"1\"/></svg>"
    );
    assert!(!out.contains("attacker"));
}

#[test]
fn fence_attrs_override_root_without_duplicating() {
    let ext = ImgFence::new().allow_inline(true);
    let out = html(
        &fence(
            " {inline #outer .fence}",
            "<svg id=\"inner\" class=\"orig\" viewBox=\"0 0 1 1\"><rect width=\"1\" height=\"1\"/></svg>",
        ),
        &ext,
    );
    assert_eq!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" id=\"outer\" class=\"fence\" viewBox=\"0 0 1 1\"><rect width=\"1\" height=\"1\"/></svg>"
    );
    assert_eq!(out.matches(" id=").count(), 1);
    assert_eq!(out.matches(" class=").count(), 1);
    assert!(!out.contains("inner"));
    assert!(!out.contains("orig"));
}

#[test]
fn inline_with_no_author_attrs_emits_bare_svg() {
    let ext = ImgFence::new().allow_inline(true);
    let out = html(
        &fence(
            " {inline}",
            "<svg viewBox=\"0 0 10 10\"><path d=\"M0 0\"/></svg>",
        ),
        &ext,
    );
    assert_eq!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\"><path d=\"M0 0\"/></svg>"
    );
}

// ---------------------------------------------------------------------------
// fallback + off-by-default
// ---------------------------------------------------------------------------

#[test]
fn non_svg_body_degrades_to_escaped_code_block() {
    let ext = ImgFence::new();
    let out = html(&fence("", "not an svg <b>x</b>"), &ext);
    assert_eq!(
        out,
        "<pre><code class=\"language-img\">not an svg &lt;b&gt;x&lt;/b&gt;\n</code></pre>"
    );
}

#[test]
fn off_unless_registered() {
    let out = html_plain(&fence("", "<svg><rect/></svg>"));
    assert!(out.contains("<pre"));
    assert!(out.contains("<code"));
    assert!(!out.contains("<img"));
}

// ---------------------------------------------------------------------------
// non-HTML targets leave the source fence untouched
// ---------------------------------------------------------------------------

#[test]
fn markdown_target_keeps_source_fence() {
    let ext = ImgFence::new();
    let opts = Options::new().with_extension(&ext);
    let doc = carve::parse(&fence("", SB));
    let md = carve::render_markdown_with_options(&doc, &opts);
    // The SVG source survives verbatim as a fenced code block (not an <img>).
    assert!(md.contains("```img"));
    assert!(md.contains(SB));
    assert!(!md.contains("<img"));
}

// ---------------------------------------------------------------------------
// KNOWN LIMITATION: captioned fences (parity gap with carve-js)
// ---------------------------------------------------------------------------

/// A captioned `img` fence parses as `Figure { target: CodeBlock }`. carve-js
/// renders it as `<figure><img …><figcaption>…`, but carve-rs currently leaves
/// the source because `FigureTarget` has no raw-HTML variant a `before_render`
/// transform can inject (same gap as `fenced_render` for mermaid/chart). This
/// test pins the DESIRED carve-js-parity behavior and is `#[ignore]`d until the
/// extension model gains figure support - un-ignore it when that lands. See the
/// KNOWN LIMITATION comment in src/extensions/img_fence.rs.
#[test]
#[ignore = "captioned img fences render source, not <figure><img> - parity gap with carve-js, shared with fenced_render; needs FigureTarget raw support"]
fn captioned_fence_should_render_figure_img() {
    let ext = ImgFence::new();
    let out = html(&format!("{}\n^ A caption", fence("", SB)), &ext);
    assert!(out.contains("<figure"), "expected a <figure> wrapper");
    assert!(
        out.contains("<img src=\"data:image/svg+xml,"),
        "expected sandboxed <img>"
    );
    assert!(out.contains("<figcaption>A caption</figcaption>"));
    assert!(!out.contains("<pre"), "must not fall back to source");
}
