//! Code-fence opener title (`"..."`) edge cases: the title is resolved onto the
//! node attrs at parse time so it survives a caption Figure and a FencedRender
//! extension, and a preceding `{title=...}` line always wins.

use carve::{FencedRender, Options};

#[test]
fn captioned_block_with_attr_title_has_no_duplicate() {
    // {title=...} attaches to the wrapping <figure> and wins; the inner <pre>
    // must NOT also carry the opener title.
    let html = carve::to_html("{title=\"attr\"}\n```php \"hdr\"\nx\n```\n^ cap");
    assert!(html.contains("<figure title=\"attr\">"), "{html}");
    assert!(!html.contains("<pre title="), "{html}");
    assert!(!html.contains("hdr"), "{html}");
}

#[test]
fn captioned_block_keeps_opener_title_on_pre() {
    let html = carve::to_html("```php \"greet.py\"\nx\n```\n^ cap");
    assert!(html.contains("<figure>"), "{html}");
    assert!(html.contains("<pre title=\"greet.py\">"), "{html}");
}

#[test]
fn preceding_title_wins_for_uncaptioned_block() {
    let html = carve::to_html("{title=\"attr\"}\n```php \"hdr\"\nx\n```");
    assert!(html.contains("<pre title=\"attr\">"), "{html}");
    assert!(!html.contains("hdr"), "{html}");
}

#[test]
fn fenced_render_extension_carries_the_title() {
    // The opener title rides on the code block's attrs, which the FencedRender
    // extension clones onto its wrapper element.
    let ext = FencedRender::mermaid();
    let opts = Options::new().with_extension(&ext);
    let html = carve::to_html_with_options("``` mermaid \"Arch\"\ngraph TD; A-->B\n```\n", &opts);
    assert!(html.contains("class=\"mermaid\""), "{html}");
    assert!(html.contains("title=\"Arch\""), "{html}");
}
