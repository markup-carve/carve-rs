use std::fmt::Write as _;
use std::sync::Mutex;
use std::time::Instant;

/// Serializes the TIMING tests in this file against each other.
///
/// Every test here measures wall clock, and `cargo test` runs the tests in a
/// binary on parallel threads - so 34 timing tests spend their measurements
/// competing with each other for cores. That is the whole cause of the flake in
/// carve-rs#523: `unterminated_comment_fence_openers_parse_in_near_linear_time`
/// failed 2 of 8 full-file runs and 0 of 6 with `--test-threads=1`.
///
/// More rounds does not fix it, which is worth recording because it is the
/// obvious move: median-of-five measured 3 of 8 failures against main's 2 of 8.
/// The contention is sustained for the whole run rather than a transient spike,
/// so every sample is contaminated and the median moves with them.
///
/// The estimator and the bounds are untouched. This removes the cause instead
/// of widening the tolerance - a ratio bound that had to be loosened to survive
/// its own test suite would no longer be measuring the engine.
static PERF_LOCK: Mutex<()> = Mutex::new(());

/// Take the timing lock, ignoring poisoning: a panic in one perf test must not
/// cascade into every other one reporting a lock error instead of its result.
fn perf_guard() -> std::sync::MutexGuard<'static, ()> {
    PERF_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Wall-clock ceiling for the DoS guards below.
///
/// These tests guard against reintroducing QUADRATIC behavior, not against
/// small constant-factor drift: on these inputs a per-position rescan costs
/// tens of seconds to minutes, while the linear implementations finish in
/// well under a second. The bound therefore needs enough headroom to survive
/// a loaded, shared CI runner in a debug build - a tight 2s cap flaked
/// repeatedly on `deeply_nested_list_parse_is_bounded` (2.07s / 2.33s on CI
/// while taking ~0.8s locally), reddening main on unrelated commits.
///
/// A ratio-based check was tried and removed for the same timing noise
/// (PR 337 / 338); the wall-clock cap stays the guard, calibrated wide.
const MAX_SECS: f32 = 10.0;

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
        start.elapsed().as_secs_f32() < MAX_SECS,
        "abbreviation parse took {:?}",
        start.elapsed()
    );
}

#[test]
fn many_unterminated_colon_fence_openers_do_not_rescan_document() {
    on_big_stack(|| {
        let mut source = String::new();
        source.push_str("intro\n");
        for _ in 0..8_000 {
            source.push_str("::: note\n");
        }

        let start = Instant::now();
        let html = carve::to_html(&source);

        assert!(!html.is_empty(), "expected bounded output");
        assert!(
            start.elapsed().as_secs_f32() < MAX_SECS,
            "unterminated colon-fence parse took {:?}",
            start.elapsed()
        );
    });
}

#[test]
fn long_single_paragraph_does_not_rescan_prior_lines() {
    let mut source = String::new();
    for i in 0..20_000 {
        writeln!(source, "continuation line {i} of one big paragraph here").unwrap();
    }

    let start = Instant::now();
    let html = carve::to_html(&source);

    assert!(
        html.contains("one big paragraph"),
        "expected paragraph output"
    );
    assert!(
        start.elapsed().as_secs_f32() < MAX_SECS,
        "long single-paragraph parse took {:?}",
        start.elapsed()
    );
}

#[test]
fn distinct_fence_length_openers_parse_bounded() {
    // Every line opens an unterminated colon fence of a DISTINCT length. Fence
    // lengths cycle in a bounded range so total input bytes stay linear; this
    // guards the colon-body scan and nesting cap under the EOF-close rule.
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

    assert!(html.contains(" |"), "expected fence body text in output");
    assert!(
        start.elapsed().as_secs_f32() < MAX_SECS,
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
        start.elapsed().as_secs_f32() < MAX_SECS,
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
            start.elapsed().as_secs_f32() < MAX_SECS,
            "deeply nested list parse took {:?}",
            start.elapsed()
        );
    });
}

#[test]
fn deeply_nested_div_parse_is_bounded() {
    on_big_stack(|| {
        // Finding 4: deeply nested divs collect-and-reparse per level, and each
        // opener is an unterminated colon fence of a distinct length. The
        // colon-body nesting cap bounds recursion even though each opener is a
        // real block that closes at EOF. 600 levels is well past the cap while
        // the input stays small enough to hold the bound in debug.
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
            start.elapsed().as_secs_f32() < MAX_SECS,
            "deeply nested div parse took {:?}",
            start.elapsed()
        );
    });
}

/// Build `n` balanced nested inline links: `[` * n + "x" + "]()" * n, i.e.
/// `[[[...x]()]()...]()`. Before the bracket-match precompute, each `[` re-scanned
/// O(n) bytes to find its closing `]`; after it, each `[` still eagerly copied
/// its label to a `String` before validating the `()` target, so the parse was
/// still O(n^2) on this shape (the target never validates, so every one of the
/// n candidate `[` paid an O(n) label copy).
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

/// The image variant of `nested_links`: `![` * n + "x" + "]()" * n. Exercises
/// the same label-allocation path through `parse_image_at`.
fn nested_images(n: usize) -> String {
    let mut s = String::with_capacity(5 * n + 1);
    for _ in 0..n {
        s.push_str("![");
    }
    s.push('x');
    for _ in 0..n {
        s.push_str("]()");
    }
    s
}

/// Interleaved, median-of-three, PER-BYTE scaling sample.
///
/// The guards below used to time the small size a few times, then the large
/// size a few times, and compare the two totals. That is mis-calibrated rather
/// than merely unlucky: a healthy parse measures ~2x for a 2x input, so a "< 3x"
/// threshold sat only 1.5x above the expected value, and either sample could be
/// taken while the runner was busy. carve-js and carve-php hit exactly this and
/// were reworked the same way.
///
/// Three changes make the assertion robust without weakening it:
///
/// - Compare cost PER BYTE, not total elapsed. "Linear" means per-byte cost is
///   constant as input grows, so a healthy parse measures ~1 and a quadratic one
///   measures the size multiple itself. With the 4x multiple used here the
///   threshold sits midway between 1 and 4 instead of between 2 and 4. This is
///   also build-invariant: a debug build is ~10-20x slower per byte, but
///   uniformly so, and the ratio is unchanged.
/// - INTERLEAVE the sizes. Timing all the small runs and then all the large runs
///   lets a runner busy for only part of the test skew one side of the ratio;
///   alternating means load drift lands on both.
/// - Take the MEDIAN of the rounds. A mean is still dragged by one stall, and a
///   minimum throws away the fact that the machine was loaded at all.
struct Scaling {
    small_secs: f64,
    large_secs: f64,
    small_per_byte: f64,
    large_per_byte: f64,
}

impl Scaling {
    /// Per-byte growth across the size multiple: ~1 when linear, ~SIZE_MULTIPLE
    /// when quadratic.
    fn per_byte_ratio(&self) -> f64 {
        self.large_per_byte / self.small_per_byte.max(f64::MIN_POSITIVE)
    }
}

/// Small/large input sizes. A 4x multiple separates linear (~1x per byte) from
/// quadratic (~4x per byte) far more cleanly than the doubling it replaces, and
/// costs less total work than the old min-of-5/min-of-7 at 100k/200k did.
const SCALE_SMALL_N: usize = 50_000;
const SCALE_LARGE_N: usize = 200_000;
const SCALE_ROUNDS: usize = 3;

/// A healthy parse measures ~1.0 (worst real shape here measured 1.16); a
/// quadratic parse measures ~4.0. Sitting at 2.0 leaves roughly a 1.7x margin
/// above the noisiest healthy shape and a 2x margin below a real regression.
const SCALE_MAX_PER_BYTE_RATIO: f64 = 2.0;

fn measure_scaling(build: &impl Fn(usize) -> String) -> Scaling {
    measure_scaling_at(build, SCALE_SMALL_N, SCALE_LARGE_N)
}

/// `measure_scaling` at explicit sizes, keeping the 4x multiple.
///
/// A BLOCK-level shape is several LINES per unit, where the inline shapes above
/// are a few bytes, so the default 50k/200k builds a document two orders of
/// magnitude larger than such a shape needs to separate linear from quadratic.
/// Only the sizes move; the interleaving, the rounds and the median are shared,
/// because a second spelling of the timing is what this helper exists to avoid.
fn measure_scaling_at(build: &impl Fn(usize) -> String, small_n: usize, large_n: usize) -> Scaling {
    measure_conversion_scaling_at(
        &|source| drop(carve::to_html(source)),
        build,
        small_n,
        large_n,
    )
}

/// `measure_scaling_at` over any conversion, not just `to_html`.
///
/// Split out so a shape whose cost lives on ANOTHER entry point - an HTML
/// import, say - is measured by THIS calibration rather than by a second
/// spelling of the timing. The interleaving, the rounds and the best-of are
/// exactly what this helper exists to keep in one place.
fn measure_conversion_scaling_at(
    convert: &impl Fn(&str),
    build: &impl Fn(usize) -> String,
    small_n: usize,
    large_n: usize,
) -> Scaling {
    let _guard = perf_guard();
    let small = build(small_n);
    let large = build(large_n);
    let small_bytes = small.len() as f64;
    let large_bytes = large.len() as f64;

    // Prime caches/allocator so round 1 does not measure warm-up.
    convert(&small);
    convert(&large);

    let time_once = |source: &str| {
        let start = Instant::now();
        convert(source);
        start.elapsed().as_secs_f64()
    };

    let mut small_samples = Vec::with_capacity(SCALE_ROUNDS);
    let mut large_samples = Vec::with_capacity(SCALE_ROUNDS);
    for round in 0..SCALE_ROUNDS {
        // ALTERNATE which size is timed first, which is what the doc comment
        // above already promises. Measuring small then large every round leaves
        // exactly the bias it describes: the second sample is always taken
        // later, so load that ramps during the test lands on `large`
        // systematically and inflates the ratio in the one direction the
        // threshold is watching.
        if round % 2 == 0 {
            small_samples.push(time_once(&small));
            large_samples.push(time_once(&large));
        } else {
            large_samples.push(time_once(&large));
            small_samples.push(time_once(&small));
        }
    }

    // BEST of the rounds, not the median. Scheduler noise is one-sided: it can
    // only make a sample slower, never faster, so the minimum is the closest
    // estimate of the parse's own cost and the median carries whatever load the
    // machine happened to be under. With a median, the ratio clustered at
    // 2.02-2.06x against a 2.0 cutoff and tripped roughly one run in three while
    // the parse was unchanged - which trains everyone to re-run a red perf test,
    // and that is how a real quadratic regression gets waved through
    // (carve-rs#952).
    //
    // It does not weaken the guard. A quadratic shape is ~4x per byte at 4x the
    // input in EVERY round, so its minimum is quadratic too; only the noise the
    // threshold was catching goes away.
    let best = |xs: Vec<f64>| -> f64 { xs.into_iter().fold(f64::INFINITY, f64::min) };

    let small_secs = best(small_samples);
    let large_secs = best(large_samples);

    Scaling {
        small_secs,
        large_secs,
        small_per_byte: small_secs / small_bytes,
        large_per_byte: large_secs / large_bytes,
    }
}

/// Assert `build(n)` parses without an O(n^2) blowup.
///
/// NOTE: these sizes (50k / 200k, i.e. ~200 KB / ~800 KB of input) are chosen to
/// expose a quadratic *constant* -- the old n=4000/8000 sizes ran in low
/// single-digit milliseconds and could not distinguish linear from quadratic
/// through scheduler noise. Run against a release build (`cargo test --release`);
/// a debug build is ~10-20x slower and may exceed the absolute bound without any
/// regression, though the per-byte ratio itself is build-invariant.
fn assert_near_linear(build: impl Fn(usize) -> String, label: &str) {
    assert_near_linear_at(build, label, SCALE_SMALL_N, SCALE_LARGE_N);
}

/// `assert_near_linear` at explicit sizes. See `measure_scaling_at`.
fn assert_near_linear_at(
    build: impl Fn(usize) -> String,
    label: &str,
    small_n: usize,
    large_n: usize,
) {
    assert_conversion_near_linear_at(
        |source| drop(carve::to_html(source)),
        build,
        label,
        small_n,
        large_n,
    );
}

/// `assert_near_linear_at` over any conversion. See `measure_conversion_scaling_at`.
fn assert_conversion_near_linear_at(
    convert: impl Fn(&str),
    build: impl Fn(usize) -> String,
    label: &str,
    small_n: usize,
    large_n: usize,
) {
    let scaling = measure_conversion_scaling_at(&convert, &build, small_n, large_n);

    let ratio = scaling.per_byte_ratio();
    assert!(
        ratio < SCALE_MAX_PER_BYTE_RATIO,
        "{label} per-byte cost grew {ratio:.2}x at {}x the input (linear ~1x, quadratic ~{}x): \
         small={:.4}us/byte large={:.4}us/byte",
        large_n / small_n,
        large_n / small_n,
        scaling.small_per_byte * 1e6,
        scaling.large_per_byte * 1e6
    );

    // Absolute wall-clock guard, catastrophic-only: the per-byte ratio above is
    // the real (build-invariant) quadratic detector. This bound just backstops a
    // full O(n^2) reintroduction. CI runs `cargo test` in DEBUG, ~10-20x slower
    // per byte than release, so n=200000 legitimately takes ~2 s here; a wide
    // 30 s bound tolerates a loaded debug runner while a reintroduced quadratic
    // (tens of seconds to minutes at this n) still trips it.
    assert!(
        scaling.large_secs < 30.0,
        "{label} parse for n={large_n} took {:.4}s (expected near-instant)",
        scaling.large_secs
    );
}

#[test]
fn deeply_nested_balanced_links_parse_in_near_linear_time() {
    on_big_stack(|| assert_near_linear(nested_links, "nested-link"));
}

#[test]
fn deeply_nested_balanced_images_parse_in_near_linear_time() {
    on_big_stack(|| assert_near_linear(nested_images, "nested-image"));
}

/// A flat run of unclosed link openers with NO `)` anywhere: `[a](` * n. Each
/// `[` reaches the link-destination reader, which used to scan to end-of-text
/// looking for the mandatory `)` -- O(n) per `[`, so O(n^2) overall. The
/// last-`)` short-circuit bounds each attempt to O(1).
fn flat_unclosed_links(n: usize) -> String {
    "[a](".repeat(n)
}

/// A flat run of underline openers each butting against `](`: `_a](` * n. No
/// `_` ever closes (each candidate closer is a word-boundary miss), so the
/// emphasis-closer scan walked to EOF for every one of the n openers -- O(n^2).
/// The per-delimiter no-close memo bounds it to O(1) after the first failure.
fn flat_unclosed_underline(n: usize) -> String {
    "_a](".repeat(n)
}

/// The `*`/strong variant of `flat_unclosed_underline`: `*a](` * n.
fn flat_unclosed_strong(n: usize) -> String {
    "*a](".repeat(n)
}

#[test]
fn flat_unclosed_link_destinations_parse_in_near_linear_time() {
    assert_near_linear(flat_unclosed_links, "flat-unclosed-link");
}

#[test]
fn flat_unclosed_underline_openers_parse_in_near_linear_time() {
    assert_near_linear(flat_unclosed_underline, "flat-unclosed-underline");
}

#[test]
fn flat_unclosed_strong_openers_parse_in_near_linear_time() {
    assert_near_linear(flat_unclosed_strong, "flat-unclosed-strong");
}

/// The code-fence twin of the `%%%` case below.
///
/// `comment_closer_last_index` was added to make an unterminated `%%%` opener
/// answer from an index instead of scanning to the end of the input. Code
/// fences shared the defect and never got the index, so a document of
/// unterminated ``` openers inside a container stayed quadratic - with no test
/// to say so, because this file only covered `%%%` (carve#515).
#[test]
fn unterminated_code_fence_openers_in_a_container_parse_in_near_linear_time() {
    let build = |n: usize| {
        let mut source = String::from(":::\n");
        for _ in 0..n {
            source.push_str("``` x\n");
        }
        source.push_str(":::\n");
        source
    };

    assert_near_linear(build, "unterminated-code-fence-in-container");
}

#[test]
fn unterminated_comment_fence_openers_parse_in_near_linear_time() {
    let build = |n: usize| {
        let mut source = String::new();
        for len in 3..n + 3 {
            for _ in 0..len {
                source.push('%');
            }
            source.push_str(" x\n");
        }
        source
    };

    let _guard = perf_guard();
    let small = 500;
    let large = 1000;
    let small_source = build(small);
    let large_source = build(large);

    let _ = carve::to_html(&small_source);
    let _ = carve::to_html(&large_source);

    let time_once = |source: &str| {
        let start = Instant::now();
        let _ = carve::to_html(source);
        start.elapsed().as_secs_f64()
    };
    let mut small_samples = Vec::new();
    let mut large_samples = Vec::new();
    for _ in 0..3 {
        small_samples.push(time_once(&small_source));
        large_samples.push(time_once(&large_source));
    }
    let median = |mut xs: Vec<f64>| {
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        xs[xs.len() / 2]
    };
    let large_samples_for_ceiling = large_samples.clone();
    let small_per_byte = median(small_samples) / small_source.len() as f64;
    let large_per_byte = median(large_samples) / large_source.len() as f64;

    let ratio = large_per_byte / small_per_byte.max(f64::MIN_POSITIVE);
    // Measured on this input: 0.73 answering from the width index, versus just
    // under 2.0 with a scan to end of input per opener. The old 2.0 bound sat
    // exactly at that boundary, so it PASSED the scan version - it only took 162
    // seconds to do it. Hence both a tighter ratio and a wall-clock ceiling: the
    // ceiling is ~100x the observed time, loose enough not to flake on a shared
    // runner but nowhere near a full rescan.
    assert!(
        ratio < 1.2,
        "unterminated-comment-fence per-byte cost grew {ratio:.2}x"
    );
    let large_elapsed = median(large_samples_for_ceiling);
    assert!(
        large_elapsed < 30.0,
        "unterminated-comment-fence parse took {large_elapsed:.1}s; a per-opener rescan is likely back"
    );
}

#[test]
fn flat_unclosed_link_destinations_preserve_output() {
    // The `[a](`×n shape never forms a real link: every opener stays literal.
    // The last-`)` short-circuit must not change that.
    let link = carve::to_html(&flat_unclosed_links(5));
    assert_eq!(link.matches("<a ").count(), 0, "{link}");
    assert!(link.contains("[a]("), "literal text must survive: {link}");
    // A genuine link with a destination still renders as an anchor.
    assert_eq!(
        carve::to_html("[text](https://example.com)"),
        "<p><a href=\"https://example.com\">text</a></p>"
    );
}

#[test]
fn flat_unclosed_emphasis_openers_preserve_output() {
    // The `_a](`×n / `*a](`×n shapes never close, so every opener stays
    // literal. The per-delimiter no-close memo must not change that.
    let under = carve::to_html(&flat_unclosed_underline(5));
    assert_eq!(under.matches("<u>").count(), 0, "{under}");

    let strong = carve::to_html(&flat_unclosed_strong(5));
    assert_eq!(strong.matches("<strong>").count(), 0, "{strong}");

    // Genuine emphasis still renders correctly (memo off-path).
    assert_eq!(carve::to_html("_underlined_"), "<p><u>underlined</u></p>");
    assert_eq!(carve::to_html("*strong*"), "<p><strong>strong</strong></p>");
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

// ---------------------------------------------------------------------------
// Unclosed-construct quadratics (same class as the flat-unclosed-link shapes
// above). Each construct scanned forward to a mandatory closing delimiter with
// no absence short-circuit, so a run of unclosed openers re-scanned to
// end-of-text at every position -- O(n^2). A per-inline-text last-occurrence
// bound (see InlineBounds in src/parse.rs) makes each attempt O(1) when the
// closer cannot lie ahead. Output stays byte-identical (only failing scans are
// elided).
// ---------------------------------------------------------------------------

/// Assert `build(n)` parses without an O(n^2) blowup on an unclosed-construct
/// shape. The per-byte ratio is the primary, build-invariant detector; the
/// absolute wall-clock bound backstops a full regression. The ratio is applied
/// only when the smaller sample rises above timing noise -- several of these
/// fixed shapes parse in microseconds, where any ratio is pure scheduler jitter
/// (e.g. 1.5ms -> 4.8ms reads as "3x" but is O(1)).
fn assert_bounded_scan(build: impl Fn(usize) -> String, label: &str) {
    let scaling = measure_scaling(&build);

    // A reintroduced O(n^2) at n=200000 (~0.6-0.8 MB) runs in tens of seconds to
    // minutes; the fixed parser stays sub-second in release, ~seconds in a debug
    // CI build (~10-20x slower per byte). A wide 30 s bound tolerates a loaded
    // debug runner while failing hard on regression.
    assert!(
        scaling.large_secs < 30.0,
        "{label} parse for n={SCALE_LARGE_N} took {:.4}s (expected near-instant; O(n^2) regression?)",
        scaling.large_secs
    );

    // Only compare sizes when the signal is above noise (10 ms on the small
    // sample); below that the ratio is jitter and the absolute bound above
    // already guards the shape.
    if scaling.small_secs > 0.01 {
        let ratio = scaling.per_byte_ratio();
        assert!(
            ratio < SCALE_MAX_PER_BYTE_RATIO,
            "{label} per-byte cost grew {ratio:.2}x at {}x the input (linear ~1x, quadratic ~{}x): \
             small={:.4}us/byte large={:.4}us/byte",
            SCALE_LARGE_N / SCALE_SMALL_N,
            SCALE_LARGE_N / SCALE_SMALL_N,
            scaling.small_per_byte * 1e6,
            scaling.large_per_byte * 1e6
        );
    }
}

/// `[x]{`×n: span/attribute openers with no closing `}` anywhere. The `}`-attr
/// scan walked to EOF for every `{`.
fn flat_unclosed_span_attr(n: usize) -> String {
    "[x]{".repeat(n)
}

/// `[x]{`×n + one trailing `}`: the "far-brace" shape. A single `}` DOES exist,
/// so the last-`}` presence guard passes, yet the content never validates. Each
/// opener used to walk to that far `}` and re-parse the whole tail (O(n^2)); the
/// provably-invalid filter rejects the `[`-led content in O(1) before the walk.
/// Distinct from `flat_unclosed_span_attr` (no `}` at all).
fn far_brace_span_attr(n: usize) -> String {
    "[x]{".repeat(n) + "}"
}

/// Far-brace variants whose content starts with a VALID attribute-token prefix
/// but still never validates (a nested `[x]{` follows). A first-byte-only guard
/// misses these -- the token-walk filter must consume the valid prefix and bail
/// at the invalid boundary byte, still O(1) per opener.
///
/// `[x]{a `×n + `}`: a bareword, then whitespace, then the nested `[`.
fn far_brace_bareword_prefix(n: usize) -> String {
    "[x]{a ".repeat(n) + "}"
}

/// `[x]{.a `×n + `}`: a `.class`, then whitespace, then the nested `[`.
fn far_brace_class_prefix(n: usize) -> String {
    "[x]{.a ".repeat(n) + "}"
}

/// `[x]{k= `×n + `}`: a `key=` whose value is empty (whitespace follows) -- a
/// dangling `=`, which is invalid, then the nested `[`.
fn far_brace_dangling_key(n: usize) -> String {
    "[x]{k= ".repeat(n) + "}"
}

/// `{+`×n: critic-insert openers with no `+}` pair. The `find("+}")` walked to
/// EOF for every opener.
fn flat_unclosed_critic_insert(n: usize) -> String {
    "{+".repeat(n)
}

/// `{-`×n: critic-delete openers with no `-}` pair.
fn flat_unclosed_critic_delete(n: usize) -> String {
    "{-".repeat(n)
}

/// `{~ }`×n: critic-substitution openers where a `}` IS present but the `~}`
/// pair never is (a single-`}` bound is insufficient; the `~}`-pair bound is
/// what makes this linear).
fn flat_critic_sub_no_pair(n: usize) -> String {
    "{~ }".repeat(n)
}

/// `{#`×n: critic-comment openers with no `#}` pair.
fn flat_unclosed_critic_comment(n: usize) -> String {
    "{#".repeat(n)
}

/// `{/`×n: forced-emphasis openers with no `/}` pair.
fn flat_unclosed_forced_emphasis(n: usize) -> String {
    "{/".repeat(n)
}

/// `[^`×n: footnote-ref openers with no closing `]`.
fn flat_unclosed_footnote_ref(n: usize) -> String {
    "[^".repeat(n)
}

/// `^[`×n: inline-footnote openers with no closing `]`.
fn flat_unclosed_inline_footnote(n: usize) -> String {
    "^[".repeat(n)
}

/// `:a[`×n: inline-extension openers with no closing `]`.
fn flat_unclosed_inline_extension(n: usize) -> String {
    ":a[".repeat(n)
}

/// `</#`×n: crossref openers with no closing `>`.
fn flat_unclosed_crossref(n: usize) -> String {
    "</#".repeat(n)
}

/// `<`×n: autolink openers with no closing `>`.
fn flat_unclosed_autolink(n: usize) -> String {
    "<".repeat(n)
}

#[test]
fn flat_unclosed_span_attributes_parse_in_near_linear_time() {
    assert_bounded_scan(flat_unclosed_span_attr, "flat-unclosed-span-attr");
}

#[test]
fn far_brace_span_attributes_parse_in_near_linear_time() {
    assert_bounded_scan(far_brace_span_attr, "far-brace-span-attr");
}

#[test]
fn far_brace_bareword_prefix_parses_in_near_linear_time() {
    assert_bounded_scan(far_brace_bareword_prefix, "far-brace-bareword-prefix");
}

#[test]
fn far_brace_class_prefix_parses_in_near_linear_time() {
    assert_bounded_scan(far_brace_class_prefix, "far-brace-class-prefix");
}

#[test]
fn far_brace_dangling_key_parses_in_near_linear_time() {
    assert_bounded_scan(far_brace_dangling_key, "far-brace-dangling-key");
}

#[test]
fn flat_unclosed_critic_insert_openers_parse_in_near_linear_time() {
    assert_bounded_scan(flat_unclosed_critic_insert, "flat-unclosed-critic-insert");
}

#[test]
fn flat_unclosed_critic_delete_openers_parse_in_near_linear_time() {
    assert_bounded_scan(flat_unclosed_critic_delete, "flat-unclosed-critic-delete");
}

#[test]
fn flat_critic_sub_without_pair_parses_in_near_linear_time() {
    assert_bounded_scan(flat_critic_sub_no_pair, "flat-critic-sub-no-pair");
}

#[test]
fn flat_unclosed_critic_comment_openers_parse_in_near_linear_time() {
    assert_bounded_scan(flat_unclosed_critic_comment, "flat-unclosed-critic-comment");
}

#[test]
fn flat_unclosed_forced_emphasis_openers_parse_in_near_linear_time() {
    assert_bounded_scan(
        flat_unclosed_forced_emphasis,
        "flat-unclosed-forced-emphasis",
    );
}

#[test]
fn flat_unclosed_footnote_refs_parse_in_near_linear_time() {
    assert_bounded_scan(flat_unclosed_footnote_ref, "flat-unclosed-footnote-ref");
}

#[test]
fn flat_unclosed_inline_footnotes_parse_in_near_linear_time() {
    assert_bounded_scan(
        flat_unclosed_inline_footnote,
        "flat-unclosed-inline-footnote",
    );
}

#[test]
fn flat_unclosed_inline_extensions_parse_in_near_linear_time() {
    assert_bounded_scan(
        flat_unclosed_inline_extension,
        "flat-unclosed-inline-extension",
    );
}

#[test]
fn flat_unclosed_crossrefs_parse_in_near_linear_time() {
    assert_bounded_scan(flat_unclosed_crossref, "flat-unclosed-crossref");
}

#[test]
fn flat_unclosed_autolinks_parse_in_near_linear_time() {
    assert_bounded_scan(flat_unclosed_autolink, "flat-unclosed-autolink");
}

#[test]
fn bounded_attribute_and_critic_scans_preserve_output() {
    // Closed constructs still render exactly as before the bound was added.
    assert_eq!(
        carve::to_html("[word]{.hl}"),
        "<p><span class=\"hl\">word</span></p>"
    );
    assert_eq!(carve::to_html("{+ins+}"), "<p><ins>ins</ins></p>");
    assert_eq!(carve::to_html("{-del-}"), "<p><del>del</del></p>");
    assert_eq!(
        carve::to_html("{~old~>new~}"),
        "<p><del>old</del><ins>new</ins></p>"
    );
    assert_eq!(
        carve::to_html("{#note#}"),
        "<p><span class=\"critic-comment\">note</span></p>"
    );
    assert_eq!(carve::to_html("{/it/}"), "<p><em>it</em></p>");

    // Unclosed openers stay literal: no element is produced, and the source
    // text survives verbatim.
    let span = carve::to_html(&flat_unclosed_span_attr(5));
    assert_eq!(span.matches("<span").count(), 0, "{span}");
    assert!(span.contains("[x]{"), "{span}");

    // The far-brace shape's `[x]{[...` openers never form a span (the content
    // is never a valid attribute list), so they stay literal -- the
    // provably-invalid filter must not change that. The ONLY span is the trailing
    // `[x]{}` (an empty attribute block, handled separately).
    let far = carve::to_html(&far_brace_span_attr(5));
    assert_eq!(far, "<p>[x]{[x]{[x]{[x]{<span>x</span></p>", "{far}");

    // Valid-prefix variants: the filter consumes the valid token prefix and bails
    // at the nested `[`, so the `…[x]{prefix [` openers stay literal. Only the
    // final block, which has no nested opener, forms its span (or, for the
    // dangling-`=` variant, stays literal too). Byte-identical to the full parse.
    assert_eq!(
        carve::to_html(&far_brace_bareword_prefix(5)),
        "<p>[x]{a [x]{a [x]{a [x]{a <span a=\"\">x</span></p>"
    );
    assert_eq!(
        carve::to_html(&far_brace_class_prefix(5)),
        "<p>[x]{.a [x]{.a [x]{.a [x]{.a <span class=\"a\">x</span></p>"
    );
    assert_eq!(
        carve::to_html(&far_brace_dangling_key(5)),
        "<p>[x]{k= [x]{k= [x]{k= [x]{k= [x]{k= }</p>"
    );

    // A `{` whose content DOES start validly still renders, even with an
    // unbalanced inner `{` (carve-rs stops at the first `}`, unlike carve-php):
    // the filter defers on the `key=<value>` (real value) rather than rejecting.
    assert_eq!(
        carve::to_html("[x]{a=b{c}"),
        "<p><span a=\"b{c\">x</span></p>"
    );
    // A bare value and id/class chains separated by SPACES still form their
    // spans -- the filter defers on each.
    assert_eq!(
        carve::to_html("[x]{#i .c key=v}"),
        "<p><span id=\"i\" class=\"c\" key=\"v\">x</span></p>"
    );
    // A NO-BREAK SPACE does NOT separate two attributes. The inline interior is
    // space-only (PART 4, carve#906), and a no-break space is content rather
    // than syntax either way - so the block is unrecognized and its braces
    // show. This asserted the opposite while the tokenizer split on the Unicode
    // whitespace property; the executable spec renders the literal, as it does
    // for an ideographic space.
    assert_eq!(carve::to_html("[x]{a\u{00A0}b}"), "<p>[x]{a&nbsp;b}</p>");
    assert_eq!(carve::to_html("[x]{a\u{3000}b}"), "<p>[x]{a\u{3000}b}</p>");

    let ins = carve::to_html(&flat_unclosed_critic_insert(5));
    assert_eq!(ins.matches("<ins>").count(), 0, "{ins}");
    assert!(ins.contains("{+"), "{ins}");

    let del = carve::to_html(&flat_unclosed_critic_delete(5));
    assert_eq!(del.matches("<del>").count(), 0, "{del}");

    let sub = carve::to_html(&flat_critic_sub_no_pair(5));
    assert_eq!(sub.matches("<del>").count(), 0, "{sub}");
    assert_eq!(sub.matches("<ins>").count(), 0, "{sub}");

    let cmt = carve::to_html(&flat_unclosed_critic_comment(5));
    assert_eq!(cmt.matches("critic-comment").count(), 0, "{cmt}");

    // `{/`×n forms no FORCED emphasis (there is no `/}` pair), but the bare `/`
    // italic delimiter still pairs up as ordinary emphasis -- that is unchanged,
    // pre-existing behavior. What the bound must preserve is that no forced-span
    // is fabricated and the leading `{`s stay literal.
    let forced = carve::to_html(&flat_unclosed_forced_emphasis(5));
    assert!(
        forced.contains('{'),
        "leading brace must stay literal: {forced}"
    );
}

#[test]
fn bounded_bracket_and_angle_scans_preserve_output() {
    // Closed constructs still render.
    assert_eq!(
        carve::to_html(":name[body]"),
        "<p><span class=\"ext-name\">body</span></p>"
    );
    assert_eq!(
        carve::to_html("<https://example.com>"),
        "<p><a href=\"https://example.com\">https://example.com</a></p>"
    );
    assert!(
        carve::to_html("x[^a]\n\n[^a]: the note").contains("role=\"doc-noteref\""),
        "resolved footnote ref must still render"
    );
    assert!(
        carve::to_html("y^[note]").contains("role=\"doc-noteref\""),
        "inline footnote must still render"
    );

    // Unclosed openers stay literal.
    let fnref = carve::to_html(&flat_unclosed_footnote_ref(5));
    assert_eq!(fnref.matches("doc-noteref").count(), 0, "{fnref}");
    assert!(fnref.contains("[^"), "{fnref}");

    let infn = carve::to_html(&flat_unclosed_inline_footnote(5));
    assert_eq!(infn.matches("doc-noteref").count(), 0, "{infn}");

    let ext = carve::to_html(&flat_unclosed_inline_extension(5));
    assert_eq!(ext.matches("class=\"ext-").count(), 0, "{ext}");

    let xref = carve::to_html(&flat_unclosed_crossref(5));
    assert_eq!(xref.matches("<a ").count(), 0, "{xref}");

    let auto = carve::to_html(&flat_unclosed_autolink(5));
    assert_eq!(auto.matches("<a ").count(), 0, "{auto}");
}

/// A `+`-ATTACHED FENCE'S CLOSER LOOKAHEAD IS ANSWERED FROM AN INDEX, NOT A SCAN
/// (markup-carve/carve-rs#802).
///
/// Five collectors share `attached_block_end`, and each `+` it scans asks
/// whether the `%%%` opener it just met has a closer ahead. That question is
/// answered from a width -> last-index map, and the map is the CALLER'S so it is
/// built once for a line set. Rebuilding it per `+` is O(lines) per marker and
/// therefore quadratic in the document: measured on the shape below, per-byte
/// cost grew 3.62x at 4x the input and the large sample went from 0.07s to
/// 3.47s.
///
/// THE FENCES HERE CLOSE, deliberately. An UNCLOSABLE opener would swallow the
/// rest of the document into the first attachment, so only ONE lookahead would
/// ever run and the guard could not fail however bad the lookahead was. A
/// CONSTANT width is deliberate too: widening each opener grows the input bytes
/// faster than the marker count, which makes the per-byte reading pin that
/// growth rather than this one.
fn plus_attached_closed_comment_fences(n: usize) -> String {
    "- x\n+\n%%%\na\n%%%\n\n".repeat(n)
}

#[test]
fn plus_attached_comment_fence_closer_lookahead_is_indexed() {
    assert_near_linear_at(
        plus_attached_closed_comment_fences,
        "plus-attached-closed-comment-fence",
        2_000,
        8_000,
    );
}

/// A REAL EXPORT'S FOOTNOTE SHAPE: one reference per note, every note in one
/// list. This is what Word, Google Docs and Pandoc all produce, and it is the
/// shape that exposes a pairing pass which asks a question of every candidate
/// about every OTHER candidate.
fn footnote_shaped_export(notes: usize) -> String {
    let mut body = String::new();
    let mut definitions = String::new();
    for index in 1..=notes {
        let _ = writeln!(
            body,
            "<p>text {index}<a href=\"#fn{index}\" id=\"fnref{index}\">\
             <sup>{index}</sup></a> tail.</p>"
        );
        let _ = writeln!(
            definitions,
            "<li id=\"fn{index}\"><p>note {index}\
             <a href=\"#fnref{index}\">back</a></p></li>"
        );
    }
    format!("{body}<section class=\"footnotes\"><hr /><ol>{definitions}</ol></section>")
}

fn import_footnote_shaped(adapter: carve::HtmlImportAdapter) -> impl Fn(&str) {
    move |html: &str| {
        let options = carve::HtmlImportOptions {
            adapter,
            ..Default::default()
        };
        let _ = carve::html_to_carve(html, &options);
    }
}

/// RECOGNIZING FOOTNOTE-SHAPED HTML MUST NOT COST THE DOCUMENT TWICE
/// (markup-carve/carve#1210).
///
/// Three steps of the adapter pass started out quadratic in carve-php: which
/// candidate reads the same mutual pair from the other end, which block
/// contains another block, and whether an anchor sits inside a note. On a
/// document that is mostly notes each of those is O(notes^2), and carve-php
/// measured 0.603s at 800 notes before the fix. Each is answered here from an
/// index instead - the back anchor names the inverse, the containers are found
/// by climbing once per note, and the set of nodes inside a note is built once.
///
/// One note is two blocks and an anchor pair, so the file's default 50k/200k
/// would build a document two orders of magnitude past what the shape needs.
/// 250/1000 keeps the same 4x multiple.
#[test]
fn footnote_shaped_html_is_not_paired_candidate_against_candidate() {
    assert_conversion_near_linear_at(
        import_footnote_shaped(carve::HtmlImportAdapter::Word),
        footnote_shaped_export,
        "footnote-shaped HTML under the word adapter",
        250,
        1_000,
    );
}

/// The control, and not decoration: it walks the SAME document through the
/// same importer with the adapter pass switched off, so a reading that blames
/// the pass has to survive the same document measuring linear without it.
#[test]
fn the_same_footnote_document_is_bounded_without_the_adapter() {
    assert_conversion_near_linear_at(
        import_footnote_shaped(carve::HtmlImportAdapter::Generic),
        footnote_shaped_export,
        "the same document under the generic adapter",
        250,
        1_000,
    );
}

#[test]
fn a_nested_continuation_attachment_is_bounded() {
    on_big_stack(|| {
        // §17 L3 attaches ONE block, so a `+` has to know where the attached
        // block ends. Measuring that by PARSING the block makes a `+`-attached
        // container holding another `+` attachment pay for its subtree once per
        // level above it: this document took 28.71 s that way, against 0.65 s
        // before the clause was implemented at all and 0.85 s with the extent
        // read from the container's CLOSER instead.
        //
        // Three documents nested to the depth cap, ~2400 lines in total. The
        // shape matters more than the size - the blow-up is in the DEPTH, and
        // widening the bodies barely moved it (9.65 s to 9.72 s at one copy),
        // which is why this repeats the document rather than enlarging it.
        let mut one = String::from("para\n");
        for _ in 0..200 {
            one = format!("> q\n+\n::: d\n{one}:::\n");
        }
        let source = [one.as_str(), one.as_str(), one.as_str()].join("\n");

        let start = Instant::now();
        let html = carve::to_html(&source);

        // Past MAX_NESTING_DEPTH the innermost levels degrade rather than
        // recursing, so what is asserted is that the content SURVIVES - once per
        // copy - and not that it reaches a paragraph node.
        assert_eq!(
            html.matches("para").count(),
            3,
            "expected the innermost content of each copy"
        );
        assert!(
            start.elapsed().as_secs_f32() < MAX_SECS,
            "nested continuation attachment took {:?}",
            start.elapsed()
        );
    });
}

/// A definition body's marker line asks PART 1 S4's question of its own content
/// (carve-rs#1049), and that question is answered by PARSING the body. The
/// answer therefore has to stay a per-entry cost: a predicate that re-read the
/// whole preceding document, or one that got asked once per line instead of
/// once per body, would turn a run of definition entries quadratic without
/// changing a byte of output.
///
/// FIXED-WIDTH UNITS, so the byte multiple and the unit multiple agree and the
/// per-byte ratio means what the label says. Each entry is the same length: an
/// eight-digit term, a marker line holding a HEADING - the kind whose answer
/// this change moved - and the flush-left line the rule is asked about.
fn definition_marker_line_blocks(n: usize) -> String {
    let mut source = String::with_capacity(n * 24);
    for i in 0..n {
        writeln!(source, ":: t{i:08}\n:  # H\ntail\n").unwrap();
    }
    source
}

#[test]
fn definition_marker_line_s4_is_answered_per_entry() {
    // No `perf_guard` here: `measure_conversion_scaling_at` takes it, and
    // `PERF_LOCK` is a plain `Mutex`, so a second acquisition on the same
    // thread deadlocks rather than nesting.
    assert_near_linear_at(
        definition_marker_line_blocks,
        "definition marker-line S4",
        10_000,
        40_000,
    );
}

/// A quoted line that OPENS a brace and never closes it sends the wrapped-
/// attribute-block lookahead over the rest of the quote (carve-rs#1050). The
/// scan reports the window it proved empty, so a run of such lines is walked
/// ONCE rather than once per line.
///
/// WHAT IS MEASURED IS THE PER-BYTE COST AGAINST THE SAME RUN UNQUOTED, not the
/// growth ratio. Both documents are already superlinear on this shape - the
/// paragraph parse copies the remainder per line, at the top level as much as
/// inside a quote - and a second per-line scan in the collector DOUBLES the
/// constant while leaving the exponent alone. A growth-ratio row therefore
/// cannot see it: the un-barriered scan measured 3.8x against a healthy 3.6x and
/// a ratio guard passed it. Per byte, at 8000 lines, it measured 20.6us against
/// the flat document's 12.0us, where the barriered scan measures 7.7us against
/// 9.9us. The pre-existing superlinearity is deliberately NOT hidden by this
/// row; it is the baseline the quote is held to, and it is a separate defect.
///
/// The quoted document is fixed-width and two bytes per line wider, so equal
/// per-line work reads BELOW 1.0 (6/8 = 0.75, measured 0.78). The bound sits at
/// 1.2: half again above the healthy figure, and a third below the regression.
fn quoted_unclosed_brace_lines(n: usize) -> String {
    "> {abcd\n".repeat(n)
}

fn unquoted_unclosed_brace_lines(n: usize) -> String {
    "{abcd\n".repeat(n)
}

#[test]
fn a_quoted_run_of_unclosed_braces_is_scanned_once() {
    let quoted = measure_scaling_at(&quoted_unclosed_brace_lines, 4_000, 8_000);
    let flat = measure_scaling_at(&unquoted_unclosed_brace_lines, 4_000, 8_000);

    let cost = quoted.large_per_byte / flat.large_per_byte;
    assert!(
        cost < 1.2,
        "a quoted run of unclosed braces cost {cost:.2}x per byte against the same run \
         unquoted (equal per-line work reads ~0.78x; a per-line rescan in the quote \
         collector reads ~1.7x): quoted={:.4}us/byte flat={:.4}us/byte",
        quoted.large_per_byte * 1e6,
        flat.large_per_byte * 1e6
    );
}

/// A `+` continuation row is scanned with the verbatim run its predecessor left
/// open, at that run's WIDTH (carve-rs#1051). The carry is per row and per
/// column, so the cost has to stay per row: a scanner that re-read the rows
/// above to recover the width, or that re-split the assembled cell once per
/// fragment, would turn a table of carrying rows quadratic without changing a
/// byte of output.
///
/// FIXED-WIDTH UNITS - every row and every continuation is the same length - so
/// the byte multiple and the unit multiple agree.
fn carrying_continuation_rows(n: usize) -> String {
    "| aaaa ``bb |\n+ cc ` | dd`` |\n\n".repeat(n)
}

/// ONE cell carrying a run across n continuation rows, which is the other axis:
/// the fragments accumulate on a single column instead of on n separate tables.
fn one_cell_carrying_across_many_rows(n: usize) -> String {
    let mut source = String::from("| aaaa ``bb |\n");
    for _ in 0..n {
        source.push_str("+ cccc dddd |\n");
    }
    source
}

#[test]
fn a_carried_run_costs_one_scan_per_row() {
    assert_near_linear_at(
        carrying_continuation_rows,
        "carrying continuation rows",
        10_000,
        40_000,
    );
    assert_near_linear_at(
        one_cell_carrying_across_many_rows,
        "one cell carrying across many rows",
        10_000,
        40_000,
    );
}

/// ESCAPE-HEAVY MARKDOWN OUTPUT, scaled by the number of lines.
///
/// THE MARKDOWN TARGET HAD NO SCALING ROW AT ALL, in any engine, which is why
/// markup-carve/carve#1331's 33x regression shipped invisibly. That absence is
/// worth closing on its own: a target with no scaling row is a target where the
/// next regression is also silent.
///
/// Every `\#` is an authored escape and reaches PART 11 §8b M2b's
/// content-position test, and every line carries a container prefix so the test
/// is decided past one rather than at column 0. FIXED-WIDTH UNITS - one line,
/// always the same line - so the byte multiple and the unit multiple agree.
fn escape_heavy_markdown_lines(n: usize) -> String {
    "> \\#\\#\\# and \\# and \\#\\# tail\n>\n".repeat(n)
}

#[test]
fn the_markdown_target_scales_linearly_on_escape_heavy_input() {
    assert_conversion_near_linear_at(
        |source| drop(carve::to_markdown(source)),
        escape_heavy_markdown_lines,
        "escape-heavy markdown",
        25_000,
        100_000,
    );
}

/// A single line of N ADJACENT authored escapes - the shape markup-carve/carve#1331
/// measured, where §8b M2b's two O(n) scans per candidate met a line on which
/// every character is a candidate.
///
/// FIXED-WIDTH UNITS: one `\#` is one unit and two bytes, always.
fn adjacent_authored_escapes(n: usize) -> String {
    let mut source = String::with_capacity(n * 2 + 1);
    source.push_str(&"\\#".repeat(n));
    source.push('\n');
    source
}

/// The same run behind a CONTAINER PREFIX, which is where the content position
/// is measured from on the emitted line (markup-carve/carve#1332). Its own row
/// because a fix that hoisted the scan for unprefixed lines only would stay
/// quadratic here.
fn adjacent_authored_escapes_in_a_quote(n: usize) -> String {
    let mut source = String::with_capacity(n * 2 + 3);
    source.push_str("> ");
    source.push_str(&"\\#".repeat(n));
    source.push('\n');
    source
}

/// THIS ROW IS A SHARE, NOT A SLOPE, and the reason is worth reading before
/// changing it to a slope. A single line of 100k escapes is ALREADY superlinear
/// in the PARSER - measured at 3.22x per byte over a 4x input on `carve::parse`
/// alone, with no renderer involved - so a scaling row over this shape reports
/// the parser's cost and would stay red however linear the writer became. The
/// row above scales; this one isolates.
///
/// What it isolates is the Markdown target's OWN share of the work, by pricing
/// it against the HTML target on the same input. The two run the same parse, so
/// whatever the parse costs cancels, and what is left is what each writer adds.
/// Before markup-carve/carve#1331 the Markdown target cost about 29x the HTML
/// target on this shape; after, about 0.7x. A bound of 3x sits between them with
/// an order of magnitude of room on either side, and it cannot be satisfied by a
/// parser that gets slower.
fn assert_markdown_costs_no_more_than_html(build: impl Fn(usize) -> String, label: &str) {
    let markdown =
        measure_conversion_scaling_at(&|s| drop(carve::to_markdown(s)), &build, 25_000, 100_000);
    let html = measure_conversion_scaling_at(&|s| drop(carve::to_html(s)), &build, 25_000, 100_000);

    let share = markdown.large_per_byte / html.large_per_byte;
    assert!(
        share < 3.0,
        "{label}: the markdown target cost {share:.2}x the html target per byte \
         (healthy ~0.7x, a per-candidate line scan ~29x): markdown={:.4}us/byte html={:.4}us/byte",
        markdown.large_per_byte * 1e6,
        html.large_per_byte * 1e6
    );
}

#[test]
fn an_adjacent_escape_run_costs_the_markdown_target_no_more_than_the_html_target() {
    assert_markdown_costs_no_more_than_html(adjacent_authored_escapes, "adjacent authored escapes");
    assert_markdown_costs_no_more_than_html(
        adjacent_authored_escapes_in_a_quote,
        "adjacent authored escapes behind a quote prefix",
    );
}
