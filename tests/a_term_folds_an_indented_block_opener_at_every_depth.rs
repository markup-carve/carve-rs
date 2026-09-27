//! A definition term has no content column, so a block opener indented past
//! the enclosing container's content column is term text at every depth
//! (markup-carve/carve#2411). List markers keep Rule B and still open. Shapes
//! and the lint rule mirror carve-js, the parity reference.

fn flat(source: &str) -> String {
    let html = carve::to_html(source);
    let mut out = String::new();
    let mut lines = html.split('\n');
    if let Some(first) = lines.next() {
        out.push_str(first);
    }
    for line in lines {
        out.push_str(line.trim_start());
    }
    out.trim_end().to_string()
}

#[test]
fn an_indented_opener_folds_into_the_term() {
    for (source, html) in [
        (":: c\n  # H\n", "<dl><dt>c# H</dt></dl>"),
        (
            "> :: c\n>   # H\n",
            "<blockquote><dl><dt>c# H</dt></dl></blockquote>",
        ),
        (
            "- item\n\n  :: c\n    # H\n",
            "<ul><li>item<dl><dt>c# H</dt></dl></li></ul>",
        ),
        (
            ":: a\n: b\n  :: c\n    # H\n",
            "<dl><dt>a</dt><dd><p>b</p><dl><dt>c# H</dt></dl></dd></dl>",
        ),
        (
            ":: a\n: b\n  :: c\n    ::: note\n    body\n    :::\n",
            "<dl><dt>a</dt><dd><p>b</p><dl><dt>c::: notebody:::</dt></dl></dd></dl>",
        ),
        (
            "- item\n\n  :: c\n    ::: note\n    body\n    :::\n",
            "<ul><li>item<dl><dt>c::: notebody:::</dt></dl></li></ul>",
        ),
        (
            ":: a\n: b\n  :: c\n    :: d\n",
            "<dl><dt>a</dt><dd><p>b</p><dl><dt>c:: d</dt></dl></dd></dl>",
        ),
    ] {
        assert_eq!(flat(source), html, "{source:?}");
    }
}

#[test]
fn an_opener_at_the_container_column_opens() {
    assert_eq!(
        flat(":: a\n: b\n  :: c\n  # H\n"),
        "<dl><dt>a</dt><dd><p>b</p><dl><dt>c</dt></dl><h1 id=\"H\">H</h1></dd></dl>"
    );
    assert_eq!(
        flat(":: c\n# H\n"),
        "<dl><dt>c</dt></dl><section id=\"H\"><h1>H</h1></section>"
    );
}

#[test]
fn a_list_marker_still_opens_under_a_nested_term() {
    assert_eq!(
        flat(":: a\n: b\n  :: c\n    - x\n"),
        "<dl><dt>a</dt><dd><p>b</p><dl><dt>c</dt></dl><ul><li>x</li></ul></dd></dl>"
    );
}

#[test]
fn the_folded_text_formats_back_to_itself() {
    for source in [
        ":: a\n: b\n  :: c\n    # H\n",
        "- item\n\n  :: c\n    ::: note\n    body\n    :::\n",
    ] {
        let once = carve::render_carve(&carve::parse(source)).unwrap();
        assert_eq!(carve::to_html(&once), carve::to_html(source), "{source:?}");
        assert_eq!(carve::render_carve(&carve::parse(&once)).unwrap(), once);
    }
}

fn reports(source: &str) -> Vec<(usize, usize)> {
    carve::lint_carve(source)
        .into_iter()
        .filter(|w| w.rule == "definition-term-block-folded")
        .map(|w| (w.line, w.column))
        .collect()
}

#[test]
fn the_first_folded_opener_is_reported_once_per_term() {
    assert_eq!(
        reports(":: a\n: b\n  :: c\n    ::: note\n    body\n    :::\n"),
        [(4, 5)]
    );
    assert_eq!(reports("- item\n\n  :: c\n    # H\n"), [(4, 5)]);
    assert_eq!(reports(":: c\n  # H\n"), [(2, 3)]);
    assert_eq!(reports("> :: c\n>   # H\n"), [(2, 5)]);
}

#[test]
fn plain_text_list_markers_and_openers_that_open_are_quiet() {
    assert!(reports(":: c\n  more text\n").is_empty());
    assert!(reports(":: c\n  - x\n").is_empty());
    assert!(reports(":: c\n# H\n").is_empty());
    assert!(reports(":: a\n: b\n  :: c\n  # H\n").is_empty());
}

#[test]
fn a_comment_or_a_definition_still_ends_a_nested_term() {
    let heading = "<dl><dt>a</dt><dd><p>b</p><dl><dt>c</dt></dl><h1 id=\"H\">H</h1></dd></dl>";
    assert_eq!(flat(":: a\n: b\n  :: c\n    %% note\n    # H\n"), heading);
    assert_eq!(flat(":: a\n: b\n  :: c\n    [r]: /u\n    # H\n"), heading);
}

#[test]
fn a_term_in_a_footnote_body_is_linted() {
    let warnings: Vec<_> = carve::lint_carve("x[^n]\n\n[^n]: text\n\n  :: c\n    # H\n")
        .into_iter()
        .filter(|w| w.rule == "definition-term-block-folded")
        .map(|w| (w.line, w.column))
        .collect();
    assert_eq!(warnings, vec![(6, 5)]);
}

#[test]
fn a_carriage_return_or_byte_order_mark_does_not_hide_the_warning() {
    let at = |source: &str| -> Vec<(usize, usize)> {
        carve::lint_carve(source)
            .into_iter()
            .filter(|w| w.rule == "definition-term-block-folded")
            .map(|w| (w.line, w.column))
            .collect()
    };
    assert_eq!(at(":: c\r  # H\r"), vec![(2, 3)]);
    assert_eq!(at(":: c\r\n  # H\r\n"), vec![(2, 3)]);
    assert_eq!(at("\u{feff}:: c\n # H\n"), vec![(2, 2)]);
    assert_eq!(at(":: c\n  ::: |\n  a\n  :::\n"), vec![(2, 3)]);
    assert_eq!(at(":: a\n  `code\n  # H\n  end`\n"), vec![]);
    assert_eq!(at(":: *`code\n  # H\n  end`*\n"), vec![]);
}
