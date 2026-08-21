//! A group left open at end of input closes there - and has no caption slot.
//!
//! PART 9 §4c defers body and closer discipline to §12's container rules: an
//! unterminated `::: figure` closes at end of input like any container. The
//! caption slot hangs on the CLOSING fence, and that line was never written,
//! so an end-of-input group has no caption position - on the ordinary path
//! and on the closer-free ladder fast path alike.

#[test]
fn the_body_still_forms_panels() {
    let html = carve::to_html("::: figure\n![one](a.png)\n^ (a) One\n");
    assert_eq!(
        html,
        "<figure class=\"carve-figure-group\">\n  <figure class=\"carve-figure-panel\">\n    <img src=\"a.png\" alt=\"one\">\n    <figcaption>(a) One</figcaption>\n  </figure>\n</figure>"
    );
}

#[test]
fn the_ladder_fast_path_builds_the_same_group() {
    // No closer anywhere: the EOF-closed colon ladder is a separate
    // construction site and must agree (its guard is positions OFF).
    let html = carve::to_html("::: figure\n:::: note\ntext\n");
    assert_eq!(
        html,
        "<figure class=\"carve-figure-group\">\n  <aside class=\"admonition note\" aria-label=\"Note\">\n    <p>text</p>\n  </aside>\n</figure>"
    );
}

#[test]
fn a_nested_bare_opener_demotes_on_the_ladder_too() {
    let html = carve::to_html("::: figure\n:::: figure\ntext\n");
    assert_eq!(
        html,
        "<figure class=\"carve-figure-group\">\n  <div class=\"figure\">\n    <p>text</p>\n  </div>\n</figure>"
    );
}

#[test]
fn both_paths_agree_with_positions_on() {
    // The ladder runs only with positions off; the ordinary path must build
    // the identical tree with them on.
    let source = "::: figure\n:::: figure\ntext\n";
    let plain = carve::to_html(source);
    let with_positions = carve::render_html(&carve::parse_with_options(
        source,
        &carve::Options::default().with_positions(true),
    ))
    .expect("renders");
    assert_eq!(plain, with_positions);
}
