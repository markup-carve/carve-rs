//! `fencedRender` and its diagram presets (carve-js#5068).

use super::{
    Bundle, Claim, Form, Matcher, authored, bare_fallback, classes, declaration, fence_source,
    key_values, payload_text, with_attrs,
};
use crate::api::{Channel, Error, Extension, RenderContext, Renderer};
use serde_json::{Value, json};
use std::sync::Arc;

/// How a [`fenced_render`] instance places the fence body.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ContentMode {
    /// Escaped text in the element.
    #[default]
    Text,
    /// A `<script type="application/json">` in the element.
    Json,
}

/// The wrapper element of a [`fenced_render`] instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tag {
    Pre,
    Div,
}

impl Tag {
    const fn name(self) -> &'static str {
        match self {
            Self::Pre => "pre",
            Self::Div => "div",
        }
    }
}

/// A host build-time renderer: diagram source in, self-contained HTML out.
pub type DiagramBuilder = Arc<dyn Fn(&str) -> String + Send + Sync>;

/// The options of [`fenced_render`].
#[derive(Clone, Default)]
pub struct FencedRenderOptions {
    /// The fence language words the instance claims.
    pub language: Vec<String>,
    /// The class of the output element; default the first language word.
    pub css_class: Option<String>,
    /// Default `div` in json mode, else `pre`.
    pub tag: Option<Tag>,
    pub content_mode: ContentMode,
    /// Wraps the output in `<figure class="{figure_class}">`.
    pub wrap_in_figure: bool,
    /// Default `{css_class}-figure`.
    pub figure_class: Option<String>,
    /// The accessible name written with `role="img"`; default the css class,
    /// `""` writes neither.
    pub label: Option<String>,
    /// C2-X-052: registers the build-time channel; without it static mode
    /// writes the fallback code block.
    pub build: Option<DiagramBuilder>,
}

/// The options a preset passes through.
#[derive(Clone, Default)]
pub struct PresetOptions {
    pub css_class: Option<String>,
    pub tag: Option<Tag>,
    pub wrap_in_figure: bool,
    pub figure_class: Option<String>,
    pub label: Option<String>,
    pub build: Option<DiagramBuilder>,
}

impl PresetOptions {
    fn with(self, language: &[&str], content_mode: ContentMode) -> FencedRenderOptions {
        FencedRenderOptions {
            language: language.iter().map(|w| (*w).to_string()).collect(),
            css_class: self.css_class,
            tag: self.tag,
            content_mode,
            wrap_in_figure: self.wrap_in_figure,
            figure_class: self.figure_class,
            label: self.label,
            build: self.build,
        }
    }
}

/// The qualified name of the fence `word` of a [`fenced_render`] instance.
#[must_use]
pub fn fenced_render_name(word: &str) -> String {
    format!("carve.fencedRender.{word}")
}

struct Look {
    mode: ContentMode,
    css_class: String,
    tag: Tag,
    figure: Option<String>,
    label: String,
}

impl Look {
    /// The css class ahead of the author's classes; `role="img"` never
    /// without a name, the author's own winning.
    fn attributes(&self, node: &Value, cx: &RenderContext<'_>) -> String {
        let attrs = node.get("attrs");
        let authored_name = authored(attrs, "aria-label") || authored(attrs, "aria-labelledby");
        let write_name = !self.label.is_empty() && !authored_name;
        let write_role = !authored(attrs, "role") && (authored_name || !self.label.is_empty());
        let mut kv = key_values(attrs);
        if write_role {
            kv.push(json!(["role", "img"]));
        }
        if write_name {
            kv.push(json!(["aria-label", self.label]));
        }
        let mut cls = vec![Value::from(self.css_class.as_str())];
        cls.extend(classes(attrs));
        cx.attributes(&with_attrs(node, cls, kv))
    }

    fn figure(&self, html: String, cx: &RenderContext<'_>) -> String {
        match &self.figure {
            Some(class) => format!(
                r#"<figure class="{}">{html}</figure>"#,
                cx.escape_attribute(class)
            ),
            None => html,
        }
    }
}

struct Interactive(Arc<Look>);

impl Renderer for Interactive {
    fn render(&self, node: &Value, cx: &RenderContext<'_>) -> Result<String, Error> {
        let look = &self.0;
        let source = fence_source(payload_text(node));
        let body = match look.mode {
            ContentMode::Json => format!(
                r#"<script type="application/json">{}</script>"#,
                source.replace("</", "<\\/")
            ),
            // `>` stays so arrow syntax reaches the client library as written.
            ContentMode::Text => source.replace('&', "&amp;").replace('<', "&lt;"),
        };
        let tag = look.tag.name();
        Ok(look.figure(
            format!("<{tag}{}>{body}</{tag}>", look.attributes(node, cx)),
            cx,
        ))
    }
}

struct BuildTime(Arc<Look>, DiagramBuilder);

impl Renderer for BuildTime {
    fn render(&self, node: &Value, cx: &RenderContext<'_>) -> Result<String, Error> {
        let look = &self.0;
        let built = (self.1)(fence_source(payload_text(node)));
        Ok(look.figure(
            format!("<div{}>{built}</div>", look.attributes(node, cx)),
            cx,
        ))
    }
}

fn claim_fence(claim: &Claim<'_>) -> Option<Value> {
    if claim.form != Form::Fence {
        return None;
    }
    let value = claim
        .node
        .get("value")
        .cloned()
        .unwrap_or_else(|| "".into());
    Some(json!({
        "type": "block_extension",
        "name": fenced_render_name(claim.name),
        "version": "1",
        "children": [],
        "props": {"language": claim.name},
        "payload": [{"format": "text/plain", "value": value}],
        "fallback": [bare_fallback(claim.node)],
    }))
}

/// A bundle that claims the fences of `options.language` and renders each
/// as a diagram element for a client library, or through `options.build`
/// in static mode.
///
/// # Errors
///
/// [`Error::Option`] when no language word is given.
pub fn fenced_render(options: FencedRenderOptions) -> Result<Bundle, Error> {
    let words: Vec<String> = options
        .language
        .into_iter()
        .filter(|w| !w.is_empty())
        .collect();
    let Some(first) = words.first() else {
        return Err(Error::option(
            "language",
            "fencedRender requires at least one non-empty language word",
        ));
    };
    let css_class = options.css_class.unwrap_or_else(|| first.clone());
    let look = Arc::new(Look {
        mode: options.content_mode,
        tag: options.tag.unwrap_or(match options.content_mode {
            ContentMode::Json => Tag::Div,
            ContentMode::Text => Tag::Pre,
        }),
        figure: options.wrap_in_figure.then(|| {
            options
                .figure_class
                .clone()
                .unwrap_or_else(|| format!("{css_class}-figure"))
        }),
        label: options.label.unwrap_or_else(|| css_class.clone()),
        css_class,
    });
    let mut bundle = Bundle::default();
    for word in &words {
        let name = fenced_render_name(word);
        let mut renderers: Vec<(Channel, Box<dyn Renderer>)> =
            vec![(Channel::Interactive, Box::new(Interactive(look.clone())))];
        if let Some(build) = &options.build {
            renderers.push((
                Channel::BuildTime,
                Box::new(BuildTime(look.clone(), build.clone())),
            ));
        }
        bundle.extensions.push(Extension {
            declaration: declaration(&name, &[]),
            renderers,
            transform: None,
        });
        bundle.matchers.push(Matcher {
            extension: name,
            form: Form::Fence,
            name: word.clone(),
            claimer: Box::new(claim_fence),
        });
    }
    Ok(bundle)
}

fn preset(options: PresetOptions, words: &[&str], mode: ContentMode) -> Bundle {
    fenced_render(options.with(words, mode)).unwrap_or_default()
}

fn class_default(mut options: PresetOptions, class: &str) -> PresetOptions {
    options.css_class.get_or_insert_with(|| class.to_string());
    options
}

#[must_use]
pub fn mermaid(options: PresetOptions) -> Bundle {
    preset(options, &["mermaid"], ContentMode::Text)
}

#[must_use]
pub fn d2(options: PresetOptions) -> Bundle {
    preset(options, &["d2"], ContentMode::Text)
}

/// Claims `dot` and `graphviz`, class `graphviz`.
#[must_use]
pub fn graphviz(options: PresetOptions) -> Bundle {
    preset(
        class_default(options, "graphviz"),
        &["dot", "graphviz"],
        ContentMode::Text,
    )
}

#[must_use]
pub fn wavedrom(options: PresetOptions) -> Bundle {
    preset(options, &["wavedrom"], ContentMode::Text)
}

#[must_use]
pub fn abc(options: PresetOptions) -> Bundle {
    preset(options, &["abc"], ContentMode::Text)
}

/// Claims `plantuml` and `puml`, class `plantuml`.
#[must_use]
pub fn plantuml(options: PresetOptions) -> Bundle {
    preset(
        class_default(options, "plantuml"),
        &["plantuml", "puml"],
        ContentMode::Text,
    )
}

/// Claims `vega-lite` in json mode.
#[must_use]
pub fn vega_lite(options: PresetOptions) -> Bundle {
    preset(options, &["vega-lite"], ContentMode::Json)
}

/// Claims `chart` in json mode.
#[must_use]
pub fn chart(options: PresetOptions) -> Bundle {
    preset(options, &["chart"], ContentMode::Json)
}

/// Build-time renderers for [`presets`], keyed by the preset's css class.
#[derive(Clone, Default)]
pub struct PresetBuilders {
    pub mermaid: Option<DiagramBuilder>,
    pub d2: Option<DiagramBuilder>,
    pub graphviz: Option<DiagramBuilder>,
    pub wavedrom: Option<DiagramBuilder>,
    pub abc: Option<DiagramBuilder>,
    pub plantuml: Option<DiagramBuilder>,
    pub vega_lite: Option<DiagramBuilder>,
    pub chart: Option<DiagramBuilder>,
}

/// Every diagram preset; claims `mermaid`, `d2`, `dot`, `graphviz`,
/// `wavedrom`, `abc`, `plantuml`, `puml`, `vega-lite` and `chart`.
#[must_use]
pub fn presets(builders: PresetBuilders) -> Bundle {
    let with = |build: Option<DiagramBuilder>| PresetOptions {
        build,
        ..PresetOptions::default()
    };
    Bundle::merge([
        mermaid(with(builders.mermaid)),
        d2(with(builders.d2)),
        graphviz(with(builders.graphviz)),
        wavedrom(with(builders.wavedrom)),
        abc(with(builders.abc)),
        plantuml(with(builders.plantuml)),
        vega_lite(with(builders.vega_lite)),
        chart(with(builders.chart)),
    ])
}

/// The preset that claims the fence `word`, restricted to that word, with
/// default options; the form the JSON boundary selects.
pub(super) fn preset_word(word: &str) -> Option<Bundle> {
    let mut bundle = match word {
        "mermaid" => mermaid(PresetOptions::default()),
        "d2" => d2(PresetOptions::default()),
        "dot" | "graphviz" => graphviz(PresetOptions::default()),
        "wavedrom" => wavedrom(PresetOptions::default()),
        "abc" => abc(PresetOptions::default()),
        "plantuml" | "puml" => plantuml(PresetOptions::default()),
        "vega-lite" => vega_lite(PresetOptions::default()),
        "chart" => chart(PresetOptions::default()),
        _ => return None,
    };
    bundle.matchers.retain(|m| m.name == word);
    let name = fenced_render_name(word);
    bundle.extensions.retain(|e| e.declaration.name == name);
    Some(bundle)
}
