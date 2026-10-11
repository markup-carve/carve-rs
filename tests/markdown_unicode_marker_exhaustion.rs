use carve::{CheckedRenderOptions, RenderTarget};

fn pool() -> String {
    (0xe000..=0xf8ff)
        .map(|code| char::from_u32(code).unwrap())
        .collect()
}

#[test]
fn full_pool_goldens_preserve_source_and_ast_rendering() {
    let rows: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/markdown-unicode-marker-exhaustion.json"
    ))
    .unwrap();
    let pool = pool();
    for (index, row) in rows.as_array().unwrap().iter().enumerate() {
        let source = row["source"]
            .as_str()
            .unwrap()
            .replace("FULL_PRIVATE_USE_POOL", &pool);
        let expected = row["markdown"]
            .as_str()
            .unwrap()
            .replace("FULL_PRIVATE_USE_POOL", &pool);
        let document = carve::parse(&source);
        let options = CheckedRenderOptions {
            strict: true,
            ..Default::default()
        };
        let source_report = carve::to_markdown_with_report(&source, options).unwrap();
        let ast_report = carve::with_render_loss_report(RenderTarget::Markdown, options, || {
            carve::render_markdown(&document).unwrap()
        })
        .unwrap();
        for report in [source_report, ast_report] {
            assert_eq!(report.value, expected, "case {index}");
            assert!(report.value.contains(&pool), "case {index}");
            assert!(report.losses.is_empty(), "case {index}");
            assert_eq!(report.total_losses, 0);
            assert!(!report.truncated);
        }
        assert_eq!(carve::to_markdown("ordinary\n"), "ordinary\n");
    }
}

#[test]
fn capped_and_strict_reports_keep_complete_loss_counts() {
    let source = format!(
        "```\n{}\n```\n\n`one`{{=latex}} and `two`{{=typst}}",
        pool()
    );
    let report = carve::to_markdown_with_report(&source, CheckedRenderOptions::default()).unwrap();
    assert!(report.value.contains(&pool()));
    assert_eq!(report.total_losses, 2);
    assert_eq!(
        report
            .losses
            .iter()
            .map(|loss| loss.format.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("latex"), Some("typst")]
    );
    let capped = carve::to_markdown_with_report(
        &source,
        CheckedRenderOptions {
            strict: false,
            max_losses: 0,
        },
    )
    .unwrap();
    assert!(capped.losses.is_empty());
    assert_eq!(capped.total_losses, 2);
    assert!(capped.truncated);
    let error = carve::to_markdown_with_report(
        &source,
        CheckedRenderOptions {
            strict: true,
            max_losses: 1,
        },
    )
    .unwrap_err();
    assert_eq!(error.total_losses, 2);
    assert_eq!(error.losses, report.losses[..1]);
    assert!(error.truncated);
    assert_eq!(
        carve::to_markdown_with_report("ordinary\n", CheckedRenderOptions::default())
            .unwrap()
            .total_losses,
        0
    );
}

#[test]
fn trailing_heading_hashes_use_complete_marker_offsets() {
    let anchor = format!("```\n{}\n```\n\n", pool());
    for (source, expected) in [
        ("# heading \\#\n", "# heading \\#\n"),
        ("# a \\# \\#\n", "# a # \\#\n"),
        ("# company_id \\#\n", "# company_id \\#\n"),
    ] {
        assert_eq!(
            carve::to_markdown(&format!("{anchor}{source}")),
            format!("{anchor}{expected}")
        );
    }
}

#[test]
fn authored_token_families_and_long_runs_are_preserved() {
    let families: String = (0..32)
        .flat_map(|nonce| (0..10).map(move |slot| format!("§m{nonce}:{slot}§")))
        .collect();
    let payload = format!("{}{}{}", pool(), families, "\u{e008}".repeat(1024));
    let source = format!("```\n{payload}\n\n\nlast  \n```\n\n\\# heading\n");
    assert_eq!(carve::to_markdown(&source), source);
}

#[test]
fn code_block_blank_lines_survive_without_marker_exhaustion() {
    for content in ["\n\n\nfirst\n\n\nlast  \n", "first\n\n\nlast\n", "\n\n\n"] {
        let source = format!("```\n{content}```\n");
        assert_eq!(carve::to_markdown(&source), source);
    }
}

#[test]
fn partial_marker_families_touching_escapes_preserve_authored_text() {
    let fragmented: String = (0xe001..=0xf8ff)
        .step_by(3)
        .map(|code| char::from_u32(code).unwrap())
        .collect();
    for used in [pool(), fragmented] {
        let anchor = format!("```\n{used}\n```\n\n");
        for authored in ["§m0:9_x", "_m0:9§", "§m1:0§m0:0§", "§m0:0_x"] {
            let source = format!("{anchor}{authored}\n");
            let markdown = carve::to_markdown(&source);
            assert!(markdown.contains(authored), "lost {authored:?}");
            let restored = carve::markdown_to_carve(&markdown);
            assert_eq!(
                carve::to_html(&restored),
                carve::to_html(&source),
                "{authored:?}"
            );
            assert_eq!(carve::to_markdown("ordinary\n"), "ordinary\n");
        }
    }
}

#[test]
fn leading_blank_code_payloads_do_not_add_host_whitespace() {
    for markdown in ["- ```\n\n    tail\n  ```\n", "> ```\n>\n>   tail\n> ```\n"] {
        let source = carve::markdown_to_carve(markdown);
        let rendered = carve::to_markdown(&source);
        assert!(
            !rendered.lines().any(|line| line.ends_with(' ')),
            "{rendered:?}"
        );
        let imported = carve::markdown_to_carve(&rendered);
        assert_eq!(carve::to_html(&imported), carve::to_html(&source));
    }
}
