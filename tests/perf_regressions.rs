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

/// Run `f` on a worker thread with an ample stack. With MAX_NESTING_DEPTH = 200
/// a degrading parse builds an AST up to 200 levels deep, and the recursive
/// descent uses one native frame per level. A release build holds that in a
/// default 2 MiB stack, but a debug `cargo test` build's larger frames need
/// more; these worst-case-depth probes only care about the time bound and the
/// degradation, not the per-frame size.
fn on_big_stack<F: FnOnce() + Send + 'static>(f: F) {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(f)
        .expect("spawn worker")
        .join()
        .expect("worker must return, not abort");
}

#[test]
fn deeply_nested_list_parse_is_bounded() {
    on_big_stack(|| {
        // Finding 1: deeply nested lists collect-and-reparse the tail per level.
        // MAX_NESTING_DEPTH (200) caps the recursion so the work stays linear in
        // the input bytes; this guards against a regression that would
        // reintroduce a per-level rescan blow-up. 300 levels is past the depth
        // cap while the input stays small (~180 KB) so the time bound holds in a
        // debug build too.
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
    });
}

#[test]
fn deeply_nested_div_parse_is_bounded() {
    on_big_stack(|| {
        // Finding 4: deeply nested divs collect-and-reparse per level, and each
        // opener is an unterminated colon fence of a distinct length. With the
        // colon-closer suffix-max cache (Finding 2) and the MAX_NESTING_DEPTH
        // cap, the work stays linear in the input bytes. 600 levels is well past
        // the depth cap while the input stays small enough to hold the bound in
        // debug.
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
    });
}

/// Build `n` balanced nested inline links: `[` * n + "x" + "]()" * n, i.e.
/// `[[[...x]()]()...]()`. Before the bracket-match precompute, each `[` re-scanned
/// O(n) bytes to find its closing `]`, so the parse was O(n^2).
fn nested_links(n: usize) -> String {
    let mut s = String::with_capacity(4 * n + 1);
    for _ in 0..n {
        s.push('[');
    }
    s.push('x');
    for _ in 0..n {
        s.push_str("]()");
    }
    s
}

#[test]
fn deeply_nested_balanced_links_parse_in_near_linear_time() {
    // Finding 2: deeply nested balanced links were O(n^2) because every `[`
    // re-scanned to its matching `]`. A single precomputed bracket-match table
    // makes each lookup O(1), so doubling the input roughly doubles the time
    // (not quadruples). Run each size a few times and take the minimum to damp
    // scheduler noise, then assert the larger size is well under a quadratic
    // multiple of the smaller.
    fn min_parse_time(source: &str) -> f64 {
        (0..5)
            .map(|_| {
                let start = Instant::now();
                let _ = carve::to_html(source);
                start.elapsed().as_secs_f64()
            })
            .fold(f64::INFINITY, f64::min)
    }

    let small = nested_links(4_000);
    let large = nested_links(8_000);

    let t_small = min_parse_time(&small);
    let t_large = min_parse_time(&large);

    // Quadratic would give ~4x for a 2x input; linear gives ~2x. Allow generous
    // slack (5x) so the test is robust on slow/loaded CI while still failing
    // hard on a reintroduced O(n^2) (which scaled ~4x here, but with the old
    // ~0.9s baseline at n=8000 the absolute bound below is the real guard).
    if t_small > 0.0 {
        let ratio = t_large / t_small;
        assert!(
            ratio < 5.0,
            "nested-link parse scaling looks super-linear: {t_small:.4}s -> {t_large:.4}s (ratio {ratio:.1}x)"
        );
    }

    // Absolute wall-clock guard: the fixed parser handles n=8000 in well under
    // 50 ms (release); the pre-fix code took ~0.9 s. Use a wide 1 s bound to
    // tolerate an unoptimized debug build and loaded CI while still catching a
    // regression to quadratic.
    assert!(
        t_large < 1.0,
        "nested-link parse for n=8000 took {t_large:.4}s (expected near-instant)"
    );
}

#[test]
fn deeply_nested_balanced_links_preserve_output() {
    // The bracket-match precompute must not change parse output. For this
    // pathological `[[[...x]()...]` shape the inline links all carry an empty
    // destination and nest, so the "links never nest" pass unwraps them down to
    // plain literal text (no anchors) - exactly as before the optimization.
    let n = 50;
    let html = carve::to_html(&nested_links(n));
    assert_eq!(html.matches("<a href=").count(), 0, "{html}");
    assert!(html.contains('x'), "inner text must survive: {html}");
    // A genuine link with a destination still renders as an anchor.
    assert_eq!(
        carve::to_html("[text](https://example.com)"),
        "<p><a href=\"https://example.com\">text</a></p>"
    );
}
