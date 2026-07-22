use std::fmt::Write as _;
use std::time::Instant;

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
    let mut source = String::new();
    source.push_str("intro\n");
    for _ in 0..8_000 {
        source.push_str("::: note\n");
    }

    let start = Instant::now();
    let html = carve::to_html(&source);

    assert!(html.contains("::: note"), "{html}");
    assert!(
        start.elapsed().as_secs_f32() < MAX_SECS,
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
    let small = build(SCALE_SMALL_N);
    let large = build(SCALE_LARGE_N);
    let small_bytes = small.len() as f64;
    let large_bytes = large.len() as f64;

    // Prime caches/allocator so round 1 does not measure warm-up.
    let _ = carve::to_html(&small);
    let _ = carve::to_html(&large);

    let time_once = |source: &str| {
        let start = Instant::now();
        let _ = carve::to_html(source);
        start.elapsed().as_secs_f64()
    };

    let mut small_samples = Vec::with_capacity(SCALE_ROUNDS);
    let mut large_samples = Vec::with_capacity(SCALE_ROUNDS);
    for _ in 0..SCALE_ROUNDS {
        small_samples.push(time_once(&small));
        large_samples.push(time_once(&large));
    }

    let median = |mut xs: Vec<f64>| -> f64 {
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        xs[xs.len() / 2]
    };

    let small_secs = median(small_samples);
    let large_secs = median(large_samples);

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
    let scaling = measure_scaling(&build);

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

    // Absolute wall-clock guard, catastrophic-only: the per-byte ratio above is
    // the real (build-invariant) quadratic detector. This bound just backstops a
    // full O(n^2) reintroduction. CI runs `cargo test` in DEBUG, ~10-20x slower
    // per byte than release, so n=200000 legitimately takes ~2 s here; a wide
    // 30 s bound tolerates a loaded debug runner while a reintroduced quadratic
    // (tens of seconds to minutes at this n) still trips it.
    assert!(
        scaling.large_secs < 30.0,
        "{label} parse for n={SCALE_LARGE_N} took {:.4}s (expected near-instant)",
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
    // A bare value, id/class chains, whitespace separators, and Unicode-space
    // separators (NBSP) all still form their spans -- the filter defers on each.
    assert_eq!(
        carve::to_html("[x]{#i .c key=v}"),
        "<p><span id=\"i\" class=\"c\" key=\"v\">x</span></p>"
    );
    assert_eq!(
        carve::to_html("[x]{a\u{00A0}b}"),
        "<p><span a=\"\" b=\"\">x</span></p>"
    );

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
