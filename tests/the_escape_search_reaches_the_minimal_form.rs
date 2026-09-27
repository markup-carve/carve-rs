//! PART 11 §2: the escape narrowing search leaves no idle escape where it can
//! finish (carve-js#2168).

use carve::html_to_carve;

fn import(html: &str) -> String {
    html_to_carve(html, &Default::default())
        .expect("imports")
        .value
}

#[test]
fn a_bracket_pair_split_by_a_nested_link_is_written_bare() {
    let html = r#"<p class="n">* a</p><p><span class="c">[<a href="/u">x</a>]</span></p>"#;
    assert_eq!(import(html), "{.n}\n\\* a\n\n[[[x](/u)]]{.c}\n");
}

#[test]
fn the_search_finishes_on_a_long_document_whose_failing_units_are_sparse() {
    let body: String = (0..1000)
        .map(|i| {
            if i % 50 == 0 {
                format!(r#"<p class="n">* item {i} and 1. two</p>"#)
            } else {
                format!("<p>note ({i}) here.</p>")
            }
        })
        .collect();
    let carve = import(&format!("<body>{body}</body>"));
    assert_eq!(carve.matches("\\* item").count(), 20);
    assert!(!carve.contains("\\("), "{carve}");
    assert!(!carve.contains("1\\."), "{carve}");
}
