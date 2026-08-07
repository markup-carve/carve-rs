//! A cross-reference label is a derived-text expansion, and it is budgeted.
//!
//! `</#slug>` republishes the target heading's whole display text while costing
//! only the slug, so K references to one long heading emit `K * heading_len`
//! bytes. That is the abbreviation expansion's shape, so it charges the
//! abbreviation expansion's budget (`markup-carve/carve-rs#805`).

/// A heading of mostly non-slug characters, so the slug (`A`) is far shorter
/// than the display text, plus `n` references to it.
fn amplification_source(heading_len: usize, references: usize) -> String {
    format!(
        "# A{}\n\n{}\n",
        "!".repeat(heading_len - 1),
        "</#A> ".repeat(references)
    )
}

/// Budget = max(1_000_000, 8 * input_len), plus the per-reference cost the
/// reference itself pays for (a degraded label, an anchor) and a slack term.
fn ceiling(input_len: usize, references: usize) -> usize {
    1_000_000usize.max(8 * input_len) + 60 * references + 10_000
}

#[test]
fn crossref_label_expansion_is_bounded_on_every_target() {
    let source = amplification_source(10_000, 1_600);
    let input_len = source.len();
    let ceiling = ceiling(input_len, 1_600);

    let doc = carve::parse(&source);
    let unbounded = 10_000 * 1_600;
    assert!(
        unbounded > 4 * ceiling,
        "the input must be able to overshoot the budget for this test to mean anything"
    );

    let html = carve::to_html(&source);
    assert!(
        html.len() < ceiling,
        "html {} exceeded the budget ceiling {}",
        html.len(),
        ceiling
    );
    let markdown = carve::render_markdown(&doc).expect("markdown renders");
    assert!(
        markdown.len() < ceiling,
        "markdown {} exceeded the budget ceiling {}",
        markdown.len(),
        ceiling
    );
    let plain = carve::render_plain_text(&doc).expect("plain renders");
    assert!(
        plain.len() < ceiling,
        "plain {} exceeded the budget ceiling {}",
        plain.len(),
        ceiling
    );
    let ansi = carve::render_ansi(&doc).expect("ansi renders");
    assert!(
        ansi.len() < ceiling,
        "ansi {} exceeded the budget ceiling {}",
        ansi.len(),
        ceiling
    );
}

/// The bound has to be on the RATIO, not on one measurement: doubling the input
/// must not multiply the output. Unbudgeted, output grows with the square of
/// the input, so the ratio doubles with it.
#[test]
fn doubling_the_input_does_not_multiply_the_output() {
    let small = amplification_source(5_000, 800);
    let large = amplification_source(10_000, 1_600);

    let small_ratio = carve::to_html(&small).len() as f64 / small.len() as f64;
    let large_ratio = carve::to_html(&large).len() as f64 / large.len() as f64;

    assert!(
        large_ratio < small_ratio,
        "amplification ratio grew from {small_ratio:.1}x to {large_ratio:.1}x with input size"
    );
}

/// The degraded label is the AUTHORED target, the way an over-budget
/// abbreviation degrades to its plain key - not an empty string, and not the
/// unresolved `</#A>` source form.
#[test]
fn an_over_budget_label_degrades_to_the_authored_target() {
    let source = amplification_source(10_000, 1_600);
    let html = carve::to_html(&source);
    assert!(
        html.contains("<a href=\"#A\">A</a>"),
        "an over-budget crossref should still anchor, labelled with its target"
    );
}

/// The Carve target reproduces the author's document (PART 11 section 1,
/// `markup-carve/carve#759`): it re-emits `</#A>` rather than the label, so it
/// never amplified and must not be touched by the budget.
#[test]
fn the_carve_target_is_unchanged() {
    let source = amplification_source(10_000, 1_600);
    let out = carve::to_carve(&source);
    assert!(
        out.len() < source.len() + 100,
        "the carve target should reproduce the source, got {} bytes from {}",
        out.len(),
        source.len()
    );
    assert!(out.contains("</#A>"), "the authored reference survives fmt");
}

/// Every target sizes the budget from the same document, so on an input large
/// enough that `8 * len` clears the 1 MB floor they all clip at the same place.
///
/// A target that never installed the guard would fall back to the floor and
/// emit about a third as much here, which is what the plain-text target did
/// before it was given one.
#[test]
fn every_target_scales_the_budget_from_the_same_document() {
    let source = amplification_source(50_000, 50_000);
    assert!(
        8 * source.len() > 2 * 1_000_000,
        "the input must clear the budget floor for this test to mean anything"
    );

    let doc = carve::parse(&source);
    let html = carve::to_html(&source).len();
    let plain = carve::render_plain_text(&doc).expect("plain renders").len();
    let markdown = carve::render_markdown(&doc)
        .expect("markdown renders")
        .len();
    let ansi = carve::render_ansi(&doc).expect("ansi renders").len();

    for (name, len) in [("plain", plain), ("markdown", markdown), ("ansi", ansi)] {
        let ratio = len as f64 / html as f64;
        assert!(
            (0.75..1.25).contains(&ratio),
            "{name} emitted {len} against html's {html}: the two are not sharing one budget"
        );
    }
    assert!(
        plain > 2 * 1_000_000,
        "plain emitted {plain}, which is the floor budget - the guard was not installed"
    );
}

/// CONTROL: an ordinary document is nowhere near the budget, so every label
/// renders in full on every target. If a mutation to the budget broke this,
/// it broke ordinary rendering.
#[test]
fn an_ordinary_document_renders_every_label_in_full() {
    let source =
        "# The Long Heading Here\n\nsee </#the-long-heading-here> and </#the-long-heading-here>\n";
    let doc = carve::parse(source);
    let html = carve::to_html(source);
    assert_eq!(html.matches("The Long Heading Here").count(), 3);
    assert_eq!(
        carve::render_plain_text(&doc)
            .expect("plain renders")
            .matches("The Long Heading Here")
            .count(),
        3
    );
    assert_eq!(
        carve::render_markdown(&doc)
            .expect("markdown renders")
            .matches("The Long Heading Here")
            .count(),
        3
    );
}
