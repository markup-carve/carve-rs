#[test]
fn breaks_cover_their_original_line_endings() {
    for eol in ["\n", "\r\n", "\r"] {
        let source = ["> :: 😀", ">   %% note", ">   more", ""].join(eol);
        let tree: serde_json::Value = serde_json::from_str(&carve::to_json(
            &carve::parse_with_options(&source, &carve::Options::new().with_positions(true)),
        ))
        .unwrap();
        let mut pending = vec![&tree];
        let mut count = 0;
        while let Some(value) = pending.pop() {
            match value {
                serde_json::Value::Array(values) => pending.extend(values),
                serde_json::Value::Object(fields) => {
                    if fields.get("type").and_then(|v| v.as_str()) == Some("soft_break") {
                        count += 1;
                        let pos = &fields["pos"];
                        let start = pos["startOffset"].as_u64().unwrap() as usize;
                        let end = pos["endOffset"].as_u64().unwrap() as usize;
                        assert_eq!(
                            source
                                .chars()
                                .skip(start)
                                .take(end - start)
                                .collect::<String>(),
                            format!("{eol}{}", if pos["startLine"] == 1 { ">   " } else { "> " })
                        );
                        assert_eq!(pos["endColumn"], if pos["startLine"] == 1 { 5 } else { 3 });
                        assert_eq!(
                            pos["endLine"].as_u64().unwrap(),
                            pos["startLine"].as_u64().unwrap() + 1
                        );
                    }
                    pending.extend(
                        fields
                            .iter()
                            .filter(|(key, _)| *key != "pos")
                            .map(|(_, value)| value),
                    );
                }
                _ => {}
            }
        }
        assert_eq!(count, 2);
    }
}
