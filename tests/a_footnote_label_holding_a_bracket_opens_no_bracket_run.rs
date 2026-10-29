//! A `[^` reference is read before bracket scanning (PART 8) and its label runs
//! to the first `]`, so a `[` in the label opens no bracket run and a delimiter
//! after the reference can close a span opened before it (carve-rs#3499,
//! corpus 773 to 775).

use carve::to_html;

fn html(source: &str) -> String {
    to_html(source).trim().to_string()
}

#[test]
fn a_delimiter_after_the_reference_closes_the_span() {
    assert_eq!(html("/[^[g]/] b/\n"), "<p><em>[^[g]</em>] b/</p>");
    assert_eq!(html("*[^[g]*] b*\n"), "<p><strong>[^[g]</strong>] b*</p>");
    assert_eq!(html("_[^a[b]_] c_\n"), "<p><u>[^a[b]</u>] c_</p>");
    assert!(html("/[^[g]/] b/\n\n[^[g]: note\n").starts_with(
        "<p><em><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></em>] b/</p>"
    ));
}

/// A run's content is read again as inline text, so the reference is atomic
/// inside a run too, while the run keeps its plain extent.
#[test]
fn a_run_keeps_its_extent_and_reads_the_reference_inside() {
    assert_eq!(html("[a /[^[g]/] b/]\n"), "<p>[a <em>[^[g]</em>] b/]</p>");
    assert_eq!(
        html("[a /[^[g]/] b](u)\n"),
        "<p><a href=\"u\">a <em>[^[g]</em>] b</a></p>"
    );
    assert_eq!(html("/[[^[g]](u) b/\n"), "<p><em>[[^[g]](u) b</em></p>");
    assert_eq!(html("[a [^[g] b](u)\n"), "<p>[a [^[g] b](u)</p>");
    assert_eq!(html("{+[^[g]+}] b+}\n"), "<p><ins>[^[g]</ins>] b+}</p>");
}

/// `![` with an image tail reads as an image first; without one the `!` is text.
#[test]
fn an_image_opener_wins_only_with_a_tail() {
    assert_eq!(html("![[^[g]/](i.png) /\n"), "<p>![[^[g]/](i.png) /</p>");
    assert_eq!(
        html("/![^[g]/](i.png) b/\n"),
        "<p><em><img src=\"i.png\" alt=\"^[g]/\"> b</em></p>"
    );
    assert_eq!(html("/![^[g]/] b/\n"), "<p><em>![^[g]</em>] b/</p>");
    assert_eq!(
        html("/![^[g]/](<x b/\n"),
        "<p><em>![^[g]</em>](&lt;x b/</p>"
    );
}

/// An inline note's body reads no `[^` reference (SS16), so there the run is an
/// ordinary bracket run.
#[test]
fn a_note_body_keeps_the_bracket_run() {
    assert!(html("^[/[^[g]/] b/]\n").contains("<p><em>[^[g]/] b</em><a href=\"#fnref1\""));
}

/// The editorial closer scan reads the reference before a bracket run, so an
/// image opener behind `!` does not hide the closer (`edScan`, corpus 774).
#[test]
fn an_editorial_scan_reads_the_reference_before_an_image() {
    assert_eq!(
        html("{+![^[g]+}](u) b+}\n"),
        "<p><ins>![^[g]</ins>](u) b+}</p>"
    );
    assert_eq!(
        html("{-a ![^[g]-}][x] b\n"),
        "<p><del>a ![^[g]</del>][x] b</p>"
    );
    assert_eq!(
        html("{~![^[g]~>a](u) b~>c~}\n"),
        "<p><del>![^[g]</del><ins>a](u) b~&gt;c</ins></p>"
    );
}
