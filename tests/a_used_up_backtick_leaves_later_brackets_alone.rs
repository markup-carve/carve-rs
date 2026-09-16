//! A link's text is scanned from its own `[` (CARVE-P3-001), so a backtick an
//! earlier construct already used up cannot keep a later bracket from closing
//! (carve-rs#1733, markup-carve/carve#2074).

/// Both routes: `to_html`, which may take the layout fast path, and a full parse.
fn html(source: &str) -> String {
    let parsed = carve::render_html(&carve::parse(source)).unwrap();
    assert_eq!(carve::to_html(source), parsed, "the two routes disagree");
    parsed.trim_end().to_string()
}

macro_rules! cases {
    ($($name:ident: $source:expr => $expected:expr,)*) => {
        $(
            #[test]
            fn $name() {
                assert_eq!(html($source), $expected);
            }
        )*
    };
}

cases! {
    after_a_forced_strong: "Run {*`make*}[the docs](u)." => "<p>Run <strong><code>make</code></strong><a href=\"u\">the docs</a>.</p>",
    after_a_forced_emphasis: "x{/`a/}[n](u)" => "<p>x<em><code>a</code></em><a href=\"u\">n</a></p>",
    after_an_empty_run_in_a_forced_strong: "x{*``*}[n](u)" => "<p>x<strong><code></code></strong><a href=\"u\">n</a></p>",
    an_image_after_it: "x{*`a*}![n](u)" => "<p>x<strong><code>a</code></strong><img src=\"u\" alt=\"n\"></p>",
    a_span_after_it: "x{*`a*}[n]{.k}" => "<p>x<strong><code>a</code></strong><span class=\"k\">n</span></p>",
    after_an_insertion: "{+`a+} [n](u)" => "<p><ins><code>a</code></ins> <a href=\"u\">n</a></p>",
    after_an_attribute_value: "[a]{title=\"`\"} [n](u)" => "<p><span title=\"`\">a</span> <a href=\"u\">n</a></p>",
    after_a_comment: "{% a ` %}[n](u)" => "<p><a href=\"u\">n</a></p>",
    after_a_destination_with_a_later_pair_of_runs: "[m](u`) and [m](`v) and [n](w)"
        => "<p><a href=\"u`\">m</a> and <a href=\"`v\">m</a> and <a href=\"w\">n</a></p>",
    an_escaped_closer_after_a_used_up_run: "[m](u`) and [\\]m](`v)"
        => "<p><a href=\"u`\">m</a> and <a href=\"`v\">]m</a></p>",
    a_comment_closer_after_a_used_up_run: "[m](u`) and [{#]#}m](`v)"
        => "<p><a href=\"u`\">m</a> and <a href=\"`v\"><span class=\"critic-comment\">]</span>m</a></p>",
}

// Controls: what a scan from the `[` itself still refuses.
cases! {
    an_unclosed_run_before_the_only_closer: "[a `b](u)" => "<p>[a <code>b](u)</code></p>",
    an_unclosed_run_after_a_used_up_run: "[m](u`) and [m ``b](`v)"
        => "<p><a href=\"u`\">m</a> and [m <code>b](`v)</code></p>",
    an_editorial_comment_holding_the_closer: "[{#a]b#}](u)"
        => "<p><a href=\"u\"><span class=\"critic-comment\">a]b</span></a></p>",
}
