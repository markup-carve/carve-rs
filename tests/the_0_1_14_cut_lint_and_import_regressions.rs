//! Regressions the 0.1.14 cut gates found: carve#5950, carve#5951, carve#5952
//! and carve-rs#4260.

fn findings(source: &str) -> Vec<(usize, usize, &'static str)> {
    carve::lint_carve(source)
        .into_iter()
        .map(|w| (w.line, w.column, w.rule))
        .collect()
}

#[test]
fn a_refused_djot_brace_run_imports_with_an_escaped_opener() {
    let converted = carve::djot_to_carve("{#one\nkey=1}\nalpha\n");
    assert_eq!(converted, "\\{\\#one\nkey=1}\nalpha\n");
    assert_eq!(
        carve::to_html(&converted).trim(),
        "<p>{#one\nkey=1}\nalpha</p>"
    );
}

#[test]
fn a_refused_plus_at_the_host_marker_column_leaves_its_fence_unclosed() {
    for (source, fence) in [
        ("- li\n  ```\n+\n  p\n", (2, 3)),
        ("- ```\n  x\n+\n\ntail\n", (1, 3)),
        ("[^a]: +\n- li\n  ```\n+\n  p\n\nsee[^a]\n", (3, 3)),
    ] {
        let found = findings(source);
        assert!(
            found.contains(&(fence.0, fence.1, "unclosed-code-fence")),
            "{source:?}: {found:?}"
        );
        assert_eq!(
            found
                .iter()
                .filter(|f| f.2 == "unclosed-code-fence")
                .count(),
            1,
            "{source:?}: {found:?}"
        );
    }
}

#[test]
fn an_unattached_attribute_after_a_comment_run_reports_its_source_column() {
    let note = "x[^n]\n\n[^n]: {% a\n     b\n     z %}\n    {.x}\nplain\n";
    assert_eq!(findings(note), [(6, 5, "unattached-block-attribute")]);
    let description = ":: t\n:   {% a\n     {k=v}\n: next\n";
    assert_eq!(
        findings(description),
        [(3, 6, "unattached-block-attribute")]
    );
}

#[test]
fn lines_inside_a_braced_comment_run_are_not_linted_as_blocks() {
    for source in [
        "- {% a\n   %%%\n   z %}\nplain\n",
        "1. {% a\n    %%% x\n    z %}\nplain\n",
        "- p\n  {% a\n   %%%\n   z %}\nplain\n",
        "- {% a\n   {lang=de}\n   z %} y\n",
        "- p\n  {% a\n   # h\n   z %}\n",
        "{% a\n > q\n z %} y\n",
    ] {
        assert_eq!(findings(source), [], "{source:?}");
    }
}

/// The written column of a dangling attribute after a marker-line comment run,
/// over indents, hosts and run lengths.
#[test]
fn the_dangling_attribute_column_is_the_written_one_across_variants() {
    let (mut checked, mut reported) = (0, 0);
    for host in ["[^n]: ", ":   ", "- ", "> "] {
        for run_indent in 1..=4 {
            for run_lines in 0..=2 {
                for attr_indent in 0..=6 {
                    let pad = " ".repeat(host.len() + run_indent - 1);
                    let mut source = String::new();
                    if host.starts_with("[^") {
                        source.push_str("x[^n]\n\n");
                    }
                    if host.starts_with(':') {
                        source.push_str(":: t\n");
                    }
                    source.push_str(&format!("{host}{{% a\n"));
                    for k in 0..run_lines {
                        source.push_str(&format!("{pad}b{k}\n"));
                    }
                    source.push_str(&format!("{pad}z %}}\n"));
                    let attr_line = source.lines().count() + 1;
                    let attr = format!("{}{{.x}}", " ".repeat(attr_indent));
                    source.push_str(&format!("{attr}\nplain\n"));
                    for w in carve::lint_carve(&source) {
                        if w.rule == "unattached-block-attribute" && w.line == attr_line {
                            assert_eq!(w.column, attr_indent + 1, "{source:?}");
                            reported += 1;
                        }
                    }
                    checked += 1;
                }
            }
        }
    }
    assert_eq!((checked, reported), (336, 156));
}
