//! AN UNSUPPORTED ELEMENT WITH NOTHING TO UNWRAP IS DROPPED, NOT UNWRAPPED
//! (ruling markup-carve/carve#1738).
//!
//! `element-unwrapped` makes a claim about CONTENT: the wrapper went and what
//! it held stayed. An empty `<progress>` held nothing and a void `<input>` can
//! hold nothing, so nothing stayed, and calling either one unwrapped states
//! something about content that did not happen. `element-dropped` says the
//! element and its content both went, which for an empty one is exactly true.
//! The severity follows the code: an unwrap preserves text and is `info`, a
//! drop does not and is `warning`.
//!
//! BOTH HALVES OF THE SAME ELEMENT, deliberately. A test that only pinned the
//! empty case passes an implementation that made every unsupported element
//! `dropped`, which would be a worse report than the one this replaces - so
//! each element below is asserted twice, once holding fallback content and once
//! empty.
//!
//! NOT A LIST OF TAG NAMES. The elements the ruling names agree with carve-php
//! whenever they have children and diverge only when they do not, so what
//! decides is content and a name list would be back next sweep
//! (markup-carve/carve#1704). The CONTROL half of that is the unrelated tags at
//! the bottom, which have no place on any list and take the same two answers.
//!
//! ORDER IS ASSERTED WITH THE ROW, because markup-carve/carve-php#1739 pinned
//! the element row ahead of the attribute rows for its element and this change
//! rewrites the row that stands there. Asserting the code alone would pass an
//! implementation that emitted it after the attributes it introduces.

use carve::{html_to_carve, HtmlImportOptions};

/// Every row, as `code :: severity`, in the order the report gives them.
fn rows(html: &str) -> Vec<String> {
    html_to_carve(html, &HtmlImportOptions::default())
        .expect("import")
        .report
        .diagnostics
        .iter()
        .map(|d| format!("{} :: {:?}", d.code.as_str(), d.severity))
        .collect()
}

/// Only the rows that name what became of an ELEMENT.
fn element_rows(html: &str) -> Vec<String> {
    rows(html)
        .into_iter()
        .filter(|row| row.starts_with("element-"))
        .collect()
}

/// The four the ruling measured, plus four the ruling did not, each written
/// twice: holding fallback content, and holding nothing.
const PAIRS: &[(&str, &str)] = &[
    (
        "<progress value=\"1\">FALLBACK</progress>",
        "<progress value=\"1\"></progress>",
    ),
    (
        "<meter value=\"1\">FALLBACK</meter>",
        "<meter value=\"1\"></meter>",
    ),
    (
        "<audio controls>FALLBACK</audio>",
        "<audio controls></audio>",
    ),
    (
        "<video controls>FALLBACK</video>",
        "<video controls></video>",
    ),
    ("<canvas>FALLBACK</canvas>", "<canvas></canvas>"),
    ("<button>FALLBACK</button>", "<button></button>"),
    ("<form>FALLBACK</form>", "<form></form>"),
    ("<section>FALLBACK</section>", "<section></section>"),
];

#[test]
fn an_element_with_children_unwraps_and_one_without_is_dropped() {
    for (with_content, empty) in PAIRS {
        assert_eq!(
            element_rows(with_content),
            vec!["element-unwrapped :: Info".to_string()],
            "{with_content}"
        );
        assert_eq!(
            element_rows(empty),
            vec!["element-dropped :: Warning".to_string()],
            "{empty}"
        );
    }
}

#[test]
fn a_void_input_is_dropped_because_it_can_never_have_had_children() {
    assert_eq!(
        element_rows("<input type=\"text\" value=\"v\">"),
        vec!["element-dropped :: Warning".to_string()]
    );
}

/// WHITESPACE IS NOT CONTENT. An unwrap that leaves a blank line behind
/// preserved nothing a reader can see, and the emitted source proves it: the
/// document is empty either way.
#[test]
fn an_element_holding_only_whitespace_is_dropped_and_writes_nothing() {
    let imported = html_to_carve(
        "<progress value=\"1\">   </progress>",
        &HtmlImportOptions::default(),
    )
    .expect("import");
    assert_eq!(imported.value.trim(), "");
    assert_eq!(
        element_rows("<progress value=\"1\">   </progress>"),
        vec!["element-dropped :: Warning".to_string()]
    );
}

/// LAYOUT IS THE HTML WHITESPACE SET AND NOTHING ELSE (PART 11 §7). SPACE,
/// TAB and the line terminators an HTML parser folds in with them are layout;
/// NO-BREAK SPACE, NARROW NO-BREAK SPACE, EN SPACE, IDEOGRAPHIC SPACE and
/// VERTICAL TAB are content, and this importer writes every one of them to the
/// output as itself.
///
/// THE EMITTED SOURCE IS ASSERTED BESIDE THE ROW, because that is the whole
/// claim: `element-dropped` on an element whose character is right there in the
/// output is exactly the false statement this ruling exists to stop. A
/// `char::is_whitespace` trim - which is what `str::trim()` does - takes all
/// five of the content characters below with it and produces that statement.
#[test]
fn layout_is_the_html_whitespace_set_and_everything_else_is_content() {
    for layout in ["&#32;", "&#9;", "&#10;", "&#13;", "&#12;"] {
        let html = format!("<progress value=\"1\">{layout}</progress>");
        let written = html_to_carve(&html, &HtmlImportOptions::default())
            .expect("import")
            .value;
        assert_eq!(written.trim_matches('\n'), "", "{html} wrote something");
        assert_eq!(
            element_rows(&html),
            vec!["element-dropped :: Warning".to_string()],
            "{html}"
        );
    }
    for content in ["&#160;", "&#8239;", "&#8194;", "&#12288;", "&#11;"] {
        let html = format!("<progress value=\"1\">{content}</progress>");
        let written = html_to_carve(&html, &HtmlImportOptions::default())
            .expect("import")
            .value;
        assert_ne!(written.trim_matches('\n'), "", "{html} wrote nothing");
        assert_eq!(
            element_rows(&html),
            vec!["element-unwrapped :: Info".to_string()],
            "{html}"
        );
    }
}

/// AN ACTIVE CHILD IS NOT CONTENT EITHER. A `<script>` never survives an
/// import, so an element whose only child is one had nothing an unwrap could
/// preserve - and the `<script>` reports its own drop, which is why the parent
/// saying `element-unwrapped` would be the only false row of the two.
#[test]
fn an_element_whose_only_child_is_active_is_dropped_beside_the_active_drop() {
    assert_eq!(
        element_rows("<progress value=\"1\"><script>1</script></progress>"),
        vec![
            "element-dropped :: Warning".to_string(),
            "element-dropped :: Warning".to_string(),
        ]
    );
}

/// A NON-ACTIVE CHILD ELEMENT IS CONTENT, whatever it writes. The question is
/// asked of the INPUT (markup-carve/carve#1723's framing), and reading it off
/// the emitted source instead would call this `<audio>` dropped for a loss that
/// belongs to the `<span>` inside it - which reports its own row.
#[test]
fn an_element_holding_an_empty_child_element_unwraps_and_the_child_is_dropped() {
    assert_eq!(
        element_rows("<audio controls><span></span></audio>"),
        vec![
            "element-unwrapped :: Info".to_string(),
            "element-dropped :: Warning".to_string(),
        ]
    );
}

/// THE ELEMENT ROW STANDS AHEAD OF THE ATTRIBUTE ROWS IT INTRODUCES
/// (markup-carve/carve-php#1739), in both outcomes. A consumer reads the rows
/// in order, and in the other order it is told what happened to a
/// `<progress>`'s `value` before it is told the `<progress>` is gone.
#[test]
fn the_element_row_stands_ahead_of_its_attribute_rows_in_both_outcomes() {
    assert_eq!(
        rows("<progress value=\"1\">FALLBACK</progress>"),
        vec![
            "element-unwrapped :: Info".to_string(),
            "attribute-dropped :: Info".to_string(),
        ]
    );
    assert_eq!(
        rows("<progress value=\"1\"></progress>"),
        vec![
            "element-dropped :: Warning".to_string(),
            "attribute-dropped :: Info".to_string(),
        ]
    );
}

// ---------------------------------------------------------------------------
// THE CONTROLS. Every row below reported exactly this before the ruling and has
// to report exactly this after it: the change is the code and the severity on
// the empty arm, and nothing about WHICH elements report at all.

/// A `<div>` never earned an element row and still does not, empty or not.
#[test]
fn a_div_reports_no_element_row_either_way() {
    for html in ["<div></div>", "<div>TEXT</div>"] {
        assert_eq!(element_rows(html), Vec::<String>::new(), "{html}");
    }
}

/// A renderer-derived `<section role="doc-endnotes">` is silent, and the
/// content question does not reach it (PART 9 §16a).
#[test]
fn a_renderer_derived_section_stays_silent() {
    assert_eq!(
        element_rows("<section role=\"doc-endnotes\"><ol><li>n</li></ol></section>"),
        Vec::<String>::new()
    );
}

/// An active element keeps the wording its own arm gives it, which is not the
/// wording this ruling introduces.
#[test]
fn an_active_element_keeps_its_own_drop() {
    let diagnostics = html_to_carve("<script>1</script>", &HtmlImportOptions::default())
        .expect("import")
        .report
        .diagnostics
        .iter()
        .map(|d| format!("{} :: {:?} :: {}", d.code.as_str(), d.severity, d.message))
        .collect::<Vec<_>>();
    assert_eq!(
        diagnostics,
        vec!["element-dropped :: Warning :: Dropped active <script> element".to_string()]
    );
}

/// A mapped element is silent whether it is empty or not.
#[test]
fn a_mapped_element_stays_silent_either_way() {
    for html in [
        "<p></p>",
        "<p>TEXT</p>",
        "<em></em>",
        "<em>TEXT</em>",
        "<hr>",
        "<br>",
    ] {
        assert_eq!(element_rows(html), Vec::<String>::new(), "{html}");
    }
}

/// `roundtrip` preserves the element instead of reporting either outcome.
#[test]
fn roundtrip_preserves_instead_of_reporting_either_outcome() {
    let options = HtmlImportOptions {
        mode: carve::HtmlImportMode::Roundtrip,
        ..HtmlImportOptions::default()
    };
    let codes = html_to_carve("<progress value=\"1\"></progress>", &options)
        .expect("import")
        .report
        .diagnostics
        .iter()
        .map(|d| d.code.as_str().to_string())
        .collect::<Vec<_>>();
    assert_eq!(codes, vec!["raw-preserved".to_string()]);
}

// ---------------------------------------------------------------------------
// THE CANARY.
//
// A test binary that linked a STALE rlib - one built from a checkout that does
// not carry this change - passes every assertion above by accident if that
// checkout happens to carry an equivalent fix, and fails all of them in a way
// that reads like a bug in this source if it does not. Neither reading is
// visible from the failure text, and a shared `CARGO_TARGET_DIR` driven by a
// second session makes both reachable.
//
// So this asserts the ONE string that exists nowhere but in this change. A
// binary linked against any other build of the crate cannot produce it, which
// turns a wrong-rlib run into a named failure instead of a silent one.
#[test]
fn the_binary_under_test_linked_this_source() {
    let message = html_to_carve(
        "<progress value=\"1\"></progress>",
        &HtmlImportOptions::default(),
    )
    .expect("import")
    .report
    .diagnostics
    .iter()
    .map(|d| d.message.clone())
    .collect::<Vec<_>>();
    assert!(
        message.contains(&"Dropped empty <progress> element".to_string()),
        "the linked library is not built from this source: {message:?}"
    );
}
