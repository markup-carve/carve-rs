//! carve#5957: an attribute line in a plus-skipped note stays in the note.
//! carve#5886: a lazy line after a quoted span that S1 ended folds at every depth.

const ENDNOTE_N: &str = r##"<section role="doc-endnotes" aria-label="Footnotes">
  <hr>
  <ol>
    <li id="fn1">
      <p>n<a href="#fnref1" role="doc-backlink" aria-label="Back to reference">↩</a></p>
    </li>
  </ol>
</section>"##;

#[test]
fn a_note_attribute_line_under_a_plus_is_dropped_with_the_note() {
    assert_eq!(
        carve::to_html("- +\n[^m]: n\n\t{y}\n\n[^m]\n").trim(),
        format!(
            "<ul>\n  <li></li>\n</ul>\n<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n{ENDNOTE_N}"
        )
    );
    let expected = format!(
        "<ul>\n  <li>q</li>\n</ul>\n<p>p<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n{ENDNOTE_N}"
    );
    for lead in ["  ", "   ", "    ", "      ", "\t"] {
        for attrs in ["{y}", "{.c}", "{#i}"] {
            let source = format!("- +\n[^m]: n\n{lead}{attrs}\nq\n\np[^m]\n");
            assert_eq!(carve::to_html(&source).trim(), expected, "{source:?}");
        }
    }
}

#[test]
fn an_attribute_line_left_of_the_note_body_column_stays_the_hosts() {
    let html = carve::to_html("- +\n[^m]: n\n {y}\nq\n\np[^m]\n");
    assert!(
        html.contains("<ul>\n  <li></li>\n</ul>\n<p y=\"\">q</p>"),
        "{html}"
    );
}

#[test]
fn a_lazy_line_folds_after_a_quoted_span_ended_left_of_its_item() {
    for depth in 1..=3 {
        let q = "> ".repeat(depth);
        for (host, tail) in [
            ("- [^n]: a", "\nx[^n]\n"),
            ("- a", ""),
            (":: t\n: [^n]: a", "\nx[^n]\n"),
        ] {
            let mut source = String::new();
            for line in host.lines() {
                source.push_str(&format!("{q}{line}\n"));
            }
            source.push_str(&format!("{q}  %%%\n{q}z\nplain\n{tail}"));
            let html = carve::to_html(&source);
            assert!(html.contains("<p>z\nplain</p>"), "{source:?}: {html}");
        }
    }
}
