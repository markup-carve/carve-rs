use carve::{from_json, parse, render_carve, to_html, to_json};
use serde_json::Value;

fn remove_positions(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            fields.remove("pos");
            for child in fields.values_mut() {
                remove_positions(child);
            }
        }
        Value::Array(children) => {
            for child in children {
                remove_positions(child);
            }
        }
        _ => {}
    }
}

#[test]
fn imported_list_item_keeps_a_comment_run_together() {
    let source = "- a\n\n  %% y\n  %% z\n- c\n";
    let mut wire: Value = serde_json::from_str(&to_json(&parse(source))).unwrap();
    remove_positions(&mut wire);
    let imported = from_json(&wire.to_string()).unwrap();
    let written = render_carve(&imported).unwrap();
    assert_eq!(written, "- a\n\n  %% y\n  %% z\n\n- c\n");
    assert_eq!(to_html(&written), to_html(source));
}

#[test]
fn one_thousand_position_free_item_runs_read_back() {
    let mut checked = 0;
    for marker in ["-", "*"] {
        for nested in [false, true] {
            for count in 2..=6 {
                for first in ["a", "b", "one", "two", "three"] {
                    for tail in [
                        "c", "d", "end", "next", "last", "six", "seven", "eight", "nine", "ten",
                    ] {
                        let (lead, comment_lead) = if nested { ("  ", "    ") } else { ("", "  ") };
                        let mut source = if nested {
                            format!("{marker} outer\n\n{lead}{marker} {first}\n\n")
                        } else {
                            format!("{marker} {first}\n\n")
                        };
                        for n in 0..count {
                            source.push_str(&format!("{comment_lead}%% comment {n}\n"));
                        }
                        source.push_str(&format!("{lead}{marker} {tail}\n"));
                        let mut wire: Value =
                            serde_json::from_str(&to_json(&parse(&source))).unwrap();
                        remove_positions(&mut wire);
                        let imported = from_json(&wire.to_string()).unwrap();
                        let written = render_carve(&imported).unwrap();
                        assert_eq!(
                            to_html(&written),
                            to_html(&source),
                            "{source:?} -> {written:?}"
                        );
                        for n in 0..count - 1 {
                            assert!(
                                written.contains(&format!(
                                    "%% comment {n}\n{comment_lead}%% comment {}",
                                    n + 1
                                )),
                                "{source:?} -> {written:?}"
                            );
                        }
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked, 1_000);
}
#[test]
fn item_owned_comment_stays_between_sub_list_items() {
    let source = "- a\n  - b\n  %%\n\n  - c\n";
    let written = carve::to_carve(source);
    assert_eq!(written, "- a\n  - b\n\n  %%\n\n  - c\n");
    assert_eq!(carve::to_html(&written), carve::to_html(source));
}
