//! The attributes of one element (C2-H-009 to C2-H-017): order, merging of
//! classes and styles, collisions, carriers and the authored value filter.

use super::{esc, url};
use crate::ast::Attrs;

/// The 18 names a roundtrip render reserves (C2-H-115, gap HTML-001).
pub const RESERVED: [&str; 18] = [
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

/// An attribute value; `None` is a bare name (C2-H-117).
pub type Val = Option<String>;

/// The parts of an element's attribute list, written in the order of
/// C2-H-009.
#[derive(Default)]
pub struct El<'a> {
    pub structural: Vec<(&'static str, Val)>,
    pub carriers: Vec<(&'static str, Val)>,
    /// The id the element carries: authored, or the one it owns (a section,
    /// a note mark).
    pub id: Option<String>,
    /// C2-H-011: the id stands before the structural attributes.
    pub id_first: bool,
    /// C2-H-011: the class stands first.
    pub class_first: bool,
    pub base: Vec<String>,
    pub trailing: Vec<String>,
    pub attrs: Option<&'a Attrs>,
    pub lang: Option<&'a str>,
    /// Generated attributes after the keys; an authored key replaces one.
    pub generated: Vec<(&'static str, Val)>,
    /// A generated style declaration before an authored style (C2-H-032).
    pub style_before: Option<String>,
    /// Generated declarations after an authored style (C2-H-015, C2-H-057).
    pub style_after: Vec<String>,
    /// Authored keys that win over a structural attribute (C2-H-048, C2-H-075).
    pub authored_wins: &'static [&'static str],
    /// Turns an authored `align` into a style declaration (C2-H-015).
    pub align_style: bool,
    pub source_line: Option<usize>,
    /// Keys left out, beside the reserved ones.
    pub skip_keys: &'static [&'static str],
}

/// The render switches the filter reads.
#[derive(Clone, Copy)]
pub struct Filter {
    pub roundtrip: bool,
}

impl<'a> El<'a> {
    /// An element with the node's authored attributes and language.
    #[must_use]
    pub fn new(attrs: Option<&'a Attrs>, lang: Option<&'a str>) -> Self {
        El {
            attrs,
            lang,
            id: attrs.and_then(|a| a.id.clone()),
            ..El::default()
        }
    }

    /// Writes ` name="value"` for each attribute.
    #[must_use]
    pub fn write(mut self, f: Filter) -> String {
        let keys = self.keys(f);
        let mut list: Vec<(String, Val)> = Vec::new();
        let classes = self.classes();
        if self.class_first {
            push_class(&mut list, &classes);
        }
        if self.id_first {
            push_id(&mut list, self.id.take());
        }
        list.extend(self.structural.drain(..).map(|(k, v)| (k.to_string(), v)));
        list.extend(self.carriers.drain(..).map(|(k, v)| (k.to_string(), v)));
        push_id(&mut list, self.id.take());
        if !self.class_first {
            push_class(&mut list, &classes);
        }
        if let Some(lang) = self.lang {
            list.push(("lang".to_string(), Some(url::filter_value("lang", lang))));
        }
        let has_style = keys.iter().any(|(k, _)| k.eq_ignore_ascii_case("style"));
        for (k, v) in keys {
            if k.eq_ignore_ascii_case("style") {
                list.push((k, Some(self.merge_style(v.as_deref()))));
            } else {
                list.push((k, v));
            }
        }
        if !has_style && (self.style_before.is_some() || !self.style_after.is_empty()) {
            list.push(("style".to_string(), Some(self.merge_style(None))));
        }
        list.extend(self.generated.drain(..).map(|(k, v)| (k.to_string(), v)));
        if let Some(n) = self.source_line {
            list.push(("data-source-line".to_string(), Some(n.to_string())));
        }
        let mut out = String::new();
        for (k, v) in list {
            out.push(' ');
            out.push_str(&k);
            if let Some(v) = v {
                out.push_str("=\"");
                esc::attr(&v, &mut out);
                out.push('"');
            }
        }
        out
    }

    fn merge_style(&self, authored: Option<&str>) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(b) = &self.style_before {
            parts.push(b.clone());
        }
        if let Some(a) = authored.filter(|a| !a.is_empty()) {
            parts.push(
                a.trim_end_matches(|c: char| c == ';' || c.is_whitespace())
                    .to_string(),
            );
        }
        let mut s = parts.join("; ");
        for d in &self.style_after {
            if !s.is_empty() {
                s.push_str(if s.ends_with(';') { " " } else { "; " });
            }
            s.push_str(d);
        }
        s
    }

    /// The authored keys that are written, filtered and with collisions
    /// settled (C2-H-013, C2-H-116, C2-H-161).
    fn keys(&mut self, f: Filter) -> Vec<(String, Val)> {
        let Some(attrs) = self.attrs else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (k, v) in &attrs.key_values {
            if self.lang.is_some() && k.eq_ignore_ascii_case("lang") {
                continue;
            }
            if url::dropped_name(k) || self.skip_keys.iter().any(|s| s.eq_ignore_ascii_case(k)) {
                continue;
            }
            if f.roundtrip && RESERVED.iter().any(|r| r.eq_ignore_ascii_case(k)) {
                continue;
            }
            if self.align_style
                && k.eq_ignore_ascii_case("align")
                && matches!(v.as_str(), "left" | "right" | "center")
            {
                self.style_after.push(format!("text-align: {v};"));
                continue;
            }
            if self
                .structural
                .iter()
                .any(|(s, _)| s.eq_ignore_ascii_case(k))
            {
                if !self.authored_wins.iter().any(|w| w.eq_ignore_ascii_case(k)) {
                    continue;
                }
                self.structural.retain(|(s, _)| !s.eq_ignore_ascii_case(k));
            }
            self.generated.retain(|(s, _)| !s.eq_ignore_ascii_case(k));
            out.push((k.clone(), Some(url::filter_value(k, v))));
        }
        out
    }

    fn classes(&self) -> Vec<String> {
        let authored = self
            .attrs
            .map_or(&[] as &[String], |a| a.classes.as_slice());
        let mut out: Vec<String> = Vec::new();
        let all = self
            .base
            .iter()
            .chain(authored.iter().filter(|c| !url::value_fails(c)))
            .chain(&self.trailing);
        for c in all {
            if !out.contains(c) {
                out.push(c.clone());
            }
        }
        out
    }
}

fn push_id(list: &mut Vec<(String, Val)>, id: Option<String>) {
    if let Some(id) = id {
        list.push(("id".to_string(), Some(id)));
    }
}

fn push_class(list: &mut Vec<(String, Val)>, classes: &[String]) {
    if !classes.is_empty() {
        list.push(("class".to_string(), Some(classes.join(" "))));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attrs() -> Attrs {
        Attrs {
            id: Some("i".into()),
            classes: vec!["c".into()],
            key_values: vec![("k".into(), "v".into())],
        }
    }

    #[test]
    fn c2_h_009_fixed_order() {
        let a = attrs();
        let el = El::new(Some(&a), Some("en"));
        assert_eq!(
            el.write(Filter { roundtrip: false }),
            r#" id="i" class="c" lang="en" k="v""#
        );
    }

    #[test]
    fn c2_h_015_align_follows_authored_style() {
        let a = Attrs {
            key_values: vec![
                ("align".into(), "left".into()),
                ("style".into(), "color: red".into()),
            ],
            ..Attrs::default()
        };
        let el = El {
            align_style: true,
            ..El::new(Some(&a), None)
        };
        assert_eq!(
            el.write(Filter { roundtrip: false }),
            r#" style="color: red; text-align: left;""#
        );
    }

    #[test]
    fn c2_h_116_reserved_names_drop_under_roundtrip() {
        let a = Attrs {
            key_values: vec![("data-carve-div".into(), String::new())],
            ..Attrs::default()
        };
        assert_eq!(
            El::new(Some(&a), None).write(Filter { roundtrip: true }),
            ""
        );
        assert_eq!(
            El::new(Some(&a), None).write(Filter { roundtrip: false }),
            r#" data-carve-div="""#
        );
    }
}
