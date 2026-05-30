use carve::{
    BlockMatch, BlockNode, CarveExtension, Document, InlineExtension, InlineMatch, InlineNode,
    MatcherContext, Options, Paragraph, RenderContext,
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
            node: InlineNode::Text(format!("WIKI:{inner}")),
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
                node: InlineNode::Text("hijacked".to_string()),
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

    fn before_render(&self, mut doc: Document) -> Document {
        if let Some(BlockNode::Paragraph(p)) = doc.children.first_mut() {
            if let Some(InlineNode::Text(text)) = p.children.first_mut() {
                *text = text.to_uppercase();
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
