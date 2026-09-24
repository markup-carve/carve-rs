//! `math.label` and `math.number` reach this engine and survive it
//! (carve-rs#1859, CARVE-P12-051).
//!
//! Both fields were in `WIRE_FIELDS` because the schema names them, so the
//! generated table said yes and nothing implemented them. `decode_math` refused
//! the tree rather than decode-and-drop, which PART 12 §9(b) prefers - but a
//! refusal is still no interchange: a labelled equation from a bridge could not
//! be read at all.
//!
//! Carve 0.1 source spells neither field, so every case here starts from JSON.
//! PART 9 §19 fixes what the targets do with the pair: one U+0020, then the
//! label, then one U+0020, then the number, unbracketed, with only the HTML
//! target wrapping it in `<span class="equation-number">`.

use carve::{CheckedRenderOptions, RenderTarget};

fn doc(math: &str) -> String {
    format!(
        r#"{{"type":"document","srcByteLength":0,"children":[
            {{"type":"paragraph","children":[{math}]}}]}}"#
    )
}

/// A resolved labelled display equation: the shape a bridge publishes.
const NUMBERED: &str = r#"{"type":"math","display":true,"content":"E = mc^2","label":"Equation","number":3,"attrs":{"id":"emc"}}"#;

/// A label with no number - what an unresolved tree, or one inside a numbered
/// figure, carries.
const LABELLED: &str = r#"{"type":"math","display":true,"content":"a + b","label":"Eq."}"#;

fn first_math(document: &carve::Document) -> carve::Math {
    let carve::BlockNode::Paragraph(p) = &document.children[0] else {
        panic!("the first block is a paragraph");
    };
    p.children
        .iter()
        .find_map(|node| match node {
            carve::InlineNode::Math(math) => Some(math.clone()),
            _ => None,
        })
        .expect("the paragraph holds a math node")
}

fn decoded(math: &str) -> carve::Document {
    carve::from_json(&doc(math)).expect("the tree decodes")
}

#[test]
fn a_numbered_equation_decodes_into_both_fields() {
    let math = first_math(&decoded(NUMBERED));
    assert_eq!(math.label.as_deref(), Some("Equation"));
    assert_eq!(math.number, Some(3));
    assert!(math.display);
}

#[test]
fn a_label_without_a_number_decodes_on_its_own() {
    let math = first_math(&decoded(LABELLED));
    assert_eq!(math.label.as_deref(), Some("Eq."));
    assert_eq!(math.number, None);
}

#[test]
fn the_pair_survives_a_round_trip() {
    let document = decoded(NUMBERED);
    let republished = carve::to_json(&document);
    assert!(
        republished.contains(r#""label":"Equation","number":3"#),
        "the pair is republished beside the formula: {republished}"
    );
    let again = carve::from_json(&republished).expect("the republished tree decodes");
    assert_eq!(first_math(&again), first_math(&document));
}

#[test]
fn a_label_alone_survives_a_round_trip() {
    let document = decoded(LABELLED);
    let republished = carve::to_json(&document);
    assert!(
        republished.contains(r#""label":"Eq.""#),
        "the label is republished: {republished}"
    );
    assert!(
        !republished.contains("\"number\""),
        "an absent number stays absent: {republished}"
    );
    assert_eq!(
        first_math(&carve::from_json(&republished).expect("decodes")),
        first_math(&document)
    );
}

#[test]
fn html_renders_the_number_after_the_formula() {
    let html = carve::render_html(&decoded(NUMBERED)).expect("renders");
    assert!(
        html.contains(
            "<span class=\"math display\" id=\"emc\" role=\"math\">\\[E = mc^2\\]</span> \
             <span class=\"equation-number\">Equation 3</span>"
        ),
        "the number follows the math span, one space apart: {html}"
    );
}

#[test]
fn html_renders_nothing_extra_for_a_label_without_a_number() {
    let html = carve::render_html(&decoded(LABELLED)).expect("renders");
    assert!(
        !html.contains("equation-number"),
        "an unnumbered equation draws no number: {html}"
    );
    assert!(html.contains("\\[a + b\\]"), "{html}");
}

#[test]
fn a_label_holding_markup_characters_is_escaped_in_html() {
    let hostile = r#"{"type":"math","display":true,"content":"x","label":"<b>&","number":1}"#;
    let html = carve::render_html(&decoded(hostile)).expect("renders");
    assert!(
        html.contains("<span class=\"equation-number\">&lt;b&gt;&amp; 1</span>"),
        "the label is text, escaped like any other: {html}"
    );
}

#[test]
fn the_other_three_targets_append_the_same_unbracketed_pair() {
    let document = decoded(NUMBERED);
    let markdown = carve::render_markdown(&document).expect("renders");
    assert!(
        markdown.contains("$$E = mc^2$$ Equation 3"),
        "markdown: {markdown}"
    );
    let plain = carve::render_plain_text(&document).expect("renders");
    assert!(plain.contains("E = mc^2 Equation 3"), "plain: {plain}");
    let ansi = carve::render_ansi(&document).expect("renders");
    assert!(ansi.contains("Equation 3"), "ansi: {ansi}");
    assert!(
        !ansi.contains("(Equation 3)") && !ansi.contains("[Equation 3]"),
        "no brackets: {ansi}"
    );
}

#[test]
fn the_canonical_writer_keeps_the_formula_and_does_not_refuse() {
    let document = decoded(NUMBERED);
    let written = carve::render_carve(&document).expect("the writer spells the formula");
    assert!(
        written.contains("$$`E = mc^2`{#emc}"),
        "the formula and its id survive: {written}"
    );
    assert!(
        !written.contains("Equation"),
        "Carve source has no spelling for the label: {written}"
    );
    // The loss is real and unreported: the published render-loss vocabulary has
    // no code for it, which markup-carve/carve#2245 asks for. Pinned so the
    // count changes visibly when that ruling lands.
    let report = carve::with_render_loss_report(
        RenderTarget::Carve,
        CheckedRenderOptions::default(),
        || carve::render_carve(&document),
    )
    .expect("not strict");
    assert_eq!(report.total_losses, 0);
}

#[test]
fn a_number_without_a_label_is_refused() {
    let error = carve::from_json(&doc(
        r#"{"type":"math","display":true,"content":"x","number":2}"#,
    ))
    .expect_err("a number with no counter bucket is refused");
    assert!(
        error.to_string().contains("math.number needs math.label"),
        "{error}"
    );
}

#[test]
fn a_number_on_inline_math_is_refused() {
    let error = carve::from_json(&doc(
        r#"{"type":"math","display":false,"content":"x","label":"Eq.","number":2}"#,
    ))
    .expect_err("inline math draws no number");
    assert!(
        error.to_string().contains("only valid on display math"),
        "{error}"
    );
}

#[test]
fn a_zero_number_is_refused() {
    let error = carve::from_json(&doc(
        r#"{"type":"math","display":true,"content":"x","label":"Eq.","number":0}"#,
    ))
    .expect_err("the number is a positive integer");
    assert!(error.to_string().contains("positive integer"), "{error}");
}

#[test]
fn a_padded_or_empty_label_is_refused() {
    for label in [r#""""#, r#"" Eq.""#, r#""Eq. ""#, r#""\tEq.""#] {
        let payload = format!(r#"{{"type":"math","display":true,"content":"x","label":{label}}}"#);
        let error = carve::from_json(&doc(&payload))
            .expect_err("a padded or empty label is not a counter key");
        assert!(
            error
                .to_string()
                .contains("no leading or trailing whitespace"),
            "{label}: {error}"
        );
    }
}

#[test]
fn a_label_on_inline_math_is_kept() {
    // CARVE-P12-051: inline math draws no number, but its label is retained for
    // interchange rather than refused.
    let math = first_math(&decoded(
        r#"{"type":"math","display":false,"content":"x","label":"Eq."}"#,
    ));
    assert_eq!(math.label.as_deref(), Some("Eq."));
    assert_eq!(math.number, None);
}

#[test]
fn the_prosemirror_bridge_carries_the_pair_both_ways() {
    let document = decoded(NUMBERED);
    let pm = carve::to_prosemirror(&document);
    assert!(
        pm.json.contains("\"label\":\"Equation\"") && pm.json.contains("\"number\":3"),
        "the pair rides as attributes: {}",
        pm.json
    );
    let back = carve::from_prosemirror(&pm.json).expect("the bridge doc comes back");
    // Only the pair is under test: the bridge does not preserve `attrs.order`,
    // which is a separate, pre-existing gap.
    let round_tripped = first_math(&back);
    assert_eq!(round_tripped.label.as_deref(), Some("Equation"));
    assert_eq!(round_tripped.number, Some(3));
    assert_eq!(round_tripped.content, first_math(&document).content);
}
