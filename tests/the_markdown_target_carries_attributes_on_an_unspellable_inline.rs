//! PART 11 section 8c (CARVE-P11-045): `underline`, `highlight`, `subscript`
//! and `superscript` have no Markdown delimiter spelling and fall back to
//! inline HTML, carrying their attribute set onto the element they emit. The
//! target wrote the tag bare, so an authored attribute reached the element on
//! the HTML target and left the document at the Markdown writer, with nothing
//! in the output to show a reader anything was lost (markup-carve/carve#2839,
//! carve-rs#2418).
//!
//! `small_caps` is the older half. Section 8c has merged its other attributes
//! under the ordinary span rules since well before that clause, so the bare
//! `<span class="smallcaps">` violated a rule already in place. `ruby` was
//! already right and is a control here.

/// Source, and the whole Markdown document it writes.
const CASES: [(&str, &str); 4] = [
    ("x{^2^}{.c} y", "x<sup class=\"c\">2</sup> y\n"),
    ("h =hi={.c} y", "h <mark class=\"c\">hi</mark> y\n"),
    ("u _u_{.c} y", "u <u class=\"c\">u</u> y\n"),
    ("s {,s,}{.c} y", "s <sub class=\"c\">s</sub> y\n"),
];

#[test]
fn a_class_reaches_the_element() {
    for (source, expected) in CASES {
        assert_eq!(carve::to_markdown(source), expected, "{source}");
    }
}

/// Not only `class`: an id, a plain key-value pair, and several at once.
#[test]
fn every_attribute_kind_reaches_the_element() {
    assert_eq!(
        carve::to_markdown("id {^2^}{#sid} y"),
        "id <sup id=\"sid\">2</sup> y\n"
    );
    assert_eq!(
        carve::to_markdown("kv =hi={data-x=1} y"),
        "kv <mark data-x=\"1\">hi</mark> y\n"
    );
    assert_eq!(
        carve::to_markdown("multi _u_{#uid .c1 .c2 data-k=v} y"),
        "multi <u id=\"uid\" class=\"c1 c2\" data-k=\"v\">u</u> y\n"
    );
    assert_eq!(
        carve::to_markdown("sub {,s,}{#k .c} y"),
        "sub <sub id=\"k\" class=\"c\">s</sub> y\n"
    );
}

/// The control that keeps the fix from leaving an empty-attribute artifact.
#[test]
fn a_construct_with_no_attributes_keeps_its_bare_tag() {
    assert_eq!(
        carve::to_markdown("bare {^2^} =hi= _u_ {,s,} y"),
        "bare <sup>2</sup> <mark>hi</mark> <u>u</u> <sub>s</sub> y\n"
    );
}

/// The attribute is still in the document after Carve to Markdown to Carve.
/// The tag itself returns as raw inline HTML rather than as the construct,
/// which is markup-carve/carve#2838's subject and not this one - but the
/// attribute is no longer lost at the writer.
#[test]
fn the_attribute_survives_a_round_trip() {
    for (source, _) in CASES {
        let back = carve::markdown_to_carve(&carve::to_markdown(source));
        assert!(back.contains("class=\"c\""), "{source}: {back}");
    }
}

/// The HTML target was already right and must not move.
#[test]
fn the_html_target_is_unchanged() {
    for (source, _) in CASES {
        assert!(carve::to_html(source).contains("class=\"c\""), "{source}");
    }
}

fn from_ast(inline: &str) -> String {
    let json = format!(
        r#"{{"type":"document","srcByteLength":0,"children":[{{"type":"paragraph","children":[{inline}]}}]}}"#
    );
    let document = carve::from_json(&json).expect("the tree decodes");
    carve::render_markdown(&document).expect("the tree writes")
}

/// The two constructs section 8c already gave an attribute rule. Neither has a
/// Carve source spelling, so they are reached through the AST.
#[test]
fn small_caps_and_ruby_still_carry_their_attributes() {
    assert_eq!(
        from_ast(
            r#"{"type":"small_caps","attrs":{"classes":["c"]},"children":[{"type":"text","value":"sc"}]}"#
        ),
        "<span class=\"smallcaps c\">sc</span>\n"
    );
    assert_eq!(
        from_ast(
            r#"{"type":"ruby","attrs":{"classes":["c"]},"pairs":[{"base":[{"type":"text","value":"x"}],"annotation":[{"type":"text","value":"r"}]}]}"#
        ),
        "<ruby class=\"c\">x<rp>(</rp><rt>r</rt><rp>)</rp></ruby>\n"
    );
}

/// Small caps with NO attributes keeps the base class alone.
#[test]
fn small_caps_with_no_attributes_keeps_only_its_base_class() {
    assert_eq!(
        from_ast(r#"{"type":"small_caps","children":[{"type":"text","value":"sc"}]}"#),
        "<span class=\"smallcaps\">sc</span>\n"
    );
}
