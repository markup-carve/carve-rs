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
    let mut sources = vec![
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
    for body in [
        "a\\ b\n",
        "`x\n > y`\n",
        "- first\n\n  ```\n  a\n  ```\n+\n```\nb\n```\n",
        "a\\\nb\n",
        "*a\\ *\n",
        "| a\\ \n| b\n",
        "\\-\\-\\-\n",
        "x[^n]\n\n[^n]: body\n\n  ```\n  x  \n\n  y\t\n  ```\n",
        "> ```\n> x  \n>\n> y\t\n> ```\n",
        "- item\n\n  ```\n  x  \n\n  y\t\n  ```\n",
        "| h | v |\n|---|---|\n| a | b |\n",
    ] {
        sources.push(format!("```\n{payload}\n```\n\n{body}"));
    }
    sources.push(format!("| {payload} | v |\n|---|---|\n| a | b |\n"));
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

#[test]
fn long_authored_runs_and_token_lookalikes_preserve_large_space_runs() {
    let pool: String = (0xe000..=0xf8ff)
        .map(|code| char::from_u32(code).unwrap())
        .collect();
    let lookalikes: String = (0..64)
        .map(|nonce| format!("\u{e009}m{nonce}\u{e00a}"))
        .collect();
    let payload = format!("{pool}{lookalikes}{}", "\u{e008}".repeat(1024));
    for source in [
        format!("```\n{payload}{}\n\nlast\t\n```\n", " ".repeat(1024)),
        format!("| {payload}{}\n| last\n", " ".repeat(1024)),
    ] {
        let result =
            carve::to_carve_with_report(&source, carve::CheckedRenderOptions::default()).unwrap();
        assert!(result.value.contains(&payload));
        let mut before: serde_json::Value =
            serde_json::from_str(&carve::to_json(&carve::parse(&source))).unwrap();
        let mut after: serde_json::Value =
            serde_json::from_str(&carve::to_json(&carve::parse(&result.value))).unwrap();
        semantic_tree(&mut before);
        semantic_tree(&mut after);
        assert_eq!(before, after);
        assert_eq!(result.total_losses, 0);
    }
}

#[test]
fn occupied_pool_keeps_the_footnote_code_refusal_specific() {
    let pool: String = (0xe000..=0xf8ff)
        .map(|code| char::from_u32(code).unwrap())
        .collect();
    let mut document = carve::parse(&format!("```\n{pool}\n```\n\nx[^n]\n\n[^n]: body\n"));
    let carve::BlockNode::Paragraph(paragraph) =
        &mut document.footnote_defs.get_mut("n").unwrap()[0]
    else {
        panic!("expected a footnote paragraph");
    };
    paragraph.children = vec![carve::InlineNode::Code(carve::Code {
        value: "x\n> tail".into(),
        attrs: None,
        pos: None,
    })];
    match carve::render_carve(&document).expect_err("the continuation escapes its footnote") {
        carve::RenderCarveError::SourceUnspellable(error) => {
            assert_eq!(error.node_type(), "code");
            assert_eq!(
                error.reason(),
                "a block-marker continuation cannot stay inside a footnote paragraph"
            );
        }
        other => panic!("unexpected refusal: {other}"),
    }
}
