//! A substitution splits at a top-level `~>` only, and each half is inline
//! content (ruling B on markup-carve/carve#2083 and the node-shape ruling on
//! markup-carve/carve-js#1827, markup-carve/carve-rs#1742, carve-rs#1752).

use carve::{parse, to_carve, to_html, BlockNode, InlineNode};

fn html(source: &str) -> String {
    let parsed = carve::render_html(&parse(source)).unwrap();
    assert_eq!(to_html(source), parsed, "the two routes disagree");
    parsed.trim_end().to_string()
}

macro_rules! cases {
    ($($name:ident: $source:expr => $expected:expr,)*) => {
        $(
            mod $name {
                #[test]
                fn reads_as_ruled() {
                    assert_eq!(super::html($source), $expected);
                }

                #[test]
                fn the_writer_reproduces_it() {
                    let written = carve::to_carve(&format!("{}\n", $source));
                    assert_eq!(super::html(&written), $expected, "{written}");
                    assert_eq!(carve::to_carve(&written), written);
                }
            }
        )*
    };
}

// The arrow does not split the pair where it is not at the top level.
cases! {
    inside_an_unclosed_code_span: "{~`a~>b~}" => "<p><s><code>a~&gt;b</code></s></p>",
    inside_a_closed_code_span: "{~a `x~>y` b~}" => "<p><s>a <code>x~&gt;y</code> b</s></p>",
    escaped: "{~a\\~>b~}" => "<p><s>a~&gt;b</s></p>",
}

// It splits at the first top-level one, and each half is inline content.
cases! {
    emphasis_in_both_halves: "{~/old/~>/new/~}"
        => "<p><del><em>old</em></del><ins><em>new</em></ins></p>",
    after_math_holding_one: "{~$`a~>b`~>c~}"
        => "<p><del><span class=\"math inline\" role=\"math\">\\(a~&gt;b\\)</span></del><ins>c</ins></p>",
    after_an_inline_literal_holding_one: "{~!`a~>b`~>c~}" => "<p><del>a~&gt;b</del><ins>c</ins></p>",
    after_an_editorial_comment_holding_one: "{~a{# ~> #}b~>c~}"
        => "<p><del>a<span class=\"critic-comment\"> ~&gt; </span>b</del><ins>c</ins></p>",
    with_a_brace_in_a_half: "{~a}~>b~}" => "<p><del>a}</del><ins>b</ins></p>",
    with_an_empty_old_half: "{~~>b~}" => "<p><del></del><ins>b</ins></p>",
    inside_an_open_strike: "~a {~b~>c~} d~"
        => "<p><s>a <del>b</del><ins>c</ins> d</s></p>",
}

/// The halves reach the AST as inline arrays, so resolution and positions apply.
#[test]
fn the_halves_are_inline_nodes() {
    let doc = carve::parse_with_options(
        "{~/a/~>[t][r]~}\n\n[r]: /u\n",
        &carve::Options::new().with_positions(true),
    );
    let BlockNode::Paragraph(paragraph) = &doc.children[0] else {
        panic!("expected a paragraph");
    };
    let [InlineNode::CriticSubstitute(sub)] = &paragraph.children[..] else {
        panic!("expected one substitution");
    };
    assert!(matches!(sub.old[..], [InlineNode::Emphasis(_)]));
    let [InlineNode::Link(link)] = &sub.new[..] else {
        panic!("expected the reference to resolve to a link");
    };
    assert_eq!(link.href, "/u");
    let InlineNode::Emphasis(old) = &sub.old[0] else {
        panic!("expected the emphasis");
    };
    assert!(old.pos.is_some(), "the halves carry positions");
}

/// The AST JSON carries `old` and `new` inline arrays.
#[test]
fn the_json_carries_the_two_arrays() {
    let json = carve::to_json(&parse("{~a~>b~}\n"));
    assert!(
        json.contains(r#""type":"substitution","old":[{"type":"text","value":"a""#),
        "{json}"
    );
    assert!(
        json.contains(r#""new":[{"type":"text","value":"b""#),
        "{json}"
    );
    let back = carve::from_json(&json).unwrap();
    assert_eq!(to_carve(&carve::render_carve(&back).unwrap()), "{~a~>b~}\n");
}

/// Control: a pair with no top-level arrow is a forced strike, not a split.
#[test]
fn a_pair_with_no_arrow_is_a_strike() {
    assert_eq!(html("{~view~}"), "<p><s>view</s></p>");
}

/// A half whose text holds the arrow or the closer, which only an ingested tree
/// can carry, is escaped so the pair reads back the same way.
#[test]
fn the_writer_escapes_an_arrow_in_a_half() {
    let json = r#"{"type":"document","children":[{"type":"paragraph","children":[{"type":"substitution","old":[{"type":"text","value":"a~>b"}],"new":[{"type":"text","value":"c~}d"}]}]}],"srcByteLength":0}"#;
    let doc = carve::from_json(json).unwrap();
    let written = carve::render_carve(&doc).unwrap();
    assert_eq!(written, "{~a\\~>b~>c~\\}d~}\n");
    assert_eq!(carve::to_html(&written), carve::render_html(&doc).unwrap());
}

/// Ingest refuses the string fields the node no longer has.
#[test]
fn the_old_string_fields_are_refused() {
    let json = r#"{"type":"document","children":[{"type":"paragraph","children":[{"type":"substitution","oldText":"a","newText":"b","old":[],"new":[]}]}],"srcByteLength":0}"#;
    assert!(carve::from_json(json).is_err());
}
