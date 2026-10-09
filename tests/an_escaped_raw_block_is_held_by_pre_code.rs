//! A raw block a safe policy escapes is written as the PART 10 §6 fenced code
//! block with the raw format as its language (markup-carve/carve#2795).

use carve::{CheckedRenderOptions, Options, RenderResult, RenderTarget};

fn html(source: &str, safe: bool) -> RenderResult<String> {
    let options = Options::default().with_raw_html(!safe);
    carve::with_render_loss_report(RenderTarget::Html, CheckedRenderOptions::default(), || {
        carve::to_html_with_options(source, &options)
    })
    .expect("a non-strict collection cannot fail")
}

#[test]
fn the_contract_example() {
    let source = "Before.\n\n```=html\n<p>raw</p>\n```\n\nAfter.\n";
    assert_eq!(
        html(source, true).value,
        "<p>Before.</p>\n<pre><code class=\"language-html\">&lt;p&gt;raw&lt;/p&gt;\n</code></pre>\n<p>After.</p>"
    );
}

#[test]
fn it_matches_the_fenced_code_block_byte_for_byte() {
    let raw = "```=html\na & b\n\n<c>\n```\n";
    let code = "```html\na & b\n\n<c>\n```\n";
    let escaped = html(raw, true).value;
    assert_eq!(
        escaped,
        "<pre><code class=\"language-html\">a &amp; b\n\n&lt;c&gt;\n</code></pre>"
    );
    assert_eq!(escaped, html(code, true).value);
}

#[test]
fn an_all_blank_payload_keeps_its_lines() {
    assert_eq!(
        html("```=html\n\n\n```\n", true).value,
        html("```html\n\n\n```\n", true).value
    );
    assert_eq!(
        html("```=html\n```\n", true).value,
        "<pre><code class=\"language-html\"></code></pre>"
    );
}

#[test]
fn escaping_is_not_a_render_loss() {
    let checked = html("```=html\n<p>raw</p>\n```\n", true);
    assert_eq!(checked.total_losses, 0);
    assert!(checked.losses.is_empty());
}

#[test]
fn raw_html_allowed_passes_the_payload_through() {
    let checked = html("Before.\n\n```=html\n<p>raw</p>\n```\n\nAfter.\n", false);
    assert_eq!(checked.value, "<p>Before.</p>\n<p>raw</p>\n<p>After.</p>");
    assert_eq!(checked.total_losses, 0);
}

#[test]
fn a_non_html_raw_block_is_still_dropped_under_safe() {
    let checked = html("```=latex\n\\emph{x}\n```\n", true);
    assert_eq!(checked.value, "");
    assert_eq!(checked.totals_by_code.get("raw-format-dropped"), Some(&1));
}
