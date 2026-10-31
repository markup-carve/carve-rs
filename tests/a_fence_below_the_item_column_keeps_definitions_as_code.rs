//! A fence written below an open item's content column opens at the column of
//! its own line, so a definition-shaped line in its payload stays code in both
//! definition prepasses (CARVE-P0-024 S1).

#[test]
fn corpus_620_6_keeps_the_footnote_line_in_the_code() {
    assert_eq!(
        carve::to_html("- para\n%% c\n```\n- a\n  [^f]: t\n"),
        "<ul>\n  <li>para</li>\n</ul>\n<pre><code>- a\n  [^f]: t\n</code></pre>"
    );
}

#[test]
fn every_definition_spelling_stays_payload() {
    for separator in ["", "%% c\n", "%%%\nx\n%%%\n"] {
        for fence in ["```", "~~~", "````"] {
            for first in ["- a", "a"] {
                for def in ["  [^f]: t", "[^f]: t", "  [d]: /u", "  *[A]: x"] {
                    let source = format!("- para\n{separator}{fence}\n{first}\n{def}\n");
                    let html = carve::to_html(&source);
                    assert!(
                        html.contains(&format!("<pre><code>{first}\n")),
                        "{source:?}: {html}"
                    );
                    assert!(
                        !html.chars().any(|c| ('\u{e000}'..='\u{f8ff}').contains(&c)),
                        "{source:?} leaked a placeholder: {html:?}"
                    );
                }
            }
        }
    }
}
