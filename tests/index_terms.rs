use carve::{Index, Options};

fn h(source: &str) -> String {
    let index = Index::new();
    let options = Options::new().with_extension(&index);
    carve::to_html_with_options(source, &options)
        .trim()
        .to_string()
}

fn off(source: &str) -> String {
    carve::to_html(source).trim().to_string()
}

#[test]
fn emits_invisible_span_per_marker() {
    let out = h("A :index[parser] here.\n\n::: index\n:::");
    assert!(out.contains("<span id=\"idx-parser-1\" class=\"index-term\"></span>"));
    assert!(out.contains("<p>A <span id=\"idx-parser-1\" class=\"index-term\"></span> here.</p>"));
}

#[test]
fn full_golden_matches_carve_js() {
    let out = h("A :index[parser] and :index[lexer], then :index[parser] again.\n\n::: index\n:::");
    assert_eq!(
        out,
        "<p>A <span id=\"idx-parser-1\" class=\"index-term\"></span> and \
<span id=\"idx-lexer-1\" class=\"index-term\"></span>, then \
<span id=\"idx-parser-2\" class=\"index-term\"></span> again.</p>\n\
<ul class=\"index\">\n  <li>lexer <a href=\"#idx-lexer-1\" class=\"index-backref\" aria-label=\"Back to lexer\">\u{21a9}</a></li>\n  \
<li>parser <a href=\"#idx-parser-1\" class=\"index-backref\" aria-label=\"Back to parser 1\">\u{21a9}<sup>1</sup></a> \
<a href=\"#idx-parser-2\" class=\"index-backref\" aria-label=\"Back to parser 2\">\u{21a9}<sup>2</sup></a></li>\n</ul>"
    );
}

#[test]
fn nested_index_block_opening_ul_indented_once() {
    // Inside a container the injected <ul class="index"> opening tag must sit at
    // the container's indent (2 spaces), not double-indented (4). Regression for
    // the framework-first-line + self-pad double-indent, matching carve-js.
    let out = h("X :index[A] Y\n\n:::: note\n::: index\n:::\n::::");
    assert!(
        out.contains("<aside class=\"admonition note\">\n  <ul class=\"index\">\n    <li>A "),
        "{out}"
    );
    assert!(!out.contains("    <ul class=\"index\">"), "{out}");
}

#[test]
fn sorted_with_backlinks() {
    let out = h("A :index[parser] and :index[lexer], then :index[parser].\n\n::: index\n:::");
    assert!(out.contains("<ul class=\"index\">"));
    assert!(out.find(">lexer ").unwrap() < out.find(">parser ").unwrap());
}

#[test]
fn numbers_occurrences_in_order() {
    let out = h(":index[a] :index[a] :index[a].\n\n::: index\n:::");
    assert!(out.contains("id=\"idx-a-1\""));
    assert!(out.contains("id=\"idx-a-2\""));
    assert!(out.contains("id=\"idx-a-3\""));
}

#[test]
fn no_markers_keeps_plain_div() {
    let out = h("No terms.\n\n::: index\n:::");
    assert!(out.contains("<div class=\"index\">"));
    assert!(!out.contains("<ul class=\"index\">"));
}

#[test]
fn off_uses_generic_fallback() {
    let out = off("A :index[parser] here.");
    assert!(out.contains("<span class=\"ext-index\">parser</span>"));
}

#[test]
fn marker_in_link_label_uses_span_not_nested_a() {
    let out = h("[see :index[parser]](/x).\n\n::: index\n:::");
    assert!(out.contains("<span id=\"idx-parser-1\" class=\"index-term\"></span>"));
    assert!(!out.contains("</a></a>"));
}

#[test]
fn footnote_def_marker_is_inert_no_dangling() {
    let out = h("Body :index[x].[^a]\n\n[^a]: Note :index[x].\n\n::: index\n:::");
    assert_eq!(out.matches("id=\"idx-x-").count(), 1);
    assert!(out.contains("id=\"idx-x-1\""));
    assert!(!out.contains("id=\"idx-x-2\""));
    assert!(out.contains("<span class=\"index-term\"></span>"));
    assert!(!out.contains("href=\"#idx-x-2\""));
}

#[test]
fn preserves_authored_content_before_list() {
    let out = h("A :index[parser].\n\n::: index\nGenerated below.\n:::");
    assert!(out.contains("Generated below."));
    assert!(out.contains("<ul class=\"index\">"));
    assert!(out.find("Generated below.").unwrap() < out.find("<ul class=\"index\">").unwrap());
}

#[test]
fn carries_block_attrs_on_ul() {
    let out = h("A :index[parser].\n\n{#book-index .two-col}\n::: index\n:::");
    assert!(out.contains("<ul id=\"book-index\" class=\"index two-col\">"));
}

#[test]
fn nested_in_blockquote() {
    let out = h("A :index[parser].\n\n> ::: index\n> :::");
    assert!(out.contains("<ul class=\"index\">"));
    assert!(out.contains(
        "<li>parser <a href=\"#idx-parser-1\" class=\"index-backref\" aria-label=\"Back to parser\">\u{21a9}</a></li>"
    ));
}

// --- index-expansion budget (memory-amplification DoS) ----------------------

/// Build a worst-case amplification input: `markers` distinct `:index[term]`
/// markers in the body plus `blocks` `::: index` blocks. Each block re-emits the
/// COMPLETE sorted backlink list, so without a budget the output would be
/// `blocks * markers * ~52` bytes, far larger than the input.
fn index_amplification_source(markers: usize, blocks: usize) -> String {
    let mut body = String::new();
    for i in 0..markers {
        body.push_str(&format!(":index[term{i}] "));
    }
    body.push_str("\n\n");
    for _ in 0..blocks {
        body.push_str("::: index\n:::\n\n");
    }
    body
}

#[test]
fn index_expansion_output_is_bounded() {
    let source = index_amplification_source(3_000, 400);
    let input_len = source.len();
    let start = std::time::Instant::now();
    let html = h(&source);
    let elapsed = start.elapsed();

    // The budget = max(1_000_000, 8 * input_len) caps the re-emitted backlink
    // list content (the amplifying part). Uncharged overhead is bounded too: the
    // one-time body marker spans and the per-block `<ul>` wrappers (empty once
    // the budget is exhausted). Allowing 2x the budget as a generous ceiling, the
    // output stays near the budget instead of ballooning to blocks * markers *
    // ~52 bytes (well over 100 MB here) - a ~50x reduction at minimum.
    let budget = 1_000_000usize.max(8 * input_len);
    let unbounded_estimate = 400usize * 3_000 * 52;
    assert!(
        html.len() < 2 * budget,
        "html output {} exceeded bounded ceiling {} (budget {}, unbounded would be ~{}, input {})",
        html.len(),
        2 * budget,
        budget,
        unbounded_estimate,
        input_len
    );
    assert!(
        elapsed.as_secs_f32() < 5.0,
        "bounded index render took {elapsed:?}"
    );
}

/// One very large index term plus many `::: index` blocks. Once the budget is
/// spent, later blocks must NOT re-escape the large term: the escape-then-reject
/// CPU/allocation path has to stay closed.
///
/// THE OUTPUT-SIZE HALF ONLY. The claim is really about the CPU, and the
/// wall-clock ratio that measures it now lives in `tests/perf_regressions.rs`,
/// which CI runs alone and single-threaded (carve-rs#1092). It could not stay
/// here: a ratio over two sub-measurements taken alongside ~3900 other test
/// processes is the shape that made `main` intermittently red on commits
/// touching no engine code, and this one took a best-of-2 with no alternation.
/// What stays is the assertion that needs no clock at all.
#[test]
fn large_first_term_with_many_blocks_stays_under_the_budget() {
    let big_term = "x".repeat(500_000);
    for blocks in [100usize, 200] {
        let mut source = format!(":index[{big_term}]\n\n");
        for _ in 0..blocks {
            source.push_str("::: index\n:::\n\n");
        }
        let html = h(&source);
        let budget = 1_000_000usize.max(8 * source.len());
        assert!(
            html.len() < 2 * budget,
            "html output {} exceeded bounded ceiling {} at {blocks} blocks",
            html.len(),
            2 * budget
        );
    }
}

#[test]
fn normal_index_renders_fully_under_budget() {
    // A small index stays far under the 1 MB floor and renders every backlink
    // in every block, unchanged.
    let out = h("A :index[parser] and :index[lexer].\n\n::: index\n:::\n\n::: index\n:::");
    // Both blocks emit the complete two-entry list with all backlinks.
    assert_eq!(out.matches("<ul class=\"index\">").count(), 2);
    assert_eq!(out.matches("class=\"index-backref\"").count(), 4);
    assert!(out.contains(
        "<li>parser <a href=\"#idx-parser-1\" class=\"index-backref\" aria-label=\"Back to parser\">\u{21a9}</a></li>"
    ));
}
