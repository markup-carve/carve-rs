#[test]
fn unused_footnote_headings_do_not_resolve_references() {
    let mut checked = 0;
    for prefix in ["", "> ", "- ", "> > ", "::: note\n"] {
        for heading in 0..10 {
            for label in 0..10 {
                for used in [false, true] {
                    let reference = if label % 2 == 0 {
                        format!("[H{heading}][]")
                    } else {
                        format!("</#H{heading}>")
                    };
                    let wrapped = if prefix == "::: note\n" {
                        format!("{prefix}{reference}\n:::")
                    } else {
                        format!("{prefix}{reference}")
                    };
                    let use_line = if used {
                        format!("see[^n{label}]\n\n")
                    } else {
                        String::new()
                    };
                    let source = format!("{use_line}[^n{label}]: # H{heading}\n\n{wrapped}");
                    let html = carve::to_html(&source);
                    assert_eq!(
                        html.contains(&format!("href=\"#H{heading}\"")),
                        used,
                        "{source:?}: {html}"
                    );
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 1_000);
}

#[test]
fn transitive_footnote_headings_enter_the_heading_index() {
    let source = "see[^outer] and [Nested][]\n\n[^outer]: go[^inner]\n\n[^inner]: # Nested";
    let html = carve::to_html(source);
    assert!(html.contains("href=\"#Nested\""), "{html}");
}

#[test]
fn quote_direction_reads_the_following_source_delimiter() {
    let forms = ["*/\"*", "_/\"_", "~/\"~", "=/\"="];
    let prefixes = ["", "a ", "b ", "c ", "d ", "e ", "f ", "g ", "h ", "i "];
    let suffixes = ["", " end", " 1", " &", " z"];
    let hosts: [fn(&str) -> String; 5] = [
        |text| text.to_owned(),
        |text| format!("> {text}"),
        |text| format!("- {text}"),
        |text| format!("> > {text}"),
        |text| format!("::: note\n{text}\n:::"),
    ];
    let mut checked = 0;
    for host in hosts {
        for form in forms {
            for prefix in prefixes {
                for suffix in suffixes {
                    let source = host(&format!("{prefix}{form}{suffix}"));
                    let html = carve::to_html(&source);
                    assert!(html.contains('“'), "{source:?}: {html}");
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 1_000);
}

#[test]
fn nested_first_block_plus_releases_to_its_surviving_owner() {
    let list = carve::to_html("- - +\n+\ntail");
    assert!(list.contains("</ul>\n    tail"), "{list}");
    let quote = carve::to_html("> > +\n+\ntail");
    assert!(quote.contains("</blockquote>\n  <p>tail</p>"), "{quote}");
    let sibling = carve::to_html("> - +\n+\ntail\n- sibling");
    assert!(sibling.contains("</blockquote>\n<ul>"), "{sibling}");
}
