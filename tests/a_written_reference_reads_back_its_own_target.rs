//! A reference written from the AST reads back to the link's own target:
//! the last definition of a label wins, note bodies included (carve#5012).

use carve::{from_json, parse, render_carve, to_json};
use serde_json::{json, Value};

fn hrefs(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(fields) => {
            if value["type"] == "link" {
                out.push(value["href"].as_str().unwrap_or_default().to_owned());
            }
            for child in fields.values() {
                hrefs(child, out);
            }
        }
        Value::Array(values) => values.iter().for_each(|value| hrefs(value, out)),
        _ => {}
    }
}

fn definition(href: &str, place: &str) -> Value {
    let def = json!({"type":"link_reference_definition","label":"x","href":href});
    match place {
        "top" => def,
        "note" => json!({"type":"footnote","label":format!("n{}", &href[1..]),"children":[def]}),
        "note-later" => {
            json!({"type":"footnote","label":format!("n{}", &href[1..]),"children":[{"type":"paragraph","children":[{"type":"text","value":"p"}]},def]})
        }
        "quote" => json!({"type":"block_quote","children":[def]}),
        "item" => {
            json!({"type":"list","ordered":false,"tight":true,"items":[{"type":"list_item","children":[{"type":"paragraph","children":[{"type":"text","value":"i"}]},def]}]})
        }
        _ => unreachable!(),
    }
}

#[test]
fn every_written_reference_reads_back_to_its_own_target() {
    let places = ["top", "note", "note-later", "quote", "item"];
    let targets = ["/a", "/b", "/c"];
    let mut written = 0;
    let mut checked = 0;
    for first in places {
        for second in places {
            for third in [None, Some("top"), Some("note"), Some("item")] {
                for href in targets {
                    for link_first in [true, false] {
                        let mut children = vec![definition("/a", first), definition("/b", second)];
                        if let Some(place) = third {
                            children.push(definition("/c", place));
                        }
                        let link = json!({"type":"paragraph","children":[{"type":"link","href":href,"ref":"x","children":[{"type":"text","value":"t"}]}]});
                        if link_first {
                            children.insert(0, link);
                        } else {
                            children.push(link);
                        }
                        let wire = json!({"type":"document","srcByteLength":0,"children":children});
                        let document = from_json(&wire.to_string()).unwrap();
                        checked += 1;
                        let Ok(source) = render_carve(&document) else {
                            continue;
                        };
                        written += 1;
                        let value: Value = serde_json::from_str(&to_json(&parse(&source))).unwrap();
                        let mut found = Vec::new();
                        hrefs(&value, &mut found);
                        assert_eq!(found, [href], "{wire}\n{source}");
                    }
                }
            }
        }
    }
    assert_eq!(checked, 600);
    assert!(written > 0);
}

#[test]
fn a_note_body_definition_written_last_is_the_reference_target() {
    for (href, written) in [("/outer", None), ("/note", Some("[t][x]"))] {
        let wire = json!({"type":"document","srcByteLength":0,"children":[
            {"type":"paragraph","children":[{"type":"link","href":href,"ref":"x","children":[{"type":"text","value":"t"}]}]},
            {"type":"link_reference_definition","label":"x","href":"/outer"},
            {"type":"footnote","label":"n","children":[{"type":"link_reference_definition","label":"x","href":"/note"}]},
        ]});
        let result = render_carve(&from_json(&wire.to_string()).unwrap());
        match written {
            Some(spelling) => assert!(result.unwrap().starts_with(spelling)),
            None => assert!(matches!(
                result,
                Err(carve::RenderCarveError::SourceUnspellable(_))
            )),
        }
    }
}
