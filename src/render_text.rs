pub(crate) fn clean_smart_text(input: &str) -> String {
    smart_text(&clean_escaped_text(input))
}

fn clean_escaped_text(input: &str) -> String {
    let mut out = String::new();
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.peek().copied() {
                if matches!(next, '*' | '#' | '_') {
                    chars.next();
                    continue;
                }
            }
        }
        out.push(ch);
    }
    out
}

fn smart_text(input: &str) -> String {
    let mut s = unescape_text(input);
    let replacements = [
        ("<->", "↔"),
        ("->", "→"),
        ("<-", "←"),
        ("=>", "⇒"),
        ("!=", "≠"),
        ("<=", "≤"),
        (">=", "≥"),
        ("+-", "±"),
        ("(c)", "©"),
        ("(r)", "®"),
        ("(tm)", "™"),
        ("------", "——"),
        ("-----", "—–"),
        ("----", "––"),
        ("---", "—"),
        ("--", "–"),
        ("...", "…"),
    ];
    for (from, to) in replacements {
        s = s.replace(from, to);
    }
    s = s.replace("&#NO_SMART_ARROW;", "->");
    s = s.replace("§NO_SMART_DOTS§", "...");

    let chars = s.chars().collect::<Vec<_>>();
    let mut out = String::new();
    let mut open_double = true;
    let mut open_single = true;
    for (idx, ch) in chars.iter().copied().enumerate() {
        if ch == '\u{e000}' {
            continue;
        }
        let escaped = idx > 0 && chars[idx - 1] == '\u{e000}';
        if ch == '"' {
            if escaped {
                out.push(ch);
            } else {
                out.push(if open_double { '“' } else { '”' });
                open_double = !open_double;
            }
        } else if ch == '\'' {
            if escaped {
                out.push(ch);
                continue;
            }
            let prev_ws = idx == 0 || chars[idx - 1].is_whitespace();
            let next_alpha = chars.get(idx + 1).is_some_and(|c| c.is_alphabetic());
            if prev_ws && next_alpha {
                out.push('‘');
                open_single = false;
            } else if !open_single {
                out.push('’');
                open_single = true;
            } else {
                out.push('’');
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn unescape_text(input: &str) -> String {
    let chars = input.chars().collect::<Vec<_>>();
    let mut out = String::new();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            if chars[i + 1] == ' ' {
                out.push('\u{00a0}');
                i += 2;
                continue;
            }
            if chars[i + 1] == '-' && chars.get(i + 2) == Some(&'>') {
                out.push_str("&#NO_SMART_ARROW;");
                i += 3;
                continue;
            }
            if chars[i + 1] == '.' {
                let mut j = i + 1;
                let mut dots = 0usize;
                while chars.get(j) == Some(&'.') {
                    dots += 1;
                    j += 1;
                }
                if dots >= 3 {
                    out.push_str("§NO_SMART_DOTS§");
                } else {
                    for _ in 0..dots {
                        out.push('\u{e000}');
                        out.push('.');
                    }
                }
                i = j;
                continue;
            }
            if !is_render_escapable(chars[i + 1]) {
                out.push('\\');
                i += 1;
                continue;
            }
            out.push('\u{e000}');
            out.push(chars[i + 1]);
            i += 2;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn is_render_escapable(ch: char) -> bool {
    matches!(
        ch,
        '\\' | '`'
            | '*'
            | '_'
            | '{'
            | '}'
            | '['
            | ']'
            | '('
            | ')'
            | '"'
            | '\''
            | '#'
            | '+'
            | '-'
            | '.'
            | '!'
            | '~'
            | '^'
            | '/'
            | '<'
            | '>'
            | '@'
            | '%'
            | '|'
            | '='
            | ','
            | ':'
            | ';'
            | '$'
            | '&'
            | '?'
    )
}
