//! AN ELEMENT CONSUMED FOR ITS CHILDREN STILL REPORTS ITS OWN ATTRIBUTES
//! (carve-rs#1257, sibling of markup-carve/carve-js#1332).
//!
//! A caption line holds inline content and has no attribute slot, so a
//! `<figcaption>` and a table `<caption>` are read for their CHILDREN and the
//! element itself contributes no node. This importer already routed a
//! `<figcaption>` through `attrs` for exactly that reason; the table
//! `<caption>` was lifted out of the child list by hand and its children read
//! straight off it, so its own attributes were never looked at. An `onclick`
//! there was stripped - correctly - and never mentioned.
//!
//! A SILENT DROP IS THE ONE FAILURE MODE THE REPORT EXISTS TO PREVENT, and it
//! is worse than a wrong path: a wrong path is visible and gets fixed, a
//! missing row reads as "nothing was dropped". carve-php reported this input;
//! this engine and carve-js did not, and no fixture covered the shape, which is
//! how three engines disagreed in silence.
//!
//! THE FIX IS THE CATEGORY. `caption_inlines` is the answer the importer
//! already had - it takes the tag name now instead of assuming `figcaption` -
//! and the sweep that found the table caption also found a `<dd>` with no
//! `<dt>` before it, which keeps its content as blocks ahead of the list and
//! dropped its own attributes just as quietly. Naming `<caption>` in a branch
//! would have fixed the reported input and left that one exactly as silent,
//! which is what the rows below are here to prove.

use carve::{html_to_carve, HtmlImportDiagnosticCode, HtmlImportOptions};

fn dropped(html: &str) -> Vec<(String, Option<String>)> {
    html_to_carve(html, &HtmlImportOptions::default())
        .expect("import")
        .report
        .diagnostics
        .into_iter()
        .filter(|d| d.code == HtmlImportDiagnosticCode::AttributeDropped)
        .map(|d| (d.message, d.path))
        .collect()
}

/// The reported case, and the neighbours the sweep swept it up with. The table
/// caption rows are the ticket's own input; the figcaption rows are the control
/// that already worked, and they are here so a regression in either direction
/// shows up as a failing row rather than as a quiet one.
#[test]
fn every_caption_site_reports_the_attributes_it_cannot_carry() {
    let cases: [(&str, &str, &str); 5] = [
        (
            "a figcaption event handler",
            r#"<figure><img src="i.png"><figcaption onclick="x()">c</figcaption></figure>"#,
            "/figure[1]/figcaption[2]",
        ),
        (
            "a figcaption id and class",
            r#"<figure><img src="i.png"><figcaption id="cap" class="c">c</figcaption></figure>"#,
            "/figure[1]/figcaption[2]",
        ),
        (
            "a table caption event handler",
            r#"<table><caption onclick="x()">c</caption><tr><td>a</td></tr></table>"#,
            "/table[1]/caption[1]",
        ),
        (
            "a table caption id and class",
            r#"<table><caption id="tc" class="k">c</caption><tr><td>a</td></tr></table>"#,
            "/table[1]/caption[1]",
        ),
        (
            "a figure group's own caption",
            r#"<figure class="carve-figure-group"><div class="carve-figure-panels"><figure class="carve-figure-panel"><img src="a.png"><figcaption>p</figcaption></figure></div><figcaption onclick="x()">g</figcaption></figure>"#,
            "/figure[1]/figcaption[2]",
        ),
    ];
    for (label, html, path) in cases {
        let rows = dropped(html);
        assert!(!rows.is_empty(), "{label}: nothing reported for {html}");
        assert!(
            rows.iter().any(|(_, p)| p.as_deref() == Some(path)),
            "{label}: expected a row at {path}, got {rows:?}"
        );
    }
}

/// The slot the caption could not hold it in is NAMED, and named the same way
/// for both spellings of a caption - the reader is told which element carried
/// the attribute and why the model has nowhere to put it.
#[test]
fn a_table_caption_names_the_caption_line_as_the_slot_that_could_not_hold_it() {
    assert_eq!(
        dropped(r#"<table><caption id="tc">c</caption><tr><td>a</td></tr></table>"#),
        vec![(
            "Dropped id=\"tc\" on <caption>: a caption line carries no attributes".to_owned(),
            Some("/table[1]/caption[1]".to_owned()),
        )],
    );
    assert_eq!(
        dropped(r#"<figure><img src="i.png"><figcaption id="cap">c</figcaption></figure>"#),
        vec![(
            "Dropped id=\"cap\" on <figcaption>: a caption line carries no attributes".to_owned(),
            Some("/figure[1]/figcaption[2]".to_owned()),
        )],
    );
}

/// The report is a record of a loss, not a licence to keep the value: the
/// conversion is unchanged and the event handler is still stripped.
#[test]
fn a_reported_caption_attribute_is_still_dropped() {
    let result = html_to_carve(
        r#"<table><caption onclick="x()">c</caption><tr><td>a</td></tr></table>"#,
        &HtmlImportOptions::default(),
    )
    .expect("import");
    assert!(!result.value.contains("onclick"), "{}", result.value);
    assert_eq!(result.value, "| a |\n^ c\n");
}

/// A caption carrying nothing stays silent. An unconditional row would be the
/// mirror defect - a report claiming a drop that never happened, which is what
/// markup-carve/carve-php#1579 was accepted for removing.
#[test]
fn a_caption_that_carries_nothing_reports_nothing() {
    let result = html_to_carve(
        "<table><caption>c</caption><tr><td>a</td></tr></table>",
        &HtmlImportOptions::default(),
    )
    .expect("import");
    assert!(
        result.report.diagnostics.is_empty(),
        "{:?}",
        result.report.diagnostics
    );
}

/// THE SECOND SITE THE SWEEP FOUND. A `<dd>` with no `<dt>` before it cannot
/// become a definition group - `:  text` alone re-reads as a paragraph - so its
/// content is emitted as blocks ahead of the list and the `element-unwrapped`
/// row states the role that did not survive. It said nothing about the
/// attributes that went with the role, which is the same silence reached by a
/// different route: every other `<dd>` puts them on its `DefinitionDef`, and
/// this one has no node to put them on.
#[test]
fn a_dd_with_no_dt_reports_the_attributes_it_lost_with_its_role() {
    let rows = dropped(r#"<dl><dd onclick="x()" id="q">a</dd></dl>"#);
    assert!(
        rows.iter().any(|(m, _)| m.contains("onclick")),
        "the event handler is unreported: {rows:?}"
    );
    assert!(
        rows.iter().any(|(m, p)| m
            == "Dropped id=\"q\" on <dd>: a <dd> with no <dt> keeps its content as blocks, and blocks ahead of the list have no slot for it"
            && p.as_deref() == Some("/dl[1]/dd[1]")),
        "the id is unreported: {rows:?}"
    );
    // The role row is still there; this change adds to the report rather than
    // rewording what it already said.
    let codes: Vec<_> = html_to_carve(
        r#"<dl><dd id="q">a</dd></dl>"#,
        &HtmlImportOptions::default(),
    )
    .expect("import")
    .report
    .diagnostics
    .into_iter()
    .map(|d| d.code)
    .collect();
    assert!(
        codes.contains(&HtmlImportDiagnosticCode::ElementUnwrapped),
        "{codes:?}"
    );
}

/// A `<dd>` that DOES have a term before it puts its attributes on the
/// `DefinitionDef` the importer builds, so it must stay silent - the new row
/// belongs to the roleless one alone.
#[test]
fn a_dd_that_keeps_its_role_says_nothing() {
    let result = html_to_carve(
        r#"<dl><dt>t</dt><dd id="q">d</dd></dl>"#,
        &HtmlImportOptions::default(),
    )
    .expect("import");
    assert!(
        result.report.diagnostics.is_empty(),
        "{:?}",
        result.report.diagnostics
    );
}
