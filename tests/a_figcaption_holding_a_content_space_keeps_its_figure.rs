//! markup-carve/carve-rs#1339. A `<figcaption>` holding one content space
//! destroyed the figure and deleted the caption, and the row left beside it
//! described the wrapper rather than the content that went missing.
//!
//! Measured on `main` at `fc236f85`:
//!
//! ```text
//! <figure><img src="i.png" alt="a"><figcaption>&#160;</figcaption></figure>
//!
//! html_to_ast   -> [BlockImage]      diagnostics = [ElementUnwrapped]
//! html_to_carve -> "![a](i.png)\n"
//! ```
//!
//! ## One predicate, a third spelling
//!
//! `inlines_are_blank` read `t.value.trim().is_empty()`. `str::trim` is
//! `char::is_whitespace`, so it is Unicode `White_Space` and holds U+00A0,
//! U+202F and U+3000 - which markup-carve/carve#1628 puts on the CONTENT side
//! of the line, verified empirically rather than reasoned. `trim_edge_whitespace`
//! and `visible` were the other two spellings of the same predicate on this
//! path and were corrected in carve-rs#1336; `is_layout_space`, in the same
//! file, is the one that spells the set right.
//!
//! ## The carve-out this is NOT
//!
//! The call site drops an EMPTY caption on purpose: it would write a bare `^`
//! line, which re-parses as a literal caret, so the figure would be destroyed
//! AND a character the author never typed would appear. That reasoning is about
//! a caption that WRITES NOTHING. A content space writes a real caption line,
//! and the table caption path already kept it - which is what made this a defect
//! rather than a judgement call. The ordinary-space caption is pinned below as
//! the control, so a fix that widened past the ruling fails here.

use carve::{html_to_ast, html_to_carve, HtmlImportOptions};

const NBSP: &str = "\u{a0}";
const NNBSP: &str = "\u{202f}";
const IDEOGRAPHIC: &str = "\u{3000}";

fn tree(html: &str) -> carve::Document {
    html_to_ast(html, &HtmlImportOptions::default())
        .expect("import")
        .value
}

fn carve(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .expect("import")
        .value
}

/// EVERY code, from BOTH exits. This closes with no row on the shapes it fixes,
/// and a filter on one code passes whether or not a spurious row of another was
/// emitted beside it.
fn diagnostics(html: &str) -> Vec<String> {
    let mut all: Vec<String> = html_to_ast(html, &HtmlImportOptions::default())
        .expect("import")
        .report
        .diagnostics
        .iter()
        .map(|d| format!("{:?}", d.code))
        .collect();
    all.extend(
        html_to_carve(html, &HtmlImportOptions::default())
            .expect("import")
            .report
            .diagnostics
            .iter()
            .map(|d| format!("{:?}", d.code)),
    );
    all
}

fn kinds(document: &carve::Document) -> Vec<String> {
    document
        .children
        .iter()
        .map(|block| {
            format!("{block:?}")
                .split('(')
                .next()
                .expect("a variant name")
                .to_string()
        })
        .collect()
}

fn figure(html: &str) -> String {
    format!("{:?}", tree(html).children)
}

/// THE TICKET'S OWN SHAPE, on all three characters. A caption holding one of
/// them is a caption, so the figure survives and carries it.
#[test]
fn a_content_space_is_a_caption_and_keeps_the_figure() {
    for space in [NBSP, NNBSP, IDEOGRAPHIC] {
        let html = format!(
            "<figure><img src=\"i.png\" alt=\"a\"><figcaption>{space}</figcaption></figure>"
        );
        assert_eq!(
            kinds(&tree(&html)),
            vec!["Figure".to_string()],
            "{space:?} is content, so the figure is not destroyed over it"
        );
        assert!(
            figure(&html).contains(&format!("value: {space:?}")),
            "{space:?} must reach the caption: {}",
            figure(&html)
        );
        assert_eq!(
            carve(&html),
            format!("![a](i.png)\n^ {space}\n"),
            "{space:?}"
        );
        assert_eq!(diagnostics(&html), Vec::<String>::new(), "{space:?}");
    }
}

/// THE CONTROL, AND THE NEAR MISS A WIDER FIX WOULD ALSO CHANGE. An
/// ordinary-space caption writes nothing, so it still takes the no-caption path
/// and still says so.
#[test]
fn a_layout_only_caption_is_still_absent_and_still_reported() {
    for caption in [" ", "\t", "\n  \n", ""] {
        let html = format!(
            "<figure><img src=\"i.png\" alt=\"a\"><figcaption>{caption}</figcaption></figure>"
        );
        assert_eq!(
            kinds(&tree(&html)),
            vec!["BlockImage".to_string()],
            "{caption:?}: a caption that writes nothing is absent"
        );
        assert_eq!(carve(&html), "![a](i.png)\n", "{caption:?}");
        assert!(
            diagnostics(&html).contains(&"ElementUnwrapped".to_string()),
            "{caption:?}: the drop is still declared, got {:?}",
            diagnostics(&html)
        );
    }
}

/// THE SIBLING PATH THAT WAS ALREADY RIGHT, pinned beside the fixed one so a
/// change that collapses the two fails here rather than silently.
#[test]
fn a_table_caption_kept_it_all_along() {
    for space in [NBSP, NNBSP, IDEOGRAPHIC] {
        let html = format!("<table><caption>{space}</caption><tr><td>c</td></tr></table>");
        assert_eq!(carve(&html), format!("| c |\n^ {space}\n"), "{space:?}");
        assert_eq!(diagnostics(&html), Vec::<String>::new(), "{space:?}");
    }
}

/// A CAPTION HOLDING A CONTENT SPACE BESIDE OTHER CONTENT was never the failing
/// shape, and stays exactly as it was - the boundary is the all-content-space
/// case, which a fixture padded with ordinary spaces cannot see.
#[test]
fn a_content_space_beside_content_is_unchanged() {
    let html = format!(
        "<figure><img src=\"i.png\" alt=\"a\"><figcaption>{NBSP}x{NBSP}</figcaption></figure>"
    );
    assert_eq!(kinds(&tree(&html)), vec!["Figure".to_string()]);
    assert_eq!(carve(&html), format!("![a](i.png)\n^ {NBSP}x{NBSP}\n"));
    assert_eq!(diagnostics(&html), Vec::<String>::new());
}

/// THE SECOND CALLER, which the same change reaches: the `contributes` test in
/// the inline flattening walk, where a run that contributes nothing is dropped
/// outright rather than kept beside the separator.
///
/// A CONTENT SPACE BETWEEN TWO FLATTENED BLOCKS WAS BEING DELETED THERE TOO, and
/// the shape has to hold a BLOCK to reach it - `flattening` is set by a block
/// tag among the siblings, so `<p>a<span>&#160;</span>b</p>` never reaches this
/// arm at all and cannot see the difference. Its layout twin is pinned beside it
/// as the control, because dropping inter-element whitespace inside a flatten is
/// the behavior this must NOT widen past.
#[test]
fn a_content_space_contributes_when_a_wrapper_is_flattened() {
    for space in [NBSP, NNBSP, IDEOGRAPHIC] {
        let html = format!("<table><tr><td><p>a</p>{space}<p>b</p></td></tr></table>");
        assert_eq!(carve(&html), format!("| a {space} b |\n"), "{space:?}");

        let caption = format!(
            "<figure><img src=\"i.png\" alt=\"a\"><figcaption><p>x</p>{space}<p>y</p></figcaption></figure>"
        );
        assert_eq!(
            carve(&caption),
            format!("![a](i.png)\n^ x {space} y\n"),
            "{space:?}"
        );
    }

    // THE CONTROL. Inter-element layout whitespace inside a flatten is dropped
    // rather than kept beside the separator, which is what stops `a  b`.
    assert_eq!(
        carve("<table><tr><td><p>a</p> <p>b</p></td></tr></table>"),
        "| a b |\n"
    );
    assert_eq!(
        carve("<table><tr><td><p>a</p>\n  <p>b</p></td></tr></table>"),
        "| a b |\n"
    );
}
