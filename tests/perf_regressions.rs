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

/// Run each size a few times, take the minimum to damp scheduler noise, and
/// assert the larger size is well under a quadratic multiple of the smaller
/// while staying within an absolute wall-clock bound. A quadratic parse gives
/// ~4x for a 2x input; linear gives ~2x.
///
/// NOTE: these sizes (100k / 200k `[`, i.e. ~400 KB / ~800 KB of input) are
/// chosen to expose a quadratic *constant* -- the old n=4000/8000 sizes ran in
/// low single-digit milliseconds and could not distinguish linear from
/// quadratic through scheduler noise. Run against a release build
/// (`cargo test --release`); a debug build is ~10-20x slower and may exceed the
/// absolute bound without any regression.
fn assert_near_linear(build: impl Fn(usize) -> String, label: &str) {
    fn min_parse_time(source: &str) -> f64 {
        (0..5)
            .map(|_| {
                let start = Instant::now();
                let _ = carve::to_html(source);
                start.elapsed().as_secs_f64()
            })
            .fold(f64::INFINITY, f64::min)
    }

    let small = build(100_000);
    let large = build(200_000);

    let t_small = min_parse_time(&small);
    let t_large = min_parse_time(&large);

    if t_small > 0.0 {
        let ratio = t_large / t_small;
        assert!(
            ratio < 3.0,
            "{label} parse scaling looks super-linear: {t_small:.4}s -> {t_large:.4}s (ratio {ratio:.1}x)"
        );
    }

    // Absolute wall-clock guard, catastrophic-only: the ratio check above is the
    // real (build-invariant) quadratic detector. This bound just backstops a full
    // O(n^2) reintroduction. CI runs `cargo test` in DEBUG, ~10-20x slower per
    // byte than release, so n=200000 legitimately takes ~2 s here; a wide 30 s
    // bound tolerates a loaded debug runner while a reintroduced quadratic (tens
    // of seconds to minutes at this n) still trips it.
    assert!(
        t_large < 30.0,
        "{label} parse for n=200000 took {t_large:.4}s (expected near-instant)"
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
/// shape. A reintroduced quadratic here takes seconds at n=200000, so the
/// absolute wall-clock bound is the primary guard. The size ratio is a
/// secondary check, applied only when the smaller sample rises above timing
/// noise -- several of these fixed shapes parse in microseconds, where a ratio
/// is pure scheduler jitter (e.g. 1.5ms -> 4.8ms reads as "3x" but is O(1)).
/// Run against a release build; a debug build is far slower per byte.
fn assert_bounded_scan(build: impl Fn(usize) -> String, label: &str) {
    fn min_parse_time(source: &str) -> f64 {
        (0..7)
            .map(|_| {
                let start = Instant::now();
                let _ = carve::to_html(source);
                start.elapsed().as_secs_f64()
            })
            .fold(f64::INFINITY, f64::min)
    }

    let t_small = min_parse_time(&build(100_000));
    let t_large = min_parse_time(&build(200_000));

    // A reintroduced O(n^2) at n=200000 (~0.6-0.8 MB) runs in tens of seconds to
    // minutes; the fixed parser stays sub-second in release, ~seconds in a debug
    // CI build (~10-20x slower per byte). A wide 30 s bound tolerates a loaded
    // debug runner while failing hard on regression; the ratio check below is the
    // build-invariant detector.
    assert!(
        t_large < 30.0,
        "{label} parse for n=200000 took {t_large:.4}s (expected near-instant; O(n^2) regression?)"
    );

    // Only compare sizes when the signal is above noise (30 ms); below that the
    // ratio is jitter and the absolute bound above already guards the shape.
    if t_small > 0.03 {
        let ratio = t_large / t_small;
        assert!(
            ratio < 3.5,
            "{label} parse scaling looks super-linear: {t_small:.4}s -> {t_large:.4}s (ratio {ratio:.1}x)"
        );
    }
}

/// `[x]{`×n: span/attribute openers with no closing `}` anywhere. The `}`-attr
/// scan walked to EOF for every `{`.
fn flat_unclosed_span_attr(n: usize) -> String {
    "[x]{".repeat(n)
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
