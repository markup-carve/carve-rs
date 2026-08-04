//! PART 12 §3a, A RESOLVED REFERENCE KEEPS ITS DESTINATION:
//!
//!   {"type":"link","href":"/start","ref":"getting started",
//!    "rawRef":"[getting started][]"}
//!
//! The authored construct survives BESIDE the resolution result, the same way
//! §5 has footnote numbering added alongside rather than in place of the
//! reference. Dropping them made `[a][]` and `[a](/url)` the same tree - the
//! distinction the clause exists to protect (carve#597).

fn link_fields(source: &str) -> (String, Option<String>, Option<String>) {
    let json = carve::to_json(&carve::parse(source));
    // Read the first link node's three fields straight out of the JSON text:
    // no serde dependency here, and the shape is fixed by the schema.
    let at = json.find("\"type\":\"link\"").expect("no link node");
    let tail = &json[at..];
    let field = |key: &str| -> Option<String> {
        let start = tail.find(&format!("\"{key}\":\""))? + key.len() + 4;
        let rest = &tail[start..];
        let end = rest.find('"')?;
        Some(rest[..end].to_string())
    };
    (
        field("href").unwrap_or_default(),
        field("ref"),
        field("rawRef"),
    )
}

#[test]
fn a_resolved_reference_keeps_ref_and_raw_ref() {
    let (href, label, raw) = link_fields("see [t][r].\n\n[r]: /u\n");

    assert_eq!(href, "/u");
    assert_eq!(label.as_deref(), Some("r"));
    assert_eq!(raw.as_deref(), Some("[t][r]"));
}

#[test]
fn the_collapsed_form_keeps_them_too() {
    let (href, label, raw) = link_fields("see [r][].\n\n[r]: /u\n");

    assert_eq!(href, "/u");
    assert_eq!(label.as_deref(), Some("r"));
    assert_eq!(raw.as_deref(), Some("[r][]"));
}

#[test]
fn an_implicit_heading_reference_already_kept_them() {
    let (href, label, raw) = link_fields("# H\n\nSee [H][].\n");

    assert_eq!(href, "#H");
    assert_eq!(label.as_deref(), Some("H"));
    assert_eq!(raw.as_deref(), Some("[H][]"));
}

#[test]
fn an_unresolved_reference_is_unchanged() {
    let (href, label, raw) = link_fields("see [t][miss].\n");

    assert_eq!(href, "");
    assert_eq!(label.as_deref(), Some("miss"));
    assert_eq!(raw.as_deref(), Some("[t][miss]"));
}

#[test]
fn an_inline_link_carries_no_reference() {
    let (href, label, raw) = link_fields("see [t](/u).\n");

    assert_eq!(href, "/u");
    assert_eq!(label, None);
    assert_eq!(raw, None);
}

#[test]
fn a_resolved_reference_still_renders_as_a_link() {
    let html = carve::to_html("see [t][r].\n\n[r]: /u\n");

    assert!(html.contains("<a href=\"/u\">t</a>"), "{html}");
}
