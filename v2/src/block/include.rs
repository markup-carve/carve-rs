//! The include directive (C2-B-201 to C2-B-206). A line that misses the shape
//! is text; the option table decides the rest (`data/include-options.json`).

use crate::tables::INCLUDE_OPTIONS;
use serde_json::{Map, Value};

/// A parsed directive: path, section and decoded options (C2-A-181).
pub struct Include {
    pub path: String,
    pub section: Option<String>,
    pub options: Map<String, Value>,
}

struct Scan<'a> {
    s: &'a str,
    i: usize,
}

impl Scan<'_> {
    fn peek(&self) -> Option<char> {
        self.s.get(self.i..).and_then(|r| r.chars().next())
    }

    fn rest(&self) -> &str {
        self.s.get(self.i..).unwrap_or("")
    }

    fn spaces(&mut self) -> usize {
        let n = self.rest().bytes().take_while(|&b| b == b' ').count();
        self.i += n;
        n
    }

    fn take_while(&mut self, f: impl Fn(char) -> bool) -> &str {
        let start = self.i;
        let n: usize = self
            .rest()
            .chars()
            .take_while(|&c| f(c))
            .map(char::len_utf8)
            .sum();
        self.i += n;
        self.s.get(start..self.i).unwrap_or("")
    }

    /// The raw inside of a run quoted with `q`, where a backslash takes the next character.
    fn quoted(&mut self, q: char) -> Option<&str> {
        self.i += q.len_utf8();
        let start = self.i;
        let mut it = self.rest().char_indices();
        while let Some((at, c)) = it.next() {
            if c == '\\' {
                it.next()?;
            } else if c == q {
                self.i = start + at + 1;
                return self.s.get(start..start + at);
            }
        }
        None
    }

    fn name(&mut self, first: fn(char) -> bool) -> Option<String> {
        let c = self.peek().filter(|&c| first(c))?;
        self.i += c.len_utf8();
        let tail = self
            .take_while(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            .to_owned();
        Some(format!("{c}{tail}"))
    }
}

/// C2-I-033: a backslash before ASCII punctuation is dropped, before anything else it stays.
fn unescape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut it = raw.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' && it.peek().is_some_and(char::is_ascii_punctuation) {
            if let Some(n) = it.next() {
                out.push(n);
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_section_start(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Parses `line`, already without its line end; `None` means the line is text.
#[must_use]
pub fn parse(line: &str) -> Option<Include> {
    let line = line.trim_end_matches([' ', '\t']);
    let mut sc = Scan {
        s: line.strip_prefix("{{")?,
        i: 0,
    };
    if sc.spaces() == 0 {
        return None;
    }
    let path = path(&mut sc)?;
    let mut section = None;
    let mut options = Map::new();
    loop {
        let gap = sc.spaces();
        match sc.peek()? {
            '}' if gap > 0 && sc.rest() == "}}" => break,
            '#' => {
                sc.i += 1;
                let name = sc.name(is_section_start)?;
                if section.replace(name).is_some() {
                    return None;
                }
            }
            '@' => {
                sc.i += 1;
                option(&mut sc, &mut options)?;
            }
            _ => return None,
        }
    }
    Some(Include {
        path,
        section,
        options,
    })
}

/// C2-B-202. An empty path makes the line text.
fn path(sc: &mut Scan<'_>) -> Option<String> {
    let p = match sc.peek()? {
        '"' => unescape(sc.quoted('"')?),
        '\u{201C}' => {
            sc.i += '\u{201C}'.len_utf8();
            let p = sc.take_while(|c| c != '\u{201D}').to_owned();
            sc.peek().filter(|&c| c == '\u{201D}')?;
            sc.i += '\u{201D}'.len_utf8();
            p
        }
        _ => sc
            .take_while(|c| !matches!(c, ' ' | '#' | '@' | '}' | '"' | '\u{201C}'))
            .to_owned(),
    };
    (!p.is_empty()).then_some(p)
}

/// One written value: the decoded text, and the raw inside when it stood in `"`.
struct Written {
    text: String,
    dquoted_raw: Option<String>,
}

/// C2-B-204 to C2-B-206: reads one option into `out`; `None` sends the line to text.
fn option(sc: &mut Scan<'_>, out: &mut Map<String, Value>) -> Option<()> {
    let key = sc.name(is_ident_start)?;
    let &(_, _, shape, without, outside, repeated) =
        INCLUDE_OPTIONS.iter().find(|row| row.1 == key)?;
    let written = if sc.peek() == Some(':') {
        sc.i += 1;
        Some(value(sc)?)
    } else {
        None
    };
    let decoded = match written {
        None => match without {
            "paragraph" => return None,
            "valid" => Value::Bool(true),
            _ => Value::String(String::new()),
        },
        Some(w) => match shaped(shape, &w) {
            Some(v) => v,
            None if outside == "paragraph" => return None,
            None => Value::String(w.text),
        },
    };
    if out.contains_key(&key) {
        return if repeated == "paragraph" {
            None
        } else {
            Some(())
        };
    }
    out.insert(key, decoded);
    Some(())
}

fn value(sc: &mut Scan<'_>) -> Option<Written> {
    let q = sc.peek()?;
    if matches!(q, '"' | '\'') {
        let raw = sc.quoted(q)?.to_owned();
        return Some(Written {
            text: unescape(&raw),
            dquoted_raw: (q == '"').then_some(raw),
        });
    }
    let v = sc
        .take_while(|c| !matches!(c, '@' | '}' | ' ' | '\t'))
        .to_owned();
    (!v.is_empty()).then_some(Written {
        text: v,
        dquoted_raw: None,
    })
}

/// The value in its tree form, or `None` when it is outside its shape (C2-B-205).
fn shaped(shape: &str, w: &Written) -> Option<Value> {
    let t = w.text.as_str();
    match shape {
        "range" => is_range(t).then(|| Value::String(t.into())),
        "identifier" => is_identifier(t).then(|| Value::String(t.into())),
        "range_or_names" => match &w.dquoted_raw {
            Some(raw) => {
                names(raw).map(|v| Value::Array(v.into_iter().map(Value::String).collect()))
            }
            None => is_range(t).then(|| Value::String(t.into())),
        },
        "signed_integer_or_auto" => integer_or_auto(t),
        "language" => is_language(t).then(|| Value::String(t.into())),
        _ => None,
    }
}

fn number(s: &str) -> Option<u64> {
    let ok = s.bytes().next().is_some_and(|b| (b'1'..=b'9').contains(&b))
        && s.bytes().all(|b| b.is_ascii_digit());
    if ok { s.parse().ok() } else { None }
}

fn is_range(t: &str) -> bool {
    t.split_once('-')
        .and_then(|(a, b)| Some((number(a)?, number(b)?)))
        .is_some_and(|(a, b)| a <= b)
}

fn is_identifier(t: &str) -> bool {
    let mut cs = t.chars();
    cs.next().is_some_and(is_ident_start)
        && cs.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn is_language(t: &str) -> bool {
    !t.is_empty()
        && t.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_+#./".contains(c))
}

/// A list of column names; a backslash takes the next character, and no name is empty.
fn names(raw: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut it = raw.chars();
    while let Some(c) = it.next() {
        match c {
            '\\' => cur.push(it.next()?),
            ',' => {
                if cur.is_empty() {
                    return None;
                }
                out.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
    }
    if cur.is_empty() {
        return None;
    }
    out.push(cur);
    Some(out)
}

/// `auto` or a decimal integer with an optional sign. The clauses set no range (gap G-007).
fn integer_or_auto(t: &str) -> Option<Value> {
    if t == "auto" {
        return Some(Value::String(t.into()));
    }
    let digits = t.strip_prefix(['+', '-']).unwrap_or(t);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let t = t.strip_prefix('+').unwrap_or(t);
    match t.parse::<i64>() {
        Ok(n) => Some(Value::from(n)),
        Err(_) => t
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number),
    }
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn c2_b_201_spaces_and_closer() {
        assert!(parse("{{ a.crv }}").is_some());
        assert!(parse("{{a.crv }}").is_none());
        assert!(parse("{{ a.crv@code:js}}").is_none());
        assert!(parse("{{ a.crv }} %% c").is_none());
        assert!(parse("{{ a.crv#s@shift:1 }}").is_some());
    }

    #[test]
    fn c2_b_206_option_table() {
        assert!(parse("{{ a.crv @x }}").is_none());
        assert!(parse("{{ a.crv #s#t }}").is_none());
        assert!(parse("{{ a.crv @code:js @code:ts }}").is_none());
        let inc = parse("{{ a.crv @rows:1-2 @rows:3-4 @code }}");
        assert_eq!(
            inc.map(|i| serde_json::Value::Object(i.options).to_string()),
            Some(r#"{"code":true,"rows":"1-2"}"#.into())
        );
    }

    #[test]
    fn c2_b_205_cols_names() {
        let inc = parse(r#"{{ a.crv @cols:"a,b\\,c" }}"#);
        assert_eq!(
            inc.map(|i| serde_json::Value::Object(i.options).to_string()),
            Some(r#"{"cols":["a","b\\","c"]}"#.into())
        );
    }
}
