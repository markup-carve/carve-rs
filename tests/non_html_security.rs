//! Non-HTML render targets are safe-by-default: Markdown cannot carry XSS into a
//! downstream Markdown -> HTML render, and ANSI/plain cannot inject terminal
//! escape sequences.

use std::collections::BTreeMap;

fn md(src: &str) -> String {
    carve::to_markdown(src).trim().to_string()
}

fn assert_no_author_controls(out: &str) {
    assert!(!out.contains('\x1b'), "{out:?}");
    assert!(!out.contains('\x07'), "{out:?}");
}

fn assert_no_author_osc(out: &str) {
    assert!(!out.contains("\x1b]"), "{out:?}");
    assert!(!out.contains('\x07'), "{out:?}");
}

#[test]
fn markdown_blanks_dangerous_url_schemes() {
    assert!(md("[x](javascript:alert(1))").contains("[x]()"));
    assert!(md("![a](javascript:alert(1))").contains("![a]()"));
    assert!(md("[ok](https://e.com)").contains("[ok](https://e.com)"));
    assert_eq!(md("<javascript:alert(1)>"), "[javascript:alert(1)]()");
}

#[test]
fn markdown_percent_encodes_destination_breakouts() {
    // A `)` reaching a destination via a reference definition (where the URL
    // runs to end-of-line, not `)`-delimited) must be percent-encoded so it
    // cannot break out of the `(...)` in Markdown output.
    assert_eq!(
        md("[x][r]\n\n[r]: https://e.com/a)b"),
        "[x](https://e.com/a%29b)"
    );
    let image_doc = carve::Document {
        frontmatter: BTreeMap::new(),
        frontmatter_source: None,
        source_len: 0,
        footnote_defs: BTreeMap::new(),
        children: vec![carve::BlockNode::Paragraph(carve::Paragraph {
            at_content_column: false,
            pos: None,
            attrs: None,
            children: vec![carve::InlineNode::Image(carve::Image {
                attrs: None,
                src: "https://e.com/a b<c>".to_string(),
                alt: "x".to_string(),
                title: None,
                ref_label: None,
                raw_ref: None,
            })],
        })],
    };
    assert_eq!(
        carve::render_markdown(&image_doc).trim(),
        "![x](https://e.com/a%20b%3Cc%3E)"
    );
    assert_eq!(
        md("<https://example.com>"),
        "[https://example.com](https://example.com)"
    );
}

#[test]
fn markdown_escapes_raw_html() {
    let out = md("```=html\n<script>alert(1)</script>\n```");
    assert!(!out.contains("<script>"));
    assert!(out.contains("&lt;script&gt;"));
}

#[test]
fn markdown_neutralizes_embedded_html() {
    assert!(!md("plain <img onerror=x> text").contains("<img"));
    let sup = md("{^<img src=x onerror=alert(1)>^}");
    assert!(sup.contains("<sup>"));
    assert!(!sup.contains("<img"));
    assert_eq!(md("a < b & c"), "a &lt; b &amp; c");
}

#[test]
fn ansi_and_plain_strip_control_bytes() {
    let ansi = carve::to_ansi("hi \x1b[31mX\x1b[0m\x07 there");
    assert!(!ansi.contains('\x1b'));
    assert!(!ansi.contains('\x07'));
    assert!(ansi.contains("there"));
    let plain = carve::to_plain_text("a\x1bb\x07c");
    assert!(!plain.contains('\x1b'));
    assert!(!plain.contains('\x07'));
}

#[test]
fn ansi_and_plain_strip_link_href_control_bytes() {
    let src = "[x](http://a/\x1b]0;PWNED\x07/b)";
    let ansi = carve::to_ansi(src);
    assert!(!ansi.contains("\x1b]0;"), "{ansi:?}");
    assert!(!ansi.contains('\x07'), "{ansi:?}");

    let plain = carve::to_plain_text(src);
    assert!(!plain.contains('\x1b'), "{plain:?}");
    assert!(!plain.contains('\x07'), "{plain:?}");
}

#[test]
fn markdown_escapes_link_and_image_titles() {
    assert_eq!(md("[x](u \"a \\\"b\")"), "[x](u \"a \\\"b\")");
    assert_eq!(md("![alt](img \"a \\\"b\")"), "![alt](img \"a \\\"b\")");
}

#[test]
fn markdown_sanitizes_code_fence_info_string() {
    let doc = carve::Document {
        frontmatter: BTreeMap::new(),
        frontmatter_source: None,
        source_len: 0,
        footnote_defs: BTreeMap::new(),
        children: vec![carve::BlockNode::CodeBlock(carve::CodeBlock {
            attrs: None,
            lang: Some("rs ```\n# injected".to_string()),
            title: None,
            label: None,
            content: "let x = 1;".to_string(),
        })],
    };

    // First whitespace-delimited token (`rs`) survives; the rest (backticks +
    // injected line) is dropped. Byte-identical with carve-js / carve-php.
    assert_eq!(
        carve::render_markdown(&doc).trim(),
        "```rs\nlet x = 1;\n```"
    );
}

#[test]
fn markdown_escapes_image_alt_label_metacharacters() {
    let out = carve::render_markdown(&carve::Document {
        frontmatter: BTreeMap::new(),
        frontmatter_source: None,
        source_len: 0,
        footnote_defs: BTreeMap::new(),
        children: vec![carve::BlockNode::Paragraph(carve::Paragraph {
            at_content_column: false,
            pos: None,
            attrs: None,
            children: vec![carve::InlineNode::Image(carve::Image {
                attrs: None,
                src: "/safe".to_string(),
                alt: r"x](javascript:alert(1))![y\z".to_string(),
                title: None,
                ref_label: None,
                raw_ref: None,
            })],
        })],
    });

    assert_eq!(out.trim(), r"![x\](javascript:alert(1))!\[y\\z](/safe)");
}

#[test]
fn markdown_strips_control_bytes_from_author_leaf_fields() {
    let c = "\x1b]0;p\x07";
    let mut footnote_defs = BTreeMap::new();
    footnote_defs.insert(
        format!("fn{c}"),
        vec![carve::BlockNode::Paragraph(carve::Paragraph {
            at_content_column: false,
            pos: None,
            attrs: None,
            children: vec![carve::InlineNode::Text(format!("note{c}"))],
        })],
    );
    let doc = carve::Document {
        frontmatter: BTreeMap::new(),
        frontmatter_source: None,
        source_len: 0,
        footnote_defs,
        children: vec![
            carve::BlockNode::CodeBlock(carve::CodeBlock {
                attrs: None,
                lang: None,
                title: None,
                label: None,
                content: format!("code{c}"),
            }),
            carve::BlockNode::RawBlock(carve::RawBlock {
                format: "html".to_string(),
                content: format!("<b>{c}</b>"),
            }),
            carve::BlockNode::Paragraph(carve::Paragraph {
                pos: None,
                at_content_column: false,
                attrs: None,
                children: vec![
                    carve::InlineNode::Text(format!("text{c}")),
                    carve::InlineNode::Code(format!("code{c}"), None),
                    carve::InlineNode::Math(carve::Math {
                        attrs: None,
                        display: false,
                        content: format!("math{c}"),
                    }),
                    carve::InlineNode::Link(carve::Link {
                        attrs: None,
                        href: "https://e".to_string(),
                        title: Some(format!("title{c}")),
                        children: vec![carve::InlineNode::Text("link".to_string())],
                        ref_label: None,
                        raw_ref: None,
                        from_crossref: false,
                    }),
                    carve::InlineNode::Image(carve::Image {
                        attrs: None,
                        src: "img".to_string(),
                        alt: format!("alt{c}"),
                        title: Some(format!("ititle{c}")),
                        ref_label: None,
                        raw_ref: None,
                    }),
                    carve::InlineNode::Abbreviation(carve::Abbreviation {
                        abbr: format!("abbr{c}"),
                        expansion: format!("exp{c}"),
                    }),
                    carve::InlineNode::Mention(carve::Mention {
                        user: format!("user{c}"),
                    }),
                    carve::InlineNode::Tag(carve::Tag {
                        name: format!("tag{c}"),
                    }),
                    carve::InlineNode::Footnote(carve::Footnote {
                        attrs: None,
                        id: Some(format!("id{c}")),
                        inline: None,
                        number: None,
                        ref_id: None,
                    }),
                    carve::InlineNode::CriticSubstitute(carve::CriticSubstitute {
                        old_text: format!("old{c}"),
                        new_text: format!("new{c}"),
                    }),
                    carve::InlineNode::CrossRef(carve::CrossRef {
                        target: format!("target{c}"),
                    }),
                    carve::InlineNode::CitationGroup(carve::CitationGroup {
                        items: Vec::new(),
                        raw: format!("[@key{c}]"),
                        mode: None,
                        integral: false,
                    }),
                ],
            }),
        ],
    };

    assert_no_author_controls(&carve::render_markdown(&doc));
}

#[test]
fn plain_and_ansi_strip_control_bytes_from_author_leaf_fields() {
    let c = "\x1b]0;p\x07";
    let mut footnote_defs = BTreeMap::new();
    footnote_defs.insert(
        format!("fn{c}"),
        vec![carve::BlockNode::Paragraph(carve::Paragraph {
            at_content_column: false,
            pos: None,
            attrs: None,
            children: vec![carve::InlineNode::CitationGroup(carve::CitationGroup {
                items: Vec::new(),
                raw: format!("[@key{c}]"),
                mode: None,
                integral: false,
            })],
        })],
    );
    let doc = carve::Document {
        frontmatter: BTreeMap::new(),
        frontmatter_source: None,
        source_len: 0,
        footnote_defs,
        children: vec![
            carve::BlockNode::RawBlock(carve::RawBlock {
                format: format!("fmt{c}"),
                content: format!("raw{c}"),
            }),
            carve::BlockNode::BlockImage(carve::Image {
                attrs: None,
                src: "img".to_string(),
                alt: format!("alt{c}"),
                title: None,
                ref_label: None,
                raw_ref: None,
            }),
            carve::BlockNode::Paragraph(carve::Paragraph {
                pos: None,
                at_content_column: false,
                attrs: None,
                children: vec![
                    carve::InlineNode::Image(carve::Image {
                        attrs: None,
                        src: "img".to_string(),
                        alt: format!("ialt{c}"),
                        title: None,
                        ref_label: None,
                        raw_ref: None,
                    }),
                    carve::InlineNode::Abbreviation(carve::Abbreviation {
                        abbr: format!("abbr{c}"),
                        expansion: format!("exp{c}"),
                    }),
                    carve::InlineNode::Mention(carve::Mention {
                        user: format!("user{c}"),
                    }),
                    carve::InlineNode::Tag(carve::Tag {
                        name: format!("tag{c}"),
                    }),
                    carve::InlineNode::Footnote(carve::Footnote {
                        attrs: None,
                        id: Some(format!("id{c}")),
                        inline: None,
                        number: None,
                        ref_id: None,
                    }),
                    carve::InlineNode::CriticSubstitute(carve::CriticSubstitute {
                        old_text: format!("old{c}"),
                        new_text: format!("new{c}"),
                    }),
                    carve::InlineNode::CrossRef(carve::CrossRef {
                        target: format!("target{c}"),
                    }),
                ],
            }),
        ],
    };

    assert_no_author_controls(&carve::render_plain_text(&doc));
    assert_no_author_osc(&carve::render_ansi(&doc));
}
