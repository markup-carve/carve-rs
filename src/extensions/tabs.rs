//! Render a `:::: tabs` container as a tab set, one tab per `::: tab` child.
//!
//! Port of carve-js `tabs.ts` and carve-php `TabsExtension`. Two interactive
//! modes: `Css`, the default, needs no script - a radio input per tab drives
//! the panels through CSS alone - and `Aria`, which emits `role="tablist"` with
//! buttons and panels for a page that ships its own behaviour.
//!
//! ```
//! use carve::{Options, Tabs};
//! let ext = Tabs::new();
//! let opts = Options::new().with_extension(&ext);
//! let src = ":::: tabs\n::: tab [One]\nFirst.\n:::\n::::";
//! let html = carve::to_html_with_options(src, &opts);
//! assert!(html.contains("class=\"tabs\""));
//! assert!(html.contains(">One</label>"));
//! ```

use std::cell::Cell;

use crate::ast::{Attrs, BlockExtension, BlockNode, Document, InlineNode};
use crate::extension::{BeforeRenderContext, CarveExtension, RenderContext};
use crate::render::render_attrs;

/// Sentinel name for the rewritten carrier node.
pub(crate) const CARRIER: &str = "carve-tabs";

/// How the interactive tab set is wired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TabsMode {
    /// A radio input per tab, driven by CSS alone. No script. The default.
    #[default]
    Css,
    /// `role="tablist"` with buttons and panels, for a page that ships its own
    /// keyboard and selection behaviour.
    Aria,
}

/// Class names, id prefix and mode the rendered markup uses.
#[derive(Debug, Clone)]
pub struct TabsOptions {
    /// `Css` (default) or `Aria`.
    pub mode: TabsMode,
    /// Class on the container. Default `tabs`.
    pub wrapper_class: String,
    /// Class on each panel. Default `tabs-panel`.
    pub tab_class: String,
    /// Class on each label or button. Default `tabs-label`.
    pub label_class: String,
    /// Class on each radio input, `Css` mode only. Default `tabs-radio`.
    pub radio_class: String,
    /// Prefix for generated ids. Default `tabset`.
    pub id_prefix: String,
}

impl Default for TabsOptions {
    fn default() -> Self {
        Self {
            mode: TabsMode::Css,
            wrapper_class: "tabs".to_string(),
            tab_class: "tabs-panel".to_string(),
            label_class: "tabs-label".to_string(),
            radio_class: "tabs-radio".to_string(),
            id_prefix: "tabset".to_string(),
        }
    }
}

/// Render `:::: tabs` containers as tab sets.
///
/// A tab's name is its opener `[label]`, then a `label=` attribute, then the
/// text of its first heading - which is then treated as the name rather than
/// as content - and finally `Tab N`. A tab carrying `selected` opens first;
/// without one the first tab does.
///
/// A container with no `::: tab` child is left to the core div renderer.
///
/// In static mode there is no interaction to preserve: every panel is shown in
/// sequence as a `<section>` headed by its label, so a reader of the PDF or the
/// archived page can tell the panels apart.
#[derive(Debug, Default)]
pub struct Tabs {
    opts: TabsOptions,
    /// Per-document counters, reset in `before_render`. `Cell` because the
    /// render hooks take `&self`; the trait has no `Sync` bound, and an
    /// extension instance renders one document at a time.
    set_counter: Cell<usize>,
    label_counter: Cell<usize>,
}

impl Tabs {
    /// A tab set with the default class names, in CSS mode.
    pub fn new() -> Self {
        Self::default()
    }

    /// A tab set with caller-chosen options.
    pub fn with_options(opts: TabsOptions) -> Self {
        Self {
            opts,
            set_counter: Cell::new(0),
            label_counter: Cell::new(0),
        }
    }
}

/// One tab: its name, its rendered body, and its initial state.
struct TabItem {
    label: String,
    content: String,
    selected: bool,
    id: Option<String>,
}

impl CarveExtension for Tabs {
    fn name(&self) -> &'static str {
        "tabs"
    }

    fn before_render(&self, mut doc: Document, _ctx: &BeforeRenderContext<'_>) -> Document {
        self.set_counter.set(0);
        self.label_counter.set(0);
        rewrite_blocks(&mut doc.children);
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

        let items = self.collect_tabs(&node.children, ctx);
        // No tab children: hand it back to the core renderer rather than
        // emitting an empty tab strip.
        if items.is_empty() {
            return None;
        }

        let pad = ctx.indent(ctx.level());
        let inner_pad = ctx.indent(ctx.level() + 1);

        if ctx.is_static() {
            let mut html = format!(
                "{pad}<div{}>\n",
                render_attrs(&self.wrapper_attrs(node, None))
            );
            for item in &items {
                html.push_str(&format!(
                    "{inner_pad}<section class=\"{}\">\n",
                    ctx.escape_attr(&self.opts.tab_class),
                ));
                html.push_str(&format!(
                    "{inner_pad}<h3 class=\"{}\">{}</h3>\n",
                    ctx.escape_attr(&self.opts.label_class),
                    ctx.escape_html(&item.label),
                ));
                html.push_str(&item.content);
                html.push_str(&format!("{inner_pad}</section>\n"));
            }
            html.push_str(&format!("{pad}</div>"));

            return Some(html);
        }

        self.set_counter.set(self.set_counter.get() + 1);
        // Generated ids join the document id namespace, so an explicit
        // `{#tabset-1}` or a colliding heading slug bumps these rather than
        // producing a duplicate.
        let set_id = ctx.unique_id(&format!(
            "{}-{}",
            self.opts.id_prefix,
            self.set_counter.get()
        ));

        match self.opts.mode {
            TabsMode::Css => Some(self.render_css(node, &items, &set_id, ctx)),
            TabsMode::Aria => Some(self.render_aria(node, &items, &set_id, ctx)),
        }
    }
}

impl Tabs {
    fn render_css(
        &self,
        node: &BlockExtension,
        items: &[TabItem],
        set_id: &str,
        ctx: &RenderContext<'_>,
    ) -> String {
        let pad = ctx.indent(ctx.level());
        let inner_pad = ctx.indent(ctx.level() + 1);
        let mut html = format!(
            "{pad}<div{}>\n",
            render_attrs(&self.wrapper_attrs(node, None))
        );

        for (index, item) in items.iter().enumerate() {
            let input_id = match &item.id {
                Some(id) => id.clone(),
                None => ctx.unique_id(&format!("{set_id}-tab-{}", index + 1)),
            };
            html.push_str(&format!(
                "{inner_pad}<input type=\"radio\" name=\"{}\" id=\"{}\" class=\"{}\"{}>\n",
                ctx.escape_attr(set_id),
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

        for item in items {
            html.push_str(&format!(
                "{inner_pad}<div class=\"{}\">\n{}</div>\n",
                ctx.escape_attr(&self.opts.tab_class),
                item.content,
            ));
        }

        html.push_str(&format!("{pad}</div>"));

        html
    }

    fn render_aria(
        &self,
        node: &BlockExtension,
        items: &[TabItem],
        set_id: &str,
        ctx: &RenderContext<'_>,
    ) -> String {
        let pad = ctx.indent(ctx.level());
        let inner_pad = ctx.indent(ctx.level() + 1);

        // Both id pairs are computed ONCE and reused by the two loops below, so
        // a bumped generated id keeps the aria-controls / aria-labelledby
        // wiring pointing at the right element.
        let pairs: Vec<(String, String)> = items
            .iter()
            .enumerate()
            .map(|(index, item)| match &item.id {
                Some(id) => (format!("{id}-tab"), format!("{id}-panel")),
                None => (
                    ctx.unique_id(&format!("{set_id}-tab-{}", index + 1)),
                    ctx.unique_id(&format!("{set_id}-panel-{}", index + 1)),
                ),
            })
            .collect();

        let mut html = format!(
            "{pad}<div{}>\n",
            render_attrs(&self.wrapper_attrs(node, Some("tablist"))),
        );

        for (item, (tab_id, panel_id)) in items.iter().zip(&pairs) {
            html.push_str(&format!(
                "{inner_pad}<button role=\"tab\" id=\"{}\" aria-selected=\"{}\" \
                 aria-controls=\"{}\" class=\"{}\"{}>{}</button>\n",
                ctx.escape_attr(tab_id),
                if item.selected { "true" } else { "false" },
                ctx.escape_attr(panel_id),
                ctx.escape_attr(&self.opts.label_class),
                if item.selected {
                    ""
                } else {
                    " tabindex=\"-1\""
                },
                ctx.escape_html(&item.label),
            ));
        }

        for (item, (tab_id, panel_id)) in items.iter().zip(&pairs) {
            html.push_str(&format!(
                "{inner_pad}<div role=\"tabpanel\" id=\"{}\" aria-labelledby=\"{}\" \
                 class=\"{}\"{}>\n{}</div>\n",
                ctx.escape_attr(panel_id),
                ctx.escape_attr(tab_id),
                ctx.escape_attr(&self.opts.tab_class),
                if item.selected { "" } else { " hidden" },
                item.content,
            ));
        }

        html.push_str(&format!("{pad}</div>"));

        html
    }

    /// Wrapper attributes: the wrapper class first, then the author's other
    /// classes, minus the structural `tabs` that selected this renderer.
    fn wrapper_attrs(&self, node: &BlockExtension, role: Option<&str>) -> Option<Attrs> {
        let mut attrs = node.attrs.clone().unwrap_or_default();
        let mut classes = vec![self.opts.wrapper_class.clone()];
        for class in &attrs.classes {
            if class != "tabs" && class != &self.opts.wrapper_class {
                classes.push(class.clone());
            }
        }
        attrs.classes = classes;
        if let Some(role) = role {
            attrs
                .key_values
                .insert("role".to_string(), role.to_string());
        }

        Some(attrs)
    }

    fn collect_tabs(&self, blocks: &[BlockNode], ctx: &RenderContext<'_>) -> Vec<TabItem> {
        let mut items = Vec::new();

        for block in blocks {
            let Some(tab) = as_tab(block) else { continue };

            items.push(TabItem {
                label: self.tab_label(&tab),
                content: self.tab_content(&tab, ctx),
                selected: tab
                    .attrs
                    .is_some_and(|attrs| attrs.key_values.contains_key("selected")),
                id: tab.attrs.and_then(|attrs| attrs.id.clone()),
            });
        }

        // Something has to be open. Without an authored `selected`, the first
        // tab is the one a reader sees.
        if !items.is_empty() && !items.iter().any(|item| item.selected) {
            items[0].selected = true;
        }

        items
    }

    /// The opener `[label]`, then a `label=` attribute, then the first
    /// heading's text, then `Tab N`.
    fn tab_label(&self, tab: &TabRef<'_>) -> String {
        if let Some(label) = explicit_label(tab) {
            return label;
        }

        for child in tab.children {
            if let BlockNode::Heading(heading) = child {
                return inline_text(&heading.children);
            }
        }

        self.label_counter.set(self.label_counter.get() + 1);

        format!("Tab {}", self.label_counter.get())
    }

    fn tab_content(&self, tab: &TabRef<'_>, ctx: &RenderContext<'_>) -> String {
        // A heading only stops being content when it is what NAMED the tab.
        let mut skip_heading = explicit_label(tab).is_none();
        let mut html = String::new();

        // The quoted opener title is CONTENT, not the tab's name - naming is
        // the `[label]`'s job - so it stays inside the panel as the same
        // admonition-title line core would emit.
        if let Some(title) = tab.title {
            html.push_str(&format!(
                "<p class=\"admonition-title\">{}</p>\n",
                ctx.render_inlines(title),
            ));
        }

        for child in tab.children {
            if skip_heading && matches!(child, BlockNode::Heading(_)) {
                skip_heading = false;
                continue;
            }
            let fragment = ctx.render_blocks_at(std::slice::from_ref(child), 0);
            if !fragment.is_empty() {
                html.push_str(&fragment);
                html.push('\n');
            }
        }

        html
    }
}

/// The parts of a `::: tab` this extension reads, from either spelling.
struct TabRef<'a> {
    attrs: Option<&'a Attrs>,
    title: Option<&'a Vec<InlineNode>>,
    label: Option<&'a String>,
    children: &'a [BlockNode],
}

fn explicit_label(tab: &TabRef<'_>) -> Option<String> {
    if let Some(label) = tab.label {
        return Some(label.clone());
    }

    tab.attrs
        .and_then(|attrs| attrs.key_values.get("label"))
        .cloned()
}

/// A tab is either the typed `::: tab`, which parses to an admonition, or a
/// div carrying the class.
fn as_tab(block: &BlockNode) -> Option<TabRef<'_>> {
    match block {
        BlockNode::Admonition(admonition) if admonition.kind == "tab" => Some(TabRef {
            attrs: admonition.attrs.as_ref(),
            title: admonition.title.as_ref(),
            label: admonition.label.as_ref(),
            children: &admonition.children,
        }),
        BlockNode::Div(div)
            if div
                .attrs
                .as_ref()
                .is_some_and(|attrs| attrs.classes.iter().any(|class| class == "tab")) =>
        {
            Some(TabRef {
                attrs: div.attrs.as_ref(),
                title: None,
                label: div.label.as_ref(),
                children: &div.children,
            })
        }
        _ => None,
    }
}

fn is_tabs(block: &BlockNode) -> bool {
    match block {
        BlockNode::Admonition(admonition) => admonition.kind == "tabs",
        BlockNode::Div(div) => div
            .attrs
            .as_ref()
            .is_some_and(|attrs| attrs.classes.iter().any(|class| class == "tabs")),
        _ => false,
    }
}

fn rewrite_blocks(blocks: &mut [BlockNode]) {
    for block in blocks.iter_mut() {
        if is_tabs(block) {
            *block = match block {
                BlockNode::Admonition(admonition) => BlockNode::Extension(BlockExtension {
                    attrs: admonition.attrs.take(),
                    name: CARRIER.to_string(),
                    children: std::mem::take(&mut admonition.children),
                    summary: None,
                    label: admonition.label.take(),
                    pos: None,
                }),
                BlockNode::Div(div) => BlockNode::Extension(BlockExtension {
                    attrs: div.attrs.take(),
                    name: CARRIER.to_string(),
                    children: std::mem::take(&mut div.children),
                    summary: None,
                    label: div.label.take(),
                    pos: None,
                }),
                _ => unreachable!("is_tabs matched neither variant"),
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

fn inline_text(nodes: &[InlineNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            InlineNode::Text(text) => out.push_str(&text.value),
            InlineNode::Code(code) => out.push_str(&code.value),
            InlineNode::LiteralInline(literal) => out.push_str(&literal.content),
            InlineNode::Emphasis(emphasis) => out.push_str(&inline_text(&emphasis.children)),
            InlineNode::Link(link) => out.push_str(&inline_text(&link.children)),
            InlineNode::Span(span) => out.push_str(&inline_text(&span.children)),
            InlineNode::Extension(extension) => out.push_str(&inline_text(&extension.children)),
            _ => {}
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Mode, Options};

    const TWO_TABS: &str =
        ":::: tabs\n::: tab [One]\nFirst.\n:::\n\n::: tab [Two]\nSecond.\n:::\n::::";

    fn html(source: &str) -> String {
        let ext = Tabs::new();
        let opts = Options::new().with_extension(&ext);
        crate::to_html_with_options(source, &opts)
    }

    fn aria_html(source: &str) -> String {
        let ext = Tabs::with_options(TabsOptions {
            mode: TabsMode::Aria,
            ..TabsOptions::default()
        });
        let opts = Options::new().with_extension(&ext);
        crate::to_html_with_options(source, &opts)
    }

    fn static_html(source: &str) -> String {
        let ext = Tabs::new();
        let opts = Options::new().with_extension(&ext).with_mode(Mode::Static);
        crate::to_html_with_options(source, &opts)
    }

    #[test]
    fn a_tabs_container_becomes_a_tab_strip() {
        let out = html(TWO_TABS);
        assert!(out.contains("class=\"tabs\""), "{out}");
        assert_eq!(out.matches("type=\"radio\"").count(), 2, "{out}");
        assert!(out.contains(">One</label>"), "{out}");
        assert!(out.contains(">Two</label>"), "{out}");
    }

    #[test]
    fn the_first_tab_opens_when_none_claims_it() {
        let out = html(TWO_TABS);
        assert_eq!(out.matches(" checked>").count(), 1, "{out}");
    }

    #[test]
    fn a_heading_names_the_tab_and_stops_being_content() {
        let out = html(":::: tabs\n::: tab\n# Named\nBody.\n:::\n::::");
        assert!(out.contains(">Named</label>"), "{out}");
        assert!(!out.contains("<h1"), "{out}");
        assert!(out.contains("Body."), "{out}");
    }

    #[test]
    fn a_tab_with_neither_label_nor_heading_is_numbered() {
        let out = html(":::: tabs\n::: tab\nBody.\n:::\n::::");
        assert!(out.contains(">Tab 1</label>"), "{out}");
    }

    #[test]
    fn a_container_with_no_tab_child_is_left_to_the_core_renderer() {
        let out = html(":::: tabs\nJust prose.\n::::");
        assert!(!out.contains("type=\"radio\""), "{out}");
        assert!(out.contains("Just prose."), "{out}");
    }

    #[test]
    fn aria_mode_emits_a_tablist_rather_than_radios() {
        let out = aria_html(TWO_TABS);
        assert!(!out.contains("type=\"radio\""), "{out}");
        assert!(out.contains("role=\"tablist\""), "{out}");
        assert_eq!(out.matches("role=\"tab\"").count(), 2, "{out}");
        assert_eq!(out.matches("role=\"tabpanel\"").count(), 2, "{out}");
        assert!(out.contains("aria-selected=\"true\""), "{out}");
        assert!(out.contains(" hidden>"), "{out}");
    }

    #[test]
    fn aria_wiring_points_each_panel_at_its_own_tab() {
        let out = aria_html(TWO_TABS);
        let tab_id = out
            .split("id=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .unwrap()
            .to_string();
        assert!(
            out.contains(&format!("aria-labelledby=\"{tab_id}\"")),
            "{out}"
        );
    }

    #[test]
    fn static_mode_shows_every_panel_headed_by_its_label() {
        let out = static_html(TWO_TABS);
        assert!(!out.contains("type=\"radio\""), "{out}");
        assert_eq!(out.matches("<section").count(), 2, "{out}");
        assert!(out.contains("<h3 class=\"tabs-label\">One</h3>"), "{out}");
        assert!(out.contains("Second."), "{out}");
    }

    #[test]
    fn an_author_class_survives_beside_the_wrapper_class() {
        let out = html("{.tabs .compact}\n::::\n::: tab [One]\nFirst.\n:::\n::::");
        assert!(out.contains("class=\"tabs compact\""), "{out}");
    }
}
