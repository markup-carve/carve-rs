use std::fmt::Write as _;
use std::time::Instant;

#[test]
fn many_abbreviations_do_not_scan_every_definition_at_every_position() {
    let mut source = String::new();
    for i in 0..1500 {
        writeln!(source, "[A{i}]: expansion {i}").unwrap();
    }
    source.push('\n');
    source.push_str(&"z".repeat(12_000));

    let start = Instant::now();
    let html = carve::to_html(&source);

    assert!(html.contains(&"z".repeat(80)), "{html}");
    assert!(
        start.elapsed().as_secs_f32() < 2.0,
        "abbreviation parse took {:?}",
        start.elapsed()
    );
}

#[test]
fn many_unterminated_colon_fence_openers_do_not_rescan_document() {
    let mut source = String::new();
    source.push_str("intro\n");
    for _ in 0..8_000 {
        source.push_str("::: note\n");
    }

    let start = Instant::now();
    let html = carve::to_html(&source);

    assert!(html.contains("::: note"), "{html}");
    assert!(
        start.elapsed().as_secs_f32() < 2.0,
        "unterminated colon-fence parse took {:?}",
        start.elapsed()
    );
}

#[test]
fn distinct_fence_length_openers_do_not_defeat_closer_cache() {
    // Finding 2: every line opens an unterminated colon fence of a DISTINCT
    // length, so a cache keyed by exact fence length missed every line and did
    // a full forward scan to EOF per line (O(N^2)). Fence lengths cycle in a
    // bounded range so total input bytes stay linear -- any super-linear time
    // here is the per-line rescan, not the input size.
    let mut source = String::from("intro\n");
    for i in 0..20_000 {
        let len = 3 + (i % 60);
        for _ in 0..len {
            source.push(':');
        }
        source.push_str(" |\n");
    }

    let start = Instant::now();
    let html = carve::to_html(&source);

    assert!(html.contains(" |"), "expected literal fence text in output");
    assert!(
        start.elapsed().as_secs_f32() < 2.0,
        "distinct-fence-length colon-fence parse took {:?}",
        start.elapsed()
    );
}

#[test]
fn wide_table_row_colspan_render_is_linear() {
    // A single row with 100k cells and no colspan markers must not re-scan the
    // rest of the row per cell (Finding 3: O(cells^2) colspan resolution).
    let mut source = String::from("|");
    for _ in 0..100_000 {
        source.push_str("x|");
    }

    let start = Instant::now();
    let html = carve::to_html(&source);

    assert!(html.contains("<td>x</td>"), "expected cells in output");
    assert!(
        start.elapsed().as_secs_f32() < 2.0,
        "wide-table colspan render took {:?}",
        start.elapsed()
    );
}

#[test]
fn deeply_nested_list_parse_is_bounded() {
    // Finding 1: deeply nested lists collect-and-reparse the tail per level.
    // MAX_NESTING_DEPTH (40) caps the recursion so the work stays linear in the
    // input bytes; this guards against a regression that would reintroduce a
    // per-level rescan blow-up. 300 levels is far past the depth cap while the
    // input stays small (~180 KB) so the bound holds in a debug build too.
    let mut source = String::new();
    for i in 0..300 {
        for _ in 0..i {
            source.push_str("  ");
        }
        source.push_str("- x\n");
    }

    let start = Instant::now();
    let html = carve::to_html(&source);

    assert!(html.contains("<li>x"), "expected nested list items");
    assert!(
        start.elapsed().as_secs_f32() < 2.0,
        "deeply nested list parse took {:?}",
        start.elapsed()
    );
}

#[test]
fn deeply_nested_div_parse_is_bounded() {
    // Finding 4: deeply nested divs collect-and-reparse per level, and each
    // opener is an unterminated colon fence of a distinct length. With the
    // colon-closer suffix-max cache (Finding 2) and the MAX_NESTING_DEPTH cap,
    // the work stays linear in the input bytes. 600 levels is well past the
    // depth cap while the input stays small enough to hold the bound in debug.
    let mut source = String::new();
    for i in 0..600 {
        for _ in 0..(3 + i) {
            source.push(':');
        }
        source.push_str(" d\n");
    }
    source.push('x');

    let start = Instant::now();
    let html = carve::to_html(&source);

    assert!(!html.is_empty(), "expected output");
    assert!(
        start.elapsed().as_secs_f32() < 2.0,
        "deeply nested div parse took {:?}",
        start.elapsed()
    );
}
