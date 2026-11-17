//! Findings of a parse and a render (`data/lint-rules.json`, channels
//! `parse` and `render-loss`).

use crate::ast::Pos;
use crate::tables::MESSAGES;

/// One finding: its code, the file it stands in (C2-N-063) and its range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub channel: &'static str,
    pub severity: &'static str,
    pub message: String,
    /// The id of the included file the range is counted in; `None` for the top-level document.
    pub file: Option<String>,
    pub pos: Option<Pos>,
    /// JSON pointer to the node or source container that caused the finding.
    pub path: Option<String>,
    /// C2-N-087: include sites from the entry document to this file.
    pub included_by: Vec<IncludeSite>,
}

/// C2-N-087: a source location of a directive that reached an included file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncludeSite {
    pub file: Option<String>,
    pub line: u32,
    pub column: u32,
}

impl Diagnostic {
    /// A finding of `code` with the values its message template names.
    #[must_use]
    pub fn new(
        code: &'static str,
        values: &[(&str, &str)],
        file: Option<&str>,
        pos: Option<Pos>,
    ) -> Self {
        Self::nth(code, 0, values, file, pos)
    }

    /// A finding with the `n`th message of its code.
    #[must_use]
    pub fn nth(
        code: &'static str,
        n: usize,
        values: &[(&str, &str)],
        file: Option<&str>,
        pos: Option<Pos>,
    ) -> Self {
        let (channel, severity, template) =
            MESSAGES
                .iter()
                .find(|m| m.0 == code)
                .map_or(("parse", "warning", code), |m| {
                    (
                        m.1,
                        m.2,
                        m.3.get(n).or(m.3.first()).copied().unwrap_or(code),
                    )
                });
        Self {
            code,
            channel,
            severity,
            message: fill(template, values),
            file: file.map(str::to_string),
            pos,
            path: None,
            included_by: Vec::new(),
        }
    }
}

impl Diagnostic {
    /// A finding of the block pass in the file `file`.
    pub(crate) fn from_finding(
        f: &crate::block::Finding,
        src: &crate::source::Source<'_>,
        file: Option<&str>,
    ) -> Self {
        let values: Vec<(&str, &str)> = f.values.iter().map(|(k, v)| (*k, v.as_str())).collect();
        Self::nth(
            f.code,
            f.message,
            &values,
            file,
            Some(src.pos(f.start, f.end)),
        )
    }
}

/// The message template with each `${name}` replaced by its value.
fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find("${") {
        out.push_str(rest.get(..open).unwrap_or(""));
        let after = rest.get(open + 2..).unwrap_or("");
        let Some(close) = after.find('}') else {
            rest = after;
            break;
        };
        let name = after.get(..close).unwrap_or("");
        out.push_str(values.iter().find(|v| v.0 == name).map_or("", |v| v.1));
        rest = after.get(close + 1..).unwrap_or("");
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn c2_n_068_message_names_the_path_as_written() {
        let d = super::Diagnostic::new("include-not-found", &[("path", "a.crv")], None, None);
        assert_eq!(
            d.message,
            "Include \"a.crv\" was not expanded: the file does not exist."
        );
        assert_eq!(d.channel, "parse");
    }
}
