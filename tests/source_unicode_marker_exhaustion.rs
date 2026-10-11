fn semantic_tree(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(fields) => {
            fields.remove("pos");
            fields.remove("srcByteLength");
            for child in fields.values_mut() {
                semantic_tree(child);
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                semantic_tree(child);
            }
        }
        _ => {}
    }
}

#[test]
fn source_and_ast_preserve_an_occupied_marker_pool() {
    let pool: String = (0xe000..=0xf8ff)
        .map(|code| char::from_u32(code).unwrap())
        .collect();
    let payload = format!("{pool}\u{e009}{}\u{e010}\u{e00a}", "\u{e008}".repeat(24));
    let sources = [
        format!("a{payload}b\n"),
        format!("`{payload}`\n"),
        format!("!`{payload}`\n"),
        format!("```\na{payload}  \n\nb\t\n```\n"),
        format!("![{payload}](image)\n"),
        format!("[{payload}](/target)\n"),
        format!("| a{payload}\n| b\n"),
        format!("> a{payload}\n> b\n"),
        format!("- a{payload}\n  b\n"),
        format!("a{payload}b\n\n[^\u{e009}\u{e008}\u{e008}\u{e010}\u{e00a}]: definition\n"),
    ];
    for (index, source) in sources.iter().enumerate() {
        let document = carve::parse(source);
        let report =
            carve::to_carve_with_report(source, carve::CheckedRenderOptions::default()).unwrap();
        let direct = carve::render_carve(&document).unwrap();
        assert_eq!(report.value, direct, "case {index}");
        assert!(direct.contains(&payload), "case {index}");
        let mut before: serde_json::Value =
            serde_json::from_str(&carve::to_json(&document)).unwrap();
        let mut after: serde_json::Value =
            serde_json::from_str(&carve::to_json(&carve::parse(&direct))).unwrap();
        semantic_tree(&mut before);
        semantic_tree(&mut after);
        assert_eq!(before, after, "case {index}");
        assert!(report.losses.is_empty());
        assert_eq!(report.total_losses, 0);
        assert!(!report.truncated);
        assert_eq!(carve::to_carve(&direct), direct, "case {index}");
        assert_eq!(carve::to_carve("ordinary\n"), "ordinary\n");
    }
}
