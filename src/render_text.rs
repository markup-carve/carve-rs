use crate::ast::{BlockNode, Document, FigureTarget, InlineNode};
use crate::escape::is_bidi_control;
use std::collections::BTreeSet;

/// The `(term, expansion)` pairs whose expansion this render will emit.
pub(crate) type ConsumedAbbreviations = BTreeSet<(String, String)>;

/// Which abbreviation DEFINITIONS the plain-text and terminal targets drop.
///
/// PART 11 §10f splits the referenced definition by target: Markdown keeps the
/// `*[TERM]: expansion` line, because that spelling is PHP Markdown Extra's own
/// and the export round-trips through it, while plain and the terminal drop it
/// and print `TERM (expansion)` at each occurrence instead. §10a - the UNUSED
/// definition, which every non-HTML target still emits - is untouched.
///
/// THE TEST IS WHETHER THIS DEFINITION'S EXPANSION IS EMITTED, not whether its
/// term appears. The line goes because the content is emitted TWICE, and it is
/// emitted twice only where the expansion is emitted. So the answer is a set of
/// `(term, expansion)` PAIRS rather than a set of terms, and two shapes already
/// in the corpus turn on that:
///
/// - PART 9R R3 is last-wins, so in `*[A]: a` / `*[A]: b` the parser resolves
///   every occurrence to `b`. Only `("A", "b")` lands in the set, so `*[A]: b`
///   goes and `*[A]: a` stays - dropping both would delete the string `a` from
///   the document outright, which is the content loss §10a exists to prevent.
/// - PART 9 §9 makes an authored `abbr` authoritative, and a resolved
///   abbreviation inside such a span contributes only its visible text. Its
///   expansion therefore reaches no target, so the occurrence does not count and
///   the definition keeps its line (`45-inline-extensions-11`).
///
/// The suppression rule here mirrors the `suppress_automatic_abbreviation` flag
/// the three inline renderers carry, INCLUDING that an empty `{abbr=""}`
/// suppresses too: it is the author's spelling for "mark this, expand nothing",
/// so the definition's expansion is not emitted there either.
///
/// A footnote definition's body is walked because the plain and terminal targets
/// render it, so an occurrence inside one emits its expansion like any other.
///
/// WHAT THIS DELIBERATELY DOES NOT MODEL is `crate::abbr_budget`. Past the
/// budget an occurrence degrades to its plain key, so on a document large enough
/// to exhaust a megabyte of expansion the line can be dropped while a late
/// occurrence prints no expansion. Simulating the budget here would mean
/// replaying every spend in render order from a second walk that has to stay in
/// step with three renderers - a far likelier source of divergence than the
/// case it covers, which is a §25 denial-of-service defense degrading a document
/// that is already being clipped.
pub(crate) fn consumed_abbreviation_definitions(doc: &Document) -> ConsumedAbbreviations {
    let mut out = ConsumedAbbreviations::new();
    collect_blocks(&doc.children, false, &mut out);
    for body in doc.footnote_defs.values() {
        collect_blocks(body, false, &mut out);
    }
    out
}

fn collect_blocks(blocks: &[BlockNode], suppressed: bool, out: &mut ConsumedAbbreviations) {
    for block in blocks {
        collect_block(block, suppressed, out);
    }
}

fn collect_block(block: &BlockNode, suppressed: bool, out: &mut ConsumedAbbreviations) {
    match block {
        BlockNode::Heading(h) => collect_inlines(&h.children, suppressed, out),
        BlockNode::Paragraph(p) => collect_inlines(&p.children, suppressed, out),
        BlockNode::List(l) => {
            for item in &l.items {
                collect_blocks(&item.children, suppressed, out);
            }
        }
        BlockNode::BlockQuote(b) => collect_block_quote(b, suppressed, out),
        BlockNode::Table(t) => collect_table(t, suppressed, out),
        BlockNode::Admonition(a) => {
            if let Some(title) = &a.title {
                collect_inlines(title, suppressed, out);
            }
            collect_blocks(&a.children, suppressed, out);
        }
        BlockNode::Div(d) => collect_blocks(&d.children, suppressed, out),
        BlockNode::LineBlock(lb) => collect_blocks(&lb.children, suppressed, out),
        BlockNode::DefinitionList(d) => {
            for item in &d.items {
                for term in &item.terms {
                    collect_inlines(term, suppressed, out);
                }
                for definition in &item.definitions {
                    collect_blocks(definition, suppressed, out);
                }
            }
        }
        BlockNode::Figure(f) => {
            match &f.target {
                FigureTarget::Image(_) | FigureTarget::CodeBlock(_) => {}
                FigureTarget::BlockQuote(b) => collect_block_quote(b, suppressed, out),
                FigureTarget::Table(t) => collect_table(t, suppressed, out),
                FigureTarget::Paragraph(p) => collect_inlines(&p.children, suppressed, out),
            }
            collect_inlines(&f.caption, suppressed, out);
        }
        BlockNode::Extension(e) => collect_blocks(&e.children, suppressed, out),
        // A code block's text is never abbreviation-expanded, and the remaining
        // block kinds carry no inline children at all.
        BlockNode::CodeBlock(_)
        | BlockNode::AbbreviationDef(_)
        | BlockNode::LinkReferenceDefinition(_)
        | BlockNode::RawBlock(_)
        | BlockNode::Comment(_)
        | BlockNode::BlockImage(_)
        | BlockNode::ThematicBreak(_) => {}
    }
}

fn collect_block_quote(
    quote: &crate::ast::BlockQuote,
    suppressed: bool,
    out: &mut ConsumedAbbreviations,
) {
    if let Some(attribution) = &quote.attribution {
        collect_inlines(attribution, suppressed, out);
    }
    collect_blocks(&quote.children, suppressed, out);
}

fn collect_table(table: &crate::ast::Table, suppressed: bool, out: &mut ConsumedAbbreviations) {
    if let Some(caption) = &table.caption {
        collect_inlines(caption, suppressed, out);
    }
    for row in &table.rows {
        for cell in &row.cells {
            collect_inlines(&cell.children, suppressed, out);
        }
    }
}

fn collect_inlines(nodes: &[InlineNode], suppressed: bool, out: &mut ConsumedAbbreviations) {
    for node in nodes {
        match node {
            InlineNode::Abbreviation(abbr) => {
                if !suppressed {
                    out.insert((abbr.abbr.clone(), abbr.expansion.clone()));
                }
            }
            InlineNode::Span(span) => {
                // An authored `abbr` outranks the document definition, so every
                // resolved abbreviation below it contributes visible text only.
                let authored = span
                    .attrs
                    .as_ref()
                    .is_some_and(|a| a.key_values.contains_key("abbr"));
                collect_inlines(&span.children, suppressed || authored, out);
            }
            InlineNode::Emphasis(e) => collect_inlines(&e.children, suppressed, out),
            InlineNode::Link(l) => collect_inlines(&l.children, suppressed, out),
            InlineNode::Extension(e) => collect_inlines(&e.children, suppressed, out),
            InlineNode::Footnote(f) => {
                if let Some(inline) = &f.inline {
                    collect_inlines(inline, suppressed, out);
                }
            }
            InlineNode::CriticInsert(c) => collect_inlines(&c.children, suppressed, out),
            InlineNode::CriticDelete(c) => collect_inlines(&c.children, suppressed, out),
            InlineNode::CitationGroup(g) => {
                for item in &g.items {
                    for part in [&item.prefix, &item.locator, &item.suffix]
                        .into_iter()
                        .flatten()
                    {
                        collect_inlines(part, suppressed, out);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Drop EVERY control character (keeping tab and newline) from author content,
/// so an attacker's `ESC` / OSC sequence cannot inject into terminal output.
///
/// THE TERMINAL TARGET ONLY (PART 9 §29 T4). It is the one target whose consumer
/// ACTS on the character: a form feed feeds or clears, and U+001B introduces a
/// sequence that can move the cursor, rewrite earlier output or reach the
/// clipboard. That is a property of the DEVICE, so it reaches this target and no
/// other. The breadth is deliberate and T4 says so in as many words: §25
/// NON-HTML TARGETS requires DEL (U+007F) and the C1 controls to go too, because
/// CSI (U+009B) and OSC (U+009D) are single-character forms of the very
/// sequences the requirement exists to stop. Narrowing this to C0 would be a
/// security regression.
///
/// The Markdown and plain-text targets use [`strip_high_controls`] instead.
pub(crate) fn strip_terminal_controls(input: &str) -> String {
    input
        .chars()
        .filter(|c| (*c == '\t' || *c == '\n' || !c.is_control()) && !is_bidi_control(*c))
        .collect()
}

/// Drop DEL and the C1 controls, and NOTHING BELOW U+007F, from author content.
///
/// What the Markdown and plain-text targets strip (PART 9 §29 T2, T3). After
/// markup-carve/carve#963 the whitespace of this language is exactly U+0020,
/// U+0009, U+000A and U+000D; every other C0 control - U+0000..U+0008, U+000B,
/// U+000C, U+000E..U+001F - is ordinary CONTENT that parses as content, survives
/// into the AST, and satisfies no whitespace slot. §29 then says what each target
/// does with that content, and for these two the answer is EMIT: a target that
/// silently removes content is lossy in exactly the way markup-carve/carve#817
/// rejected for the wire, and the reason first offered for the strip - that a
/// Markdown reader would reclassify these as whitespace - was measured against
/// four readers and did not hold.
///
/// DEL AND THE C1 CONTROLS ARE NOT PART OF THAT, and stay stripped here. §29 T5
/// puts them outside the section explicitly and leaves them to a ticket of their
/// own; removing them from this filter as well would have made this change
/// introduce that defect rather than leave it where it is
/// (markup-carve/carve-rs#812).
///
/// NEITHER IS U+000D, and for the opposite reason: carve#963 makes it
/// WHITESPACE, so §29's class - "every OTHER C0 control" - excludes it and this
/// section rules on it not at all. The parser never lets one through (a CRLF is
/// normalized before any block is read), so it can only arrive on a tree built
/// through the API or read by `from_json`, where it is a LINE TERMINATOR inside
/// a leaf the writer is laying out in lines of its own: a Markdown reader may
/// take it as a line boundary, and on a terminal it returns the cursor over what
/// was already printed. The previous filter dropped it and so does this one -
/// leaving a character §29 does not govern exactly where it was (raised by
/// `codex review`).
pub(crate) fn strip_high_controls(input: &str) -> String {
    if !input.chars().any(is_not_emitted) {
        return input.to_string();
    }
    input.chars().filter(|c| !is_not_emitted(*c)).collect()
}

/// DEL (U+007F), the C1 controls (U+0080..U+009F), the carriage return, and
/// §26's bidi override/isolate controls on every presentation target.
fn is_not_emitted(c: char) -> bool {
    matches!(c, '\u{7f}'..='\u{9f}' | '\r') || is_bidi_control(c)
}

/// A renderer's whitespace terminal: U+0020 and U+0009, and NOTHING ELSE.
///
/// `blank_line = {whitespace}` takes a space or a tab (PART 1, carve#890), and
/// PART 2's NO TRAILING WHITESPACE drops the same two (carve#926). Every other
/// character is CONTENT and has to reach the output, however invisible - a
/// no-break space, an OGHAM SPACE MARK, an EN QUAD, a THIN SPACE, a NARROW
/// NO-BREAK SPACE, a MEDIUM MATHEMATICAL SPACE, an IDEOGRAPHIC SPACE, a
/// zero-width space, a FORM FEED and a VERTICAL TAB.
///
/// The two LINE TERMINATORS are in the set as well, and that is a different
/// job: these helpers also trim the newlines around a rendered block, which is
/// layout rather than line content. A form feed is NOT a terminator here - it
/// is content that PART 2 keeps - so the set is named rather than spelled
/// `is_ascii_whitespace`, which would take it.
///
/// This was the Unicode whitespace PROPERTY with U+00A0 carved out by hand,
/// which is the shape the rule keeps being written in wrongly: the one
/// character anyone thinks of survived and the other eight did not, so a line
/// holding one of them was written back EMPTY and reparsed as a blank - which
/// split its paragraph in two and lost the character. Naming the two terminal
/// characters removes the exception along with the defect.
///
/// It lives HERE, shared, because `str::trim` is the default reach for "drop
/// the layout around this rendered fragment", and `str::trim` takes
/// `char::is_whitespace`, U+00A0 included. The canonical writer learned that
/// once and kept its own copy; the plain-text, Markdown and ANSI writers each
/// went on calling `.trim()` on footnote-definition bodies, table cells and a
/// caption, so the same character was preserved on one target and deleted on
/// three by the same engine (carve-rs#614 fixed the Markdown heading case
/// alone). One spelling, in the module the presentation renderers already
/// share for exactly this.
pub(crate) fn trim_non_nbsp(text: &str) -> &str {
    text.trim_matches([' ', '\t', '\n', '\r'])
}

/// `trim_non_nbsp`, at the end only.
pub(crate) fn trim_end_non_nbsp(text: &str) -> &str {
    text.trim_end_matches([' ', '\t', '\n', '\r'])
}
