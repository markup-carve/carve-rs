#[test]
fn a_nested_item_owns_the_plus_at_its_marker_column() {
    let source = "- a\n  - b\n  +\n  > q";
    let expected = "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <blockquote><p>q</p></blockquote>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_tabbed_nested_item_keeps_its_marker_out_of_the_quote() {
    let source = "- x\n\t- > a\n    +\n    b\n";
    let expected = "<ul>\n  <li>x\n    <ul>\n      <li>\n        <blockquote><p>a</p></blockquote>\n        b\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn an_ordered_item_takes_a_tabbed_follower_after_its_marker() {
    let source = "1. ordered\n  [^n]: note\n    1. ordered\n    +\n\ttext\n";
    let expected = "<ol>\n  <li>ordered</li>\n</ol>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn a_refused_plus_after_a_fence_remains_a_paragraph() {
    let source = "- ```\n+\n";
    let expected = "<ul>\n  <li>\n    <pre><code></code></pre>\n  </li>\n</ul>\n<p>+</p>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn two_attached_quotes_remain_in_the_same_list_item() {
    let source = "- x\n+\n> q\n+\n> q";
    let expected = "<ul>\n  <li>x\n    <blockquote><p>q</p></blockquote>\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_sibling_list_interrupts_a_refused_plus_paragraph() {
    let source = "- a\n\n+\n- s";
    let expected = "<ul>\n  <li>a</li>\n</ul>\n<p>+\n- s</p>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn refused_plus_after_nested_ordered_items_precedes_a_list() {
    for middle in [
        "  1. ```\n+",
        "  1. b\n\n+",
        "  1. ```\n\n+",
        "  1. b\n    1. c\n\n+",
        "  1. b\n    1. ```\n\n+",
    ] {
        let source = format!("1. a\n\n{middle}\n- s\n");
        let html = carve::to_html(&source);
        assert!(html.contains("<p>+\n- s</p>"), "{source:?}: {html}");
    }
}

#[test]
fn refused_plus_after_a_paragraph_can_fold_a_list_marker() {
    assert_eq!(carve::to_html("a\n\n+\n- b"), "<p>a</p>\n<p>+\n- b</p>");
}

#[test]
fn a_marker_at_the_inner_quotes_column_separates_its_paragraphs() {
    let source = "> > q\n> +\n> para";
    let expected = "<blockquote>\n  <blockquote>\n    <p>q</p>\n    <p>para</p>\n  </blockquote>\n</blockquote>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_quote_first_block_inside_an_item_attaches_a_quoted_follower() {
    let source = "- > +\n> z";
    let expected = "<ul>\n  <li>\n    <blockquote><p>+</p></blockquote>\n  </li>\n</ul>\n<blockquote><p>z</p></blockquote>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_refused_indented_plus_folds_the_following_list_line() {
    let source = "- a\n\n +\n- s";
    let expected = "<ul>\n  <li>a</li>\n</ul>\n<p>+\n- s</p>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_dangling_plus_in_a_nested_quote_remains_text() {
    let source = "> > q\n> +";
    let expected = "<blockquote>\n  <blockquote><p>q\n+</p></blockquote>\n</blockquote>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_plus_below_an_inner_quoted_task_item_attaches_to_the_quote() {
    let source = "> > - [ ] a\n> +\n> tail";
    let html = carve::to_html(source);
    assert!(html.contains("</ul>\n    <p>tail</p>"), "{html}");
}

#[test]
fn a_nested_quote_first_block_accepts_the_outer_quote_column() {
    let source = "> > +\n> b";
    let expected = "<blockquote>\n  <blockquote><p>b</p></blockquote>\n</blockquote>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_list_hosted_quote_owns_its_marker_column() {
    let source = "- > a\n  +\n  b";
    let expected = "<ul>\n  <li>\n    <blockquote>\n      <p>a</p>\n      <p>b</p>\n    </blockquote>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_refused_quoted_plus_after_a_closed_block_can_precede_a_list() {
    let html = carve::to_html("> - ```\n> +\n> - s");
    assert!(html.contains("<p>+\n- s</p>"), "{html}");
}

#[test]
fn a_refused_quoted_plus_after_a_blank_folds_the_list_line() {
    let html = carve::to_html("> - a\n\n> +\n> - s");
    assert!(
        html.contains("<blockquote>\n  <blockquote>\n    <ul>\n      <li>s</li>"),
        "{html}"
    );
}

#[test]
fn a_quote_marker_after_a_document_blank_does_not_claim_a_closed_list() {
    let html = carve::to_html("> - a\n\n> +\n> tail");
    assert!(
        html.contains("<blockquote>\n  <blockquote><p>tail</p></blockquote>"),
        "{html}"
    );
}

#[test]
fn a_quote_first_block_after_a_paragraph_blank_attaches() {
    assert_eq!(
        carve::to_html("a\n\n> +\nq"),
        "<p>a</p>\n<blockquote><p>q</p></blockquote>"
    );
}

#[test]
fn a_nested_first_block_list_item_takes_the_document_column() {
    let source = "- a\n  - +\npara";
    let expected = "<ul>\n  <li>a\n    <ul>\n      <li>+\npara</li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_nested_first_block_quote_takes_a_heading() {
    let html = carve::to_html("- a\n  - > +\n# h");
    assert!(
        html.contains("</blockquote>\n      </li>\n    </ul>\n  </li>\n</ul>\n<section id=\"h\">"),
        "{html}"
    );
}

#[test]
fn a_refused_inner_plus_keeps_its_indented_quote_follower() {
    let source = "- a\n  - b\n  +\n    > q";
    let expected = "<ul>\n  <li>a\n    <ul>\n      <li>b\n+\n        <blockquote><p>q</p></blockquote>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_refused_plus_in_a_quoted_nested_item_keeps_lazy_text() {
    let source = "> - a\n>   - b\n>  +\n> tail";
    let expected = "<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li>b\n+\ntail</li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_list_hosted_inner_quote_takes_its_relative_marker() {
    let source = "> - > a\n>   +\n>   b\n";
    let expected = "<blockquote>\n  <ul>\n    <li>\n      <blockquote>\n        <p>a</p>\n        <p>b</p>\n      </blockquote>\n    </li>\n  </ul>\n</blockquote>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_refused_quoted_plus_does_not_pull_a_fence_into_the_quote() {
    let source = "> quote\n> +\n```\ncode\n```\n";
    let expected = "<blockquote><p>quote\n+</p></blockquote>\n<pre><code>code\n</code></pre>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_description_body_keeps_its_marker_after_nested_lists() {
    let source = ":: term\n:  detail\n  - a\n    1. ordered\n      * [ ] task\n+\n- item\n";
    let expected = "<dl>\n  <dt>term</dt>\n  <dd>\n    <p>detail\n- a\n1. ordered\n* [ ] task</p>\n    <ul>\n      <li>item</li>\n    </ul>\n  </dd>\n</dl>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_quote_opened_on_a_later_item_line_refuses_an_unprefixed_marker() {
    let source = "> - a\n>   > q\n>   +\n>   para\n";
    let expected = "<blockquote>\n  <ul>\n    <li>a\n      <blockquote>\n        <p>q</p>\n        <p>para</p>\n      </blockquote>\n    </li>\n  </ul>\n</blockquote>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_refused_plus_after_a_nested_fence_stays_outside_the_list() {
    let source = "- a\n  - ```\n +\ntail";
    let expected = "<ul>\n  <li>a\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>+\ntail</p>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn an_attached_block_does_not_duplicate_a_nested_fence_blank() {
    let source = "- a\n  - ```\n\n+\ntail";
    let expected = "<ul>\n  <li>a\n    <ul>\n      <li>\n        <pre><code>\n</code></pre>\n      </li>\n    </ul>\n    tail\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn an_indented_list_line_stays_in_a_refused_plus_paragraph() {
    assert_eq!(carve::to_html("[^a]: a\n+\n  - b"), "");
}

#[test]
fn a_quote_refuses_a_plus_followed_by_a_sibling_list_opener() {
    for (source, expected) in [
        (
            "- a\n  > q\n  +\n  - b",
            "<ul>\n  <li>a\n    <blockquote>\n      <p>q</p>\n      <ul>\n        <li>b</li>\n      </ul>\n    </blockquote>\n  </li>\n</ul>",
        ),
        (
            "- a\n  > q\n+\n  - b",
            "<ul>\n  <li>a\n    <blockquote><p>q\n+\n- b</p></blockquote>\n  </li>\n</ul>",
        ),
    ] {
        assert_eq!(carve::to_html(source).trim_end(), expected, "{source}");
    }
}

#[test]
fn a_refused_quote_marker_after_a_blank_and_nested_fence_keeps_the_list_line() {
    let source = "> - a\n\n>   - ```\n> +\n> - s\n";
    let html = carve::to_html(source);
    assert!(html.contains("<p>+\n- s</p>"), "{html}");
}

#[test]
fn a_marker_after_a_collected_definition_reaches_the_surviving_item() {
    for (definition, expected) in [
        (
            "[^n]: note",
            "<ul>\n  <li>a\n    <ol>\n      <li>ordered</li>\n    </ol>\n    <blockquote><p>quote</p></blockquote>\n  </li>\n</ul>",
        ),
        (
            "[r]: /target",
            "<ul>\n  <li>a\n    <ol>\n      <li>ordered</li>\n    </ol>\n    +\n    <blockquote><p>quote</p></blockquote>\n  </li>\n</ul>",
        ),
    ] {
        let source = format!("- a\n  1. ordered\n    {definition}\n  +\n  > quote\n");
        assert_eq!(carve::to_html(&source), expected, "{source}");
    }
}

#[test]
fn an_indented_footnote_marker_attaches_at_the_notes_marker_column() {
    let source = "[^n]: note\n  [^m]: body\n  +\n  tail\n\nref[^m]\n";
    let html = carve::to_html(source);
    assert!(html.contains("<p>body</p>\n      <p>tail<a"), "{html}");
    assert!(!html.contains('+'), "{html}");
}

#[test]
fn a_below_column_list_line_stays_lazy_after_either_plus_or_prose() {
    let expected =
        "<ul>\n  <li>\n    <ul>\n      <li>+</li>\n    </ul>\n  </li>\n  <li>x</li>\n</ul>";
    for text in ["+", "ordinary"] {
        let source = format!("- - +\n  {text}\n - x\n");
        assert_eq!(carve::to_html(&source).replace(text, "+"), expected);
    }
}

#[test]
fn a_new_quote_keeps_a_plus_before_a_prefixed_follower_as_text() {
    let expected = "<blockquote>\n  <blockquote><p>quote</p></blockquote>\n</blockquote>";
    assert_eq!(carve::to_html("> +\n> quote\n"), expected);
}

#[test]
fn a_refused_plus_keeps_the_same_below_column_code_run_as_prose() {
    let source = "- a\n  :: term\n  :  detail\n +\n ```\ncode\n ```\n";
    let ordinary = source.replacen(" +\n", " ordinary\n", 1);
    assert_eq!(
        carve::to_html(source),
        carve::to_html(&ordinary).replace("ordinary", "+")
    );
}

#[test]
fn a_lazy_nested_item_before_a_refused_plus_still_precedes_a_list() {
    let source = "- a\n  - b\nlazy\n\n+\n- s\n";
    assert!(carve::to_html(source).ends_with("<p>+\n- s</p>"));
}

#[test]
fn a_quoted_footnote_attaches_a_deeper_quote_after_plus() {
    let source = "> [^a]: x\n> +\n> > r\n\n[^a]\n";
    let html = carve::to_html(source);
    assert!(!html.contains("<p>+</p>"), "{html}");
    assert!(
        html.contains("<li id=\"fn1\">\n      <p>x</p>\n      <blockquote><p>r</p></blockquote>"),
        "{html}"
    );
}

#[test]
fn a_quoted_description_attaches_a_deeper_quote_after_plus() {
    let html = carve::to_html("> :: t\n> : a\n> +\n> > q\n");
    assert!(
        html.contains("<dd>\n      <p>a</p>\n      <blockquote><p>q</p></blockquote>\n    </dd>"),
        "{html}"
    );
}
