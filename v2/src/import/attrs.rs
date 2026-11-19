//! The element and attribute tables of `import-html.md` and the attribute
//! mapping of C2-M-052.

use super::dom::{Dom, Id};
use super::{Code, Cx};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Treatment {
    Map,
    Div,
    Span,
    Unwrap,
    Raw,
    Drop,
}

fn table() -> &'static HashMap<String, Treatment> {
    static TABLE: OnceLock<HashMap<String, Treatment>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let data: Value = serde_json::from_str(include_str!("../../spec-data/import-html.json"))
            .unwrap_or_default();
        data.get("elements")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|row| {
                let kind = match row["treatment"].as_str()? {
                    "map" => Treatment::Map,
                    "div" => Treatment::Div,
                    "unwrap" => Treatment::Unwrap,
                    "raw" => Treatment::Raw,
                    "drop" => Treatment::Drop,
                    _ => Treatment::Span,
                };
                Some((row["name"].as_str()?.to_string(), kind))
            })
            .collect()
    })
}

/// C2-M-056: the treatment of an element; one with no row is a span.
pub(crate) fn treatment(dom: &Dom, id: Id) -> Treatment {
    dom.tag(id)
        .and_then(|t| table().get(t).copied())
        .unwrap_or(Treatment::Span)
}

const BLOCK_ELEMENTS: &[&str] = &[
    "p",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "blockquote",
    "section",
    "div",
    "ul",
    "ol",
    "menu",
    "dl",
    "pre",
    "table",
    "figure",
    "aside",
    "li",
    "dt",
    "dd",
    "caption",
    "figcaption",
    "summary",
    "legend",
    "html",
    "body",
    "head",
    "title",
    "meta",
    "link",
    "base",
];

/// C2-M-011, C2-M-012: the elements whose node is a block.
pub(crate) fn is_block_element(dom: &Dom, id: Id) -> bool {
    dom.tag(id).is_some_and(|t| BLOCK_ELEMENTS.contains(&t)) || treatment(dom, id) == Treatment::Div
}

fn needs_parent(tag: &str) -> Option<&'static [&'static str]> {
    Some(match tag {
        "li" => &["ul", "ol", "menu"],
        "dt" | "dd" => &["dl", "div"],
        "caption" => &["table"],
        "figcaption" => &["figure"],
        "summary" => &["details"],
        "legend" => &["fieldset"],
        _ => return None,
    })
}

fn parent_element(dom: &Dom, id: Id) -> Option<Id> {
    dom.parent(id).filter(|&p| dom.is_element(p))
}

/// C2-M-058: an element whose counterpart needs a parent and stands outside it.
pub(crate) fn is_orphan(dom: &Dom, id: Id) -> bool {
    let Some(parents) = dom.tag(id).and_then(needs_parent) else {
        return false;
    };
    let Some(parent) = parent_element(dom, id) else {
        return true;
    };
    if !dom.tag(parent).is_some_and(|t| parents.contains(&t)) {
        return true;
    }
    // C2-M-036: a term or description stands in a `<dl>` or in a `<div>` group of one.
    dom.is(parent, "div") && !parent_element(dom, parent).is_some_and(|g| dom.is(g, "dl"))
}

/// C2-H-115: the reserved names a render with `roundtrip` writes.
pub(crate) const CARRIERS: &[&str] = &[
    "data-marker",
    "data-carve-explicit-id",
    "data-carve-generated-id",
    "data-carve-ref",
    "data-carve-ref-collapsed",
    "data-carve-ref-shortcut",
    "data-carve-autolink",
    "data-carve-footnote-label",
    "data-carve-inline-footnote",
    "data-carve-raw",
    "data-carve-abbreviations",
    "data-carve-label",
    "data-carve-tight",
    "data-carve-xref",
    "data-carve-caption-number",
    "data-carve-div",
    "data-carve-line-block",
    "data-carve-src",
];

pub(crate) fn is_carrier(name: &str) -> bool {
    CARRIERS.contains(&name)
}

/// C2-I-030: an ASCII letter or `_`, then letters, digits, `_` and `-`.
pub(crate) fn is_key(s: &str) -> bool {
    let mut bytes = s.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// C2-M-052 row `dir`: `ltr`, `rtl` or `auto` in any ASCII case.
pub(crate) fn direction_of(value: Option<&str>) -> Option<&'static str> {
    let v = value?.to_ascii_lowercase();
    match v.as_str() {
        "ltr" => Some("ltr"),
        "rtl" => Some("rtl"),
        "auto" => Some("auto"),
        _ => None,
    }
}

/// C2-M-052, C2-M-054: why an attribute is dropped wherever it stands.
pub(crate) fn dropped_attribute(name: &str, value: &str) -> Option<&'static str> {
    if name.starts_with("on") {
        return Some("an event handler");
    }
    if name == "srcdoc" || name == "formaction" {
        return Some("a dropped attribute");
    }
    if name == "dir" {
        return direction_of(Some(value))
            .is_none()
            .then_some("an invalid direction");
    }
    (!is_key(name) && !is_carrier(name)).then_some("a name that is no key")
}

/// C2-M-016: ASCII whitespace without the vertical tab.
pub(crate) fn class_names(value: &str) -> Vec<String> {
    value
        .split([' ', '\t', '\n', '\x0c', '\r'])
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

pub(crate) fn class_list(dom: &Dom, id: Id) -> Vec<String> {
    class_names(dom.attr(id, "class").unwrap_or(""))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Attrs {
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub key_values: Vec<(String, String)>,
}

impl Attrs {
    pub fn is_empty(&self) -> bool {
        self.id.is_none() && self.classes.is_empty() && self.key_values.is_empty()
    }

    pub fn to_value(&self) -> Option<Value> {
        if self.is_empty() {
            return None;
        }
        let mut m = Map::new();
        if let Some(id) = &self.id {
            m.insert("id".into(), id.clone().into());
        }
        if !self.classes.is_empty() {
            m.insert("classes".into(), self.classes.clone().into());
        }
        if !self.key_values.is_empty() {
            m.insert(
                "keyValues".into(),
                Value::Array(
                    self.key_values
                        .iter()
                        .map(|(k, v)| Value::Array(vec![k.clone().into(), v.clone().into()]))
                        .collect(),
                ),
            );
        }
        Some(Value::Object(m))
    }

    pub fn from_value(v: Option<&Value>) -> Self {
        let Some(v) = v else {
            return Self::default();
        };
        Self {
            id: v["id"].as_str().map(str::to_string),
            classes: v["classes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|c| c.as_str().map(str::to_string))
                .collect(),
            key_values: v["keyValues"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| Some((p[0].as_str()?.to_string(), p[1].as_str()?.to_string())))
                .collect(),
        }
    }
}

/// The attributes of a node, with its `lang` and `dir`.
#[derive(Debug, Clone, Default)]
pub(crate) struct Mapped {
    pub attrs: Attrs,
    pub lang: Option<String>,
    pub dir: Option<String>,
}

impl Mapped {
    pub fn apply(&self, m: &mut Map<String, Value>) {
        if let Some(a) = self.attrs.to_value() {
            m.insert("attrs".into(), a);
        }
        if let Some(l) = &self.lang {
            m.insert("lang".into(), l.clone().into());
        }
        if let Some(d) = &self.dir {
            m.insert("dir".into(), d.clone().into());
        }
    }

    /// Removes the classes a render adds to every element of a kind.
    pub fn without_classes(mut self, drop: &[&str]) -> Self {
        self.attrs.classes.retain(|c| !drop.contains(&c.as_str()));
        self
    }

    /// Removes the keys the renderer derives from the node.
    pub fn without_keys(mut self, drop: &[&str]) -> Self {
        self.attrs
            .key_values
            .retain(|(k, _)| !drop.contains(&k.as_str()));
        self
    }
}

pub(crate) fn attrs_of(entries: Vec<(String, String)>) -> Attrs {
    let mut attrs = Attrs::default();
    for (key, value) in entries {
        if key.eq_ignore_ascii_case("id") {
            attrs.id = Some(value);
        } else if key.eq_ignore_ascii_case("class") {
            attrs.classes.extend(class_names(&value));
        } else {
            attrs.key_values.push((key, value));
        }
    }
    attrs
}

const ALIGN_VALUES: &[&str] = &["left", "right", "center"];

/// C2-M-075: the value of `text-align: VALUE;` as C2-H-015 reads it.
fn align_style(style: Option<&str>) -> Option<&'static str> {
    let inner = style?.strip_prefix("text-align: ")?.strip_suffix(';')?;
    let value = inner
        .trim_matches(|c| matches!(c, '\t' | '\n' | '\x0c' | '\r' | ' '))
        .to_ascii_lowercase();
    ALIGN_VALUES.iter().copied().find(|v| *v == value)
}

fn is_authored(name: &str, value: &str, omit: &[&str]) -> bool {
    if omit.contains(&name) || name == "lang" || name == "dir" {
        return false;
    }
    !is_carrier(name) && dropped_attribute(name, value).is_none()
}

impl Cx {
    /// The authored attributes of an element: without `omit`, the reserved
    /// names and the names C2-M-052 drops. With `align_styled`, an exact
    /// alignment declaration is the key `align` (C2-M-075).
    pub(crate) fn attrs(&mut self, id: Id, omit: &[&str], align_styled: bool) -> Mapped {
        let styled = align_styled && align_style(self.dom.attr(id, "style")).is_some();
        let list = self.dom.attrs(id).to_vec();
        let mut entries = Vec::new();
        for (name, value) in list {
            if !is_authored(&name, &value, omit) {
                continue;
            }
            if styled && name == "align" {
                self.report(
                    id,
                    Code::AttributeDropped,
                    "An align attribute beside a text-align declaration dropped",
                );
                continue;
            }
            let align = if name == "style" && align_styled {
                align_style(Some(&value))
            } else {
                None
            };
            match align {
                Some(a) => entries.push(("align".to_string(), a.to_string())),
                None => entries.push((name, value)),
            }
        }
        Mapped {
            attrs: attrs_of(entries),
            lang: self
                .dom
                .attr(id, "lang")
                .filter(|l| !l.is_empty())
                .map(str::to_string),
            dir: direction_of(self.dom.attr(id, "dir")).map(str::to_string),
        }
    }

    pub(crate) fn plain_attrs(&mut self, id: Id) -> Mapped {
        self.attrs(id, &[], false)
    }
}

/// C2-B-225: an ASCII letter, then ASCII letters, digits, `_` and `-`.
pub(crate) fn is_raw_format(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_alphabetic())
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// C2-H-186: a span whose `data-carve-raw` is read.
pub(crate) fn is_raw_span(dom: &Dom, id: Id) -> bool {
    dom.is(id, "span")
        && dom.attr(id, "data-carve-raw").is_some_and(is_raw_format)
        && dom.attrs(id).len() == 1
        && dom.children(id).iter().all(|&c| dom.is_text(c))
}

/// The block kinds the reader counts toward its block nesting cap.
pub(crate) fn is_nesting_block(kind: &str) -> bool {
    matches!(
        kind,
        "block_quote"
            | "definition_description"
            | "definition_list"
            | "div"
            | "footnote"
            | "list"
            | "list_item"
    )
}

/// The inline kinds the reader counts toward its inline nesting cap.
pub(crate) fn is_nesting_inline(kind: &str) -> bool {
    matches!(
        kind,
        "citation_group"
            | "delete"
            | "emphasis"
            | "highlight"
            | "inline_extension"
            | "inline_footnote"
            | "insert"
            | "link"
            | "ruby"
            | "ruby_pair"
            | "small_caps"
            | "span"
            | "strike"
            | "strong"
            | "subscript"
            | "superscript"
            | "underline"
    )
}
