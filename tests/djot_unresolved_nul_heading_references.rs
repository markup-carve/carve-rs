use serde_json::Value;

#[test]
fn unresolved_references_do_not_select_replacement_character_headings() {
    let controls: Value = serde_json::from_str(include_str!(
        "fixtures/djot_unresolved_nul_heading_references.json"
    ))
    .unwrap();
    let destination = regex::Regex::new(r#"<(?:a|img)\b[^>]*\b(?:href|src)="#).unwrap();
    let inline = regex::Regex::new(r"<(?:span|em|strong|code)\b").unwrap();
    let attribute = regex::Regex::new(r#"<[^>]*\b(?:class="c"|id="i")"#).unwrap();
    for row in controls.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        assert!(!destination.is_match(row["html"].as_str().unwrap()));
        let report = carve::migrate_djot(source);
        let html = carve::to_html(&report.value);
        if let Some(depth) = row.get("quoteOwn").and_then(Value::as_u64) {
            assert_eq!(
                row["html"]
                    .as_str()
                    .unwrap()
                    .matches("<blockquote>")
                    .count(),
                depth as usize,
                "{source}"
            );
            assert_eq!(
                html.matches("<blockquote>").count(),
                depth as usize,
                "{source}: {html}"
            );
            assert!(!html.contains("&gt;"), "{source}: {html}");
        }
        assert!(!destination.is_match(&html), "{source}: {html}");
        assert!(!inline.is_match(&html), "{source}: {html}");
        assert!(!attribute.is_match(&html), "{source}: {html}");
        for marker in ["carve-djot-reference-", "{%comment%}"] {
            assert!(!html.contains(marker), "{source}: {html}");
        }
        assert!(
            report
                .report
                .diagnostics
                .iter()
                .any(|item| item.message.contains("unresolved Djot")),
            "{source}: {:?}",
            report.report.diagnostics
        );
    }
}

#[test]
fn resolved_references_select_their_own_heading() {
    let sections = regex::Regex::new(r#"<section id="([^"]*)""#).unwrap();
    let links = regex::Regex::new(r#"href="([^"]*)""#).unwrap();
    for (source, target) in [
        ("# a\0\n\n# a�\n\n[a�][]", 1),
        ("# a�\n\n# a\0\n\n[a\0][]", 1),
        ("# a\0\n\n# a�\n\n[a\0][]", 0),
    ] {
        let html = carve::to_html(&carve::migrate_djot(source).value);
        let ids: Vec<_> = sections
            .captures_iter(&html)
            .map(|caps| caps[1].to_owned())
            .collect();
        let hrefs: Vec<_> = links
            .captures_iter(&html)
            .map(|caps| caps[1].to_owned())
            .collect();
        assert_eq!(ids.len(), 2, "{source}");
        assert_eq!(hrefs, vec![format!("#{}", ids[target])], "{source}");
    }
}

#[test]
fn explicit_definitions_take_precedence() {
    for key in ["a\0", "a�"] {
        let source = format!("# a\0\n\n[x][{key}]\n\n[{key}]: /real");
        let report = carve::migrate_djot(&source);
        assert!(carve::to_html(&report.value).contains("href=\"/real\""));
        assert!(!report
            .report
            .diagnostics
            .iter()
            .any(|item| item.message.contains("unresolved Djot")));
    }
}
