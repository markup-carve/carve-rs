//! `roundtrip` rebuilds a foreign `<figure>` when a Carve spelling reproduces
//! the element, raw-preserves it when none does, and loses nothing silently
//! either way (markup-carve/carve#1704).
//!
//! The mode used to raw-preserve EVERY figure. The rationale for that was
//! sound and still is - a figure around a bare paragraph writes `x` then
//! `^ Cap`, which reads back as one paragraph with the caption as literal
//! prose, and one around a list detaches the caption - but it was applied to
//! targets it did not describe. An image, a code block and a quote each write
//! a caption line the parser reads back as the same figure, so preserving them
//! bought no fidelity at all: it turned the most common input of the whole
//! family, a captioned image, into an opaque `=html` block for a loss that was
//! not there.
//!
//! What the rule pins is therefore the PROPERTY, not a list of blessed tag
//! names, so a caption target added later inherits it instead of needing
//! another sweep to discover it. `semantic` is unaffected: being lossy is what
//! distinguishes the two modes, and every `semantic` control here says so.
//!
//! ONE CARVE-OUT, DELIBERATE. `<figure><table>…<figcaption>` has no spelling
//! that reproduces it - the rebuild reads back as `<table id="t">` with a
//! `<caption>` - so strictly it would raw-preserve. It rebuilds anyway, with
//! its existing warning, because `<table><caption>` is the idiomatic HTML for
//! a captioned table and raw-preserving would throw the `| a |` spelling away
//! for a common shape.

use carve::{html_to_carve, to_html, HtmlImportDiagnosticCode, HtmlImportMode, HtmlImportOptions};

struct Imported {
    carve: String,
    codes: Vec<HtmlImportDiagnosticCode>,
}

fn import(html: &str, mode: HtmlImportMode) -> Imported {
    let options = HtmlImportOptions {
        mode,
        ..HtmlImportOptions::default()
    };
    let result = html_to_carve(html, &options).expect("imports");
    Imported {
        carve: result.value,
        codes: result
            .report
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect(),
    }
}

/// The three targets whose caption line re-parses. Each REBUILDS, with nothing
/// to report, and the proof is the round trip rather than the string: the
/// rebuilt Carve renders back to the element the import was handed.
#[test]
fn a_target_a_caption_line_reproduces_rebuilds_and_reports_nothing() {
    let mut wrong = Vec::new();
    for html in [
        "<figure id=\"f\"><img src=\"a.png\" alt=\"A\"><figcaption>Cap</figcaption></figure>",
        "<figure id=\"c\"><pre><code>x</code></pre><figcaption>Cap</figcaption></figure>",
        "<figure id=\"q\"><blockquote><p>a</p></blockquote><figcaption>Cap</figcaption></figure>",
    ] {
        let imported = import(html, HtmlImportMode::Roundtrip);
        if imported.carve.contains("=html") {
            wrong.push(format!("{html}\n  raw-preserved: {:?}", imported.carve));
            continue;
        }
        if !imported.codes.is_empty() {
            wrong.push(format!("{html}\n  reported: {:?}", imported.codes));
        }
        // THE PROPERTY, not the spelling. A figure element in, the same figure
        // element out - which is what "a Carve spelling reproduces it" means
        // and the only reason the rebuild is allowed here at all.
        let rendered = to_html(&imported.carve);
        if !rendered.contains("<figure") || !rendered.contains("<figcaption>Cap</figcaption>") {
            wrong.push(format!(
                "{html}\n  did not read back as a figure: {rendered}"
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "these must rebuild in roundtrip and report nothing:\n{}",
        wrong.join("\n")
    );
}

/// THE CARVE-OUT. A table target does not read back as a figure, and it
/// rebuilds regardless - but not silently: the `structure-unspellable` row it
/// already carried is what keeps this inside the rule's second half.
#[test]
fn a_table_target_rebuilds_as_a_captioned_table_and_says_so() {
    let html =
        "<figure id=\"t\"><table><tr><td>a</td></tr></table><figcaption>Cap</figcaption></figure>";
    let imported = import(html, HtmlImportMode::Roundtrip);
    assert_eq!(imported.carve, "{#t}\n| a |\n^ Cap\n");
    assert_eq!(
        imported.codes,
        vec![HtmlImportDiagnosticCode::StructureUnspellable],
        "the carve-out is only allowed while it is reported"
    );
    // What the carve-out costs, measured rather than asserted from memory: the
    // element that comes back is the captioned TABLE, not the figure.
    let rendered = to_html(&imported.carve);
    assert!(
        rendered.contains("<table id=\"t\">") && rendered.contains("<caption>Cap</caption>"),
        "the rebuild should read back as a captioned table: {rendered}"
    );
    assert!(
        !rendered.contains("<figure"),
        "if this ever reads back as a figure the carve-out is no longer a carve-out: {rendered}"
    );
}

/// The two targets NO spelling reproduces. Both keep the element, and both say
/// they did - this is the half of the rule carve#1286 established and #1704
/// left standing.
#[test]
fn a_target_no_spelling_reproduces_is_preserved_and_warned() {
    let mut wrong = Vec::new();
    for html in [
        "<figure id=\"l\"><ul><li>a</li></ul><figcaption>Cap</figcaption></figure>",
        "<figure id=\"g\"><p>x</p><figcaption>Cap</figcaption></figure>",
    ] {
        let imported = import(html, HtmlImportMode::Roundtrip);
        if !imported.carve.contains("```=html") || !imported.carve.contains(html) {
            wrong.push(format!("{html}\n  not preserved: {:?}", imported.carve));
        }
        if imported.codes != vec![HtmlImportDiagnosticCode::RawPreserved] {
            wrong.push(format!("{html}\n  reported: {:?}", imported.codes));
        }
    }
    assert!(
        wrong.is_empty(),
        "these must be preserved with exactly the raw-preserved warning:\n{}",
        wrong.join("\n")
    );
}

/// A CAPTION THAT COMES FIRST IS NOT REPRODUCED EITHER. A Carve caption line
/// follows its target, so `render_figure` writes the `<figcaption>` last and
/// nothing spells a figure that opens with one. The rebuild produces a perfectly
/// good figure whose re-render has MOVED the caption to the end - well-formed
/// bytes, a silent reorder, and exactly the failure this rule exists to stop.
#[test]
fn a_figure_whose_caption_comes_first_is_preserved() {
    let html =
        "<figure id=\"f\"><figcaption>Cap</figcaption><img src=\"a.png\" alt=\"A\"></figure>";
    let imported = import(html, HtmlImportMode::Roundtrip);
    assert!(
        imported.carve.contains("```=html") && imported.carve.contains(html),
        "the caption-first figure was rebuilt instead of preserved: {:?}",
        imported.carve
    );
    assert_eq!(imported.codes, vec![HtmlImportDiagnosticCode::RawPreserved]);
    // The measurement behind the rule: rebuilt, this is what came back.
    let rebuilt = import(html, HtmlImportMode::Semantic);
    assert_eq!(rebuilt.carve, "{#f}\n![A](a.png)\n^ Cap\n");
    assert!(
        to_html(&rebuilt.carve).find("<figcaption").unwrap()
            > to_html(&rebuilt.carve).find("<img").unwrap(),
        "this fixture only means something while the renderer writes the caption last"
    );
}

/// A BODY TOO DEEP TO WALK TAKES THE SAME EXIT. Before the per-target rule,
/// `roundtrip` preserved a figure without walking it, so a body past `max_depth`
/// imported fine as an opaque block. Deciding per target means walking, and
/// failing the whole document for a figure that was going to be preserved anyway
/// would be a regression bought with the fix: an element that cannot be walked
/// is an element no rebuild reproduces.
#[test]
fn a_figure_too_deep_to_rebuild_is_preserved_rather_than_failing_the_import() {
    let body = "<div>".repeat(200) + "x" + &"</div>".repeat(200);
    let html = format!("<figure id=\"d\">{body}<figcaption>Cap</figcaption></figure>");
    let options = HtmlImportOptions {
        mode: HtmlImportMode::Roundtrip,
        ..HtmlImportOptions::default()
    };
    let imported = html_to_carve(&html, &options).expect("a figure past max_depth still imports");
    assert!(
        imported.value.contains("```=html"),
        "the over-deep figure was not preserved: {:?}",
        imported.value
    );
    // CONTROL: the limit itself is untouched. `semantic` has to rebuild, so it
    // still refuses the same document - a fix that raised the bound instead of
    // taking a different exit would show up here.
    let semantic = HtmlImportOptions {
        mode: HtmlImportMode::Semantic,
        ..HtmlImportOptions::default()
    };
    assert!(
        html_to_carve(&html, &semantic).is_err(),
        "semantic must still reach the depth limit on this document"
    );
}

/// A REJECTED REBUILD LEAVES NO TRACE. The raw-preserve arm runs only after
/// `figure_panel` has walked the subtree, so every row that walk pushed
/// describes a tree this mode then throws away. A figure around a LIST loses
/// its `id` in the rebuild and says so twice; preserved, it loses nothing, and
/// reporting those rows anyway would describe losses the output does not have.
#[test]
fn the_rows_a_rejected_rebuild_pushed_do_not_reach_the_report() {
    let html = "<figure id=\"l\"><ul><li>a</li></ul><figcaption>Cap</figcaption></figure>";
    let semantic = import(html, HtmlImportMode::Semantic);
    assert_eq!(
        semantic.codes,
        vec![
            HtmlImportDiagnosticCode::ElementUnwrapped,
            HtmlImportDiagnosticCode::AttributeDropped,
        ],
        "this test only means something while the rebuild reports these"
    );
    let roundtrip = import(html, HtmlImportMode::Roundtrip);
    assert_eq!(
        roundtrip.codes,
        vec![HtmlImportDiagnosticCode::RawPreserved],
        "the rejected rebuild's rows leaked into the report"
    );
}

/// CONTROL: `semantic` is untouched by all of this. It rebuilds every target,
/// including the two `roundtrip` now preserves, and keeps exactly the
/// diagnostics it had.
#[test]
fn semantic_still_rebuilds_every_target() {
    let cases: [(&str, &str, Vec<HtmlImportDiagnosticCode>); 6] = [
        (
            "<figure id=\"f\"><img src=\"a.png\" alt=\"A\"><figcaption>Cap</figcaption></figure>",
            "{#f}\n![A](a.png)\n^ Cap\n",
            vec![],
        ),
        (
            "<figure id=\"c\"><pre><code>x</code></pre><figcaption>Cap</figcaption></figure>",
            "{#c}\n```\nx\n```\n^ Cap\n",
            vec![],
        ),
        (
            "<figure id=\"q\"><blockquote><p>a</p></blockquote><figcaption>Cap</figcaption></figure>",
            "{#q}\n> a\n^ Cap\n",
            vec![],
        ),
        (
            "<figure id=\"t\"><table><tr><td>a</td></tr></table><figcaption>Cap</figcaption></figure>",
            "{#t}\n| a |\n^ Cap\n",
            vec![HtmlImportDiagnosticCode::StructureUnspellable],
        ),
        (
            "<figure id=\"l\"><ul><li>a</li></ul><figcaption>Cap</figcaption></figure>",
            "- a\n\nCap\n",
            vec![
                HtmlImportDiagnosticCode::ElementUnwrapped,
                HtmlImportDiagnosticCode::AttributeDropped,
            ],
        ),
        (
            "<figure id=\"g\"><p>x</p><figcaption>Cap</figcaption></figure>",
            "{#g}\nx\n^ Cap\n",
            vec![HtmlImportDiagnosticCode::StructureUnspellable],
        ),
    ];
    let mut wrong = Vec::new();
    for (html, expected, codes) in cases {
        let imported = import(html, HtmlImportMode::Semantic);
        if imported.carve != expected {
            wrong.push(format!(
                "{html}\n  wrote {:?}, want {expected:?}",
                imported.carve
            ));
        }
        if imported.codes != codes {
            wrong.push(format!(
                "{html}\n  reported {:?}, want {codes:?}",
                imported.codes
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "semantic must be unchanged by the roundtrip ruling:\n{}",
        wrong.join("\n")
    );
}

/// THE SEVENTH SHAPE THE RULING NAMES, AND THE ONE THIS ENGINE CANNOT REACH.
///
/// carve#1704 settles an orphan table part - `<td>`, `<tr>`, `<tbody>` and the
/// six others at document top level with no `<table>` around them - by the same
/// property: no Carve spelling reproduces one, so `roundtrip` must preserve it
/// with the `raw-preserved` warning. carve-js already does.
///
/// carve-rs cannot, and the obstacle is BELOW the importer. html5ever
/// implements the HTML5 tree-construction algorithm, whose "in body" insertion
/// mode says of a `caption`, `col`, `colgroup`, `tbody`, `td`, `tfoot`, `th`,
/// `thead` or `tr` start tag: "Parse error. Ignore the token." The element is
/// never built, its attributes never exist, and the children are reparented
/// into the enclosing element. Measured on html5ever 0.27:
/// `<td id="x"><h1>H</h1></td>` gives a body holding one `<h1>` and nothing
/// else, and `<span id="y"><td id="x"><h1>H</h1></td></span>` gives a span
/// holding the `<h1>` with the `<td>` gone from between them.
///
/// So the importer is handed a document the orphan is already absent from.
/// There is no node to preserve, no path to report it at, and `dom.errors`
/// carries untyped strings ("Unexpected token") with neither a tag name nor a
/// position - nothing a diagnostic could be built from. Preserving it needs the
/// PARSE to change, which is a different piece of work with a blast radius over
/// every import.
///
/// This test is the tripwire for that. It asserts the constraint rather than
/// blessing the loss: the day it fails is the day carve-rs is handed the
/// element, and the raw-preserve arm carve#1704 asks for becomes implementable.
#[test]
fn an_orphan_table_part_never_reaches_this_engines_importer() {
    const ORPHANS: [&str; 9] = [
        "caption", "col", "colgroup", "tbody", "td", "tfoot", "th", "thead", "tr",
    ];
    let mut reached = Vec::new();
    for tag in ORPHANS {
        // Two documents differing ONLY in the orphan's id. Equal output is
        // proof the element and everything on it was gone before the importer
        // looked, rather than proof of any decision the importer made.
        let one = import(
            &format!("<{tag} id=\"x\"><h1>H</h1></{tag}>"),
            HtmlImportMode::Roundtrip,
        );
        let other = import(
            &format!("<{tag} id=\"y\"><h1>H</h1></{tag}>"),
            HtmlImportMode::Roundtrip,
        );
        if one.carve != other.carve || !one.codes.is_empty() {
            reached.push(format!(
                "<{tag}>: {:?} / {:?} codes {:?}",
                one.carve, other.carve, one.codes
            ));
        }
    }
    assert!(
        reached.is_empty(),
        "html5ever now delivers these orphan table parts to the importer, so \
         markup-carve/carve#1704's raw-preserve arm can and must be implemented \
         for them:\n  {}",
        reached.join("\n  ")
    );
}
