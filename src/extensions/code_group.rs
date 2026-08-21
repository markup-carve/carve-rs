//! Render a `::: code-group` container as a set of radio-driven tabs, one per
//! fenced code block.
//!
//! Port of carve-js `code-group.ts` and carve-php `CodeGroupExtension`.
//! carve-js keys a block renderer on the `admonition` and `div` node types;
//! carve-rs has no per-node render hook for an existing node, so this follows
//! the same shape [`crate::Details`] uses: a `before_render` pass rewrites
//! every code group into a [`BlockNode::Extension`] carrier, and
//! [`CarveExtension::render_block_extension`] renders it.
//!
//! ```
//! use carve::{CodeGroup, Options};
//! let ext = CodeGroup::new();
//! let opts = Options::new().with_extension(&ext);
//! let src = "::: code-group\n``` js\nlet a = 1\n```\n:::";
//! let html = carve::to_html_with_options(src, &opts);
//! assert!(html.contains("class=\"code-group\""));
//! assert!(html.contains("type=\"radio\""));
//! ```

use std::cell::Cell;

use crate::ast::{AttrSlot, Attrs, BlockExtension, BlockNode, CodeBlock, Document};
use crate::extension::{BeforeRenderContext, CarveExtension, RenderContext};
use crate::render::render_attrs;

/// Sentinel name for the rewritten carrier node. The profile filter still
/// gates it as a `div` (its origin), so a restrictive profile strips the group
/// exactly as it would the underlying container.
pub(crate) const CARRIER: &str = "carve-code-group";

/// CSS class names and the id prefix the rendered markup uses.
#[derive(Debug, Clone)]
pub struct CodeGroupOptions {
    /// Class on the wrapper. Default `code-group`.
    pub wrapper_class: String,
    /// Class on each code panel. Default `code-group-panel`.
    pub panel_class: String,
    /// Class on each tab label. Default `code-group-label`.
    pub label_class: String,
    /// Class on each radio input. Default `code-group-radio`.
    pub radio_class: String,
    /// Prefix for generated ids and radio-group names. Default `codegroup`.
    pub id_prefix: String,
    /// Accessible name for the code group AS A WHOLE, overriding the render's
    /// `labels` map for this instance; empty writes no name.
    ///
    /// Each tab was already named by its own `<label>`; the GROUP was anonymous
    /// (markup-carve/carve#1468). `None` reads `codeGroup` from the map.
    pub group_label: Option<String>,
}

impl Default for CodeGroupOptions {
    fn default() -> Self {
        Self {
            wrapper_class: "code-group".to_string(),
            panel_class: "code-group-panel".to_string(),
            label_class: "code-group-label".to_string(),
            radio_class: "code-group-radio".to_string(),
            id_prefix: "codegroup".to_string(),
            group_label: None,
        }
    }
}

/// Render `::: code-group` containers as radio-driven tabbed code panels.
///
/// A group is either a `::: code-group` typed div, which parses to an
/// admonition, or any div carrying the `code-group` class. Each fenced code
/// block inside becomes one tab; the tab's name is the block's `[label]`, its
/// language, or `Code N` in that order. A block carrying a `selected`
/// attribute opens first, and without one the first block does.
///
/// A container holding no code block is left to the core div renderer, which
/// is what carve-js and carve-php do - the class alone is not a reason to
/// build an empty tab strip.
///
/// In static mode there are no radios: each panel becomes a `<section>` headed
/// by its label, so a reader offline can still tell the panels apart.
#[derive(Debug, Default)]
pub struct CodeGroup {
    opts: CodeGroupOptions,
    /// Per-document counter, reset in `before_render`. `Cell` because the
    /// render hooks take `&self`; the trait has no `Sync` bound, and an
    /// extension instance renders one document at a time. Same shape as
    /// `Tabs::set_counter`, for the same reason.
    group_counter: Cell<usize>,
}

impl CodeGroup {
    /// A code-group extension with the default class names.
    pub fn new() -> Self {
        Self::default()
    }

    /// A code-group extension with caller-chosen class names.
    pub fn with_options(opts: CodeGroupOptions) -> Self {
        Self {
            opts,
            group_counter: Cell::new(0),
        }
    }
}

/// One tab: the code block, plus the name and initial state resolved for it.
struct GroupItem<'a> {
    block: &'a CodeBlock,
    language: Option<&'a str>,
    label: String,
    selected: bool,
}

impl CarveExtension for CodeGroup {
    fn name(&self) -> &'static str {
        "code-group"
    }

    fn before_render(&self, mut doc: Document, _ctx: &BeforeRenderContext<'_>) -> Document {
        self.group_counter.set(0);
        rewrite_blocks(&mut doc.children);
        // Footnote bodies live outside the tree but are still rendered, so a
        // group inside a footnote definition must be rewritten too.
        for blocks in doc.footnote_defs.values_mut() {
            rewrite_blocks(blocks);
        }
        doc
    }

    fn render_block_extension(
        &self,
        node: &BlockExtension,
        ctx: &RenderContext<'_>,
    ) -> Option<String> {
        if node.name != CARRIER {
            return None;
        }

        let items = collect_items(&node.children);
        // No code blocks: hand it back to the core renderer rather than
        // emitting an empty tab strip.
        if items.is_empty() {
            return None;
        }

        let level = ctx.level();
        let pad = ctx.indent(level);
        let inner_pad = ctx.indent(level + 1);
        let attrs = self.wrapper_attrs(node.attrs.as_ref(), ctx);

        if ctx.is_static() {
            let mut html = format!("{pad}<div{}>\n", render_attrs(&attrs));
            for item in &items {
                html.push_str(&format!(
                    "{inner_pad}<section class=\"{}\">\n",
                    ctx.escape_attr(&self.opts.panel_class),
                ));
                html.push_str(&format!(
                    "{inner_pad}<h3 class=\"{}\">{}</h3>\n",
                    ctx.escape_attr(&self.opts.label_class),
                    ctx.escape_html(&item.label),
                ));
                html.push_str(&self.render_code(item, ctx));
                html.push_str(&format!("{inner_pad}</section>\n"));
            }
            html.push_str(&format!("{pad}</div>"));

            return Some(html);
        }

        // Number the group FIRST, then reserve the numbered string in the
        // document id namespace. Taking the bare prefix instead made the first
        // group `codegroup` and left `-2` to arrive as a COLLISION suffix, so
        // carve-js and carve-php said `codegroup-1` where this said
        // `codegroup`, and an unrelated `{#codegroup}` elsewhere in the
        // document shifted every group's name. Tabs has always done it this
        // way (`tabs.rs`); this is that shape.
        self.group_counter.set(self.group_counter.get() + 1);
        let group_id = ctx.unique_id(&format!(
            "{}-{}",
            self.opts.id_prefix,
            self.group_counter.get()
        ));
        let mut html = format!("{pad}<div{}>\n", render_attrs(&attrs));

        for (index, item) in items.iter().enumerate() {
            let input_id = ctx.unique_id(&format!("{group_id}-tab-{}", index + 1));
            html.push_str(&format!(
                "{inner_pad}<input type=\"radio\" name=\"{}\" id=\"{}\" class=\"{}\"{}>\n",
                ctx.escape_attr(&group_id),
                ctx.escape_attr(&input_id),
                ctx.escape_attr(&self.opts.radio_class),
                if item.selected { " checked" } else { "" },
            ));
            html.push_str(&format!(
                "{inner_pad}<label for=\"{}\" class=\"{}\">{}</label>\n",
                ctx.escape_attr(&input_id),
                ctx.escape_attr(&self.opts.label_class),
                ctx.escape_html(&item.label),
            ));
        }

        for item in &items {
            html.push_str(&format!(
                "{inner_pad}<div class=\"{}\">",
                ctx.escape_attr(&self.opts.panel_class),
            ));
            html.push_str(&self.render_code(item, ctx));
            html.push_str("</div>\n");
        }

        html.push_str(&format!("{pad}</div>"));

        Some(html)
    }
}

impl CodeGroup {
    /// Wrapper attributes: the wrapper class first, then the author's other
    /// classes in order, minus the structural `code-group` that selected this
    /// renderer in the first place.
    fn wrapper_attrs(&self, source: Option<&Attrs>, ctx: &RenderContext<'_>) -> Option<Attrs> {
        let mut attrs = source.cloned().unwrap_or_default();
        let mut classes = vec![self.opts.wrapper_class.clone()];
        for class in &attrs.classes {
            if class != "code-group" && class != &self.opts.wrapper_class {
                classes.push(class.clone());
            }
        }
        attrs.classes = classes;
        // The group carries a ROLE and a NAME (markup-carve/carve#1468). Each
        // tab was named by its own `<label>` and the group was not; this
        // extension's own docs used to send you to Tabs for it, which costs the
        // language labels and the highlighting that are the reason to use it.
        //
        // Anything the author wrote WINS, matched ASCII-case-insensitively.
        let authored = |name: &str| {
            source.is_some_and(|a| a.key_values.keys().any(|k| k.eq_ignore_ascii_case(name)))
        };
        if !authored("role") {
            attrs
                .key_values
                .insert("role".to_string(), "group".to_string());
            crate::extension::record_attr_order(&mut attrs, "role");
        }
        let group_label = self
            .opts
            .group_label
            .clone()
            .unwrap_or_else(|| ctx.label(crate::extension::LABEL_CODE_GROUP));
        if !group_label.is_empty() && !authored("aria-label") && !authored("aria-labelledby") {
            attrs
                .key_values
                .insert("aria-label".to_string(), group_label);
            crate::extension::record_attr_order(&mut attrs, "aria-label");
        }

        Some(attrs)
    }

    fn render_code(&self, item: &GroupItem<'_>, ctx: &RenderContext<'_>) -> String {
        let lang_attr = match item.language {
            Some(lang) => format!(" class=\"language-{}\"", ctx.escape_attr(lang)),
            None => String::new(),
        };
        let attrs = strip_selected(item.block.attrs.as_ref());

        format!(
            "<pre{}><code{lang_attr}>{}\n</code></pre>\n",
            render_attrs(&attrs),
            ctx.escape_html(&item.block.content),
        )
    }
}

/// The `selected` marker is this extension's own input, not something to hand
/// on to the rendered `<pre>`.
fn strip_selected(source: Option<&Attrs>) -> Option<Attrs> {
    let attrs = source?;
    if !attrs.key_values.contains_key("selected") {
        return Some(attrs.clone());
    }

    let mut stripped = attrs.clone();
    stripped.key_values.remove("selected");
    stripped
        .order
        .retain(|slot| !matches!(slot, AttrSlot::Key(key) if key == "selected"));

    Some(stripped)
}

/// Resolve one tab per fenced code block: its name and whether it opens first.
fn collect_items(blocks: &[BlockNode]) -> Vec<GroupItem<'_>> {
    let mut items = Vec::new();

    for block in blocks {
        let BlockNode::CodeBlock(code) = block else {
            continue;
        };

        let language = code.lang.as_deref().filter(|lang| !lang.is_empty());
        let label = code
            .label
            .as_deref()
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .map(str::to_string)
            .or_else(|| language.map(str::to_string))
            .unwrap_or_else(|| format!("Code {}", items.len() + 1));
        let selected = code
            .attrs
            .as_ref()
            .is_some_and(|attrs| attrs.key_values.contains_key("selected"));

        items.push(GroupItem {
            block: code,
            language,
            label,
            selected,
        });
    }

    // Something has to be open. Without an authored `selected`, the first tab
    // is the one a reader sees.
    if !items.is_empty() && !items.iter().any(|item| item.selected) {
        items[0].selected = true;
    }

    items
}

/// Is this node a code group - either the `::: code-group` typed div, which
/// parses to an admonition, or a div carrying the class?
fn is_code_group(block: &BlockNode) -> bool {
    match block {
        BlockNode::Admonition(admonition) => admonition.kind == "code-group",
        BlockNode::Div(div) => div
            .attrs
            .as_ref()
            .is_some_and(|attrs| attrs.classes.iter().any(|class| class == "code-group")),
        _ => false,
    }
}

fn rewrite_blocks(blocks: &mut [BlockNode]) {
    for block in blocks.iter_mut() {
        if is_code_group(block) {
            *block = match block {
                BlockNode::Admonition(admonition) => {
                    rewrite_blocks(&mut admonition.children);
                    BlockNode::Extension(BlockExtension {
                        attrs: admonition.attrs.take(),
                        name: CARRIER.to_string(),
                        children: std::mem::take(&mut admonition.children),
                        summary: None,
                        label: admonition.label.take(),
                        pos: None,
                    })
                }
                BlockNode::Div(div) => {
                    rewrite_blocks(&mut div.children);
                    BlockNode::Extension(BlockExtension {
                        attrs: div.attrs.take(),
                        name: CARRIER.to_string(),
                        children: std::mem::take(&mut div.children),
                        summary: None,
                        label: div.label.take(),
                        pos: None,
                    })
                }
                _ => unreachable!("is_code_group matched neither variant"),
            };

            continue;
        }

        match block {
            BlockNode::List(list) => {
                for item in &mut list.items {
                    rewrite_blocks(&mut item.children);
                }
            }
            BlockNode::BlockQuote(quote) => rewrite_blocks(&mut quote.children),
            BlockNode::Admonition(admonition) => rewrite_blocks(&mut admonition.children),
            BlockNode::Div(div) => rewrite_blocks(&mut div.children),
            BlockNode::Extension(extension) => rewrite_blocks(&mut extension.children),
            BlockNode::DefinitionList(definition_list) => {
                for item in &mut definition_list.items {
                    for definition in &mut item.definitions {
                        rewrite_blocks(definition);
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Mode, Options};

    fn html(source: &str) -> String {
        let ext = CodeGroup::new();
        let opts = Options::new().with_extension(&ext);
        crate::to_html_with_options(source, &opts)
    }

    fn static_html(source: &str) -> String {
        let ext = CodeGroup::new();
        let opts = Options::new().with_extension(&ext).with_mode(Mode::Static);
        crate::to_html_with_options(source, &opts)
    }

    const TWO_PANELS: &str = "::: code-group\n``` js\nlet a = 1\n```\n\n``` php\n$a = 1;\n```\n:::";

    #[test]
    fn a_typed_div_becomes_a_tab_strip() {
        let out = html(TWO_PANELS);
        assert!(out.contains("class=\"code-group\""), "{out}");
        assert_eq!(out.matches("type=\"radio\"").count(), 2, "{out}");
        assert_eq!(out.matches("code-group-panel").count(), 2, "{out}");
    }

    #[test]
    fn the_language_names_the_tab() {
        let out = html(TWO_PANELS);
        assert!(out.contains(">js</label>"), "{out}");
        assert!(out.contains(">php</label>"), "{out}");
        assert!(out.contains("class=\"language-js\""), "{out}");
    }

    #[test]
    fn the_first_tab_opens_when_no_block_claims_it() {
        let out = html(TWO_PANELS);
        assert_eq!(out.matches(" checked>").count(), 1, "{out}");
        let checked = out.find(" checked>").unwrap();
        let second = out.find("-tab-2").unwrap();
        assert!(checked < second, "the FIRST tab should be checked: {out}");
    }

    #[test]
    fn a_class_carrying_div_is_a_group_too() {
        let out = html("{.code-group}\n:::\n``` js\nlet a = 1\n```\n:::");
        assert!(out.contains("type=\"radio\""), "{out}");
    }

    #[test]
    fn a_group_with_no_code_block_is_left_to_the_core_renderer() {
        let out = html("::: code-group\nJust prose.\n:::");
        assert!(!out.contains("type=\"radio\""), "{out}");
        assert!(out.contains("Just prose."), "{out}");
    }

    #[test]
    fn an_author_class_survives_beside_the_wrapper_class() {
        let out = html("{.code-group .tight}\n:::\n``` js\nlet a = 1\n```\n:::");
        assert!(out.contains("class=\"code-group tight\""), "{out}");
    }

    #[test]
    fn static_mode_drops_the_radios_for_headed_sections() {
        let out = static_html(TWO_PANELS);
        assert!(!out.contains("type=\"radio\""), "{out}");
        assert_eq!(out.matches("<section").count(), 2, "{out}");
        assert!(
            out.contains("<h3 class=\"code-group-label\">js</h3>"),
            "{out}"
        );
    }

    #[test]
    fn two_groups_in_one_document_get_distinct_ids() {
        let out = html(&format!("{TWO_PANELS}\n\n{TWO_PANELS}"));
        let ids: Vec<&str> = out
            .match_indices("name=\"")
            .map(|(i, _)| &out[i..i + 24])
            .collect();
        assert!(ids.len() >= 4, "{out}");
        assert!(
            ids.iter().any(|id| !id.contains("codegroup\"")),
            "a second group must not reuse the first group's name: {out}"
        );
    }

    /// The names carve-js and carve-php emit, asserted literally. The test
    /// above cannot see this defect: it only asks that SOME name differs from
    /// `codegroup`, which held while the first group was named `codegroup` and
    /// the second `codegroup-2`.
    #[test]
    fn the_first_group_is_numbered_like_the_other_engines() {
        let out = html(&format!("{TWO_PANELS}\n\n{TWO_PANELS}"));
        assert!(out.contains("name=\"codegroup-1\""), "{out}");
        assert!(out.contains("id=\"codegroup-1-tab-1\""), "{out}");
        assert!(out.contains("for=\"codegroup-1-tab-1\""), "{out}");
        assert!(out.contains("name=\"codegroup-2\""), "{out}");
        assert!(
            !out.contains("name=\"codegroup\""),
            "the bare prefix is carve-rs-only: {out}"
        );
    }

    /// A group's number depends on how many groups precede it, and on nothing
    /// else. Reserving the bare prefix made an unrelated `{#codegroup}` in the
    /// document push the FIRST group to `codegroup-2`.
    #[test]
    fn an_unrelated_explicit_id_does_not_renumber_the_group() {
        let out = html(&format!("# H {{#codegroup}}\n\n{TWO_PANELS}"));
        assert!(out.contains("name=\"codegroup-1\""), "{out}");
    }
}
