use carve::{html_to_carve, HtmlImportOptions};

#[test]
fn interior_colon_runs_are_refused_in_container_paragraphs() {
    for host in [
        "<ul><li><p>x <code>a\n{}\nb</code> y</p></li></ul>",
        "<blockquote><p>x <code>a\n{}\nb</code> y</p></blockquote>",
    ] {
        for width in 3..=6 {
            let html = host.replace("{}", &":".repeat(width));
            assert!(
                html_to_carve(&html, &HtmlImportOptions::default()).is_err(),
                "{html:?}"
            );
        }
    }
}

#[test]
fn a_colon_run_on_the_last_value_line_can_keep_its_closer() {
    let html = "<ul><li><p>x <code>a\n:::</code> y</p></li></ul>";
    assert!(html_to_carve(html, &HtmlImportOptions::default()).is_ok());
}

#[test]
fn a_thousand_interior_colon_run_variants_refuse_source() {
    let hosts = [
        ("<ul><li><p>", "</p></li></ul>"),
        ("<ol><li><p>", "</p></li></ol>"),
        ("<blockquote><p>", "</p></blockquote>"),
        ("<div class=note><p>", "</p></div>"),
        ("<dl><dd><p>", "</p></dd></dl>"),
    ];
    let mut checked = 0;
    for (open, close) in hosts {
        for width in 3..=6 {
            for prefix_len in 1..=10 {
                for suffix_len in 1..=5 {
                    let html = format!(
                        "{open}x <code>{}\n{}\n{}</code> y{close}",
                        "a".repeat(prefix_len),
                        ":".repeat(width),
                        "b".repeat(suffix_len),
                    );
                    assert!(
                        html_to_carve(&html, &HtmlImportOptions::default()).is_err(),
                        "{html:?}"
                    );
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 1_000);
}
