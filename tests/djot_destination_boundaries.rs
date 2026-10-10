use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    name: String,
    source: String,
    html: String,
}

#[test]
fn destination_boundaries() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/djot-destination-boundaries.json")).unwrap();
    let nested_indent = regex::Regex::new(r"\n[ \t]+(<[ou]l>)").unwrap();
    let between_tags = regex::Regex::new(r">\s+<").unwrap();
    let image_attributes = regex::Regex::new(r#"<img alt="([^"]*)" src="([^"]*)">"#).unwrap();
    let urls = regex::Regex::new(r#"(?:href|src)="([^"]*)""#).unwrap();
    for row in cases {
        let html = carve::to_html(&carve::djot_to_carve(&row.source))
            .trim()
            .replace("&nbsp;", "\u{a0}")
            .replace("<tbody>", "")
            .replace("</tbody>", "");
        let html = urls.replace_all(&html, |caps: &regex::Captures<'_>| {
            caps[0].replace(
                &caps[1],
                &caps[1]
                    .replace('(', "%28")
                    .replace(')', "%29")
                    .replace('`', "%60"),
            )
        });
        let html = nested_indent.replace_all(&html, "\n$1");
        assert_eq!(
            between_tags.replace_all(&html, "><"),
            image_attributes.replace_all(
                &row.html.replace("&nbsp;", "\u{a0}"),
                r#"<img src="$2" alt="$1">"#
            ),
            "{}",
            row.name
        );
    }
}

#[test]
fn link_immediately_after_footnote() {
    let html = carve::to_html(&carve::djot_to_carve("note[^1][t](u~x~y)\n\n[^1]: note"));
    assert!(html.contains(r#"<a href="u~x~y">t</a>"#));
    assert!(regex::Regex::new(r">\s+<")
        .unwrap()
        .replace_all(&html, "><")
        .contains("<li id=\"fn1\"><p>note"));
}

#[test]
fn percent_escapes_stay_exact() {
    let source = "[t](u%28x%29y)";
    let converted = carve::djot_to_carve(source);
    assert_eq!(converted, source);
    assert!(carve::to_html(&converted).contains(r#"href="u%28x%29y""#));
}

#[test]
fn footnote_suffix_parentheses_remain_text() {
    for prefix in ["", "!"] {
        let html = carve::to_html(&carve::djot_to_carve(&format!(
            "{prefix}[^n](a ~b~ c)\n\n[^n]: note"
        )));
        assert!(html.contains("(a <sub>b</sub> c)"));
        assert!(html.contains(r##"href="#fn1""##));
        if !prefix.is_empty() {
            assert!(html.contains("<p>!<a id=\"fnref1\""));
        }
    }
}

#[test]
fn reference_url_boundaries_do_not_hide_paragraph_notes() {
    for source in ["text\n[a]: b\n[c]: [^x]\n", "[r]: http://e/[^x] \n"] {
        let converted = carve::djot_to_carve(source);
        assert!(
            converted.contains("[^carve-djot-note-0]:"),
            "{source:?}: {converted:?}"
        );
        assert!(
            carve::to_html(&converted).contains("href=\"#fn1\""),
            "{source:?}: {converted:?}: {}",
            carve::to_html(&converted)
        );
    }
    for source in [
        "[r]: http://example.com/\n  [^x]\n",
        "[r]:\n  http://example.com/[^x]\n",
        "# heading\n[r]: [^x]\n",
    ] {
        let converted = carve::djot_to_carve(source);
        assert!(
            !converted.contains("carve-djot-note"),
            "{source:?}: {converted:?}"
        );
        assert!(converted.contains("[^x]"));
    }
}

#[test]
fn reference_continuations_stay_within_their_container() {
    for marker in ["> ", "- ", "1. "] {
        let source = format!("[r]: http://e/\n{marker}[^x]\n\n[^x]: note\n");
        let html = carve::to_html(&carve::djot_to_carve(&source));
        assert!(html.contains("href=\"#fn1\""), "{source:?}: {html}");
    }
    for prefix in [
        "```\ncode\n```\n",
        "::: x\ncontent\n:::\n",
        "* * *\n",
        "| a |\n",
    ] {
        let source = format!("{prefix}[r]: http://e/[^x]\n");
        let converted = carve::djot_to_carve(&source);
        assert!(
            !converted.contains("carve-djot-note"),
            "{source:?}: {converted:?}"
        );
        assert!(converted.contains("http://e/[^x]"));
    }
}

#[test]
fn references_after_container_exit_keep_their_urls() {
    for prefix in ["> text\n", "[^a]: note\n", "- item\n"] {
        let source = format!("{prefix}[r]: http://e/[^x]\n\n[x][r]\n");
        let converted = carve::djot_to_carve(&source);
        let html = carve::to_html(&converted);
        assert!(
            html.contains("href=\"http://e/[^x]\""),
            "{source:?}: {converted:?}: {html}"
        );
        assert!(!converted.contains("carve-djot-note"));
    }
    let source = "[r]:http://e/\n\n[x][r]\n";
    assert!(!carve::to_html(&carve::djot_to_carve(source)).contains("href=\"http://e/\""));
}
