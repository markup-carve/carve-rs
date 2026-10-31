//! The confirmation-round rulings and model fixes (carve#4548 to carve#4574),
//! each pinned in more than the reported host.

fn html(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn contains(source: &str, expected: &str) {
    let out = html(source);
    assert!(out.contains(expected), "{source:?}: {out}");
}

fn lacks(source: &str, unexpected: &str) {
    let out = html(source);
    assert!(!out.contains(unexpected), "{source:?}: {out}");
}

#[test]
fn a_blanked_style_still_takes_the_alignment() {
    contains(
        "{style=\"url(x)\" align=right}\np\n",
        "<p style=\"text-align: right;\">p</p>",
    );
    contains(
        "{align=center style=\"url(x)\"}\n# h\n",
        "style=\"text-align: center;\"",
    );
    contains(
        "{style=\"color: red\" align=right}\np\n",
        "style=\"color: red; text-align: right;\"",
    );
}

#[test]
fn a_cell_s_escaped_pipe_reaches_a_destination_as_a_pipe() {
    contains("| [x](/a\\|b) |\n", "<a href=\"/a|b\">x</a>");
    contains("| ![x](/a\\|b) |\n", "<img src=\"/a|b\" alt=\"x\">");
    contains("|= h |\n| [x](/a\\|b \"t\") |\n", "href=\"/a|b\"");
    // Outside a table the backslash stays, and a code span keeps it.
    contains("[x](/a\\|b)\n", "href=\"/a\\|b\"");
    contains("| `a\\|b` [x](/c) |\n", "<code>a\\|b</code>");
}

#[test]
fn a_literal_bracket_run_is_a_typography_region() {
    contains("a [--x] y and a [---x] y\n", "a [--x] y and a [---x] y");
    contains("a (--x) y\n", "(\u{2013}x) y");
    contains("x [a --] y\n", "[a \u{2013}] y");
    contains("[[--x] y\n", "[[--x] y");
    contains("a [--x y\n", "[\u{2013}x y");
}

#[test]
fn a_comment_at_the_start_of_content_counts_as_nothing_for_quotes() {
    contains("{%%}'word\n", "<p>\u{2018}word</p>");
    contains("{% c %}{% d %}'word\n", "<p>\u{2018}word</p>");
    contains("> {%%}'word\n", "<p>\u{2018}word</p>");
    contains("a {%%}'word\n", "a \u{2019}word");
    contains("{%%}\"q\"\n", "\u{201c}q\u{201d}");
}

#[test]
fn a_cloned_heading_drops_its_ids() {
    contains(
        "# *A*{#z}\n\n</#A>\n",
        "<h1><strong id=\"z\">A</strong></h1> <p><a href=\"#A\"><strong>A</strong></a></p>",
    );
    contains(
        "# *[b]{#y abbr=\"T\"}*{#z}\n\n</#b>\n",
        "<a href=\"#b\"><strong><abbr title=\"T\">b</abbr></strong></a>",
    );
}

#[test]
fn a_decomposed_cross_reference_finds_its_heading() {
    // The id is NFC; the link text clones the heading as written.
    contains(
        "# Cafe\u{301}\n\n</#Cafe\u{301}>\n",
        "<a href=\"#Caf\u{e9}\">Cafe\u{301}</a>",
    );
}

#[test]
fn the_top_inline_run_is_level_one_of_the_cap_in_every_host() {
    // 199 levels outgrow a debug build's default test-thread stack.
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(top_inline_run_cap)
        .unwrap()
        .join()
        .unwrap();
}

fn top_inline_run_cap() {
    let strong = |source: &str| html(source).matches("<strong>").count();
    let spans = |n: usize| "{*".repeat(n) + "x" + &"*}".repeat(n);
    assert_eq!(strong(&format!("{}\n", spans(199))), 199);
    assert_eq!(strong(&format!("{}\n", spans(200))), 199);
    assert_eq!(strong(&format!("> {}\n", spans(199))), 199);
    assert_eq!(strong(&format!("- {}\n", spans(199))), 199);
    // Bare and braced spans spend one budget.
    let mixed = "{*".repeat(198) + "/x/" + &"*}".repeat(198) + "\n";
    assert_eq!(html(&mixed).matches("<em>").count(), 1);
}

#[test]
fn a_continuation_cell_reads_the_header_cell_a_colspan_marker_extends() {
    contains(
        "|= a | < |\n+ | | c |\n",
        "<thead> <tr><th scope=\"col\" colspan=\"2\">a</th><th scope=\"col\">c</th></tr>",
    );
    // A marker that extends no header cell keeps the row a body row.
    contains(
        "| < |= a |\n+ | | c |\n",
        "<tr><td></td><th scope=\"row\">a</th><td>c</td></tr>",
    );
}

#[test]
fn a_comment_after_a_lone_image_leaves_it_a_caption_host() {
    let figure = "<figure> <img src=\"/i\" alt=\"a\"> <figcaption>cap</figcaption> </figure>";
    for source in [
        "![a](/i) %% c\n^ cap\n",
        "![a](/i){%%}\n^ cap\n",
        "![a](/i) %% c\n\n^ cap\n",
        "> ![a](/i){%%}\n> ^ cap\n",
        "- ![a](/i) %% c\n  ^ cap\n",
        "- ![a](/i) %% c\n\n  ^ cap\n",
    ] {
        contains(source, figure);
    }
    // Only the text after the image is read for comments.
    contains(
        "![a](/i{%x%}) %% c\n^ cap\n",
        "<img src=\"/i{%x%}\" alt=\"a\"> <figcaption>",
    );
    // A space before a braced comment renders, so the line is no lone image.
    lacks("![a](/i) {% c %}\n^ cap\n", "<figure>");
    lacks("- ![a](/i) {%%}\n  ^ cap\n", "<figure>");
}

#[test]
fn a_quote_holding_only_empty_containers_takes_no_caption() {
    for source in ["> >\n^ cap\n", "> :::\n> :::\n^ cap\n", "> > %%\n^ cap\n"] {
        contains(source, "<p>^ cap</p>");
        lacks(source, "<figure>");
    }
    contains("> >\n> > a\n^ cap\n", "<figcaption>cap</figcaption>");
}
