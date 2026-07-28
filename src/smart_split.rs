//! Split text into literal runs and smart-typography runs for the Carve
//! renderer.
//!
//! Every other renderer resolves smart typography to a glyph, so it can run the
//! substitution over a whole string. The Carve renderer must do the opposite:
//! reproduce the author's source. Emitting the AST text verbatim is almost
//! right - text nodes are source-faithful - but `escape_text` is doing double
//! duty. It protects block markers (a literal `>` at the start of a line must
//! stay escaped or it re-parses as a blockquote) AND it escapes punctuation
//! that smart typography owns (`"`, `-`, `.`), which must stay bare so it
//! re-derives to the same glyph on the next parse.
//!
//! Splitting resolves the conflict: literal runs still go through
//! `escape_text`, and smart runs are emitted exactly as the author typed them.

use crate::render::allocate_dashes;

/// One piece of a text node: either literal source or a smart-typography run.
pub(crate) enum SmartSegment {
    /// Source text, escapes intact. The caller escapes this as usual.
    Literal(String),
    /// A run the parser will re-derive to a glyph, emitted verbatim.
    Smart(String),
}

/// Running quote state for one block, mirroring the render-time passes.
#[derive(Default)]
pub(crate) struct SplitState {
    started: bool,
    prev_out: Option<char>,
}

impl SplitState {
    pub(crate) fn new() -> Self {
        SplitState {
            started: false,
            prev_out: None,
        }
    }

    /// Mark that a sibling inline node was rendered, so a following quote is
    /// word-adjacent rather than start-of-content.
    pub(crate) fn mark_started(&mut self) {
        self.started = true;
        self.prev_out = None;
    }
}

/// Longest-first, matching the render-time tables.
const SMART_TOKENS: &[(&str, &str)] = &[
    ("<->", "↔"),
    ("(tm)", "™"),
    ("...", "…"),
    ("->", "→"),
    ("<-", "←"),
    ("=>", "⇒"),
    ("<=", "≤"),
    (">=", "≥"),
    ("!=", "≠"),
    ("+-", "±"),
    ("(c)", "©"),
    ("(r)", "®"),
];

/// Opening-quote context, mirroring `quote_open_prev` in the render passes.
fn quote_opens(prev: Option<char>, started: bool) -> bool {
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

pub(crate) fn split_smart(input: &str, state: &mut SplitState) -> Vec<SmartSegment> {
    let chars: Vec<char> = input.chars().collect();
    let mut out: Vec<SmartSegment> = Vec::new();
    let mut literal = String::new();
    let mut i = 0usize;

    macro_rules! flush_literal {
        () => {
            if !literal.is_empty() {
                out.push(SmartSegment::Literal(std::mem::take(&mut literal)));
            }
        };
    }

    while i < chars.len() {
        let ch = chars[i];

        // An escaped character never forms a smart run, and the sequence is
        // already valid source, so it passes through verbatim. Routing it
        // through the escaper instead would either double the backslash or -
        // for a character the escaper does not escape, such as the space in a
        // non-breaking `\ ` - drop it entirely. The escaped character is what a
        // following quote flanks against.
        if ch == '\\' && i + 1 < chars.len() {
            flush_literal!();
            out.push(SmartSegment::Smart(format!("\\{}", chars[i + 1])));
            state.prev_out = Some(chars[i + 1]);
            state.started = true;
            i += 2;
            continue;
        }

        let rest: String = chars[i..].iter().collect();

        if let Some((token, glyph)) = SMART_TOKENS.iter().find(|(t, _)| rest.starts_with(t)) {
            flush_literal!();
            out.push(SmartSegment::Smart((*token).to_string()));
            state.prev_out = glyph.chars().last();
            state.started = true;
            i += token.chars().count();
            continue;
        }

        // A run of 2+ hyphens collapses to em/en dashes; a lone `-` is literal.
        if ch == '-' && chars.get(i + 1) == Some(&'-') {
            let mut n = 0usize;
            while chars.get(i + n) == Some(&'-') {
                n += 1;
            }
            flush_literal!();
            let glyphs = allocate_dashes(n);
            out.push(SmartSegment::Smart("-".repeat(n)));
            state.prev_out = glyphs.chars().last();
            state.started = true;
            i += n;
            continue;
        }

        if ch == '"' {
            flush_literal!();
            let opening = quote_opens(state.prev_out, state.started);
            out.push(SmartSegment::Smart("\"".to_string()));
            state.prev_out = Some(if opening { '“' } else { '”' });
            state.started = true;
            i += 1;
            continue;
        }

        if ch == '\'' {
            flush_literal!();
            let prev_alnum = state.prev_out.is_some_and(|c| c.is_alphanumeric());
            let next_digit = chars.get(i + 1).is_some_and(|c| c.is_ascii_digit());
            let apostrophe =
                prev_alnum || next_digit || !quote_opens(state.prev_out, state.started);
            out.push(SmartSegment::Smart("'".to_string()));
            state.prev_out = Some(if apostrophe { '’' } else { '‘' });
            state.started = true;
            i += 1;
            continue;
        }

        literal.push(ch);
        state.prev_out = Some(ch);
        state.started = true;
        i += 1;
    }

    flush_literal!();
    out
}
