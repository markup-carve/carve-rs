//! A `[^` reference is read before bracket scanning (PART 8) and its label runs
//! to the first `]`, so a `[` in the label opens no bracket run and a delimiter
//! after the reference can close a span opened before it (carve-rs#3499,
//! corpus 773).

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

/// Inside a closed run, or behind `!`, the label's brackets nest as usual.
#[test]
fn enclosing_runs_and_image_openers_keep_plain_nesting() {
    assert_eq!(
        html("[a /[^[g]/] b](u)\n"),
        "<p><a href=\"u\">a <em>[^[g]</em>] b</a></p>"
    );
    assert_eq!(html("/[[^[g]](u) b/\n"), "<p><em>[[^[g]](u) b</em></p>");
    assert_eq!(html("![[^[g]/](i.png) /\n"), "<p>![[^[g]/](i.png) /</p>");
}
