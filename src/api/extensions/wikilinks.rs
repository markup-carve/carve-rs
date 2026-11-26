//! `wikilinks` (carve-js#5070): the standard `:wiki` matcher (C2-X-057)
//! with a link renderer.

use super::{Bundle, Claim, Engine, Form, Matcher, classes, declaration, key_values, with_attrs};
use crate::api::{Channel, Error, Extension, RenderContext, Renderer};
use serde_json::{Value, json};
use std::sync::Arc;

/// Maps a page name to a URL; the anchor is appended afterwards.
pub type UrlGenerator = Arc<dyn Fn(&str) -> String + Send + Sync>;

/// The options of [`wikilinks`].
#[derive(Clone, Default)]
pub struct WikilinksOptions {
    /// Default: a lowercase hyphen slug.
    pub url_generator: Option<UrlGenerator>,
    /// The classes of the link, space separated; default `wikilink`.
    pub css_class: Option<String>,
    /// Opens links in a new tab (`target="_blank" rel="noopener"`).
    pub new_window: bool,
}

fn slug(page: &str) -> String {
    let lower = page.trim().to_lowercase();
    let mut out = String::new();
    let mut space = false;
    for c in lower.chars() {
        if c.is_whitespace() {
            space = true;
            continue;
        }
        if space {
            out.push('-');
            space = false;
        }
        if c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '/') {
            out.push(c);
        }
    }
    let mut collapsed = String::new();
    for c in out.chars() {
        if !(c == '-' && collapsed.ends_with('-')) {
            collapsed.push(c);
        }
    }
    collapsed
}

/// `Page#anchor`: the page and the anchor with its `#`, each possibly empty.
fn target(text: &str) -> (String, String) {
    let path = text.split_once('|').map_or(text, |(p, _)| p).trim();
    match path.split_once('#') {
        Some((page, anchor)) => (page.trim().to_string(), format!("#{anchor}")),
        None => (path.to_string(), String::new()),
    }
}

/// C2-X-057: the target before the first `|`, the display text (the
/// fallback) after it; without a `|` both are the whole body.
fn claim(claim: &Claim<'_>) -> Option<Value> {
    if claim.form != Form::Inline {
        return None;
    }
    let children: Vec<Value> = claim
        .node
        .get("children")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let text = super::super::standard::plain_text(&children);
    let (target, display) = text.split_once('|').unwrap_or((&text, &text));
    Some(json!({
        "type": "inline_extension",
        "name": "carve.wiki",
        "version": "1",
        "children": [],
        "props": {"target": target},
        "fallback": [{"type": "text", "value": display}],
    }))
}

/// The end of the leading ` id="..."` and ` class="..."` of a written list.
fn after_id_and_class(written: &str) -> usize {
    let mut at = 0;
    for name in [" id=\"", " class=\""] {
        if let Some(rest) = written[at..].strip_prefix(name)
            && let Some(close) = rest.find('"')
        {
            at += name.len() + close + 1;
        }
    }
    at
}

struct Link {
    generate: Option<UrlGenerator>,
    classes: Vec<String>,
    new_window: bool,
}

impl Renderer for Link {
    fn render(&self, node: &Value, cx: &RenderContext<'_>) -> Result<String, Error> {
        let fallback = node.get("fallback").cloned().unwrap_or_else(|| json!([]));
        let display = cx.plain_text(fallback.as_array().map_or(&[][..], Vec::as_slice));
        let source = node
            .pointer("/props/target")
            .and_then(Value::as_str)
            .unwrap_or(&display);
        let (page, anchor) = target(source);
        let href = match &self.generate {
            Some(g) => g(&page),
            None => slug(&page),
        } + &anchor;
        // C2-X-057: the target becomes a link only through the URL policy (C2-H-159).
        if (page.is_empty() && anchor.is_empty())
            || !crate::html::url::allowed(&href, &cx.options().url_policy)
        {
            return cx.render_inlines(&fallback);
        }
        // The generated pairs replace authored ones, so `{rel=opener}` cannot undo `noopener`.
        let mut generated = vec![("data-wikilink", page.clone())];
        if self.new_window {
            generated.push(("target", "_blank".into()));
            generated.push(("rel", "noopener".into()));
        }
        let attrs = node.get("attrs");
        let authored: Vec<Value> = key_values(attrs)
            .into_iter()
            .filter(|kv| {
                let k = kv
                    .get(0)
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_lowercase();
                k != "href" && !generated.iter().any(|(n, _)| *n == k)
            })
            .collect();
        let mut kv = authored;
        kv.extend(generated.iter().skip(1).map(|(n, v)| json!([n, v])));
        let mut cls: Vec<Value> = self
            .classes
            .iter()
            .map(|c| Value::from(c.as_str()))
            .collect();
        cls.extend(classes(attrs));
        let barless = node.pointer("/source/form").and_then(Value::as_str) == Some("inline")
            && node
                .pointer("/source/body")
                .and_then(Value::as_str)
                .is_some_and(|body| !body.contains('|'));
        let shown = if !barless {
            display
        } else if page.is_empty() {
            anchor
        } else {
            page.clone()
        };
        // C2-H-161: the page is taken from text, so it is no authored value.
        let mut written = cx.attributes(&with_attrs(node, cls, kv));
        let at = after_id_and_class(&written);
        written.insert_str(
            at,
            &format!(r#" data-wikilink="{}""#, cx.escape_attribute(&page)),
        );
        Ok(format!(
            r#"<a href="{}"{}>{}</a>"#,
            cx.escape_attribute(&href),
            written,
            cx.escape(&shown)
        ))
    }
}

/// The standard `:wiki[Page#anchor|Display]` matcher (C2-X-057) with a link
/// renderer.
#[must_use]
pub fn wikilinks(options: WikilinksOptions) -> Bundle {
    let classes = options
        .css_class
        .as_deref()
        .unwrap_or("wikilink")
        .split(' ')
        .filter(|c| !c.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let generate = options.url_generator;
    let link = || {
        Box::new(Link {
            generate: generate.clone(),
            classes: classes.clone(),
            new_window: options.new_window,
        }) as Box<dyn Renderer>
    };
    Bundle {
        extensions: vec![Extension {
            declaration: declaration("carve.wiki", &[]),
            renderers: vec![(Channel::Interactive, link()), (Channel::BuildTime, link())],
            transform: None,
        }],
        host: super::HostFields::default(),
        matchers: vec![Matcher {
            extension: "carve.wiki".into(),
            form: Form::Inline,
            name: "wiki".into(),
            claimer: Box::new(Engine(claim)),
        }],
    }
}
