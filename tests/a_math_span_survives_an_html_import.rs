//! A math span survives an HTML import, inline and display alike.
//!
//! `<span class="math inline">\(x\)</span>` is what this engine's HTML renderer
//! writes for `` $`x` `` (PART 9 §18: `math_inline = '$', code_span`), and what
//! djot.js and pandoc write too. The importer had no arm for it, so it fell
//! through to the generic attributed-span writer and the equation came back as
//! `[\\(x\\)]{.math .inline role=math}` - no diagnostic, and no `math` node
//! (carve-rs#1208, after markup-carve/carve-php#1546 and
//! markup-carve/carve-js#1295).
//!
//! WHY THE OBVIOUS CHECK MISSED IT, and why every assertion below re-parses.
//! Re-rendering that span produces byte-identical HTML: a span carrying the
//! same classes renders the same tag, so an HTML-to-HTML comparison reports
//! success on the broken import. What is lost is the NODE - and with it every
//! non-HTML target, each of which has a math case it can no longer reach. The
//! `render_markdown` case below is the one that could see the defect at all,
//! and `bytes_alone_cannot_see_this_defect` pins the trap itself so a later
//! reader does not re-introduce a byte assertion here.
//!
//! Recognition needs TWO signals to agree, the class pair and a matching
//! `\(…\)` / `\[…\]` payload, so neither a stylesheet class named `math` nor an
//! escaped paren in prose can turn text into an equation on its own. The
//! controls at the bottom hold on BOTH sides of the fix.
//!
//! The BLOCK form, `<div class="math display">`, was deliberately absent while
//! markup-carve/carve#1514 was open on which Carve spelling it takes, and this
//! file recorded the gap as a measurement rather than a guess.
//! markup-carve/carve#1518 ruled it: the CORE `$$` form, never the
//! ```` ```math ```` extension fence, because an importer must not emit a
//! construct whose meaning depends on the consumer's configuration.
//! `the_block_form_imports_as_the_core_display_math` is the rewritten
//! placeholder, and it carries the argument the fence lost.

use carve::*;

/// The node kinds of a document, depth first. The unit of every assertion here:
/// the defect is a lost NODE, and only a tree can see it.
fn kinds(doc: &Document) -> Vec<String> {
    fn inline(n: &InlineNode, out: &mut Vec<String>) {
        match n {
            InlineNode::Math(m) => out.push(format!(
                "math[{}]:{}",
                if m.display { "display" } else { "inline" },
                m.content
            )),
            InlineNode::Span(s) => {
                out.push("span".into());
                for c in &s.children {
                    inline(c, out);
                }
            }
            InlineNode::Emphasis(e) => {
                out.push(format!("emphasis[{:?}]", e.kind));
                for c in &e.children {
                    inline(c, out);
                }
            }
            other => out.push(
                format!("{other:?}")
                    .split('(')
                    .next()
                    .unwrap_or_default()
                    .to_lowercase(),
            ),
        }
    }
    fn block(n: &BlockNode, out: &mut Vec<String>) {
        match n {
            BlockNode::Paragraph(p) => {
                out.push("paragraph".into());
                for c in &p.children {
                    inline(c, out);
                }
            }
            BlockNode::Div(d) => {
                out.push("div".into());
                for c in &d.children {
                    block(c, out);
                }
            }
            other => out.push(
                format!("{other:?}")
                    .split('(')
                    .next()
                    .unwrap_or_default()
                    .to_lowercase(),
            ),
        }
    }
    let mut out = Vec::new();
    for n in &doc.children {
        block(n, &mut out);
    }
    out
}

fn imported(html: &str) -> HtmlImportResult<Document> {
    html_to_ast(html, &HtmlImportOptions::default()).expect("import")
}

/// The imported document as Carve source, which is what a caller of
/// `html_to_carve` actually receives.
fn written(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .expect("import")
        .value
}

/// Whether the imported tree holds a `math` node anywhere. The controls all ask
/// this one question: two signals agreed, or they did not.
fn has_math(html: &str) -> bool {
    kinds(&imported(html).value)
        .iter()
        .any(|k| k.starts_with("math["))
}

fn codes(html: &str) -> Vec<String> {
    imported(html)
        .report
        .diagnostics
        .iter()
        .map(|d| format!("{:?}", d.code))
        .collect()
}

/// THE TICKET'S MEASUREMENT. On `main` the imported tree held `span` and the
/// written source read `Einstein said [\\(E = mc^2\\)]{.math .inline role=math}
/// today.`, with no diagnostic naming the loss.
#[test]
fn carves_own_math_html_reads_back_as_a_math_node() {
    let source = "Einstein said $`E = mc^2` today.";
    let html = render_html(&parse(source)).expect("render");
    assert_eq!(
        html,
        "<p>Einstein said <span class=\"math inline\" role=\"math\">\\(E = mc^2\\)</span> today.</p>"
    );
    assert_eq!(kinds(&imported(&html).value), kinds(&parse(source)));
    assert_eq!(written(&html), "Einstein said $`E = mc^2` today.\n");
    assert_eq!(codes(&html), Vec::<String>::new());
}

/// The display twin, lost the same way and recovered the same way. The class
/// decides which delimiter pair is the evidence, so this is not the inline case
/// with a different flag - `\[…\]` is only display math because the class says
/// `display`.
#[test]
fn the_display_twin_reads_back_as_display_math() {
    let source = "a $$`x^2` b";
    let html = render_html(&parse(source)).expect("render");
    assert_eq!(
        html,
        "<p>a <span class=\"math display\" role=\"math\">\\[x^2\\]</span> b</p>"
    );
    assert_eq!(kinds(&imported(&html).value), kinds(&parse(source)));
    assert_eq!(written(&html), "a $$`x^2` b\n");
}

/// Recognition CONSUMES the four attributes it read - the `math` class, the
/// `inline`/`display` class and a `role="math"` - the same bargain `attrs`
/// already strikes for `<math>`'s `xmlns`, because the renderer writes all four
/// back from the node. Everything else the author wrote rides along.
#[test]
fn the_authors_attributes_survive_and_the_recognized_ones_are_not_spelled_twice() {
    let source = "a $`x^2`{#e .tall data-k=v} b";
    let html = render_html(&parse(source)).expect("render");
    assert_eq!(written(&html), "a $`x^2`{#e .tall data-k=v} b\n");
    // The full circle: same HTML out, from a tree that says `math`.
    assert_eq!(
        render_html(&parse(&written(&html))).expect("render"),
        html,
        "the attributes have to come back in the same slots"
    );
    // A `role` that is NOT `math` is the author's and stays; so does an id the
    // renderer did not put there.
    assert_eq!(
        written(r#"<p>a <span class="math inline" role="button" id="q">\(x\)</span> b</p>"#),
        "a $`x`{#q role=button} b\n"
    );
    // The FIRST of each class only. `class="math math inline"` keeps the second
    // `math` as an author class, because the renderer writes the base pair once.
    assert_eq!(
        written(r#"<p>a <span class="math math inline">\(x\)</span> b</p>"#),
        "a $`x`{.math} b\n"
    );
}

/// THE ROW THAT COULD SEE THE DEFECT AT ALL. Every non-HTML writer has a math
/// case and none of them could reach it: the tree said `span`, so Markdown
/// wrote the TeX delimiters as prose. This is the assertion that fails loudest
/// if the recognition is ever removed.
#[test]
fn the_non_html_writers_see_math_again() {
    let html = render_html(&parse("a $`x^2` b")).expect("render");
    let tree = imported(&html).value;
    let markdown = render_markdown(&tree).expect("markdown");
    assert_eq!(
        markdown,
        render_markdown(&parse("a $`x^2` b")).expect("markdown"),
        "the imported tree has to write like the tree it came from"
    );
    // On `main` this read `a \\(x^2\\) b` - the delimiters as running text.
    assert!(
        !markdown.contains("\\\\("),
        "escaped TeX delimiters leaked into Markdown: {markdown}"
    );
}

/// THE TRAP, PINNED. An HTML-to-HTML comparison passes on the broken import,
/// which is why it hid for as long as it did. Re-rendering the generic span
/// this engine used to build gives back byte-for-byte the HTML it was built
/// from, so a byte assertion here would agree with either behavior and pin
/// neither. Anyone tempted to "simplify" this file into a byte check should
/// read this test first.
#[test]
fn bytes_alone_cannot_see_this_defect() {
    let html = render_html(&parse("a $`x^2` b")).expect("render");
    // The exact node the importer built BEFORE the fix, spelled by hand.
    // The escaped BACKSLASH is what the importer wrote: `\\(` in Carve source
    // renders the literal `\(` the delimiter convention asks for.
    let as_a_generic_span = parse("a [\\\\(x^2\\\\)]{.math .inline role=math} b");
    assert_eq!(
        render_html(&as_a_generic_span).expect("render"),
        html,
        "if this ever differs, a byte check would have caught the bug and this \
         test can go"
    );
    // ... and the tree does not agree for a moment.
    assert_ne!(kinds(&as_a_generic_span), kinds(&parse("a $`x^2` b")));
}

/// Carve's math content is a `code_span`, one line by construction, so a
/// payload a pretty-printer folded across lines has exactly one spelling: the
/// whitespace run collapsed the way every other imported text run is. TeX reads
/// a newline as whitespace, so the equation is unchanged - and a `math` node
/// holding a newline would not be writable at all.
#[test]
fn a_folded_payload_collapses_to_one_line() {
    let written = written("<p>a <span class=\"math inline\">  \\( x\n   ^2 \\)  </span> b</p>");
    assert_eq!(written, "a $`x ^2` b\n");
    assert_eq!(
        kinds(&parse(&written)),
        vec!["paragraph", "text", "math[inline]:x ^2", "text"]
    );
}

/// CONTROL - THE CLASS ALONE IS NOT EVIDENCE. A stylesheet is free to name a
/// class `math`, and a span carrying one is prose. Holds on both sides of the
/// fix.
#[test]
fn control_a_math_class_without_the_delimiters_is_still_a_span() {
    let html = r#"<p>a <span class="math inline">plain</span> b</p>"#;
    assert_eq!(written(html), "a [plain]{.math .inline} b\n");
    assert!(!has_math(html));
}

/// CONTROL - THE DELIMITERS ALONE ARE NOT EVIDENCE. PART 9 §18 spells the input
/// form as a `$` prefix on a code span and has no `\(…\)` form, so `\(x\)` in
/// running prose is a pair of escaped parens. Holds on both sides of the fix.
#[test]
fn control_delimiters_without_the_class_pair_are_escaped_parens() {
    let html = r#"<p>a <span class="promo">\(x\)</span> b</p>"#;
    assert_eq!(written(html), "a [\\\\(x\\\\)]{.promo} b\n");
    assert!(!has_math(html));
}

/// CONTROL - THE TWO SIGNALS HAVE TO AGREE WITH EACH OTHER, not merely both be
/// present. The class says which delimiter to expect, so a `math display` span
/// holding `\(…\)` is left alone rather than quietly re-labeled as display
/// math. Holds on both sides of the fix.
#[test]
fn control_a_class_and_a_payload_that_disagree_are_left_alone() {
    let html = r#"<p>a <span class="math display" role="math">\(x\)</span> b</p>"#;
    assert_eq!(
        written(html),
        "a [\\\\(x\\\\)]{.math .display role=math} b\n"
    );
    // And neither class, or both at once, names no shape the renderer writes.
    for classes in ["math", "math inline display"] {
        let html = format!(r#"<p>a <span class="{classes}">\(x\)</span> b</p>"#);
        assert!(!has_math(&html), "class={classes:?} should not be math");
    }
}

/// CONTROL - THERE IS NO EMPTY MATH. `\(\)` carries the delimiters and no
/// equation, and a `math` node with empty content is not what the author wrote.
/// Holds on both sides of the fix.
#[test]
fn control_an_empty_payload_is_not_math() {
    for html in [
        r#"<p>a <span class="math inline" role="math">\(\)</span> b</p>"#,
        r#"<p>a <span class="math inline" role="math">\(   \)</span> b</p>"#,
        r#"<p>a <span class="math inline" role="math">\(</span> b</p>"#,
    ] {
        assert!(!has_math(html), "{html} should not be math");
    }
}

/// CONTROL - AN ELEMENT CHILD ENDS THE READ. The payload is taken off the
/// DIRECT children and never through the recursive text walk: a delimiter
/// payload is text, and a span holding markup is a span holding markup. This is
/// also what keeps the recognition free of the recursion `flat_text` exists to
/// avoid on the `<math>` arm.
#[test]
fn control_an_element_child_ends_the_payload_read() {
    let html = r#"<p>a <span class="math inline" role="math">\(<b>x</b>\)</span> b</p>"#;
    assert!(!has_math(html));
    assert!(
        kinds(&imported(html).value).contains(&"emphasis[Strong]".to_string()),
        "the markup inside stays markup"
    );
}

/// WHICH ARM A SPAN TAKES MUST NOT CHANGE WHAT THE LIMITS SEE. The recognition
/// runs AFTER the children have been walked, so `max_nodes` has already been
/// charged for the whole subtree - the reason the `<math>` arm, which returns
/// without walking, has to call `charge_subtree` by hand.
///
/// Asserted as an EQUALITY between two structurally identical spans rather than
/// against a number: the budget a math span costs is the budget its non-math
/// twin costs, whatever that number happens to be.
#[test]
fn the_recognition_costs_the_same_budget_as_the_span_it_replaces() {
    let math = r#"<p>a <span class="math inline" role="math">\(x\)</span> b</p>"#;
    let twin = r#"<p>a <span class="mass inline" role="math">\(x\)</span> b</p>"#;
    let ceiling = |html: &str| {
        // The smallest `max_nodes` at which this import succeeds.
        (1..64)
            .find(|n| {
                html_to_ast(
                    html,
                    &HtmlImportOptions {
                        max_nodes: *n,
                        ..HtmlImportOptions::default()
                    },
                )
                .is_ok()
            })
            .expect("some budget admits it")
    };
    assert_eq!(
        ceiling(math),
        ceiling(twin),
        "recognizing a span as math must not make it cheaper or dearer"
    );
}

/// THE SIBLING DEFECT, MEASURED RATHER THAN ASSUMED. Carve math is a PREFIX on
/// a code span with no closing delimiter, so a writer that appends a trailing
/// `$` emits the next character of the paragraph instead.
/// markup-carve/carve-php#1546 found exactly that on its MathML path, with
/// fourteen byte assertions agreeing because none re-read the result;
/// markup-carve/carve-js#1295 measured that it never had it. This engine does
/// not have it either - and this test re-parses instead of trusting the bytes,
/// which is the only reading that can tell.
#[test]
fn the_mathml_path_writes_no_trailing_delimiter() {
    let html = "<p>a <math><semantics><annotation encoding=\"application/x-tex\">x^2\
                </annotation></semantics></math> b</p>";
    let written = written(html);
    assert_eq!(written, "a $`x^2` b\n");
    assert_eq!(
        kinds(&parse(&written)),
        vec!["paragraph", "text", "math[inline]:x^2", "text"],
        "a trailing delimiter would re-parse as a stray sigil in the text run"
    );
}

/// THE BLOCK FORM TAKES THE CORE `$$` SPELLING, NEVER THE EXTENSION FENCE.
///
/// THIS TEST WAS `unimplemented_block_form_is_left_for_the_open_ruling`, and it
/// asserted that `<div class="math display">\[…\]</div>` came back as a Carve
/// div holding the delimiters as text - the same loss the inline span had. That
/// was deliberate: markup-carve/carve-php#1546 imported the div as the
/// ` ```math ` fence, because that fence is what wrote the HTML and the round
/// trip is then exact, and markup-carve/carve-js#1295 imported it as the core
/// `$$` form, because the fence is an extension. Both reasons were good, PART 9
/// §18 said nothing, and a third engine picking a third spelling was what the
/// ruling existed to prevent - so this engine measured its gap instead of
/// guessing.
///
/// markup-carve/carve#1518 ruled it (from markup-carve/carve#1514): the CORE
/// form. The fence lost because it is an EXTENSION - with it not loaded the
/// same imported document is a `language-math` code block rather than an
/// equation, so the file would be mathematics for one consumer and code for
/// another. `math_display` is core and needs nothing loaded. The round-trip
/// argument is real and is not the job: an importer produces a document that
/// MEANS what the HTML meant, and it cannot know an extension generated the
/// HTML at all. Emitting the fence only when the extension is registered was
/// rejected on purpose - it makes two runs of the same tool over the same input
/// produce different documents.
#[test]
fn the_block_form_imports_as_the_core_display_math() {
    let html = r#"<div class="math display">\[x^2\]</div>"#;
    assert_eq!(
        kinds(&imported(html).value),
        vec!["paragraph", "math[display]:x^2"],
        "a paragraph holding one display math node, not a div and not a code block"
    );
    assert_eq!(written(html), "$$`x^2`\n");
    // Never the fence, which is the half of the ruling a tree check cannot see:
    // a `code_block` would also be one block, and `kinds` would call it that,
    // but the SOURCE is where the extension dependency lives.
    assert!(!written(html).contains("```"));
    // And the written source re-reads as the node it came from, which is the
    // equality that makes the spelling a document rather than a string.
    assert_eq!(kinds(&parse(&written(html))), kinds(&imported(html).value));

    // The INLINE half of the same shape, where all three engines already
    // agreed, is unchanged by the block form landing.
    let span = r#"<p><span class="math display" role="math">\[x^2\]</span></p>"#;
    assert_eq!(
        kinds(&imported(span).value),
        vec!["paragraph", "math[display]:x^2"]
    );
}

/// The CLASS decides the mode, not the position the element was found in. A div
/// spelled `math inline` writes the inline form; under a ` ```math ` fence it
/// could only ever have been display, because that fence has no other mode.
#[test]
fn a_block_element_spelled_inline_writes_the_inline_form() {
    let html = r#"<div class="math inline">\(x^2\)</div>"#;
    assert_eq!(
        kinds(&imported(html).value),
        vec!["paragraph", "math[inline]:x^2"]
    );
    assert_eq!(written(html), "$`x^2`\n");
}

/// The author's own attributes ride the math NODE, and the two classes that
/// SPELL the math are consumed by the spelling - exactly as on the span.
#[test]
fn the_block_form_keeps_the_authors_attributes() {
    let html = r#"<div id="eq" class="math display big" data-k="v">\[x\]</div>"#;
    assert_eq!(written(html), "$$`x`{#eq .big data-k=v}\n");
    assert_eq!(
        kinds(&parse(&written(html))),
        vec!["paragraph", "math[display]:x"]
    );
}

/// THE SHARED IMPORT CONTRACT'S MATH DOCUMENT, byte for byte:
/// `tests/html-import/math-block-and-mathml` in the spec repo carries the div,
/// a block `<math>` and an inline `<math>` in one input, because a stray `$$`
/// has no render difference to find and only the bytes can see it.
#[test]
fn the_shared_import_contracts_math_document() {
    let html = r#"<div class="math display">\[E = mc^2\]</div>"#.to_string()
        + r#"<math display="block" alttext="a - b"></math>"#
        + r#"<p>x <math alttext="c + d"></math> y</p>"#;
    assert_eq!(
        written(&html),
        "$$`E = mc^2`\n\n$$`a - b`\n\nx $`c + d` y\n"
    );
}

/// The converter corpus case the ruling pins, `33-html-block-math-imports-as-
/// the-core-form`, which compares the RENDER of the produced Carve - the only
/// place the two spellings of the div differ at all. Under the fence this
/// rendered `<pre><code class="language-math">`.
#[test]
fn the_block_form_renders_back_as_an_equation_with_no_extension_loaded() {
    let written = written(r#"<div class="math display">\[E = mc^2\]</div>"#);
    assert_eq!(
        render_html(&parse(&written)).expect("render").trim(),
        r#"<p><span class="math display" role="math">\[E = mc^2\]</span></p>"#
    );
}

/// CONTROLS ON THE BLOCK FORM - the same two signals, and they hold on BOTH
/// sides of the fix. A div carrying only the class, or only the delimiters, or
/// a payload its class disagrees with, or no payload at all, is a div.
#[test]
fn control_a_block_element_needs_both_signals_too() {
    for html in [
        r#"<div class="math display">x^2</div>"#,
        r#"<div class="display">\[x^2\]</div>"#,
        r#"<div class="math">\[x^2\]</div>"#,
        r#"<div class="math display">\(x^2\)</div>"#,
        r#"<div class="math inline display">\[x^2\]</div>"#,
        r#"<div class="math display">\[\]</div>"#,
        r#"<div class="math display">\[<b>x</b>\]</div>"#,
    ] {
        assert!(!has_math(html), "two signals must agree: {html}");
        assert!(
            !written(html).contains("$`"),
            "and nothing writes math: {html}"
        );
    }
}

/// WHICH ARM A DIV TAKES MUST NOT CHANGE WHAT THE LIMITS SEE. The block arm
/// returns WITHOUT walking its children, so unlike the span - which is
/// recognized after `inlines` has already charged the subtree - it has to call
/// `charge_subtree` by hand. A recognition that read text recursively before
/// charging would let crafted HTML reach the stack ahead of the limit meant to
/// stop it; the payload read is `direct_text`, which is bounded and stops at
/// the first element child.
///
/// Asserted as an EQUALITY between two structurally identical divs rather than
/// against a number, the way the span's budget test is - and on BOTH limits,
/// because the arm skips a traversal and a skipped traversal is exactly where a
/// depth could go uncounted. Nested rows too: at the top level the two limits
/// can agree by accident.
#[test]
fn the_block_recognition_costs_the_same_budget_as_the_div_it_replaces() {
    let nodes = |html: &str| {
        (1..64)
            .find(|n| {
                html_to_ast(
                    html,
                    &HtmlImportOptions {
                        max_nodes: *n,
                        ..HtmlImportOptions::default()
                    },
                )
                .is_ok()
            })
            .expect("some node budget admits it")
    };
    let depth = |html: &str| {
        (1..64)
            .find(|n| {
                html_to_ast(
                    html,
                    &HtmlImportOptions {
                        max_depth: *n,
                        ..HtmlImportOptions::default()
                    },
                )
                .is_ok()
            })
            .expect("some depth admits it")
    };
    for (math, twin) in [
        (
            r#"<div class="math display">\[x\]</div>"#,
            r#"<div class="mass display">\[x\]</div>"#,
        ),
        (
            r#"<blockquote><div class="math display">\[x\]</div></blockquote>"#,
            r#"<blockquote><div class="mass display">\[x\]</div></blockquote>"#,
        ),
        (
            r#"<div><div class="math display">\[x\]</div></div>"#,
            r#"<div><div class="mass display">\[x\]</div></div>"#,
        ),
    ] {
        assert_eq!(
            nodes(math),
            nodes(twin),
            "recognizing a div as math must not make it cheaper or dearer: {math}"
        );
        assert_eq!(
            depth(math),
            depth(twin),
            "nor must it change the depth the limit sees: {math}"
        );
    }
}
