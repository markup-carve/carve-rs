/// Running smart-quote state for one block in the non-HTML renderers. `"`/`'`
/// toggle open/closed across the WHOLE inline flow (incl. across emphasis), so
/// a closing quote after an emphasis span renders correctly. Reset per block.
#[derive(Default)]
pub(crate) struct SmartQuoteState {
    open_double: bool,
    open_single: bool,
}

impl SmartQuoteState {
    pub(crate) fn new() -> Self {
        SmartQuoteState {
            open_double: true,
            open_single: true,
        }
    }
}

/// Smart-quote an isolated text run (fresh state).
pub(crate) fn clean_smart_text(input: &str) -> String {
    let mut state = SmartQuoteState::new();
    clean_smart_text_stateful(input, &mut state)
}

/// Smart-quote a text run, threading the block's running quote state so a
/// closing quote after an emphasis/link span is recognized.
pub(crate) fn clean_smart_text_stateful(input: &str, state: &mut SmartQuoteState) -> String {
    smart_text(&clean_escaped_text(input), state)
}

fn clean_escaped_text(input: &str) -> String {
    let mut out = String::new();
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.peek().copied() {
                if matches!(next, '*' | '#' | '_') {
                    // Drop only the backslash and KEEP the escaped character --
                    // it is the literal text the user wrote. (Plain/ansi emit it
                    // bare; the markdown renderer re-escapes it via escape_text.)
                    chars.next();
                    out.push(next);
                    continue;
                }
            }
        }
        out.push(ch);
    }
    out
}

fn smart_text(input: &str, state: &mut SmartQuoteState) -> String {
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
    for (idx, ch) in chars.iter().copied().enumerate() {
        if ch == '\u{e000}' {
            continue;
        }
        let escaped = idx > 0 && chars[idx - 1] == '\u{e000}';
        if ch == '"' {
            if escaped {
                out.push(ch);
            } else {
                out.push(if state.open_double { '“' } else { '”' });
                state.open_double = !state.open_double;
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
                state.open_single = false;
            } else if !state.open_single {
                out.push('’');
                state.open_single = true;
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
