use carve::{
    BeforeRenderContext, BlockMatch, BlockNode, CarveExtension, Document, InlineExtension,
    InlineMatch, InlineNode, MatcherContext, Options, Paragraph, RenderContext,
};

struct Kbd;

impl CarveExtension for Kbd {
    fn name(&self) -> &'static str {
        "kbd"
    }

    fn render_inline_extension(
        &self,
        node: &InlineExtension,
        ctx: &RenderContext<'_>,
    ) -> Option<String> {
        if node.name == "kbd" {
            Some(format!("<kbd>{}</kbd>", ctx.render_inlines(&node.children)))
        } else {
            None
        }
    }
}

#[test]
fn inline_extension_renderer_overrides_core_fallback() {
    let kbd = Kbd;
    let options = Options::new().with_extension(&kbd);

    assert_eq!(
        carve::to_html_with_options("Press :kbd[Ctrl].", &options),
        "<p>Press <kbd>Ctrl</kbd>.</p>"
    );
}

#[test]
fn unknown_inline_extension_uses_fallback() {
    assert_eq!(
        carve::to_html("Press :widget[Ctrl]."),
        "<p>Press <span class=\"ext-widget\">Ctrl</span>.</p>"
    );
}

#[test]
fn the_semantic_span_extension_carries_the_other_four_and_the_old_spelling() {
    use carve::extensions::semantic_span::SemanticSpan;
    let ext = SemanticSpan;
    let mut options = carve::Options::default();
    options.extensions.push(&ext);

    // The four names core does not reserve (spec PART 9 §10).
    for (source, html) in [
        ("[x]{samp}", "<samp>x</samp>"),
        ("[x]{var}", "<var>x</var>"),
        ("[Dune]{cite}", "<cite>Dune</cite>"),
        (
            "[CSS]{dfn=\"Cascading Style Sheets\"}",
            "<dfn title=\"Cascading Style Sheets\">CSS</dfn>",
        ),
    ] {
        assert_eq!(
            carve::to_html_with_options(source, &options).trim(),
            format!("<p>{html}</p>"),
            "extension name {source}"
        );
    }

    // The soft-deprecated `:name[...]` spelling, for all seven.
    assert_eq!(
        carve::to_html_with_options(":kbd[Ctrl]", &options).trim(),
        "<p><kbd>Ctrl</kbd></p>"
    );
    assert_eq!(
        carve::to_html_with_options(":samp[out]", &options).trim(),
        "<p><samp>out</samp></p>"
    );
    // ...and no name outside the seven.
    assert_eq!(
        carve::to_html_with_options(":code[x]", &options).trim(),
        "<p><span class=\"ext-code\">x</span></p>"
    );
}

#[test]
fn semantic_shorthands_render_as_html_elements() {
    // PART 9 §9: core reserves THREE names, as SPAN ATTRIBUTES.
    for (name, html) in [
        ("kbd", "<kbd>x</kbd>"),
        ("abbr", "<abbr>x</abbr>"),
        ("time", "<time>x</time>"),
    ] {
        assert_eq!(
            carve::to_html(&format!("[x]{{{name}}}")),
            format!("<p>{html}</p>"),
            "core semantic attribute {name}"
        );
    }

    // The four the SemanticSpan extension carries stay ordinary attributes
    // until it is registered, and NO `:name[...]` spelling is handled at all.
    for name in ["samp", "var", "cite", "dfn"] {
        assert_eq!(
            carve::to_html(&format!("[x]{{{name}}}")),
            format!("<p><span {name}=\"\">x</span></p>"),
            "extension-only name {name}"
        );
    }
    for name in [
        "kbd", "abbr", "time", "samp", "var", "cite", "dfn", "code", "mark",
    ] {
        assert_eq!(
            carve::to_html(&format!(":{name}[x]")),
            format!("<p><span class=\"ext-{name}\">x</span></p>"),
            "core must register no :{name}[...] handler"
        );
    }
    // A non-semantic name still falls back to a generic span.
    assert_eq!(
        carve::to_html(":foo[x]"),
        "<p><span class=\"ext-foo\">x</span></p>"
    );

    // PART 9 §9: the registry holds no element Carve already spells, so these
    // two take the same fallback. `code` is why the rule is a rule: a code span
    // is verbatim while an extension body is parsed, so the registry entry gave
    // one tag two content models.
    for (source, html) in [
        (
            ":code[*b*]",
            "<span class=\"ext-code\"><strong>b</strong></span>",
        ),
        (
            ":mark[*b*]",
            "<span class=\"ext-mark\"><strong>b</strong></span>",
        ),
        ("`*b*`", "<code>*b*</code>"),
        ("=*b*=", "<mark><strong>b</strong></mark>"),
    ] {
        assert_eq!(
            carve::to_html(source),
            format!("<p>{html}</p>"),
            "the spelling Carve already has: {source}"
        );
    }

    // The same attributes on the COMPACT spelling, which core does reserve:
    // hardened, and riding the element the author wrote them on.
    assert_eq!(
        carve::to_html("[*noon*]{#clock .local time=\"12:00\" onclick=\"x\"}"),
        "<p><time datetime=\"12:00\" id=\"clock\" class=\"local\"><strong>noon</strong></time></p>"
    );
    assert_eq!(
        carve::to_html(":widget[x]{.control}"),
        "<p><span class=\"ext-widget control\">x</span></p>"
    );
}

#[test]
fn semantic_shorthands_are_content_on_non_html_targets_and_preserve_source() {
    let source = ":abbr[*HTML*]{title=\"HyperText Markup Language\"}";
    assert_eq!(carve::to_plain_text(source), "HTML\n");
    assert!(carve::to_ansi(source).contains("HTML"));
    assert_eq!(carve::to_carve(source), format!("{source}\n"));
}

struct Wiki;

impl CarveExtension for Wiki {
    fn name(&self) -> &'static str {
        "wiki"
    }

    fn match_inline(
        &self,
        text: &str,
        pos: usize,
        _ctx: &MatcherContext<'_>,
    ) -> Option<InlineMatch> {
        let rest = text.get(pos..)?;
        let inner = rest.strip_prefix("[[")?.split_once("]]")?.0;
        Some(InlineMatch {
            node: InlineNode::text(format!("WIKI:{inner}")),
            end: pos + inner.len() + 4,
        })
    }
}

#[test]
fn inline_matcher_adds_opt_in_syntax() {
    let wiki = Wiki;
    let options = Options::new().with_extension(&wiki);

    assert_eq!(
        carve::to_html_with_options("See [[Home]].", &options),
        "<p>See WIKI:Home.</p>"
    );
}

struct HijackStrong;

impl CarveExtension for HijackStrong {
    fn name(&self) -> &'static str {
        "hijack-strong"
    }

    fn match_inline(
        &self,
        text: &str,
        pos: usize,
        _ctx: &MatcherContext<'_>,
    ) -> Option<InlineMatch> {
        if text.get(pos..)?.starts_with("*x*") {
            Some(InlineMatch {
                node: InlineNode::text("hijacked".to_string()),
                end: pos + 3,
            })
        } else {
            None
        }
    }
}

#[test]
fn inline_matchers_run_after_core_syntax_declines() {
    let hijack = HijackStrong;
    let options = Options::new().with_extension(&hijack);

    assert_eq!(
        carve::to_html_with_options("*x*", &options),
        "<p><strong>x</strong></p>"
    );
}

struct NoteBlock;

impl CarveExtension for NoteBlock {
    fn name(&self) -> &'static str {
        "note-block"
    }

    fn match_block(
        &self,
        lines: &[&str],
        start: usize,
        ctx: &MatcherContext<'_>,
    ) -> Option<BlockMatch> {
        let line = *lines.get(start)?;
        let content = line.strip_prefix("NOTE: ")?;
        Some(BlockMatch {
            node: BlockNode::Paragraph(Paragraph {
                attrs: None,
                children: ctx.parse_inlines(content),
                ..Default::default()
            }),
            lines_consumed: 1,
        })
    }
}

#[test]
fn block_matcher_adds_opt_in_syntax() {
    let note = NoteBlock;
    let options = Options::new().with_extension(&note);

    assert_eq!(
        carve::to_html_with_options("NOTE: /careful/", &options),
        "<p><em>careful</em></p>"
    );
}

struct UppercaseBeforeRender;

impl CarveExtension for UppercaseBeforeRender {
    fn name(&self) -> &'static str {
        "uppercase"
    }

    fn before_render(&self, mut doc: Document, _ctx: &BeforeRenderContext<'_>) -> Document {
        if let Some(BlockNode::Paragraph(p)) = doc.children.first_mut() {
            if let Some(InlineNode::Text(text)) = p.children.first_mut() {
                text.value = text.value.to_uppercase();
            }
        }
        doc
    }
}

#[test]
fn before_render_transform_runs_in_to_html_with_options() {
    let uppercase = UppercaseBeforeRender;
    let options = Options::new().with_extension(&uppercase);

    assert_eq!(
        carve::to_html_with_options("hello", &options),
        "<p>HELLO</p>"
    );
}
