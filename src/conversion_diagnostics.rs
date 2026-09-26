//! Source conversion losses, separate from losses of a selected renderer.

use serde::Serialize;
use serde_json::Value;

use crate::ast::Document;
use crate::ast_json::{parse_value, try_to_json, AstJsonError};

pub const DEFAULT_MAX_CONVERSION_DIAGNOSTICS: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConversionDiagnosticCode {
    StructureUnspellable,
    FieldUnspellable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionPos {
    pub start_line: usize,
    pub end_line: usize,
    pub start_column: usize,
    pub end_column: usize,
    pub start_offset: usize,
    pub end_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConversionDiagnostic {
    pub code: ConversionDiagnosticCode,
    pub node: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos: Option<ConversionPos>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionDiagnostics {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    pub diagnostics: Vec<ConversionDiagnostic>,
    pub total_diagnostics: usize,
    pub truncated: bool,
}

fn position(value: &Value) -> Option<ConversionPos> {
    let pos = value.get("pos")?;
    let number = |name: &str| {
        pos.get(name)?
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
    };
    let pos = ConversionPos {
        start_line: number("startLine")?,
        end_line: number("endLine")?,
        start_column: number("startColumn")?,
        end_column: number("endColumn")?,
        start_offset: number("startOffset")?,
        end_offset: number("endOffset")?,
    };
    if [
        pos.start_line,
        pos.end_line,
        pos.start_column,
        pos.end_column,
    ]
    .contains(&0)
    {
        None
    } else {
        Some(pos)
    }
}

/// Report AST structure and fields that a canonical Carve write discards.
///
/// The report can be requested before `render_carve`; it does not change the
/// writer's existing error behavior for other unspellable shapes.
pub fn conversion_diagnostics(
    doc: &Document,
    max_diagnostics: usize,
) -> Result<ConversionDiagnostics, AstJsonError> {
    let ast = parse_value(&try_to_json(doc)?)?;
    let mut pending = vec![&ast];
    let mut report = ConversionDiagnostics {
        file: None,
        diagnostics: Vec::new(),
        total_diagnostics: 0,
        truncated: false,
    };
    while let Some(value) = pending.pop() {
        match value {
            Value::Object(object) => {
                let ty = object.get("type").and_then(Value::as_str);
                let mut losses = Vec::new();
                match ty {
                    Some("small_caps") => losses.push((
                        ConversionDiagnosticCode::StructureUnspellable,
                        None,
                        "Carve source has no small-caps wrapper",
                    )),
                    Some("section") => losses.push((
                        ConversionDiagnosticCode::StructureUnspellable,
                        None,
                        "Carve source has no explicit section wrapper",
                    )),
                    Some("math") => {
                        if object.contains_key("label") {
                            losses.push((
                                ConversionDiagnosticCode::FieldUnspellable,
                                Some("label"),
                                "Carve source has no equation label",
                            ));
                        }
                        if object.contains_key("number") {
                            losses.push((
                                ConversionDiagnosticCode::FieldUnspellable,
                                Some("number"),
                                "Carve source has no equation number",
                            ));
                        }
                    }
                    Some("table_cell") if object.contains_key("blocks") => losses.push((
                        ConversionDiagnosticCode::FieldUnspellable,
                        Some("blocks"),
                        "Carve source has no block content in a table cell",
                    )),
                    _ => {}
                }
                if ty == Some("table") {
                    if let Some(Value::Object(groups)) = object.get("rowGroups") {
                        let mut fields = vec![
                            ("rowGroups.headAttrs".to_owned(), groups.get("headAttrs")),
                            ("rowGroups.footAttrs".to_owned(), groups.get("footAttrs")),
                        ];
                        if let Some(Value::Array(bodies)) = groups.get("bodies") {
                            for (index, body) in bodies.iter().enumerate() {
                                fields.push((
                                    format!("rowGroups.bodies[{index}].attrs"),
                                    body.as_object().and_then(|b| b.get("attrs")),
                                ));
                            }
                        }
                        for (field, attrs) in fields {
                            if attrs
                                .and_then(Value::as_object)
                                .is_some_and(|a| !a.is_empty())
                            {
                                report.total_diagnostics += 1;
                                if report.diagnostics.len() < max_diagnostics {
                                    report.diagnostics.push(ConversionDiagnostic {
                                        code: ConversionDiagnosticCode::FieldUnspellable,
                                        node: "table".to_owned(),
                                        field: Some(field),
                                        message:
                                            "Carve source cannot spell table section attributes"
                                                .to_owned(),
                                        pos: position(value),
                                    });
                                }
                            }
                        }
                    }
                }
                for (code, field, message) in losses {
                    report.total_diagnostics += 1;
                    if report.diagnostics.len() < max_diagnostics {
                        report.diagnostics.push(ConversionDiagnostic {
                            code,
                            node: ty.unwrap_or_default().to_owned(),
                            field: field.map(str::to_owned),
                            message: message.to_owned(),
                            pos: position(value),
                        });
                    }
                }
                let priority = |field: &str| match field {
                    "title" => 0,
                    "prefix" => 1,
                    "target" => 2,
                    "children" => 3,
                    "content" => 4,
                    "blocks" => 5,
                    "pairs" => 6,
                    "items" => 5,
                    "terms" => 6,
                    "definitions" => 7,
                    "rows" => 8,
                    "cells" => 9,
                    "base" => 10,
                    "annotation" => 11,
                    "old" => 12,
                    "new" => 13,
                    "locator" => 14,
                    "suffix" => 15,
                    "caption" => 16,
                    "shortCaption" => 17,
                    "fallback" => 18,
                    "inline" => 19,
                    _ => usize::MAX,
                };
                let mut children = object
                    .iter()
                    .filter(|(field, _)| crate::ast_sidecars::structural_field(field))
                    .collect::<Vec<_>>();
                children.sort_by_key(|(field, _)| priority(field));
                pending.extend(children.into_iter().rev().map(|(_, value)| value));
            }
            Value::Array(values) => pending.extend(values.iter().rev()),
            _ => {}
        }
    }
    report.truncated = report.total_diagnostics > report.diagnostics.len();
    Ok(report)
}

pub fn to_conversion_diagnostics_json(report: &ConversionDiagnostics) -> String {
    serde_json::to_string(report).expect("conversion diagnostics contain serializable values")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{from_json, parse};

    #[test]
    fn reports_interchange_only_wrapper_and_field() {
        let doc = from_json(r#"{"type":"document","srcByteLength":1,"children":[{"type":"paragraph","children":[{"type":"small_caps","children":[{"type":"text","value":"x"}]},{"type":"math","display":true,"content":"x","label":"eq"}]}]}"#).unwrap();
        let report = conversion_diagnostics(&doc, 1).unwrap();
        assert_eq!(report.total_diagnostics, 2);
        assert_eq!(report.diagnostics.len(), 1);
        assert!(report.truncated);
        let json: Value = serde_json::from_str(&to_conversion_diagnostics_json(&report)).unwrap();
        assert_eq!(json["totalDiagnostics"], 2);
    }

    #[test]
    fn ordinary_source_has_no_conversion_losses() {
        let doc = parse("plain");
        assert_eq!(
            conversion_diagnostics(&doc, 100).unwrap().total_diagnostics,
            0
        );
    }

    #[test]
    fn section_and_cell_blocks_survive_json_and_report_source_loss() {
        let input = r#"{"type":"document","srcByteLength":1,"children":[{"type":"section","level":2,"children":[{"type":"heading","level":2,"children":[{"type":"text","value":"Head"}]},{"type":"table","rows":[{"type":"table_row","cells":[{"type":"table_cell","header":false,"blocks":[{"type":"paragraph","children":[{"type":"text","value":"Cell"}]}]}]}]}]}]}"#;
        let doc = from_json(input).unwrap();
        let encoded = crate::to_json(&doc);
        let round_trip = from_json(&encoded).unwrap();
        assert_eq!(crate::to_json(&round_trip), encoded);
        let report = conversion_diagnostics(&doc, 10).unwrap();
        assert_eq!(report.total_diagnostics, 2);
        assert!(report.diagnostics.iter().any(
            |d| d.node == "section" && d.code == ConversionDiagnosticCode::StructureUnspellable
        ));
        assert!(report
            .diagnostics
            .iter()
            .any(|d| d.node == "table_cell" && d.field.as_deref() == Some("blocks")));
        let source = crate::render_carve(&doc).unwrap();
        assert!(source.contains("Head"));
        assert!(source.contains("Cell"));
        let html = crate::render_html(&doc).unwrap();
        assert!(html.contains("<section"));
        assert!(html.contains("<td><p>Cell</p></td>"));
    }

    #[test]
    fn equation_label_and_number_are_separate_diagnostics() {
        let doc = from_json(r#"{"type":"document","srcByteLength":1,"children":[{"type":"paragraph","children":[{"type":"math","display":true,"content":"x","label":"Eq.","number":1}]}]}"#).unwrap();
        let report = conversion_diagnostics(&doc, 10).unwrap();
        assert_eq!(report.total_diagnostics, 2);
        let fields = report
            .diagnostics
            .iter()
            .filter_map(|d| d.field.as_deref())
            .collect::<Vec<_>>();
        assert!(fields.contains(&"label"));
        assert!(fields.contains(&"number"));
    }

    #[test]
    fn direct_structs_cannot_encode_invalid_section_or_cell_content() {
        let mut section = from_json(r#"{"type":"document","srcByteLength":1,"children":[{"type":"section","level":2,"children":[]}]}"#).unwrap();
        if let crate::BlockNode::Section(node) = &mut section.children[0] {
            node.level = Some(7);
        }
        assert!(crate::try_to_json(&section).is_err());

        let mut table = from_json(r#"{"type":"document","srcByteLength":1,"children":[{"type":"table","rows":[{"type":"table_row","cells":[{"type":"table_cell","header":false,"blocks":[]}]}]}]}"#).unwrap();
        if let crate::BlockNode::Table(node) = &mut table.children[0] {
            node.rows[0].cells[0]
                .children
                .push(crate::InlineNode::text("extra"));
        }
        assert!(crate::try_to_json(&table).is_err());
        assert!(crate::render_carve(&table).is_err());
    }

    #[test]
    fn block_cells_flatten_to_one_line_without_block_markers() {
        let doc = from_json(r#"{"type":"document","srcByteLength":1,"children":[{"type":"table","rows":[{"type":"table_row","cells":[{"type":"table_cell","header":false,"blocks":[{"type":"paragraph","children":[{"type":"text","value":"one"}]},{"type":"paragraph","children":[{"type":"text","value":"two"}]}]}]}]}]}"#).unwrap();
        assert!(crate::render_carve(&doc).unwrap().contains("| one two |"));
        assert!(crate::render_plain_text(&doc).unwrap().contains("one two"));
        assert!(crate::render_markdown(&doc).unwrap().contains("one two"));
    }

    #[test]
    fn inline_cell_breaks_stay_within_the_table_row() {
        let doc = from_json(r#"{"type":"document","srcByteLength":0,"children":[{"type":"table","rows":[{"type":"table_row","cells":[{"type":"table_cell","header":false,"children":[{"type":"text","value":"one"},{"type":"soft_break"},{"type":"text","value":"two"},{"type":"hard_break"},{"type":"text","value":"three"}]}]}]}]}"#).unwrap();
        let carve = crate::render_carve(&doc).unwrap();
        let markdown = crate::render_markdown(&doc).unwrap();
        assert!(carve.lines().any(|line| line.contains("one two three")));
        // PART 11 section 9a: the Markdown target keeps the hard break as `<br>`.
        assert!(markdown
            .lines()
            .any(|line| line.contains("one two<br>three")));
        assert!(matches!(
            crate::parse(&carve).children[0],
            crate::BlockNode::Table(_)
        ));
    }

    #[test]
    fn block_cell_newlines_in_text_and_code_stay_on_one_row() {
        let doc = from_json(r#"{"type":"document","srcByteLength":0,"children":[{"type":"table","rows":[{"type":"table_row","cells":[{"type":"table_cell","header":false,"blocks":[{"type":"paragraph","children":[{"type":"text","value":"one\ntwo"},{"type":"code","value":"three\nfour"}]}]}]}]}]}"#).unwrap();
        for rendered in [
            crate::render_carve(&doc).unwrap(),
            crate::render_markdown(&doc).unwrap(),
        ] {
            assert!(rendered.lines().any(|line| line.contains("one")
                && line.contains("two")
                && line.contains("three")
                && line.contains("four")));
        }
    }

    #[test]
    fn block_cell_flatten_keeps_inline_formatting() {
        let doc = from_json(r#"{"type":"document","srcByteLength":1,"children":[{"type":"table","rows":[{"type":"table_row","cells":[{"type":"table_cell","header":false,"blocks":[{"type":"paragraph","children":[{"type":"strong","children":[{"type":"text","value":"bold"}]}]}]}]}]}]}"#).unwrap();
        let markdown = crate::render_markdown(&doc).unwrap();
        assert!(markdown.contains("**bold**"), "{markdown}");
        let ansi = crate::render_ansi(&doc).unwrap();
        assert!(ansi.contains("\u{1b}["), "{ansi:?}");
    }

    #[test]
    fn diagnostics_ignore_opaque_payload_and_follow_document_order() {
        let doc = from_json(r#"{"type":"document","srcByteLength":1,"children":[{"type":"paragraph","children":[{"type":"small_caps","children":[{"type":"text","value":"first"}]}]},{"type":"paragraph","children":[{"type":"math","display":false,"content":"x","label":"eq"}]},{"type":"block_extension","name":"example.org/x","fallback":{"type":"paragraph","children":[]},"payload":{"format":"application/json","value":{"type":"small_caps","children":[]}}}]}"#).unwrap();
        let report = conversion_diagnostics(&doc, 10).unwrap();
        assert_eq!(report.total_diagnostics, 2);
        assert_eq!(report.diagnostics[0].node, "small_caps");
        assert_eq!(report.diagnostics[1].node, "math");
    }

    #[test]
    fn diagnostics_reach_ruby_and_inline_extension_children() {
        let doc = from_json(r#"{"type":"document","srcByteLength":0,"children":[{"type":"paragraph","children":[{"type":"ruby","pairs":[{"base":[{"type":"small_caps","children":[{"type":"text","value":"base"}]}],"annotation":[{"type":"text","value":"note"}]}]},{"type":"inline_extension","name":"example.org/x","content":[{"type":"small_caps","children":[{"type":"text","value":"inner"}]}]}]}]}"#).unwrap();
        let report = conversion_diagnostics(&doc, 10).unwrap();
        assert_eq!(report.total_diagnostics, 2);
        assert!(report
            .diagnostics
            .iter()
            .all(|item| item.node == "small_caps"));
    }

    #[test]
    fn block_cell_breaks_stay_on_one_table_row() {
        let doc = from_json(r#"{"type":"document","srcByteLength":0,"children":[{"type":"table","rows":[{"type":"table_row","cells":[{"type":"table_cell","header":false,"blocks":[{"type":"paragraph","children":[{"type":"text","value":"one"},{"type":"soft_break"},{"type":"strong","children":[{"type":"text","value":"two"},{"type":"hard_break"},{"type":"text","value":"three"}]}]}]}]}]}]}"#).unwrap();
        let carve = crate::render_carve(&doc).unwrap();
        let markdown = crate::render_markdown(&doc).unwrap();
        assert_eq!(carve.lines().count(), 1, "{carve:?}");
        assert_eq!(markdown.lines().count(), 1, "{markdown:?}");
        assert!(carve.contains("one *two three*"), "{carve:?}");
        assert!(markdown.contains("**two<br>three**"), "{markdown:?}");
    }
}
