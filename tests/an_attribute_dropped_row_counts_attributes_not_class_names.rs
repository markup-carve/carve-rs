//! One `attribute-dropped` row per ATTRIBUTE, carrying that attribute's whole
//! value (markup-carve/carve#1735).
//!
//! `class` used to get a row per NAME inside it, so `class="a b c d e"` on an
//! element that does not survive reported five losses and `class="a"` reported
//! one - the same event, the same element, the same single attribute. The row
//! count belongs to the document's STRUCTURE, not to one attribute's contents,
//! and a consumer counting losses got a number that moved with how many class
//! names an author happened to type.
//!
//! The split also implied a granularity the loss does not have. Per-name rows
//! read as though individual classes could go independently; they cannot. The
//! attribute went, so every name in it went with it, together.
//!
//! And `class` is the only attribute whose value is a list, so the split was
//! never a general principle being applied consistently - it was one attribute
//! reported unlike every other one in the vocabulary.
//!
//! carve-php and carve-js already report it this way, so this file is what
//! keeps the three engines on the same count.
//!
//! WHAT IS NOT AT STAKE. The code, the severity and the message format are
//! settled by markup-carve/carve#1710 and the work under
//! markup-carve/carve-php#1731. Every assertion below states all three
//! explicitly, so a change that merged the rows and quietly moved the wording
//! or the severity would fail here rather than pass as a row-count fix.

use carve::{
    html_to_carve, HtmlImportDiagnostic, HtmlImportDiagnosticCode, HtmlImportOptions,
    HtmlImportSeverity,
};

fn rows(html: &str) -> Vec<HtmlImportDiagnostic> {
    html_to_carve(html, &HtmlImportOptions::default())
        .expect("imports")
        .report
        .diagnostics
}

fn dropped(html: &str) -> Vec<HtmlImportDiagnostic> {
    rows(html)
        .into_iter()
        .filter(|row| row.code == HtmlImportDiagnosticCode::AttributeDropped)
        .collect()
}

/// The ticket's document. Five names in one attribute is ONE loss.
#[test]
fn five_class_names_in_one_attribute_are_one_row() {
    let rows = dropped(r#"<canvas class="a b c d e">t</canvas>"#);

    assert_eq!(
        rows.len(),
        1,
        "one attribute reported as {} losses: {:?}",
        rows.len(),
        rows.iter().map(|r| &r.message).collect::<Vec<_>>()
    );
    assert_eq!(rows[0].code, HtmlImportDiagnosticCode::AttributeDropped);
    assert_eq!(rows[0].severity, HtmlImportSeverity::Info);
    assert_eq!(
        rows[0].message,
        "Dropped class=\"a b c d e\" on <canvas>: the element was unwrapped and has no node to carry it"
    );
    assert_eq!(rows[0].path.as_deref(), Some("/canvas[1]"));
}

/// THE COUNT DOES NOT MOVE WITH THE CONTENTS. This is the whole of the ruling:
/// one class name and five class names are the same number of losses, because
/// they are the same number of attributes.
#[test]
fn the_row_count_does_not_follow_the_number_of_class_names() {
    let counts: Vec<usize> = ["a", "a b", "a b c", "a b c d e"]
        .iter()
        .map(|value| dropped(&format!("<canvas class=\"{value}\">t</canvas>")).len())
        .collect();

    assert_eq!(
        counts,
        vec![1, 1, 1, 1],
        "the row count followed the class list instead of the attribute list"
    );
}

/// EVERY ATTRIBUTE STILL GETS ITS OWN ROW. Merging the class names must not
/// merge the attributes: an `id`, a `class` and a `data-` pair are three losses
/// and stay three rows, so a filter can still act on one without the others.
#[test]
fn three_attributes_are_still_three_rows() {
    let rows = dropped(r#"<canvas id="i" class="a b" data-x="1">t</canvas>"#);

    assert_eq!(
        rows.iter().map(|r| r.message.clone()).collect::<Vec<_>>(),
        vec![
            "Dropped id=\"i\" on <canvas>: the element was unwrapped and has no node to carry it"
                .to_string(),
            "Dropped class=\"a b\" on <canvas>: the element was unwrapped and has no node to carry it"
                .to_string(),
            "Dropped data-x on <canvas>: the element was unwrapped and has no node to carry it"
                .to_string(),
        ]
    );
    assert!(rows.iter().all(|r| r.severity == HtmlImportSeverity::Info
        && r.code == HtmlImportDiagnosticCode::AttributeDropped));
}

/// THE INLINE UNWRAP IS THE SAME SITE'S SECOND CALLER. A `<small>` keeps its
/// children and nothing else, and its class went the same way - so a fix that
/// only reached the block arm would leave the two arms counting differently.
#[test]
fn an_inline_unwrap_counts_its_class_once_too() {
    let rows = dropped(r#"<p>x <small class="a b c">y</small></p>"#);

    assert_eq!(
        rows.iter().map(|r| r.message.clone()).collect::<Vec<_>>(),
        vec![
            "Dropped class=\"a b c\" on <small>: the element was unwrapped and has no node to carry it"
                .to_string()
        ]
    );
    assert_eq!(rows[0].severity, HtmlImportSeverity::Info);
}

/// A CONTROL ON THE ROW THIS TICKET DOES NOT TOUCH. The element row still
/// stands, still ahead of the attribute rows, at its own code and severity - a
/// merge that swallowed it, or reordered it, would be a different change than
/// the one ruled.
#[test]
fn the_element_row_is_untouched_and_still_first() {
    let rows = rows(r#"<canvas id="i" class="a b">t</canvas>"#);

    assert_eq!(
        rows.iter().map(|r| r.code).collect::<Vec<_>>(),
        vec![
            HtmlImportDiagnosticCode::ElementUnwrapped,
            HtmlImportDiagnosticCode::AttributeDropped,
            HtmlImportDiagnosticCode::AttributeDropped,
        ]
    );
    assert_eq!(rows[0].message, "Unwrapped unsupported <canvas> element");
    assert_eq!(rows[0].severity, HtmlImportSeverity::Info);
}

/// A CONTROL ON AN ATTRIBUTE THAT NEVER SPLIT. A name whose value is not a list
/// reported one row before and reports one row now, with the same words - the
/// change may not reach it.
#[test]
fn an_attribute_whose_value_is_not_a_list_is_unchanged() {
    let rows = dropped(r#"<canvas title="t">x</canvas>"#);

    assert_eq!(
        rows.iter().map(|r| r.message.clone()).collect::<Vec<_>>(),
        vec![
            "Dropped title on <canvas>: the element was unwrapped and has no node to carry it"
                .to_string()
        ]
    );
}
