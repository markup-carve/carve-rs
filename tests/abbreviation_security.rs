//! Security regressions for abbreviation definition parsing.

#[test]
fn empty_abbreviation_definition_is_literal_text() {
    let doc = carve::parse("*[]: x\n\nA");
    assert!(!doc
        .children
        .iter()
        .any(|node| matches!(node, carve::BlockNode::AbbreviationDef(def) if def.abbr.is_empty())));
    assert_eq!(carve::to_html("*[]: x\n\nA"), "<p>*[]: x</p>\n<p>A</p>");
}

#[test]
fn non_alphanumeric_abbreviation_definition_is_literal_text() {
    assert_eq!(
        carve::to_html("*[C++]: C Plus Plus\n\nC++"),
        "<p>*[C++]: C Plus Plus</p>\n<p>C++</p>"
    );
}

// --- abbreviation-expansion budget (memory-amplification DoS) ---------------

/// Build a worst-case amplification input: a large expansion for `HT` plus
/// `count` whitespace-separated `HT` occurrences. Output without a budget would
/// be `expansion_len * count` bytes, far larger than the input.
fn amplification_source(expansion_len: usize, count: usize) -> String {
    let occurrences = vec!["HT"; count].join(" ");
    let expansion = "X".repeat(expansion_len);
    format!("{occurrences}\n\n*[HT]: {expansion}")
}

#[test]
fn abbr_expansion_output_is_bounded_for_html() {
    let source = amplification_source(50_000, 2_500);
    let input_len = source.len();
    let start = std::time::Instant::now();
    let html = carve::to_html(&source);
    let elapsed = start.elapsed();

    // Budget = max(1_000_000, 8 * input_len). Output must stay within a small
    // constant of that budget (plus the unavoidable `HT` key text and tags),
    // not balloon to expansion_len * count (~125 MB here).
    let budget = 1_000_000usize.max(8 * input_len);
    assert!(
        html.len() < budget + 2 * input_len + 1_000,
        "html output {} exceeded bounded budget {} (input {})",
        html.len(),
        budget,
        input_len
    );
    assert!(
        elapsed.as_secs_f32() < 2.0,
        "bounded abbr render took {elapsed:?}"
    );
}

#[test]
fn abbr_expansion_output_is_bounded_for_markdown_and_ansi() {
    let source = amplification_source(50_000, 2_500);
    let input_len = source.len();
    let budget = 1_000_000usize.max(8 * input_len);

    let md = carve::to_markdown(&source);
    assert!(
        md.len() < budget + 2 * input_len + 1_000,
        "markdown output {} exceeded bounded budget {}",
        md.len(),
        budget
    );

    let ansi = carve::to_ansi(&source);
    assert!(
        ansi.len() < budget + 2 * input_len + 1_000,
        "ansi output {} exceeded bounded budget {}",
        ansi.len(),
        budget
    );
}

#[test]
fn abbr_degradation_emits_plain_key_past_budget() {
    // Past the budget the wrapper/title are dropped, leaving the bare key.
    let source = amplification_source(50_000, 2_500);
    let html = carve::to_html(&source);
    // Some early occurrences keep the wrapper (under budget); later ones degrade
    // to plain `HT`. The degraded ones must not carry a title attribute.
    assert!(
        html.contains("<abbr title=\""),
        "at least one wrapper expected"
    );
    // The bare key appears between two spaces once degradation kicks in.
    assert!(html.contains(" HT "), "degraded plain key expected");
}

#[test]
fn normal_abbreviation_renders_identically_under_budget() {
    // A small expansion with few occurrences stays far under the budget and must
    // render the full `<abbr title=...>` wrapper in every format, unchanged.
    let source = "The HTML spec is essential reading.\n\n*[HTML]: HyperText Markup Language";
    assert_eq!(
        carve::to_html(source),
        "<p>The <abbr title=\"HyperText Markup Language\">HTML</abbr> spec is essential reading.</p>"
    );
    assert!(carve::to_markdown(source)
        .contains("<abbr title=\"HyperText Markup Language\">HTML</abbr>"));
    assert!(carve::to_ansi(source).contains("(HyperText Markup Language)"));
}

#[test]
fn many_small_abbreviations_stay_under_budget() {
    // Even hundreds of legitimate occurrences of a modest expansion stay well
    // under the 1 MB floor, so the wrapper is never dropped for a real document.
    let occurrences = vec!["HT"; 500].join(" ");
    let source = format!("{occurrences}\n\n*[HT]: HyperText Markup Language");
    let html = carve::to_html(&source);
    let wrappers = html.matches("<abbr title=").count();
    assert_eq!(wrappers, 500, "every occurrence should keep its wrapper");
}
