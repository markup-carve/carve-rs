//! A backtick run's closer is searched for across the rest of the block, so a
//! forced or editorial closer inside the span is code (ruling
//! markup-carve/carve#2079, markup-carve/carve-rs#1740). A run with no closer
//! still ends at the pair's closer (markup-carve/carve#2056).

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
    a_forced_closer_the_span_holds: "x{*`a*} and `b`" => "<p>x{*<code>a*} and </code>b<code></code></p>",
    a_forced_pair_closing_after_the_span: "{*x`a*}`*}" => "<p><strong>x<code>a*}</code></strong></p>",
    an_insertion_closer_the_span_holds: "x{+`a+} and `b`" => "<p>x{+<code>a+} and </code>b<code></code></p>",
    an_insertion_closing_after_the_span: "{+x`a+}`+}" => "<p><ins>x<code>a+}</code></ins></p>",
    a_deletion_closer_the_span_holds: "x{-`a-} and `b`" => "<p>x{-<code>a-} and </code>b<code></code></p>",
}

// Controls.
cases! {
    an_unclosed_run_ends_at_the_pair_closer: "{~` ~}" => "<p><s><code></code></s></p>",
    a_closed_run_inside_the_pair: "{*a`b`c*}" => "<p><strong>a<code>b</code>c</strong></p>",
    an_empty_pair_is_literal: "{**}" => "<p>{**}</p>",
    a_braced_hyphen_pair_is_an_en_dash: "a {--}(p) b" => "<p>a \u{2013}(p) b</p>",
    an_escaped_backtick_opens_no_span: "{*a\\`b*}" => "<p><strong>a`b</strong></p>",
    an_escaped_backtick_a_later_run_could_close: "{+\\`a\\*}{*\\`*}\\`"
        => "<p>{+`a*}<strong>`</strong>`</p>",
}

/// The emphasis scan hides what the parser builds, so a bare closer cannot
/// pair across a span that reaches past a brace pair.
#[test]
fn a_bare_closer_does_not_pair_into_the_span() {
    assert_eq!(html("*x{+`a+}*`"), "<p>*x{+<code>a+}*</code></p>");
}

/// The writer gives an empty code span a run that no later run in the block
/// closes, so its bytes read back as the tree.
#[test]
fn an_empty_span_before_a_later_run_is_written_longer() {
    let source = "{-`-}x ``\n";
    let written = carve::to_carve(source);
    assert_eq!(written, "{-```-}x ``\n");
    assert_eq!(carve::to_html(&written), carve::to_html(source));
}

/// Control: with no later run the empty span keeps its two backticks.
#[test]
fn an_empty_span_alone_keeps_two_backticks() {
    assert_eq!(carve::to_carve("{~` ~}\n"), "{~``~}\n");
}
