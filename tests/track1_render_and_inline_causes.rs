//! Reproductions from the cause tickets this file names; expected output is the
//! spec model's.

fn html(source: &str) -> String {
    carve::to_html(source).trim_end().to_string()
}

#[test]
fn a_dropped_id_reserves_nothing() {
    for reference in ["x[^n]{#i}", "[a][nope]{#i}", "![a][nope]{#i}"] {
        let out = html(&format!("{reference}\n\n## i\n"));
        assert!(out.contains("<section id=\"i\">"), "{reference}: {out}");
    }
    // A rendered id still takes part.
    assert!(html("[a](/u){#i}\n\n## i\n").contains("<section id=\"i-2\">"));
}

#[test]
fn a_typed_block_merges_its_classes_once() {
    assert_eq!(
        html("{.note .admonition}\n::: note\n42\n:::\n"),
        "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p>42</p>\n</aside>"
    );
    assert_eq!(
        html("{.calculation}\n::: calculation\n42\n:::\n"),
        "<div class=\"calculation\">\n  <p>42</p>\n</div>"
    );
}

#[test]
fn a_class_value_is_a_list_of_tokens() {
    assert_eq!(
        html("{class=\"a b\" .a}\npara\n"),
        "<p class=\"a b\">para</p>"
    );
    assert_eq!(html("{class=\"\"}\npara\n"), "<p class=\"\">para</p>");
    assert_eq!(
        html("{class=\"javascript:x y\"}\npara\n"),
        "<p class=\"y\">para</p>"
    );
}

#[test]
fn a_crossref_keeps_the_caption_label_markup() {
    let cases = [
        ("*Fig*", "<strong>Fig</strong> 1"),
        ("Fig\\ ure", "Fig&nbsp;ure 1"),
        ("[x][nope]", "[x][nope] 1"),
        ("[l](/u)", "l 1"),
    ];
    for (label, shown) in cases {
        let out = html(&format!("{{#t}}\n||\n^ {label} #: S\n\nSee </#t>.\n"));
        let want = format!("<p>See <a href=\"#t\">{shown}</a>.</p>");
        assert!(out.contains(&want), "{label}: {out}");
    }
    let out = html("[u]: a.png\n\n{#t}\n||\n^ ![x][u] #: S\n\nSee </#t>.\n");
    assert!(
        out.contains("<a href=\"#t\"><img src=\"a.png\" alt=\"x\"> 1</a>"),
        "{out}"
    );
}

#[test]
fn a_comment_only_continuation_cell_joins_nothing() {
    for cell in ["%% y", "%%%y%%%"] {
        let out = html(&format!("| a | b |\n+ x | {cell} |\n"));
        assert!(out.contains("<td>a x</td><td>b</td>"), "{cell}: {out}");
    }
    // The line comment runs to the end of the cell's text, so `q` is in it.
    let out = html("| a | b |\n+ x | %% y |\n+ p | q |\n");
    assert!(out.contains("<td>a x p</td><td>b</td>"), "{out}");
}

#[test]
fn a_div_label_keeps_the_escaped_space_before_a_comment() {
    for label in ["x\\  %% c", "x\\ %% c"] {
        let out = html(&format!("::: note [{label}]\nbody\n:::\n"));
        assert!(out.contains("<p class=\"div-label\">x&nbsp;</p>"), "{out}");
    }
    let out = html("::: note [x\\\\ %% c]\nbody\n:::\n");
    assert!(out.contains("<p class=\"div-label\">x\\</p>"), "{out}");
}

#[test]
fn a_quoted_row_attribute_value_may_hold_a_pipe() {
    assert!(html("| a |{a=\"x|y\"}\n").contains("<tr a=\"x|y\"><td>a</td></tr>"));
    assert!(html("| a |{~|{.c}\n").contains("<tr class=\"c\">"));
    assert!(html("| a |{bad!!}\n").starts_with("<p>"));
}

#[test]
fn a_list_marker_line_of_a_caption_keeps_its_indentation() {
    assert!(html("|x|\n^ a\n  - b\n").contains("<caption>a\n  - b</caption>"));
    assert!(html("|x|\n^ a\n  b\n").contains("<caption>a\n  b</caption>"));
}

#[test]
fn a_display_math_span_over_several_lines_takes_a_caption() {
    let want = "<figure>\n  <p><span class=\"math display\" role=\"math\">\\[x\ny\\]</span></p>\n  <figcaption>p</figcaption>\n</figure>";
    assert_eq!(html("$$`x\ny`\n^ p\n"), want);
    assert_eq!(html("$$`x\ny`\n\n^ p\n"), want);
    assert_eq!(html("$$`x\ny\n^ p\n"), want);
    assert!(html("- $$`x\n  y`\n  ^ p\n").contains("<figcaption>p</figcaption>"));
    // Text after the span keeps a paragraph.
    assert!(html("$$`x\ny` z\n^ p\n").starts_with("<p>"));
}

#[test]
fn a_form_feed_is_heading_content_at_any_indent() {
    assert!(html(" # \u{c}\n").contains("<h1>"));
    assert!(html(">  ## \u{c}\n").contains("<h2 id="));
}

#[test]
fn a_definition_nested_in_a_losing_duplicate_is_collected() {
    let out = html("[^g]: a\n\n[^g]: b\n  [^h]: inner\n\nx[^g] [^h]\n");
    assert!(out.contains("<sup>2</sup>"), "{out}");
    assert!(out.contains("<p>inner<a"), "{out}");
    assert!(!out.contains("<p>b"), "{out}");
}

#[test]
fn shapes_already_read_like_the_model_stay_pinned() {
    assert_eq!(html("{+`+}\n"), "<p>{+<code>+}</code></p>");
    assert_eq!(
        html("_q_ =/_a b/x_{t=\"//\"}= e*\n"),
        "<p><u>q</u> <mark>/<u t=\"//\">a b/x</u></mark> e*</p>"
    );
    assert_eq!(html("[a][a]{.c}{#i}\n"), "<p>[a][a]{.c}{#i}</p>");
    let out = html("{header-rows=7}\n|= a |\n| b |\n");
    assert!(
        out.contains("<th scope=\"row\">a</th>") && !out.contains("<thead>"),
        "{out}"
    );
}
