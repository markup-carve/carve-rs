//! MathML on HTML import, as carve#1210's D6 rules it.
//!
//! `<math>` had no branch here, so it unwrapped and its children came through
//! as text. MathML's children are a token stream, not fallback content, so
//! that concatenation is not a degraded equation but a different value: the
//! children of `<mfrac><mn>1</mn><mn>2</mn></mfrac>` concatenate to `12`, one
//! half arriving as twelve. A plausible wrong value survives review.
//!
//! The rule is a three-tier lookup for TeX the producer already put in the
//! source, and no MathML-to-TeX converter: a declared `<annotation>`, else
//! `alttext` with the assumption reported, else the element is dropped with a
//! warning naming it (`roundtrip` keeps it raw instead).
//!
//! The fixtures are the shapes real producers emit. Every `<math>` element on
//! a Wikipedia article (209 of 209) and in an ar5iv paper (142 of 142) carries
//! `<annotation encoding="application/x-tex">`, which is why tier 1 is the
//! common path and why no converter is needed to reach it.

use carve::{
    html_to_ast, html_to_carve, BlockNode, HtmlImportDiagnosticCode, HtmlImportMode,
    HtmlImportOptions, InlineNode,
};

fn imported(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .unwrap()
        .value
}

fn diagnostics(html: &str) -> Vec<(HtmlImportDiagnosticCode, String)> {
    html_to_carve(html, &HtmlImportOptions::default())
        .unwrap()
        .report
        .diagnostics
        .into_iter()
        .map(|d| (d.code, d.message))
        .collect()
}

/// Every fixture here puts the element directly in a paragraph, which is where
/// a `<math>` inside a `<p>` lands, so a direct scan reads the node without a
/// walker that would have to know every container variant.
fn math_nodes(html: &str) -> Vec<(bool, String)> {
    let doc = html_to_ast(html, &HtmlImportOptions::default())
        .unwrap()
        .value;
    let mut out = Vec::new();
    for block in &doc.children {
        if let BlockNode::Paragraph(paragraph) = block {
            for inline in &paragraph.children {
                if let InlineNode::Math(math) = inline {
                    out.push((math.display, math.content.clone()));
                }
            }
        }
    }
    out
}

/// The shape Wikipedia and ar5iv both emit: presentation MathML wrapped in
/// `<semantics>` with the original TeX beside it.
const WIKIPEDIA_SHAPED: &str = concat!(
    r#"<p>The formula <math xmlns="http://www.w3.org/1998/Math/MathML" alttext="{\displaystyle x^{2}}">"#,
    "<semantics><mrow><msup><mi>x</mi><mn>2</mn></msup></mrow>",
    r#"<annotation encoding="application/x-tex">{\displaystyle x^{2}}</annotation>"#,
    "</semantics></math> is a square.</p>",
);

#[test]
fn a_declared_tex_annotation_is_the_equation() {
    assert_eq!(
        imported(WIKIPEDIA_SHAPED),
        "The formula $`{\\displaystyle x^{2}}` is a square.\n"
    );
    assert_eq!(
        math_nodes(WIKIPEDIA_SHAPED),
        vec![(false, "{\\displaystyle x^{2}}".to_string())]
    );
}

/// Tier 1 is silent. The presentation markup and the annotation are two
/// spellings of one equation, so keeping the TeX loses nothing to report - and
/// the attributes the branch reads are not reported dropped either.
#[test]
fn a_declared_annotation_reports_nothing() {
    assert_eq!(diagnostics(WIKIPEDIA_SHAPED), Vec::new());
}

/// The wrapper stays: Carve math content is opaque TeX, and rewriting the body
/// would be a second decision. Only the whitespace around it goes, which is
/// not part of the equation and does not survive being written back.
#[test]
fn a_pretty_printed_annotation_keeps_its_body_and_loses_its_indentation() {
    let html = concat!(
        "<p><math><semantics><mrow/>",
        "<annotation encoding=\"application/x-tex\">\n  x^2\n</annotation>",
        "</semantics></math></p>",
    );
    assert_eq!(math_nodes(html), vec![(false, "x^2".to_string())]);
    // The reason it is trimmed rather than kept: an inline span holding a
    // newline does not come back from this engine's own writer unchanged.
    assert_eq!(imported(html), "$`x^2`\n");
}

/// Tier 2. MathML does not declare what `alttext` holds - `alttext="x squared"`
/// is as valid as one holding TeX - so the assumption is reported.
///
/// The CODE is the assertion, not just the message: `encoding-assumed` says the
/// produced math node is only correct if the guess holds, which is a claim about
/// the OUTPUT. `element-unwrapped` would report a structural event the consumer
/// cannot act on (carve#1235).
#[test]
fn alttext_supplies_the_tex_and_says_that_it_assumed_the_encoding() {
    let html = r#"<p><math alttext="x^2"><mi>x</mi></math></p>"#;
    assert_eq!(math_nodes(html), vec![(false, "x^2".to_string())]);
    assert_eq!(
        diagnostics(html),
        vec![(
            HtmlImportDiagnosticCode::EncodingAssumed,
            "Read <math> through its alttext: MathML does not declare the encoding of alttext, so TeX is assumed".to_string()
        )]
    );
}

/// The ordering is the ruling: a declared encoding beats an undeclared
/// attribute where the two disagree. The reverse order was carve-php's, and
/// is corrected to this one.
#[test]
fn a_declared_annotation_beats_a_disagreeing_alttext() {
    let html = concat!(
        r#"<p><math alttext="WRONG"><semantics><mrow/>"#,
        r#"<annotation encoding="application/x-tex">x^2</annotation>"#,
        "</semantics></math></p>",
    );
    assert_eq!(math_nodes(html), vec![(false, "x^2".to_string())]);
    assert_eq!(diagnostics(html), Vec::new());
}

/// Tier 3, and the case the ruling turns on.
#[test]
fn a_math_element_with_no_tex_is_dropped_rather_than_flattened() {
    let html = "<p>Bare <math><mfrac><mn>1</mn><mn>2</mn></mfrac></math> here.</p>";
    // The space on each side of the element stays, exactly as carve-js
    // leaves it: dropping an element is not a reason to rewrite the text
    // around it, and a run of spaces is one space to every renderer.
    assert_eq!(imported(html), "Bare  here.\n");
    assert!(math_nodes(html).is_empty());
    assert_eq!(
        diagnostics(html),
        vec![(
            HtmlImportDiagnosticCode::ElementDropped,
            "Dropped <math>: no TeX annotation and no alttext, and its children are a token stream, not an equation".to_string()
        )]
    );
}

/// The encoding test is an exact match on the whole value. A substring test
/// for `tex` accepts every `text/*` encoding there is, because the word `text`
/// contains it.
#[test]
fn an_encoding_that_is_not_tex_falls_to_the_third_tier() {
    for encoding in ["MathType-MTEF", "text/plain", "MathML-Content"] {
        let html = format!(
            "<p><math><semantics><mrow/><annotation encoding=\"{encoding}\">payload</annotation></semantics></math></p>"
        );
        assert!(
            math_nodes(&html).is_empty(),
            "{encoding} was read as an equation"
        );
        assert_eq!(
            diagnostics(&html)
                .into_iter()
                .map(|(code, _)| code)
                .collect::<Vec<_>>(),
            vec![HtmlImportDiagnosticCode::ElementDropped]
        );
    }
}

/// Both hops are direct children. A recursive search reaches the annotation
/// nested inside an `<annotation-xml>` payload, which describes the equation in
/// another language rather than presenting this element.
#[test]
fn an_annotation_nested_in_an_annotation_xml_payload_does_not_leak() {
    let html = concat!(
        "<p><math><semantics><mrow/>",
        r#"<annotation-xml encoding="MathML-Content">"#,
        r#"<annotation encoding="application/x-tex">nested</annotation>"#,
        "</annotation-xml></semantics></math></p>",
    );
    assert!(math_nodes(html).is_empty());
}

/// An annotation that declares TeX and holds nothing does not settle the tier:
/// a later sibling may hold the equation, and stopping at the empty one would
/// answer with the wrong tier.
#[test]
fn an_empty_annotation_keeps_looking() {
    let html = concat!(
        "<p><math><semantics><mrow/>",
        r#"<annotation encoding="application/x-tex">   </annotation>"#,
        r#"<annotation encoding="text/x-tex">x^2</annotation>"#,
        "</semantics></math></p>",
    );
    assert_eq!(math_nodes(html), vec![(false, "x^2".to_string())]);
    assert_eq!(diagnostics(html), Vec::new());
}

#[test]
fn a_block_display_math_element_keeps_its_display_flag() {
    let html = concat!(
        r#"<p><math display="block"><semantics><mrow/>"#,
        r#"<annotation encoding="application/x-tex">\int_0^1 x\,dx</annotation>"#,
        "</semantics></math></p>",
    );
    assert_eq!(
        math_nodes(html),
        vec![(true, "\\int_0^1 x\\,dx".to_string())]
    );
    assert_eq!(imported(html), "$$`\\int_0^1 x\\,dx`\n");
}

/// `roundtrip` preserves what Carve cannot express, and a `<math>` reaching it
/// is foreign markup by definition - Carve's own HTML spells math as a span.
/// So tier 3 keeps the element there instead of dropping it.
#[test]
fn roundtrip_keeps_an_untranslatable_element_verbatim() {
    let html = "<p><math><mfrac><mn>1</mn><mn>2</mn></mfrac></math></p>";
    let options = HtmlImportOptions {
        mode: HtmlImportMode::Roundtrip,
        ..HtmlImportOptions::default()
    };
    let out = html_to_carve(html, &options).unwrap();
    assert!(
        out.value
            .contains("<math><mfrac><mn>1</mn><mn>2</mn></mfrac></math>"),
        "{}",
        out.value
    );
    assert!(out
        .report
        .diagnostics
        .iter()
        .any(|d| d.code == HtmlImportDiagnosticCode::RawPreserved));
}

/// The budgets must not depend on which branch an element takes. The mapped
/// element returns without walking its subtree, so the subtree is charged
/// explicitly - otherwise a document could buy an unbounded MathML tree for
/// the price of one node.
#[test]
fn a_mapped_element_still_charges_its_subtree() {
    let html = r#"<p><math alttext="x"><mrow><mi>a</mi><mi>b</mi><mi>c</mi></mrow></math></p>"#;
    let tight = HtmlImportOptions {
        max_nodes: 4,
        ..HtmlImportOptions::default()
    };
    assert!(
        html_to_carve(html, &tight).is_err(),
        "the unwalked subtree was not charged"
    );
    let roomy = HtmlImportOptions {
        max_nodes: 1000,
        ..HtmlImportOptions::default()
    };
    assert!(html_to_carve(html, &roomy).is_ok());
}

/// A `math` start tag reaches this branch by its local name, which is what
/// carve-js and carve-php match on too. The namespace is not consulted, and
/// does not need to be: the HTML parsing spec puts a `math` start tag in the
/// MathML namespace wherever it appears, foreign content included, so an
/// element with this local name IS a MathML element by the time the tree
/// builder is done with it. Pinned rather than left incidental, because the
/// alternative - a namespace test - would diverge from the other two engines
/// and would answer differently on a fragment parse.
#[test]
fn a_math_element_inside_foreign_content_is_still_an_equation() {
    let html = r#"<p><svg><math alttext="x"><text>hello</text></math></svg></p>"#;
    assert_eq!(math_nodes(html), vec![(false, "x".to_string())]);
}
