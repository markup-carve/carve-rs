/// Running smart-quote state for one block in the non-HTML renderers. `"`/`'`
/// toggle open/closed across the WHOLE inline flow (incl. across emphasis), so
/// a closing quote after an emphasis span renders correctly. Reset per block.
#[derive(Default)]
pub(crate) struct SmartQuoteState {
    open_double: bool,
    open_single: bool,
    /// Whether any inline content has been emitted in this block yet. A
    /// leading quote in a text node opens (left) only at the true start of the
    /// inline flow; once a prior sibling exists, a boundary quote is treated as
    /// word-adjacent (closing context), matching carve-js and the HTML path.
    started: bool,
}

impl SmartQuoteState {
    pub(crate) fn new() -> Self {
        SmartQuoteState {
            open_double: true,
            open_single: true,
            started: false,
        }
    }

    /// Mark that an inline node has been (or is about to be) rendered, so a
    /// following text node's leading quote no longer counts as start-of-content.
    pub(crate) fn mark_started(&mut self) {
        self.started = true;
    }
}

/// Drop C0/C1 control characters (keeping tab and newline) from author content
/// so attacker `ESC` / OSC sequences cannot inject into terminal output (the
/// ANSI and plain-text renderers). The renderers' own styling escapes are added
/// separately and are not affected.
pub(crate) fn strip_controls(input: &str) -> String {
    input
        .chars()
        .filter(|c| *c == '\t' || *c == '\n' || !c.is_control())
        .collect()
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
    if !needs_smart_pass(input) {
        return input.to_string();
    }
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
    // Decide quote flanking from the previously EMITTED char (curly-converted),
    // matching carve-js and the HTML path, so a quote right after an opening
    // quote opens too (`"'x'"` -> `“‘x’”`). Reading the raw source instead
    // misread the second quote as word-adjacent.
    let mut prev_out: Option<char> = None;
    for (idx, ch) in chars.iter().copied().enumerate() {
        if ch == '\u{e000}' {
            prev_out = Some(crate::NBSP_PLACEHOLDER);
            continue;
        }
        let escaped = idx > 0 && chars[idx - 1] == '\u{e000}';
        if ch == '"' && !escaped {
            // Normative §8: a double quote OPENS (left `“`) in an opening
            // context (start-of-content, whitespace/NBSP, or one of
            // `( [ { = : - /`, an en/em dash, or a nested opening curly quote);
            // otherwise it CLOSES (right `”`).
            let opening = quote_open_prev(prev_out, state.started);
            let e = if opening { '“' } else { '”' };
            out.push(e);
            state.open_double = !opening;
            prev_out = Some(e);
        } else if ch == '\'' && !escaped {
            // Single quote (§8, matching djot): a closing/apostrophe `’` when
            // the previous emitted char is alphanumeric OR the next char is a
            // digit OR the context is not an opening one; an opening `‘` only in
            // an open context with a non-digit next char.
            let prev_alnum = prev_out.is_some_and(|c| c.is_alphanumeric());
            let next_digit = chars.get(idx + 1).is_some_and(|c| c.is_ascii_digit());
            let apostrophe = prev_alnum || next_digit || !quote_open_prev(prev_out, state.started);
            let e = if apostrophe { '’' } else { '‘' };
            out.push(e);
            state.open_single = apostrophe;
            prev_out = Some(e);
        } else {
            out.push(ch);
            prev_out = Some(ch);
        }
    }
    out
}

/// Normative §8 quote flanking context (non-HTML renderers), decided from the
/// previously EMITTED char (`None` at the block's start). OPENING when that
/// char is whitespace/NBSP or one of the opening/operator chars `( [ { = : - /`,
/// an en/em dash, or a nested opening curly quote. At the start (`None`) the
/// quote opens only when no inline content has been emitted yet in the block
/// (`started == false`); any prior sibling makes it word-adjacent (closing),
/// matching carve-js and the HTML path.
fn quote_open_prev(prev: Option<char>, started: bool) -> bool {
    match prev {
        None => !started,
        Some(c) => {
            c.is_whitespace()
                || c == crate::NBSP_PLACEHOLDER
                || matches!(
                    c,
                    '(' | '[' | '{' | '=' | ':' | '-' | '/' | '–' | '—' | '“' | '‘'
                )
        }
    }
}

fn needs_smart_pass(input: &str) -> bool {
    input.chars().any(|ch| {
        matches!(
            ch,
            '\\' | '<' | '-' | '=' | '!' | '>' | '+' | '(' | '.' | '"' | '\''
        )
    })
}

fn unescape_text(input: &str) -> String {
    let chars = input.chars().collect::<Vec<_>>();
    let mut out = String::new();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            if chars[i + 1] == ' ' {
                out.push(crate::NBSP_PLACEHOLDER);
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
