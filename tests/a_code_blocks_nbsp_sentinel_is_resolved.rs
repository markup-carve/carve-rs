use carve::{parse, render_ansi, render_carve, render_html, render_markdown, render_plain_text};

#[test]
fn private_use_unicode_survives_text_and_verbatim_targets() {
    for source in [
        "a\u{e000}b\n",
        "`a\u{e000}b`\n",
        "```\na\u{e000}b\n```\n",
        "!`a\u{e000}b`\n",
    ] {
        let doc = parse(source);
        for output in [
            render_html(&doc).unwrap(),
            render_markdown(&doc).unwrap(),
            render_plain_text(&doc).unwrap(),
            render_ansi(&doc).unwrap(),
            render_carve(&doc).unwrap(),
        ] {
            assert!(output.contains('\u{e000}'), "{source:?}: {output:?}");
        }
    }
}

#[test]
fn verbatim_line_block_gaps_publish_nbsp_without_internal_markers() {
    let doc = parse("::: |\n`a  b`\n:::\n");
    let html = render_html(&doc).unwrap();
    assert!(html.contains("<code>a&nbsp;&nbsp;b</code>"));
    assert!(!carve::to_json(&doc).contains("\\u0000"));
}
