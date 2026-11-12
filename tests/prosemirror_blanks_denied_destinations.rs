//! PART 9 §25 binds the ProseMirror bridge like every other target that emits
//! a resolvable URL: a destination the HTML renderer blanks is blanked in the
//! editor JSON too, and reported (markup-carve/carve#6009).

use carve::{from_prosemirror, parse, render_html, to_json, to_prosemirror};
use serde_json::{json, Value};

fn destinations(value: &Value, out: &mut Vec<(String, String)>) {
    match value {
        Value::Object(map) => {
            // A ProseMirror node keeps its destination in `attrs`, an AST node
            // on the node itself.
            let attrs = match map.get("attrs") {
                Some(Value::Object(attrs)) => attrs,
                _ => map,
            };
            let ty = map.get("type").and_then(Value::as_str).unwrap_or("");
            for key in ["href", "src"] {
                if let (Some(Value::String(v)), true) = (attrs.get(key), ty != "math") {
                    out.push((key.to_string(), v.clone()));
                }
            }
            for (k, v) in map {
                if k != "attrs" {
                    destinations(v, out);
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|v| destinations(v, out)),
        _ => {}
    }
}

#[test]
fn the_ticket_reproduction_is_blanked_and_reported() {
    let pm = to_prosemirror(&parse(
        "Siehe [x](javascript:alert(1)) und ![b](data:image/png;base64,AA).\n",
    ));
    let value: Value = serde_json::from_str(&pm.json).unwrap();
    let paragraph = &value["content"][0]["content"];
    assert_eq!(
        paragraph[1]["marks"][0],
        json!({"type": "link", "attrs": {"href": ""}})
    );
    assert_eq!(paragraph[3]["attrs"]["src"], json!(""));
    assert_eq!(paragraph[3]["attrs"]["alt"], json!("b"));
    assert_eq!(
        pm.degraded.get("destination-denied").map(String::as_str),
        Some("Blanked a denied destination scheme; Blanked a denied image source")
    );
}

/// The blanked source of a RESOLVED reference image is not the empty source
/// of an unresolved one, so the image stays a block image on the way back.
#[test]
fn a_resolved_reference_image_keeps_its_block_after_blanking() {
    let first = |source: &str| -> Value {
        let back = from_prosemirror(&to_prosemirror(&parse(source)).json).unwrap();
        let ast: Value = serde_json::from_str(&to_json(&back)).unwrap();
        ast["children"][0]["type"].clone()
    };
    let control = first("![a][r]\n\n[r]: /x\n");
    assert_eq!(control, json!("image"));
    assert_eq!(first("![a][r]\n\n[r]: data:image/png;base64,AA\n"), control);
    assert_eq!(first("![a][r]\n\n[r]:\n"), control);
    assert_eq!(first("![a][r]\n"), json!("paragraph"));
}

#[test]
fn a_safe_destination_is_kept_and_not_reported() {
    let pm = to_prosemirror(&parse(
        "[x](https://example.com/a) ![b](/i.png) <https://e.org>\n",
    ));
    let mut found = Vec::new();
    destinations(&serde_json::from_str(&pm.json).unwrap(), &mut found);
    assert!(found.iter().all(|(_, v)| !v.is_empty()), "{found:?}");
    assert!(!pm.degraded.contains_key("destination-denied"));
}

/// Every variant blanks in the bridge exactly where the HTML renderer blanks
/// the same destination, so the two sinks cannot drift.
#[test]
fn the_bridge_blanks_what_the_html_renderer_blanks() {
    let schemes = [
        "javascript",
        "JavaScript",
        "JAVASCRIPT",
        "vbscript",
        "VBScript",
        "data",
        "DATA",
        "file",
        "ms-msdt",
        "Ms-Word",
        "search-ms",
        "shell",
        "vscode",
        "jar",
        "https",
        "mailto",
        "ftp",
        "tel",
        "javascripts",
        "xjavascript",
        "java-script",
    ];
    let prefixes = ["", " ", "\u{1}", "\u{7f}", "\u{feff}"];
    let infixes = ["", "\t", "&#9;", "&Tab;"];
    let payloads = ["alert(1)", "image/png;base64,AA", "text/html,x", "//host/p"];
    let mut variants = 0;
    let mut blanked = [0usize; 5];
    for scheme in schemes {
        for prefix in prefixes {
            for infix in infixes {
                for payload in payloads {
                    let split = scheme.len().min(4);
                    let dest = format!(
                        "{prefix}{}{infix}{}:{payload}",
                        &scheme[..split],
                        &scheme[split..]
                    );
                    for (form, source) in [
                        format!("[x]({dest})\n"),
                        format!("![b]({dest})\n"),
                        format!("[x][r]\n\n[r]: {dest}\n"),
                        format!("![b][r]\n\n[r]: {dest}\n"),
                        format!("<{}:{payload}>\n", scheme),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        let doc = parse(&source);
                        let html = render_html(&doc).unwrap();
                        let html_blank =
                            html.matches("href=\"\"").count() + html.matches("src=\"\"").count();
                        let pm = to_prosemirror(&doc);
                        let mut found = Vec::new();
                        destinations(&serde_json::from_str(&pm.json).unwrap(), &mut found);
                        // An unresolved reference already carries an empty
                        // href in the AST and renders as literal text.
                        let mut authored = Vec::new();
                        destinations(
                            &serde_json::from_str(&to_json(&doc)).unwrap(),
                            &mut authored,
                        );
                        let authored_blank = authored.iter().filter(|(_, v)| v.is_empty()).count();
                        let pm_blank = found.iter().filter(|(_, v)| v.is_empty()).count();
                        // The definition node itself renders nothing in HTML.
                        let definition = usize::from(source.contains("[r]:") && html_blank > 0);
                        let expected = authored_blank + html_blank + definition;
                        assert_eq!(pm_blank, expected, "{source:?}\n{html}\n{}", pm.json);
                        assert_eq!(
                            pm.degraded.contains_key("destination-denied"),
                            pm_blank > authored_blank,
                            "{source:?}"
                        );
                        variants += 1;
                        blanked[form] += usize::from(pm_blank > authored_blank);
                    }
                }
            }
        }
    }
    assert!(variants >= 1000, "{variants}");
    // Every form, the reference definition included, meets denied and kept
    // destinations alike.
    for count in blanked {
        assert!(
            count > 100 && count < variants / 5,
            "{blanked:?} of {variants}"
        );
    }
}
