//! Non-HTML render targets are safe-by-default: Markdown cannot carry XSS into a
//! downstream Markdown -> HTML render, and the ANSI target cannot inject terminal
//! escape sequences.
//!
//! WHICH TARGET STRIPS A CONTROL CHARACTER IS NOT UNIFORM, and since spec PART 9
//! §29 (markup-carve/carve#979, carve-rs#812) it is not a security question on
//! every target either. §29 T4 gives the terminal target the strip because its
//! consumer ACTS on the character - a form feed feeds, U+001B opens a sequence
//! that can move the cursor or reach the clipboard - and says in as many words
//! that this "reaches this target and no other". T2 and T3 make the Markdown and
//! plain targets EMIT the same characters, because a target that silently
//! removes content is lossy in the way markup-carve/carve#817 rejected and
//! because four Markdown readers were measured and keep them.
//!
//! So the assertions below are split by target rather than shared. What did NOT
//! move, and is asserted here as before: a DESTINATION carries no control on any
//! target (it is percent-encoded), DEL and the C1 controls are still stripped
//! everywhere but HTML, and Markdown still neutralizes embedded HTML.

use std::collections::BTreeMap;

fn md(src: &str) -> String {
    carve::to_markdown(src).trim().to_string()
}

/// The TERMINAL target's bar: no author-supplied escape or bell, at all. The
/// renderer's OWN styling escapes are added after this content and are not what
/// is being measured, so this looks for the author's payload rather than for the
/// ESC byte, which the ANSI target legitimately emits itself.
fn assert_no_author_osc(out: &str) {
    assert!(!out.contains("\x1b]"), "{out:?}");
    assert!(!out.contains('\x07'), "{out:?}");
}

/// DEL and the C1 controls, which §29 T5 leaves OUTSIDE its scope and which the
/// Markdown and plain targets still strip. Asserted so this change is visible as
/// the narrowing it is, rather than as the removal of a filter.
fn assert_no_high_controls(out: &str) {
    for c in std::iter::once('\u{7f}')
        .chain((0x80u32..0xA0).map(|c| char::from_u32(c).expect("C1 is a char")))
    {
        assert!(!out.contains(c), "U+{:04X} in {out:?}", c as u32);
    }
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
        frontmatter_raw: None,
        source_len: 0,
        ingest_payload_len: 0,
        footnote_defs: BTreeMap::new(),
        footnote_def_pos: BTreeMap::new(),
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
                pos: None,
            })],
        })],
    };
    assert_eq!(
        carve::render_markdown(&image_doc)
            .expect("the tree under test is within the render ceiling")
            .trim(),
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
    // The claim is that no TAG opens, not that a particular spelling is used.
    // PART 11 section 8a M1e escapes the OPENER with a backslash rather than
    // rewriting it to an entity (carve#1148), so the check is on an `<img` that
    // is NOT preceded by one - a plain `contains` reports the safe escaped form
    // as a hit, because the escape sits right before the substring.
    let opens_a_tag = |s: &str| s.replace("\\<", "").contains("<img");
    assert!(!opens_a_tag(&md("plain <img onerror=x> text")));
    assert!(md("plain <img onerror=x> text").contains("\\<img"));
    let sup = md("{^<img src=x onerror=alert(1)>^}");
    assert!(sup.contains("<sup>"));
    assert!(!opens_a_tag(&sup));

    // A `<` before a SPACE was never markup, so M1e leaves it alone, and a
    // mid-line `>` is inert in every flavour.
    assert_eq!(md("a < b & c"), "a < b & c");

    // The reason `&` stopped being escaped (carve#1071): an entity in Markdown
    // TEXT decodes to a CHARACTER, and a character cannot open a tag. Text
    // authored as `&lt;script&gt;` therefore comes back as the four characters a
    // reader sees, never as live markup.
    assert_eq!(md("a &lt;script&gt; b"), "a &lt;script&gt; b");
    // A literal tag in text IS the hazard, and the backslash is what stops it.
    assert_eq!(md("a <script>x</script> b"), "a \\<script>x\\</script> b");
}

#[test]
fn ansi_strips_control_bytes() {
    let ansi = carve::to_ansi("hi \x1b[31mX\x1b[0m\x07 there");
    assert!(!ansi.contains("\x1b["), "{ansi:?}");
    assert!(!ansi.contains('\x07'), "{ansi:?}");
    assert!(ansi.contains("there"));
}

/// The plain target EMITS them (§29 T3). That row is recorded in the spec as a
/// JUDGEMENT rather than a measurement - plain text is a text serialization
/// rather than a terminal format, so it takes the fidelity answer and not the
/// device answer - and it is the row to revisit if plain output turns out to be
/// piped to a terminal about as often as ANSI output is.
#[test]
fn plain_emits_a_control_byte_and_still_strips_del_and_c1() {
    let plain = carve::to_plain_text("a\x1bb\x07c");
    assert!(plain.contains("a\x1bb\x07c"), "{plain:?}");
    assert_no_high_controls(&carve::to_plain_text("a\u{7f}b\u{9b}c"));
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
        frontmatter_raw: None,
        source_len: 0,
        ingest_payload_len: 0,
        footnote_defs: BTreeMap::new(),
        footnote_def_pos: BTreeMap::new(),
        children: vec![carve::BlockNode::CodeBlock(carve::CodeBlock {
            attrs: None,
            lang: Some("rs ```\n# injected".to_string()),
            title: None,
            label: None,
            content: "let x = 1;".to_string(),
            pos: None,
        })],
    };

    // First whitespace-delimited token (`rs`) survives; the rest (backticks +
    // injected line) is dropped. Byte-identical with carve-js / carve-php.
    assert_eq!(
        carve::render_markdown(&doc)
            .expect("the tree under test is within the render ceiling")
            .trim(),
        "```rs\nlet x = 1;\n```"
    );
}

#[test]
fn markdown_escapes_image_alt_label_metacharacters() {
    let out = carve::render_markdown(&carve::Document {
        frontmatter: BTreeMap::new(),
        frontmatter_raw: None,
        source_len: 0,
        ingest_payload_len: 0,
        footnote_defs: BTreeMap::new(),
        footnote_def_pos: BTreeMap::new(),
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
                pos: None,
            })],
        })],
    })
    .expect("the tree under test is within the render ceiling");

    assert_eq!(out.trim(), r"![x\](javascript:alert(1))!\[y\\z](/safe)");
}

#[test]
fn markdown_emits_control_bytes_from_author_leaf_fields_and_still_refuses_del_and_c1() {
    let c = "\x1b]0;p\x07";
    let mut footnote_defs = BTreeMap::new();
    footnote_defs.insert(
        format!("fn{c}"),
        vec![carve::BlockNode::Paragraph(carve::Paragraph {
            at_content_column: false,
            pos: None,
            attrs: None,
            children: vec![carve::InlineNode::text(format!("note{c}"))],
        })],
    );
    let doc = carve::Document {
        frontmatter: BTreeMap::new(),
        frontmatter_raw: None,
        source_len: 0,
        ingest_payload_len: 0,
        footnote_defs,
        footnote_def_pos: Default::default(),
        children: vec![
            carve::BlockNode::CodeBlock(carve::CodeBlock {
                attrs: None,
                lang: None,
                title: None,
                label: None,
                content: format!("code{c}"),
                pos: None,
            }),
            carve::BlockNode::RawBlock(carve::RawBlock {
                format: "html".to_string(),
                content: format!("<b>{c}</b>"),
                pos: None,
            }),
            carve::BlockNode::Paragraph(carve::Paragraph {
                pos: None,
                at_content_column: false,
                attrs: None,
                children: vec![
                    carve::InlineNode::text(format!("text{c}")),
                    carve::InlineNode::code(format!("code{c}"), None),
                    carve::InlineNode::Math(carve::Math {
                        attrs: None,
                        display: false,
                        content: format!("math{c}"),
                        pos: None,
                    }),
                    carve::InlineNode::Link(carve::Link {
                        attrs: None,
                        href: "https://e".to_string(),
                        title: Some(format!("title{c}")),
                        children: vec![carve::InlineNode::text("link".to_string())],
                        ref_label: None,
                        raw_ref: None,
                        from_crossref: false,
                        from_heading_reference: false,
                        pos: None,
                    }),
                    carve::InlineNode::Image(carve::Image {
                        attrs: None,
                        src: "img".to_string(),
                        alt: format!("alt{c}"),
                        title: Some(format!("ititle{c}")),
                        ref_label: None,
                        raw_ref: None,
                        pos: None,
                    }),
                    carve::InlineNode::Abbreviation(carve::Abbreviation {
                        abbr: format!("abbr{c}"),
                        expansion: format!("exp{c}"),
                        pos: None,
                    }),
                    carve::InlineNode::Mention(carve::Mention {
                        attrs: None,
                        user: format!("user{c}"),
                        pos: None,
                    }),
                    carve::InlineNode::Tag(carve::Tag {
                        attrs: None,
                        name: format!("tag{c}"),
                        pos: None,
                    }),
                    carve::InlineNode::Footnote(carve::Footnote {
                        attrs: None,
                        id: Some(format!("id{c}")),
                        inline: None,
                        number: None,
                        ref_id: None,
                        pos: None,
                    }),
                    carve::InlineNode::CriticSubstitute(carve::CriticSubstitute {
                        old_text: format!("old{c}"),
                        new_text: format!("new{c}"),
                        pos: None,
                    }),
                    carve::InlineNode::CrossRef(carve::CrossRef {
                        target: format!("target{c}"),
                        href: None,
                        pos: None,
                    }),
                    carve::InlineNode::CitationGroup(carve::CitationGroup {
                        items: Vec::new(),
                        raw: format!("[@key{c}]"),
                        mode: None,
                        integral: false,
                        pos: None,
                    }),
                ],
            }),
        ],
    };

    // §29 T2: the Markdown target EMITS them. DEL and the C1 controls, which
    // §29 T5 leaves outside its scope, are still stripped from every one of
    // these leaf fields - which is what this case has always been about: that
    // EVERY author-reachable leaf goes through the one filter, not that the
    // filter is broad.
    let out =
        carve::render_markdown(&doc).expect("the tree under test is within the render ceiling");
    assert!(out.contains("\x1b]0;p\x07"), "{out:?}");
    assert_no_high_controls(&render_markdown_with("\u{7f}\u{9b}"));
}

/// The same tree, with the leaf payload replaced, so the DEL / C1 half of the
/// case above reaches every field rather than one.
fn render_markdown_with(c: &str) -> String {
    let doc = carve::Document {
        frontmatter: BTreeMap::new(),
        frontmatter_raw: None,
        source_len: 0,
        ingest_payload_len: 0,
        footnote_defs: BTreeMap::new(),
        footnote_def_pos: BTreeMap::new(),
        children: vec![
            carve::BlockNode::CodeBlock(carve::CodeBlock {
                attrs: None,
                lang: None,
                title: None,
                label: None,
                content: format!("code{c}"),
                pos: None,
            }),
            carve::BlockNode::Paragraph(carve::Paragraph {
                pos: None,
                at_content_column: false,
                attrs: None,
                children: vec![
                    carve::InlineNode::text(format!("text{c}")),
                    carve::InlineNode::code(format!("code{c}"), None),
                ],
            }),
        ],
    };
    carve::render_markdown(&doc).expect("the tree under test is within the render ceiling")
}

#[test]
fn the_terminal_target_strips_control_bytes_from_every_author_leaf_field() {
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
                pos: None,
            })],
        })],
    );
    let doc = carve::Document {
        frontmatter: BTreeMap::new(),
        frontmatter_raw: None,
        source_len: 0,
        ingest_payload_len: 0,
        footnote_defs,
        footnote_def_pos: Default::default(),
        children: vec![
            carve::BlockNode::RawBlock(carve::RawBlock {
                format: format!("fmt{c}"),
                content: format!("raw{c}"),
                pos: None,
            }),
            carve::BlockNode::BlockImage(carve::Image {
                attrs: None,
                src: "img".to_string(),
                alt: format!("alt{c}"),
                title: None,
                ref_label: None,
                raw_ref: None,
                pos: None,
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
                        pos: None,
                    }),
                    carve::InlineNode::Abbreviation(carve::Abbreviation {
                        abbr: format!("abbr{c}"),
                        expansion: format!("exp{c}"),
                        pos: None,
                    }),
                    carve::InlineNode::Mention(carve::Mention {
                        attrs: None,
                        user: format!("user{c}"),
                        pos: None,
                    }),
                    carve::InlineNode::Tag(carve::Tag {
                        attrs: None,
                        name: format!("tag{c}"),
                        pos: None,
                    }),
                    carve::InlineNode::Footnote(carve::Footnote {
                        attrs: None,
                        id: Some(format!("id{c}")),
                        inline: None,
                        number: None,
                        ref_id: None,
                        pos: None,
                    }),
                    carve::InlineNode::CriticSubstitute(carve::CriticSubstitute {
                        old_text: format!("old{c}"),
                        new_text: format!("new{c}"),
                        pos: None,
                    }),
                    carve::InlineNode::CrossRef(carve::CrossRef {
                        target: format!("target{c}"),
                        href: None,
                        pos: None,
                    }),
                ],
            }),
        ],
    };

    // The TERMINAL target is unchanged and is the one that has to be: this is
    // the assertion §29 T4 exists to protect, and it must not narrow.
    assert_no_author_osc(
        &carve::render_ansi(&doc).expect("the tree under test is within the render ceiling"),
    );
    // The plain target emits the same bytes now (§29 T3), through every one of
    // the same leaf fields - which is the reach this case measures.
    let plain =
        carve::render_plain_text(&doc).expect("the tree under test is within the render ceiling");
    assert!(plain.contains("\x1b]0;p\x07"), "{plain:?}");
}
