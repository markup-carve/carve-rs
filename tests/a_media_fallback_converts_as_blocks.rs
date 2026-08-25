//! A MEDIA WRAPPER'S FALLBACK CONTENT CONVERTS AS BLOCKS (ruling
//! markup-carve/carve#1749).
//!
//! `<video controls><p>A</p><p>B</p></video>` wrote `A B` here and two
//! paragraphs in carve-php. Losing a paragraph boundary the author wrote is a
//! CONTENT change rather than a spelling difference: the document said two
//! things and came back saying one, and nothing about the wrapper being
//! unsupported requires its children to be reduced to a string - the fallback is
//! ordinary flow content Carve can spell.
//!
//! THE ROWS FALL OUT RATHER THAN BEING PORTED. The flatten reported an
//! `element-unwrapped` for every block it dissolved, and those rows were
//! truthful about that output. A `<p>` that survives as a paragraph is not
//! unwrapped and owes none, so a fix that kept them while changing the
//! conversion would start making false statements.
//!
//! THE ASSERTIONS ARE ON THE RE-RENDER, and on the BOUNDARY rather than on the
//! text: `A B` contains both letters too, so a test that only looked for them
//! would pass the flatten this ruling removes.

use carve::{html_to_carve, HtmlImportMode, HtmlImportOptions};

fn import_with(html: &str, mode: HtmlImportMode) -> String {
    let opts = HtmlImportOptions {
        mode,
        ..HtmlImportOptions::default()
    };
    html_to_carve(html, &opts).expect("imports").value
}

fn import(html: &str) -> String {
    import_with(html, HtmlImportMode::Safe)
}

fn codes(html: &str) -> Vec<String> {
    html_to_carve(html, &HtmlImportOptions::default())
        .expect("imports")
        .report
        .diagnostics
        .iter()
        .map(|d| d.code.as_str().to_string())
        .collect()
}

/// The whole family, and the boundary each one used to lose.
#[test]
fn every_media_wrapper_keeps_the_paragraph_boundary() {
    for tag in ["video", "audio", "object", "canvas", "picture"] {
        let html = format!("<{tag}><p>A</p><p>B</p></{tag}>");
        assert_eq!(import(&html), "A\n\nB\n", "<{tag}>");
        let out = carve::to_html(&import(&html));
        assert!(out.contains("<p>A</p>"), "<{tag}>: {out}");
        assert!(out.contains("<p>B</p>"), "<{tag}>: {out}");
    }
}

/// `semantic` takes the same answer: the ruling is about the conversion, and
/// only `roundtrip` has a different contract.
#[test]
fn the_semantic_mode_converts_the_fallback_the_same_way() {
    let html = "<video controls><p>A</p><p>B</p></video>";
    assert_eq!(import_with(html, HtmlImportMode::Semantic), "A\n\nB\n");
}

/// The rest of the flow content, which is what says the answer is the
/// conversion rather than a paragraph special case. Every one of these survives
/// in carve-php and flattened here.
#[test]
fn the_fallbacks_other_blocks_survive_too() {
    let cases: [(&str, &str); 4] = [
        ("<h2>H</h2>", "## H\n"),
        ("<ul><li>a</li><li>b</li></ul>", "- a\n- b\n"),
        ("<blockquote><p>Q</p></blockquote>", "> Q\n"),
        ("<pre><code>x</code></pre>", "```\nx\n```\n"),
    ];
    for (inner, expected) in cases {
        assert_eq!(
            import(&format!("<video controls>{inner}</video>")),
            expected
        );
    }
}

/// One row for the wrapper, none for the blocks it kept.
#[test]
fn the_blocks_that_survive_owe_no_unwrap_row() {
    let element_rows: Vec<String> = codes("<video controls><p>A</p><p>B</p></video>")
        .into_iter()
        .filter(|code| code == "element-unwrapped")
        .collect();
    assert_eq!(element_rows, vec!["element-unwrapped".to_string()]);
}

/// `roundtrip` is untouched. Its answer for a media wrapper is the raw INLINE
/// span all three engines write - a media element is inline content in HTML -
/// and this ruling is about the fallback conversion the lossy modes do.
#[test]
fn roundtrip_still_preserves_the_whole_element() {
    let out = import_with(
        "<video controls><p>A</p><p>B</p></video>",
        HtmlImportMode::Roundtrip,
    );
    assert!(out.contains("{=html}"), "{out}");
    assert!(out.contains("<p>A</p><p>B</p>"), "{out}");
}

/// A media wrapper in an INLINE position is inline content and stays that way.
#[test]
fn a_media_wrapper_inside_a_paragraph_stays_inline() {
    assert_eq!(
        import("<p>x <video controls>fallback</video> y</p>"),
        "x fallback y\n"
    );
}

/// A fallback that is a bare run is still one paragraph, and an empty wrapper
/// is still dropped - the shapes that already converted correctly must not
/// move.
#[test]
fn the_shapes_that_already_converted_correctly_do_not_move() {
    assert_eq!(import("<video controls>fallback</video>"), "fallback\n");
    assert!(codes("<video controls></video>").contains(&"element-dropped".to_string()));
}
