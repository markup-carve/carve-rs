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
        BlockNode::CitationDefinition(d) => collect_inlines(&d.children, suppressed, out),
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
            match &*f.target {
                FigureTarget::Image(_) | FigureTarget::CodeBlock(_) => {}
                FigureTarget::BlockQuote(b) => collect_block_quote(b, suppressed, out),
                FigureTarget::Table(t) => collect_table(t, suppressed, out),
                FigureTarget::Paragraph(p) => collect_inlines(&p.children, suppressed, out),
            }
            collect_inlines(&f.caption, suppressed, out);
        }
        BlockNode::FigureGroup(g) => {
            collect_blocks(&g.children, suppressed, out);
            if let Some(caption) = &g.caption {
                collect_inlines(caption, suppressed, out);
            }
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
            InlineNode::Link(l) => {
                // An UNRESOLVED reference link is emitted as its raw source on
                // both of these targets - the same `ref_label.is_some() &&
                // href.is_empty()` test they each apply - so nothing below it
                // reaches the output and an abbreviation in its label expands
                // nowhere. Counting it lost the expansion outright:
                // `*[HTML]: Long Form` with `[HTML][missing]` as the only
                // occurrence dropped the definition line and printed
                // `[HTML][missing]`, with "Long Form" in neither place.
                if l.ref_label.is_some() && l.href.is_empty() {
                    continue;
                }
                collect_inlines(&l.children, suppressed, out);
            }
            InlineNode::Extension(e) => collect_inlines(&e.children, suppressed, out),
            InlineNode::Footnote(f) => {
                if let Some(inline) = &f.inline {
                    collect_inlines(inline, suppressed, out);
                }
            }
            InlineNode::CriticInsert(c) => collect_inlines(&c.children, suppressed, out),
            InlineNode::CriticDelete(c) => collect_inlines(&c.children, suppressed, out),
            // A CITATION GROUP is emitted as `raw` on both targets, whether or
            // not its items resolved, so its parsed prefix, locator and suffix
            // never reach the output either. An abbreviation the Citations
            // extension resolved inside a suffix expands on HTML and on nothing
            // else, so counting it dropped the definition line on the two
            // targets that could not print the expansion. Skipping the subtree
            // is the same rule as the unresolved reference link above.
            InlineNode::CitationGroup(_) => {}
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
pub(crate) fn trim_non_nbsp(text: &str) -> &str {
    text.trim_matches([' ', '\t', '\n', '\r'])
}

/// `trim_non_nbsp`, at the end only.
pub(crate) fn trim_end_non_nbsp(text: &str) -> &str {
    text.trim_end_matches([' ', '\t', '\n', '\r'])
}
