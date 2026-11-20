//! Rule selection and reporting from the pinned 0.2 lint model.

use super::data_interpreter::Eval;
use super::{LintOptions, active_catalog};
use crate::diagnostic::Diagnostic;
use crate::source::Source;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

pub(super) fn model() -> &'static Value {
    static MODEL: OnceLock<Value> = OnceLock::new();
    MODEL.get_or_init(|| {
        serde_json::from_str(include_str!("../../spec-data/lint-model.json"))
            .expect("pinned lint model")
    })
}

pub(super) fn data_mode() -> bool {
    std::env::var("CARVE_LINT_RULES").is_ok_and(|value| value == "data")
}

pub(super) fn modeled_ids() -> &'static HashSet<&'static str> {
    static IDS: OnceLock<HashSet<&'static str>> = OnceLock::new();
    IDS.get_or_init(|| {
        model()["rules"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|rule| rule["id"].as_str())
            .collect()
    })
}

pub(super) fn block_candidate(kind: &str, parent: &str) -> bool {
    model()["rules"].as_array().is_some_and(|rules| {
        rules.iter().any(|rule| {
            rule["over"] == "block"
                && rule["types"]
                    .as_array()
                    .is_none_or(|types| types.iter().any(|item| item == kind))
                && admitted(&rule["when"], "parent.type").is_none_or(|names| names.contains(parent))
        })
    })
}

pub(super) fn inline_candidate(kind: &str) -> bool {
    model()["rules"].as_array().is_some_and(|rules| {
        rules.iter().any(|rule| {
            rule["over"] == "inline"
                && rule["types"]
                    .as_array()
                    .is_some_and(|types| types.iter().any(|item| item == kind))
        })
    })
}

pub(super) fn inline_text_candidate(text: &str) -> bool {
    static BRACKET_REQUIRED: OnceLock<bool> = OnceLock::new();
    let required = BRACKET_REQUIRED.get_or_init(|| {
        let mut found = false;
        let guarded = model()["rules"].as_array().is_some_and(|rules| {
            rules
                .iter()
                .filter(|rule| {
                    rule["over"] == "inline"
                        && rule["types"]
                            .as_array()
                            .is_some_and(|types| types.iter().any(|item| item == "text"))
                })
                .all(|rule| {
                    found = true;
                    rule["when"]["contains"].as_array().is_some_and(|args| {
                        args.first().is_some_and(|value| value["f"] == "text")
                            && args.get(1).is_some_and(|value| value == "[")
                    })
                })
        });
        found && guarded
    });
    !*required || text.contains('[')
}

pub(super) struct Record {
    pub value: Value,
    pub path: String,
    pub line_start: usize,
    pub line_index: usize,
    pub over: &'static str,
}

pub(super) struct Records {
    pub lines: Vec<Value>,
    pub items: Vec<Record>,
}

fn rule_for(record: &Record, value: &Value, rule: &Value) -> bool {
    if rule["over"].as_str() != Some(record.over) {
        return false;
    }
    if let Some(types) = rule["types"].as_array() {
        if !types.iter().any(|kind| kind == &value["type"]) {
            return false;
        }
    }
    if record.over == "block" {
        let parent = value["parent"]["type"].as_str().unwrap_or("document");
        if admitted(&rule["when"], "parent.type").is_some_and(|names| !names.contains(parent)) {
            return false;
        }
    }
    if record.over == "line" {
        let leaf = value["leaf"].as_str().unwrap_or("none");
        if admitted(&rule["when"], "leaf").is_some_and(|names| !names.contains(leaf)) {
            return false;
        }
        if model()["verbatim_leaves"]
            .as_array()
            .is_some_and(|set| set.iter().any(|kind| kind == leaf))
            && !rule["verbatim"]
                .as_array()
                .is_some_and(|set| set.iter().any(|kind| kind == leaf))
        {
            return false;
        }
    }
    true
}

pub(super) fn admitted(expr: &Value, field: &str) -> Option<HashSet<String>> {
    if let Some(args) = expr.get("eq").and_then(Value::as_array) {
        if args
            .first()
            .and_then(|x| x.get("f"))
            .and_then(Value::as_str)
            == Some(field)
        {
            return args
                .get(1)
                .and_then(Value::as_str)
                .map(|name| HashSet::from([name.to_owned()]));
        }
    }
    if let Some(args) = expr.get("in").and_then(Value::as_array) {
        if args
            .first()
            .and_then(|x| x.get("f"))
            .and_then(Value::as_str)
            == Some(field)
        {
            let source = args.get(1)?;
            let members = if let Some(name) = source.as_str() {
                model()[name].as_array()
            } else {
                source.as_array()
            }?;
            return Some(
                members
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect(),
            );
        }
    }
    if let Some(all) = expr.get("all").and_then(Value::as_array) {
        return all.iter().find_map(|item| admitted(item, field));
    }
    if let Some(any) = expr.get("any").and_then(Value::as_array) {
        let mut names = HashSet::new();
        for item in any {
            names.extend(admitted(item, field)?);
        }
        return Some(names);
    }
    None
}

fn report(
    rule: &Value,
    eval: &mut Eval<'_>,
    source: &Source<'_>,
    path: &str,
    out: &mut Vec<Diagnostic>,
) {
    let Some(reports) = rule["report"].as_array() else {
        return;
    };
    for entry in reports {
        if !entry.get("when").is_none_or(|when| eval.test(when)) {
            continue;
        }
        let start = eval.number(
            entry
                .get("start")
                .or_else(|| rule["range"].get("start"))
                .unwrap_or(&Value::Null),
        );
        let end = eval.number(
            entry
                .get("end")
                .or_else(|| rule["range"].get("end"))
                .unwrap_or(&Value::Null),
        );
        if start > end || end > source.text().len() {
            return;
        }
        let Some(code) = rule["id"].as_str() else {
            return;
        };
        let Some(&code) = modeled_ids().get(code) else {
            return;
        };
        let mut values = Vec::new();
        for (key, expression) in rule["values"]
            .as_object()
            .into_iter()
            .flat_map(|map| map.iter())
        {
            values.push((key.as_str(), eval.string(expression)));
        }
        for (key, expression) in entry["values"]
            .as_object()
            .into_iter()
            .flat_map(|map| map.iter())
        {
            if let Some((_, prior)) = values.iter_mut().find(|(name, _)| *name == key) {
                *prior = eval.string(expression);
            } else {
                values.push((key.as_str(), eval.string(expression)));
            }
        }
        let refs: Vec<(&str, &str)> = values
            .iter()
            .map(|(key, value)| (*key, value.as_str()))
            .collect();
        let mut diagnostic = Diagnostic::nth(
            code,
            entry["message"].as_u64().unwrap_or(0) as usize,
            &refs,
            None,
            Some(source.pos(start, end)),
        );
        diagnostic.path = Some(path.to_owned());
        out.push(diagnostic);
        return;
    }
}

pub(super) fn run(records: &Records, source: &str, options: &LintOptions) -> Vec<Diagnostic> {
    let positions = Source::new(source);
    let mut out = Vec::new();
    let active_rules: Vec<&Value> = model()["rules"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|rule| {
            rule["id"]
                .as_str()
                .is_some_and(|code| active_catalog(code, options))
        })
        .collect();
    let mut buckets: HashMap<(&'static str, &str, &str), Vec<&Value>> = HashMap::new();
    for record in &records.items {
        let value = if record.over == "line" {
            &records.lines[record.line_index]
        } else {
            &record.value
        };
        let kind = if record.over == "line" {
            value["leaf"].as_str()
        } else {
            value["type"].as_str()
        }
        .unwrap_or("");
        let parent = if record.over == "block" {
            value["parent"]["type"].as_str().unwrap_or("document")
        } else {
            ""
        };
        let rules = buckets
            .entry((record.over, kind, parent))
            .or_insert_with(|| {
                active_rules
                    .iter()
                    .copied()
                    .filter(|rule| rule_for(record, value, rule))
                    .collect()
            });
        for rule in rules {
            let mut eval = Eval {
                record: value,
                lines: &records.lines,
                line_index: record.line_index,
                over: record.over,
                line: records.lines[record.line_index]["text"]
                    .as_str()
                    .unwrap_or(""),
                line_start: record.line_start,
                vars: HashMap::new(),
                let_expr: HashMap::new(),
                item: None,
                spans: HashMap::new(),
                base: 0,
            };
            if let Some(lets) = rule["let"].as_object() {
                for (key, expr) in lets {
                    eval.let_expr.insert(key.clone(), expr.clone());
                }
            }
            let passed = rule.get("when").is_none_or(|when| eval.test(when));
            if !passed {
                continue;
            }
            if let Some(each) = rule.get("each") {
                let Some(items) = each["matches"].as_array() else {
                    continue;
                };
                let subject = eval.value(&items[0]);
                let Some(text) = subject.as_str() else {
                    continue;
                };
                let Some(pattern) = items.get(1).and_then(Value::as_str) else {
                    continue;
                };
                let Ok(re) = regex::Regex::new(&pattern.replace("(?<", "(?P<")) else {
                    continue;
                };
                let base = if items[0]["f"] == "text" {
                    eval.value(&serde_json::json!({"f":"start"}))
                        .as_u64()
                        .unwrap_or(0) as usize
                } else if items[0]["f"] == "body" {
                    eval.value(&serde_json::json!({"f":"body_start"}))
                        .as_u64()
                        .unwrap_or(0) as usize
                } else {
                    0
                };
                let matches: Vec<_> = re
                    .captures_iter(text)
                    .map(|found| {
                        re.capture_names()
                            .flatten()
                            .filter_map(|name| {
                                found
                                    .name(name)
                                    .map(|span| (name.to_owned(), (span.start(), span.end())))
                            })
                            .collect()
                    })
                    .collect();
                for spans in matches {
                    eval.spans = spans;
                    eval.base = base;
                    report(rule, &mut eval, &positions, &record.path, &mut out);
                }
            } else {
                report(rule, &mut eval, &positions, &record.path, &mut out);
            }
        }
    }
    out
}
