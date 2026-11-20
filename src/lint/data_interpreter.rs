//! Evaluates the expressions of the pinned 0.2 lint model.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use regex::{Regex, RegexBuilder};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

pub(super) struct Eval<'a> {
    pub record: &'a Value,
    pub lines: &'a [Value],
    pub line_index: usize,
    pub over: &'static str,
    pub line: &'a str,
    pub line_start: usize,
    pub vars: HashMap<String, Value>,
    pub let_expr: HashMap<String, Value>,
    pub item: Option<Value>,
    pub spans: HashMap<String, (usize, usize)>,
    pub base: usize,
}

type RegexCache = HashMap<u64, (String, String, Option<Regex>)>;

fn regex(pattern: &str, flags: &str) -> Option<Regex> {
    thread_local! {
        static CACHE: RefCell<RegexCache> = RefCell::new(HashMap::new());
    }
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    pattern.hash(&mut hash);
    flags.hash(&mut hash);
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let item = cache.entry(hash.finish()).or_insert_with(|| {
            let compiled = RegexBuilder::new(&pattern.replace("(?<", "(?P<"))
                .case_insensitive(flags.contains('i'))
                .build()
                .ok();
            (pattern.to_owned(), flags.to_owned(), compiled)
        });
        if item.0 == pattern && item.1 == flags {
            item.2.clone()
        } else {
            RegexBuilder::new(&pattern.replace("(?<", "(?P<"))
                .case_insensitive(flags.contains('i'))
                .build()
                .ok()
        }
    })
}

fn as_number(value: &Value) -> i64 {
    match value {
        Value::Number(n) => n.as_i64().unwrap_or(0),
        Value::Bool(b) => i64::from(*b),
        Value::String(s) => s.parse().unwrap_or(0),
        _ => 0,
    }
}

fn as_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn truth(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_i64().unwrap_or(0) != 0,
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

impl Eval<'_> {
    fn line_neighbor(&self, index: usize, name: &str) -> Option<&Value> {
        let line = self.lines.get(index)?;
        let other = match name {
            "prev" => index.checked_sub(1)?,
            "prev2" => index.checked_sub(2)?,
            "next" => index + 1,
            "next2" => index + 2,
            "leaf_head" => line["leaf_head_index"].as_u64()?.checked_sub(1)? as usize,
            "leaf_next" => {
                let last = line["leaf_last_index"].as_u64()? as usize;
                if last > 0 { last } else { index + 1 }
            }
            _ => return None,
        };
        self.lines.get(other)
    }

    fn field_ref(&self, path: &str) -> Option<&Value> {
        let mut parts = path.split('.');
        let first = parts.next().unwrap_or("");
        let mut found = if first == "item" {
            self.item.as_ref()
        } else if self.over == "line"
            && matches!(
                first,
                "prev" | "prev2" | "next" | "next2" | "leaf_head" | "leaf_next"
            )
        {
            self.line_neighbor(self.line_index, first)
        } else if self.over == "block" && first == "line" {
            self.lines.get(self.record["line_index"].as_u64()? as usize)
        } else if self.over == "block" && first == "last_line" {
            self.lines
                .get(self.record["last_line_index"].as_u64()? as usize)
        } else {
            self.record.get(first)
        };
        for part in parts {
            if let Some(line) = found.filter(|value| value.get("leaf_id").is_some()) {
                let index = line["number"].as_u64().unwrap_or(1).saturating_sub(1) as usize;
                if matches!(
                    part,
                    "prev" | "prev2" | "next" | "next2" | "leaf_head" | "leaf_next"
                ) {
                    found = self.line_neighbor(index, part);
                    continue;
                }
            }
            found = match found {
                Some(Value::Array(items)) => part.parse::<usize>().ok().and_then(|i| items.get(i)),
                Some(Value::Object(map)) => map.get(part),
                _ => None,
            };
        }
        found
    }

    fn field(&self, path: &str) -> Value {
        self.field_ref(path).cloned().unwrap_or(Value::Null)
    }

    fn has(&self, path: &str) -> bool {
        self.field_ref(path).is_some_and(|v| !v.is_null())
    }

    fn base_for(&self, expr: &Value) -> usize {
        let Some(path) = expr.get("f").and_then(Value::as_str) else {
            return 0;
        };
        let prefix = path.rsplit_once('.').map_or("", |(before, _)| before);
        let field = |name: &str| {
            self.field(&if prefix.is_empty() {
                name.to_owned()
            } else {
                format!("{prefix}.{name}")
            })
            .as_u64()
            .unwrap_or(0) as usize
        };
        match path.rsplit('.').next().unwrap_or("") {
            "body" | "raw_body" => field("body_start"),
            "text" => field("start"),
            "opener_text" => field("opener_start"),
            _ => 0,
        }
    }

    fn matched(&mut self, args: &Value) -> bool {
        let Some(items) = args.as_array() else {
            return false;
        };
        let Some(pattern) = items.get(1).and_then(Value::as_str) else {
            return false;
        };
        let subject = self.value(&items[0]);
        let Some(haystack) = subject.as_str() else {
            return false;
        };
        let flags = items.get(2).and_then(Value::as_str).unwrap_or("");
        let Some(re) = regex(pattern, flags) else {
            return false;
        };
        if re.capture_names().flatten().next().is_none() {
            return re.is_match(haystack);
        }
        let Some(found) = re.captures(haystack) else {
            return false;
        };
        if re.capture_names().flatten().next().is_some() {
            self.spans.clear();
            for name in re.capture_names().flatten() {
                if let Some(span) = found.name(name) {
                    self.spans
                        .insert(name.to_owned(), (span.start(), span.end()));
                }
            }
            self.base = self.base_for(&items[0]);
        }
        true
    }

    fn variable(&mut self, name: &str) -> Value {
        if let Some(value) = self.vars.get(name) {
            return value.clone();
        }
        let Some(expression) = self.let_expr.get(name).cloned() else {
            return Value::Null;
        };
        let spans = self.spans.clone();
        let base = self.base;
        let value = self.value(&expression);
        self.spans = spans;
        self.base = base;
        self.vars.insert(name.to_owned(), value.clone());
        value
    }

    fn order(&mut self, op: &str, left: &Value, right: &Value) -> Value {
        let left_value = self.value(left);
        let right_value = self.value(right);
        if left_value.is_null() || right_value.is_null() {
            return Value::Bool(false);
        }
        let left = as_number(&left_value);
        let right = as_number(&right_value);
        Value::Bool(match op {
            "lt" => left < right,
            "le" => left <= right,
            "gt" => left > right,
            _ => left >= right,
        })
    }

    fn contains_value(&mut self, left: &Value, right: &Value) -> Value {
        let haystack = self.value(left);
        let needle = self.value(right);
        Value::Bool(match haystack {
            Value::String(s) => needle.as_str().is_some_and(|n| s.contains(n)),
            Value::Array(v) => v.contains(&needle),
            _ => false,
        })
    }

    fn in_value(&mut self, left: &Value, right: &Value) -> Value {
        let subject = self.value(left);
        Value::Bool(if let Some(key) = right.as_str() {
            super::data_rules::model()[key]
                .as_array()
                .is_some_and(|v| v.contains(&subject))
        } else {
            right.as_array().is_some_and(|v| v.contains(&subject))
        })
    }

    fn match_value(&self, arg: &Value, object: &serde_json::Map<String, Value>) -> Value {
        let side = usize::from(object.get("at").and_then(Value::as_str) == Some("end"));
        self.spans
            .get(arg.as_str().unwrap_or(""))
            .map_or(Value::Null, |span| {
                Value::from((self.base + if side == 0 { span.0 } else { span.1 }) as u64)
            })
    }

    fn starts_value(&mut self, left: &Value, right: &Value) -> Value {
        let haystack = self.value(left);
        let prefix = self.value(right);
        Value::Bool(
            haystack
                .as_str()
                .zip(prefix.as_str())
                .is_some_and(|(s, p)| s.starts_with(p)),
        )
    }

    fn if_value(&mut self, condition: &Value, yes: &Value, no: &Value) -> Value {
        if self.test(condition) {
            self.value(yes)
        } else {
            self.value(no)
        }
    }

    fn fold_value(&mut self, op: &str, args: Option<&Vec<Value>>) -> Value {
        Value::Bool(if op == "all" {
            args.is_some_and(|values| values.iter().all(|item| self.test(item)))
        } else {
            args.is_some_and(|values| values.iter().any(|item| self.test(item)))
        })
    }

    fn some_value(&mut self, arg: &Value) -> Value {
        let list = self.field(arg["in"].as_str().unwrap_or(""));
        let from = arg["from"].as_u64().unwrap_or(0) as usize;
        let before = self.item.clone();
        let result = list.as_array().is_some_and(|items| {
            items.iter().skip(from).any(|item| {
                self.item = Some(item.clone());
                self.test(&arg["where"])
            })
        });
        self.item = before;
        Value::Bool(result)
    }

    fn shift_value(&mut self, op: &str, position: &Value, chars: &Value) -> Value {
        let mut at = as_number(&self.value(position)).max(0) as usize;
        let chars = as_text(&self.value(chars));
        if op == "extend_left" {
            while at > self.line_start
                && self
                    .line
                    .as_bytes()
                    .get(at - self.line_start - 1)
                    .is_some_and(|b| chars.as_bytes().contains(b))
            {
                at -= 1;
            }
        } else {
            while self
                .line
                .as_bytes()
                .get(at.saturating_sub(self.line_start))
                .is_some_and(|b| chars.as_bytes().contains(b))
            {
                at += 1;
            }
        }
        Value::from(at as u64)
    }

    fn slice_value(&mut self, args: &[Value]) -> Value {
        let text = as_text(&self.value(args.first().unwrap_or(&Value::Null)));
        let from = as_number(&self.value(args.get(1).unwrap_or(&Value::Null))).max(0) as usize;
        let to = args
            .get(2)
            .map_or_else(|| text.len(), |v| as_number(&self.value(v)).max(0) as usize);
        Value::String(
            text.get(from.min(text.len())..to.min(text.len()))
                .unwrap_or("")
                .to_owned(),
        )
    }

    pub fn value(&mut self, expr: &Value) -> Value {
        let Some(object) = expr.as_object() else {
            return expr.clone();
        };
        let Some((op, arg)) = object
            .get_key_value("match")
            .or_else(|| object.iter().find(|(key, _)| key.as_str() != "at"))
        else {
            return Value::Null;
        };
        if matches!(
            op.as_str(),
            "f" | "v"
                | "has"
                | "all"
                | "any"
                | "not"
                | "if"
                | "eq"
                | "ne"
                | "lt"
                | "le"
                | "gt"
                | "ge"
                | "in"
                | "contains"
                | "starts"
                | "matches"
                | "match"
        ) {
            self.value_core(op, arg, object)
        } else {
            self.value_extra(op, arg)
        }
    }

    fn value_core(
        &mut self,
        op: &str,
        arg: &Value,
        object: &serde_json::Map<String, Value>,
    ) -> Value {
        let args = arg.as_array();
        let arg_at = |i: usize| args.and_then(|list| list.get(i)).unwrap_or(&Value::Null);
        match op {
            "f" => self.field(arg.as_str().unwrap_or("")),
            "v" => self.variable(arg.as_str().unwrap_or("")),
            "has" => Value::Bool(self.has(arg.as_str().unwrap_or(""))),
            "all" | "any" => self.fold_value(op, args),
            "not" => Value::Bool(!self.test(arg)),
            "if" => self.if_value(arg_at(0), arg_at(1), arg_at(2)),
            "eq" => Value::Bool(self.value(arg_at(0)) == self.value(arg_at(1))),
            "ne" => Value::Bool(self.value(arg_at(0)) != self.value(arg_at(1))),
            "lt" | "le" | "gt" | "ge" => self.order(op, arg_at(0), arg_at(1)),
            "in" => self.in_value(arg_at(0), arg_at(1)),
            "contains" => self.contains_value(arg_at(0), arg_at(1)),
            "starts" => self.starts_value(arg_at(0), arg_at(1)),
            "matches" => Value::Bool(self.matched(arg)),
            "match" => self.match_value(arg, object),
            _ => Value::Null,
        }
    }

    fn value_extra(&mut self, op: &str, arg: &Value) -> Value {
        let args = arg.as_array();
        let arg_at = |i: usize| args.and_then(|list| list.get(i)).unwrap_or(&Value::Null);
        match op {
            "some" => self.some_value(arg),
            "len" => Value::from(self.value(arg).as_str().map_or(0, str::len) as u64),
            "add" => Value::from(args.map_or(0, |v| {
                v.iter().map(|x| as_number(&self.value(x))).sum::<i64>()
            })),
            "sub" => {
                Value::from(as_number(&self.value(arg_at(0))) - as_number(&self.value(arg_at(1))))
            }
            "slice" => self.slice_value(args.map_or(&[], Vec::as_slice)),
            "concat" => Value::String(args.map_or(String::new(), |v| {
                v.iter().map(|x| as_text(&self.value(x))).collect()
            })),
            "trim_end" => Value::String(
                as_text(&self.value(arg))
                    .trim_end_matches([' ', '\t'])
                    .to_owned(),
            ),
            "words" => Value::String(as_text(&self.value(arg)).replace('_', " ")),
            "text_from" => {
                let from = as_number(&self.value(arg)).max(0) as usize;
                Value::String(
                    self.line
                        .get(from.saturating_sub(self.line_start)..)
                        .unwrap_or("")
                        .trim_end_matches([' ', '\t'])
                        .to_owned(),
                )
            }
            "extend_left" | "skip_right" => self.shift_value(op, arg_at(0), arg_at(1)),
            _ => Value::Null,
        }
    }

    pub fn test(&mut self, expr: &Value) -> bool {
        truth(&self.value(expr))
    }
    pub fn number(&mut self, expr: &Value) -> usize {
        as_number(&self.value(expr)).max(0) as usize
    }
    pub fn string(&mut self, expr: &Value) -> String {
        as_text(&self.value(expr))
    }
}
