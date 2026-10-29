fn corpus_category(prefix: &str) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus");
    let mut count = 0;
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|ext| ext == "crv")
            && path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(prefix)
        {
            let source = std::fs::read_to_string(&path).unwrap();
            let expected = std::fs::read_to_string(path.with_extension("html")).unwrap();
            assert_eq!(
                carve::to_html(&source).trim(),
                expected.trim(),
                "{}",
                path.display()
            );
            count += 1;
        }
    }
    assert!(count > 0, "missing corpus category {prefix}");
}

#[test]
fn a_note_reference_inside_link_text_keeps_its_sup_id() {
    corpus_category("573-");
}

#[test]
fn a_span_never_covers_another_cell() {
    corpus_category("579-");
}

#[test]
fn a_quote_or_term_at_a_marker_note_body_column_opens_in_the_note() {
    corpus_category("580-");
}

#[test]
#[ignore = "rule A ledger: carve-rs#3525"]
fn marker_note_580_moved_fixture_rows_match_the_checker() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/marker_note_580_moved.json")).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 205);
    for row in rows.as_array().unwrap() {
        assert_eq!(
            carve::to_html(row["source"].as_str().unwrap()),
            row["html"].as_str().unwrap(),
            "{}",
            row["id"]
        );
    }
}

#[test]
fn marker_note_opacity_keeps_link_syntax_literal() {
    let cases = [
        (
            "- [^n]: text\n    ```\n    [r]: /url\n    ```\nafter [^n] [r]\n",
            "<ul>\n  <li></li>\n</ul>\n<p>after <a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> [r]</p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>text</p>\n      <pre><code>[r]: /url\n</code></pre>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
        ),
        (
            "- [^n]: text\n    %%%\n    [r]: /url\n    %%%\nafter [^n] [r]\n",
            "<ul>\n  <li></li>\n</ul>\n<p>after <a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> [r]</p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>text<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
        ),
    ];
    for (source, html) in cases {
        assert_eq!(carve::to_html(source), html);
    }
}

#[test]
fn an_item_quote_below_the_note_column_stops_at_a_blank() {
    let source = "- [^n]: text\n   > q\n\nafter [^n]\n";
    let html = carve::to_html(source);
    assert!(html.contains("<blockquote><p>q</p></blockquote>"));
    assert!(html.contains("</ul>\n<p>after "), "{html}");
}

#[test]
fn typography_uses_one_flank_class() {
    for flank in ['\u{000b}', '\u{000c}', '\u{0085}', '\u{2003}'] {
        assert_eq!(
            carve::to_html(&format!("x{flank}\"q\"")),
            format!("<p>x{flank}”q”</p>")
        );
    }
    for flank in [' ', '\t', '\u{00a0}'] {
        let rendered = if flank == '\u{00a0}' {
            "&nbsp;".to_owned()
        } else {
            flank.to_string()
        };
        assert_eq!(
            carve::to_html(&format!("x{flank}\"q\"")),
            format!("<p>x{rendered}“q”</p>")
        );
    }
}

#[test]
fn a_tab_before_a_trailing_comment_stays_content() {
    corpus_category("518-");
    corpus_category("516-");
    assert_eq!(carve::to_html("a\t %% c"), "<p>a\t</p>");
}

#[test]
fn a_colon_opener_on_an_item_marker_line_opens() {
    corpus_category("161-");
}

#[test]
fn comments_retain_only_below_the_content_column() {
    corpus_category("557-");
    corpus_category("570-");
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/comment_retention_wave.json")).unwrap();
    let compact = regex::Regex::new(r">\s+<").unwrap();
    for row in rows.as_array().unwrap() {
        assert_eq!(
            compact
                .replace_all(&carve::to_html(row["source"].as_str().unwrap()), "><")
                .trim(),
            row["html"].as_str().unwrap(),
            "{}",
            row["id"]
        );
    }
}

#[test]
fn a_hosted_colon_body_opens_blocks_past_its_column() {
    corpus_category("566-");
    corpus_category("571-");
    corpus_category("572-");
}

#[test]
fn a_quoted_term_takes_a_plain_lazy_line() {
    corpus_category("569-");
    corpus_category("82-blockquote-lazy-continuation-5");
}

#[test]
fn a_line_block_takes_a_lazy_line_whatever_its_payload() {
    corpus_category("575-");
}

#[test]
#[ignore = "rule A ledger: carve-rs#3523"]
fn below_column_comments_keep_definition_bands_and_fences() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/comment_fence_review_wave.json")).unwrap();
    for row in rows.as_array().unwrap() {
        assert_eq!(
            carve::to_html(row["source"].as_str().unwrap()),
            row["html"].as_str().unwrap()
        );
        let positioned = carve::parse_with_options(
            row["source"].as_str().unwrap(),
            &carve::Options {
                positions: true,
                ..Default::default()
            },
        );
        assert_eq!(
            carve::render_html(&positioned).unwrap(),
            row["html"].as_str().unwrap()
        );
    }
}

#[test]
fn a_tab_before_a_comment_does_not_hide_an_emphasis_closer() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/tab_comment_closers.json")).unwrap();
    for row in rows.as_array().unwrap() {
        assert_eq!(
            carve::to_html(row["source"].as_str().unwrap()),
            row["html"].as_str().unwrap()
        );
    }
}
