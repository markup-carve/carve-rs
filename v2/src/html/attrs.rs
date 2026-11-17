//! The attributes of one element (C2-H-009 to C2-H-017): order, merging of
//! classes and styles, collisions, carriers and the authored value filter.

use super::{esc, url};
use crate::ast::Attrs;
use std::borrow::Cow;
use std::fmt::Write as _;

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
pub type Val<'a> = Option<Cow<'a, str>>;

/// The parts of an element's attribute list, written in the order of
/// C2-H-009.
#[derive(Default)]
pub struct El<'a> {
    pub structural: Vec<(&'static str, Val<'a>)>,
    pub carriers: Vec<(&'static str, Val<'a>)>,
    /// The id the element carries: authored, or the one it owns (a section,
    /// a note mark).
    pub id: Option<Cow<'a, str>>,
    /// C2-H-011: the id stands before the structural attributes.
    pub id_first: bool,
    /// C2-H-011: the class stands first.
    pub class_first: bool,
    pub base: Vec<String>,
    pub trailing: Vec<String>,
    pub attrs: Option<&'a Attrs<'a>>,
    pub lang: Option<&'a str>,
    /// Generated attributes after the keys; an authored key replaces one.
    pub generated: Vec<(&'static str, Val<'a>)>,
    /// A generated style declaration before an authored style (C2-H-032).
    pub style_before: Option<String>,
    /// Generated declarations after an authored style (C2-H-015, C2-H-057).
    pub style_after: Vec<String>,
    /// Authored keys that win over a structural attribute (C2-H-048, C2-H-075).
    pub authored_wins: &'static [&'static str],
    /// Turns an authored `align` into a style declaration (C2-H-015).
    pub align_style: bool,
    pub source_line: Option<u32>,
    /// Keys left out, beside the reserved ones.
    pub skip_keys: &'static [&'static str],
}

/// The render switches the filter reads.
#[derive(Clone, Copy)]
pub struct Filter<'p> {
    pub roundtrip: bool,
    /// C2-H-161: the tokens of a URL list attribute meet the policy too.
    pub policy: Option<&'p crate::options::UrlPolicy>,
}

impl<'a> El<'a> {
    /// An element with the node's authored attributes and language.
    #[must_use]
    pub fn new(attrs: Option<&'a Attrs<'a>>, lang: Option<&'a str>) -> Self {
        El {
            attrs,
            lang,
            id: attrs.and_then(|a| a.id).map(Cow::Borrowed),
            ..El::default()
        }
    }

    /// Writes ` name="value"` for each attribute.
    #[must_use]
    pub fn write(self, f: Filter<'_>) -> String {
        let mut out = String::new();
        self.write_into(f, &mut out);
        out
    }

    /// `write`, appended to `out`.
    pub fn write_into(mut self, f: Filter<'_>, out: &mut String) {
        if self.is_bare() {
            return;
        }
        let keys = self.keys(f);
        if self.class_first {
            self.write_class(out);
        }
        if self.id_first {
            attr(out, "id", self.id.take().as_deref());
        }
        for (k, v) in self.structural.iter().chain(&self.carriers) {
            attr(out, k, v.as_deref());
        }
        if let Some(id) = self.id.take() {
            attr(out, "id", Some(&id));
        }
        if !self.class_first {
            self.write_class(out);
        }
        if let Some(lang) = self.lang {
            attr(out, "lang", Some(url::filter_value("lang", lang)));
        }
        let has_style = keys.iter().any(|(k, _)| k.eq_ignore_ascii_case("style"));
        for (k, v) in keys {
            if k.eq_ignore_ascii_case("style") {
                attr(out, k, Some(&self.merge_style(Some(v))));
            } else {
                attr(out, k, Some(v));
            }
        }
        if !has_style && (self.style_before.is_some() || !self.style_after.is_empty()) {
            attr(out, "style", Some(&self.merge_style(None)));
        }
        for (k, v) in &self.generated {
            attr(out, k, v.as_deref());
        }
        if let Some(n) = self.source_line {
            out.push_str(" data-source-line=\"");
            let _ = write!(out, "{n}");
            out.push('"');
        }
    }

    fn write_class(&self, out: &mut String) {
        let classes = self.classes();
        if let Some((first, rest)) = classes.split_first() {
            out.push_str(" class=\"");
            esc::attr(first, out);
            for c in rest {
                out.push(' ');
                esc::attr(c, out);
            }
            out.push('"');
        }
    }

    /// No part that writes an attribute: the list is empty.
    fn is_bare(&self) -> bool {
        self.attrs.is_none()
            && self.lang.is_none()
            && self.id.is_none()
            && self.structural.is_empty()
            && self.carriers.is_empty()
            && self.base.is_empty()
            && self.trailing.is_empty()
            && self.generated.is_empty()
            && self.style_before.is_none()
            && self.style_after.is_empty()
            && self.source_line.is_none()
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
    fn keys(&mut self, f: Filter<'_>) -> Vec<(&'a str, &'a str)> {
        let Some(attrs) = self.attrs else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (k, v) in attrs.key_values {
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
                && matches!(*v, "left" | "right" | "center")
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
            out.push((*k, url::filter_value_in(k, v, f.policy)));
        }
        out
    }

    fn classes(&self) -> Vec<&str> {
        let authored = self.attrs.map_or(&[] as &[&str], |a| a.classes);
        let mut keep: Vec<&str> = Vec::new();
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let all = self
            .base
            .iter()
            .map(String::as_str)
            .chain(authored.iter().copied().filter(|c| !url::value_fails(c)))
            .chain(self.trailing.iter().map(String::as_str));
        for c in all {
            // A short list is searched; a long one goes through the set.
            if keep.len() == 16 && seen.is_empty() {
                seen.extend(keep.iter().copied());
            }
            let new = if seen.is_empty() {
                !keep.contains(&c)
            } else {
                seen.insert(c)
            };
            if new {
                keep.push(c);
            }
        }
        keep
    }
}

fn attr(out: &mut String, k: &str, v: Option<&str>) {
    out.push(' ');
    out.push_str(k);
    if let Some(v) = v {
        out.push_str("=\"");
        esc::attr(v, out);
        out.push('"');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attrs() -> Attrs<'static> {
        Attrs {
            id: Some("i"),
            classes: &["c"],
            key_values: &[("k", "v")],
        }
    }

    #[test]
    fn c2_h_009_fixed_order() {
        let a = attrs();
        let el = El::new(Some(&a), Some("en"));
        assert_eq!(
            el.write(Filter {
                roundtrip: false,
                policy: None
            }),
            r#" id="i" class="c" lang="en" k="v""#
        );
    }

    #[test]
    fn c2_h_015_align_follows_authored_style() {
        let a = Attrs {
            key_values: &[("align", "left"), ("style", "color: red")],
            ..Attrs::default()
        };
        let el = El {
            align_style: true,
            ..El::new(Some(&a), None)
        };
        assert_eq!(
            el.write(Filter {
                roundtrip: false,
                policy: None
            }),
            r#" style="color: red; text-align: left;""#
        );
    }

    #[test]
    fn c2_h_116_reserved_names_drop_under_roundtrip() {
        let a = Attrs {
            key_values: &[("data-carve-div", "")],
            ..Attrs::default()
        };
        assert_eq!(
            El::new(Some(&a), None).write(Filter {
                roundtrip: true,
                policy: None
            }),
            ""
        );
        assert_eq!(
            El::new(Some(&a), None).write(Filter {
                roundtrip: false,
                policy: None
            }),
            r#" data-carve-div="""#
        );
    }
}
