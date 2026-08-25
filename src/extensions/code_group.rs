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
use crate::extensions::tabs::{apply_single_selection, SingleSelect, TabsMode};
use crate::render::{render_attrs, render_attrs_for};

/// Sentinel name for the rewritten carrier node. The profile filter still
/// gates it as a `div` (its origin), so a restrictive profile strips the group
/// exactly as it would the underlying container.
pub(crate) const CARRIER: &str = "carve-code-group";

/// CSS class names and the id prefix the rendered markup uses.
#[derive(Debug, Clone)]
pub struct CodeGroupOptions {
    /// `Css` (default) or `Aria`, the same two-valued option Tabs carries.
    ///
    /// THE SAME TYPE, not a copy of it: extensions §13.1 states one `mode`
    /// option with one vocabulary binding both constructs, and "two constructs
    /// of the same shape do not get different accessibility ceilings because
    /// one of them was written second". Two identical enums would be two places
    /// for that vocabulary to drift.
    ///
    /// `Css` is the default and an implementation MUST NOT ship `Aria` as one.
    /// That is §2.5 rather than compatibility: `Aria` reveals with `hidden`, so
    /// a page that registers it and ships no script loses every panel but the
    /// first, while `Css` with no stylesheet at all shows every panel. In Rust
    /// an unknown value is rejected by the type, which is what §13.1 asks of a
    /// host that spells the mode as a string.
    pub mode: TabsMode,
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
            mode: TabsMode::Css,
            panel_class: "code-group-panel".to_string(),
            label_class: "code-group-label".to_string(),
            radio_class: "code-group-radio".to_string(),
            id_prefix: "codegroup".to_string(),
            group_label: None,
        }
    }
}

/// Render `::: code-group` containers as tabbed code panels.
///
/// Two modes, the same two Tabs carries (extensions §13.1). `Css` is the
/// default: a radio per panel and a sibling `<label for=…>`, revealed by a
/// stylesheet, so with no stylesheet at all every panel is visible. `Aria`
/// emits `role="tablist"` with buttons and `role="tabpanel"` panels and hides
/// every unselected one, which needs a client script - which is exactly why it
/// is not the default: §2.5 says content is never dropped, only interaction.
///
/// A group is either a `::: code-group` typed div, which parses to an
/// admonition, or any div carrying the `code-group` class. Each fenced code
/// block inside becomes one tab; the tab's name is the block's `[label]`, its
/// language, or `Code N` in that order.
///
/// EXACTLY ONE PANEL IS SELECTED (extensions §13.5): the first block the
/// document marks `selected`, and the first block where it marks none. Marking
/// several is not an error and is not diagnosed - the later marks are ignored.
/// The rule is Tabs' rule and runs through Tabs' own step, because §13 binds
/// both constructs alike.
///
/// A container holding no code block is left to the core div renderer, which
/// is what carve-js and carve-php do - the class alone is not a reason to
/// build an empty tab strip.
///
/// In static mode there are no radios and neither mode applies: each panel
/// becomes a `<section>` headed by its label, so a reader offline can still
/// tell the panels apart.
///
/// A `Css` panel carries `role="group"` and its own label as an `aria-label`
/// (§13.2) - nothing else binds it to the control that reveals it. An `Aria`
/// panel takes neither: it is bound by `aria-labelledby` already, and a second
/// accessible name would leave which one applies undefined (§13.3).
///
/// The `Aria` control is a `<button type="button">` (§13.3), so a code group
/// inside a `<form>` switches panels instead of submitting the form.
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

impl SingleSelect for GroupItem<'_> {
    fn selected_mut(&mut self) -> &mut bool {
        &mut self.selected
    }
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
        if ctx.is_static() {
            // A STATIC RENDER TAKES NEITHER MODE (extensions §13.1). There are
            // no tabs left to list: `renderStatic` flattens the set to one
            // `<section>` per panel headed by its `[label]`, and the heading IS
            // the name. So `tablist` here would describe an interaction that
            // this output does not contain, whatever `mode` the host configured
            // - the role has to come from what was EMITTED and not from the
            // option. `group` regardless, which is also what this renderer
            // emitted before the option existed.
            let attrs = self.wrapper_attrs(node.attrs.as_ref(), "group", ctx);
            let mut html = format!("{pad}<div{}>\n", render_attrs_for(&attrs, "div"));
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

        let role = match self.opts.mode {
            TabsMode::Css => "group",
            TabsMode::Aria => "tablist",
        };
        let attrs = self.wrapper_attrs(node.attrs.as_ref(), role, ctx);

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
        match self.opts.mode {
            TabsMode::Css => Some(self.render_css(&attrs, &items, &group_id, ctx)),
            TabsMode::Aria => Some(self.render_aria(&attrs, &items, &group_id, ctx)),
        }
    }
}

impl CodeGroup {
    /// Wrapper attributes: the wrapper class first, then the author's other
    /// classes in order, minus the structural `code-group` that selected this
    /// renderer in the first place.
    fn wrapper_attrs(
        &self,
        source: Option<&Attrs>,
        role: &str,
        ctx: &RenderContext<'_>,
    ) -> Option<Attrs> {
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
            // `group` in `css` mode, `tablist` in `aria` - the same split
            // `tabs.rs` makes, for the same reason: the CSS mode has no
            // tab/panel roles to associate, so `group` is all it can claim.
            attrs
                .key_values
                .insert("role".to_string(), role.to_string());
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

    /// `css` mode: a radio per panel plus a sibling `<label for=…>`, revealed
    /// by a stylesheet. No script, and with no stylesheet at all every panel is
    /// visible - which is why extensions §13.1 makes this the default.
    fn render_css(
        &self,
        attrs: &Option<Attrs>,
        items: &[GroupItem<'_>],
        group_id: &str,
        ctx: &RenderContext<'_>,
    ) -> String {
        let level = ctx.level();
        let pad = ctx.indent(level);
        let inner_pad = ctx.indent(level + 1);
        let mut html = format!("{pad}<div{}>\n", render_attrs_for(attrs, "div"));

        for (index, item) in items.iter().enumerate() {
            let input_id = ctx.unique_id(&format!("{group_id}-tab-{}", index + 1));
            html.push_str(&format!(
                "{inner_pad}<input type=\"radio\" name=\"{}\" id=\"{}\" class=\"{}\"{}>\n",
                ctx.escape_attr(group_id),
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

        // THE PANEL CARRIES ITS OWN LABEL (extensions §13.2), keyed on the tab
        // name where one was written and the language word otherwise - which is
        // already what `item.label` resolves to. Same reasoning as Tabs:
        // `role="group"` rather than `tabpanel` because a radio reveals it, no
        // `<section>` because one landmark per panel is N per group, and no
        // `labels` key because the string is DERIVED from the document.
        for item in items {
            html.push_str(&format!(
                "{inner_pad}<div class=\"{}\" role=\"group\" aria-label=\"{}\">",
                ctx.escape_attr(&self.opts.panel_class),
                ctx.escape_attr(&item.label),
            ));
            html.push_str(&self.render_code(item, ctx));
            html.push_str("</div>\n");
        }

        html.push_str(&format!("{pad}</div>"));

        html
    }

    /// `aria` mode: `role="tablist"` with buttons, `role="tabpanel"` panels
    /// bound by `aria-labelledby`, and `hidden` on every panel but the selected
    /// one. Needs a client script, which is why it is not the default.
    ///
    /// A PANEL HERE TAKES NEITHER `role="group"` NOR A NAME (§13.3): the
    /// association already exists, and naming it as well would give one element
    /// two accessible names and pull it out of the `tablist` relationship that
    /// is the only reason to be in this mode.
    fn render_aria(
        &self,
        attrs: &Option<Attrs>,
        items: &[GroupItem<'_>],
        group_id: &str,
        ctx: &RenderContext<'_>,
    ) -> String {
        let level = ctx.level();
        let pad = ctx.indent(level);
        let inner_pad = ctx.indent(level + 1);

        // Both ids per panel are computed ONCE and reused by the two loops, so
        // a bumped generated id keeps the aria-controls / aria-labelledby
        // wiring pointing at the right element. `tabs.rs` says the same.
        let pairs: Vec<(String, String)> = (0..items.len())
            .map(|index| {
                (
                    ctx.unique_id(&format!("{group_id}-tab-{}", index + 1)),
                    ctx.unique_id(&format!("{group_id}-panel-{}", index + 1)),
                )
            })
            .collect();

        let mut html = format!("{pad}<div{}>\n", render_attrs_for(attrs, "div"));

        for (item, (tab_id, panel_id)) in items.iter().zip(&pairs) {
            html.push_str(&format!(
                // `type="button"`, NOT the implicit `submit` (extensions
                // §13.3). A bare `<button>` is a submit button, so a code group
                // inside a `<form>` submitted the form instead of switching
                // panels - the one interaction this mode exists to provide,
                // traded for the one thing the page never asked for.
                "{inner_pad}<button type=\"button\" role=\"tab\" id=\"{}\" aria-selected=\"{}\" \
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
                 class=\"{}\"{}>",
                ctx.escape_attr(panel_id),
                ctx.escape_attr(tab_id),
                ctx.escape_attr(&self.opts.panel_class),
                if item.selected { "" } else { " hidden" },
            ));
            html.push_str(&self.render_code(item, ctx));
            html.push_str("</div>\n");
        }

        html.push_str(&format!("{pad}</div>"));

        html
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

    // EXACTLY ONE PANEL IS SELECTED (extensions §13.5): the first one the
    // document marks, or the first block where it marks none. The SAME step
    // the Tabs renderer runs, because §13 binds both constructs.
    apply_single_selection(&mut items);

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

    fn aria_html(source: &str) -> String {
        let ext = CodeGroup::with_options(CodeGroupOptions {
            mode: TabsMode::Aria,
            ..CodeGroupOptions::default()
        });
        let opts = Options::new().with_extension(&ext);
        crate::to_html_with_options(source, &opts)
    }

    const TWO_PANELS: &str = "::: code-group\n``` js\nlet a = 1\n```\n\n``` php\n$a = 1;\n```\n:::";

    /// A `css` panel takes its own label (extensions §13.2), keyed on the tab
    /// name where one was written and the LANGUAGE WORD otherwise.
    #[test]
    fn a_css_panel_is_named_after_its_own_label() {
        let out = html(TWO_PANELS);
        assert!(
            out.contains("<div class=\"code-group-panel\" role=\"group\" aria-label=\"js\">"),
            "{out}"
        );
        assert!(
            out.contains("<div class=\"code-group-panel\" role=\"group\" aria-label=\"php\">"),
            "{out}"
        );

        // An authored `[label]` wins over the language word, and the panel
        // follows the tab rather than carrying a name of its own.
        let labelled = html("::: code-group\n``` js [Node]\nlet a = 1\n```\n:::");
        assert!(labelled.contains("aria-label=\"Node\">"), "{labelled}");
        assert!(!labelled.contains("aria-label=\"js\">"), "{labelled}");
    }

    /// The same escape split Tabs has: `&quot;` in the attribute, a bare quote
    /// in the `<label>` element.
    #[test]
    fn a_panel_name_is_escaped_for_an_attribute() {
        let out = html("::: code-group\n``` js [R&D \"core\" <x>]\nlet a = 1\n```\n:::");
        assert!(
            out.contains("aria-label=\"R&amp;D &quot;core&quot; &lt;x&gt;\""),
            "{out}"
        );
    }

    /// `aria` mode mirrors the Tabs one, and its panel takes NEITHER
    /// `role="group"` nor a name (§13.3) - it is bound already.
    #[test]
    fn aria_mode_binds_its_panels_rather_than_naming_them() {
        let out = aria_html(TWO_PANELS);
        assert!(out.contains("role=\"tablist\""), "{out}");
        assert_eq!(out.matches("role=\"tab\"").count(), 2, "{out}");
        assert_eq!(out.matches("role=\"tabpanel\"").count(), 2, "{out}");
        assert!(out.contains("aria-labelledby="), "{out}");
        // Exactly one panel is hidden: the reveal §13.1 says needs a script.
        assert_eq!(out.matches(" hidden>").count(), 1, "{out}");
        assert!(!out.contains("type=\"radio\""), "{out}");
        for panel in out.split("<div role=\"tabpanel\"").skip(1) {
            let opener = panel.split('>').next().unwrap_or_default();
            assert!(!opener.contains("role=\"group\""), "{out}");
            assert!(!opener.contains("aria-label="), "{out}");
        }
    }

    /// A STATIC RENDER TAKES NEITHER MODE (§13.1). `renderStatic` flattens the
    /// set to one `<section>` per panel headed by its `[label]`, so there are no
    /// tabs left to list and `role="tablist"` would describe an interaction the
    /// output does not contain - whatever the host configured.
    #[test]
    fn a_static_render_is_a_group_under_either_mode() {
        let css = static_html(TWO_PANELS);
        assert!(css.contains("role=\"group\""), "{css}");
        assert!(!css.contains("role=\"tablist\""), "{css}");

        let ext = CodeGroup::with_options(CodeGroupOptions {
            mode: TabsMode::Aria,
            ..CodeGroupOptions::default()
        });
        let opts = Options::new().with_extension(&ext).with_mode(Mode::Static);
        let aria = crate::to_html_with_options(TWO_PANELS, &opts);
        assert!(
            !aria.contains("role=\"tablist\""),
            "the interactive mode leaked into a static render: {aria}"
        );
        assert!(aria.contains("role=\"group\""), "{aria}");
        // And the pair: the same option DOES reach an interactive render, or
        // the assertion above would pass on a build where the mode did nothing.
        let interactive = aria_html(TWO_PANELS);
        assert!(interactive.contains("role=\"tablist\""), "{interactive}");
    }

    /// `css` IS THE DEFAULT (§13.1 on top of §2.5): `aria` reveals with
    /// `hidden`, so registering it without a script loses every panel but the
    /// first, while `css` with no stylesheet shows all of them.
    #[test]
    fn css_is_the_default_mode() {
        assert_eq!(CodeGroupOptions::default().mode, TabsMode::Css);
        assert!(
            !html(TWO_PANELS).contains(" hidden"),
            "{}",
            html(TWO_PANELS)
        );
        assert!(
            aria_html(TWO_PANELS).contains("hidden"),
            "{}",
            aria_html(TWO_PANELS)
        );
    }

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
