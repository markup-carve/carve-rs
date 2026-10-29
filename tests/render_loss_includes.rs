use carve::{
    expand_includes, parse_with_options, render_markdown, with_render_loss_report,
    CheckedRenderOptions, IncludeContext, IncludeDenial, IncludeOptions, IncludeResolved, Options,
    RenderTarget,
};
use std::process::Command;

#[test]
fn cli_reports_child_location_and_json_fields() {
    let root_path =
        std::env::temp_dir().join(format!("carve-loss-includes-{}", std::process::id()));
    std::fs::create_dir_all(&root_path).unwrap();
    let root = root_path.as_path();
    std::fs::create_dir(root.join("snippets")).unwrap();
    let doc = root.join("d.crv");
    std::fs::write(&doc, "Intro.\n\n{{ snippets/div.crv }}\n").unwrap();
    std::fs::write(root.join("snippets/div.crv"), "{.box}\n:::\nx\n:::\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args([
            "--md",
            "--include-root",
            root.to_str().unwrap(),
            doc.to_str().unwrap(),
            "--report-losses",
            "-",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains(&format!("snippets/div.crv:2:1 markdown-container-flattened - Flattened a container into Markdown blocks (included from {}:3:1)", doc.display())), "{stderr}");
    let report: serde_json::Value =
        serde_json::from_str(&stderr[stderr.find('{').unwrap()..]).unwrap();
    assert!(!report["losses"].as_array().unwrap().is_empty());
    for loss in report["losses"].as_array().unwrap() {
        assert_eq!(loss["file"], "snippets/div.crv");
        assert_eq!(
            loss["includedBy"],
            serde_json::json!([{"file":doc,"line":3,"column":1}])
        );
        assert!(loss["pos"].get("file").is_none());
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn nested_chain_selected_lines_and_strict_error() {
    let resolver = |path: &str, _: &IncludeContext<'_>| -> Result<IncludeResolved, IncludeDenial> {
        match path {
            "b.crv" => Ok(IncludeResolved::from("{{ c.crv @lines:3-6 }}\n")),
            "c.crv" => Ok(IncludeResolved::from("before\n\n{.box}\n:::\nx\n:::\n")),
            _ => Err(IncludeDenial::NotFound),
        }
    };
    let source = "{{ b.crv }}\n";
    let doc = parse_with_options(source, &Options::default().with_positions(true));
    let expanded = expand_includes(
        doc,
        source,
        &IncludeOptions::new()
            .with_resolver(&resolver)
            .with_source_path("a.crv"),
    );
    let options = CheckedRenderOptions::default();
    let report = with_render_loss_report(RenderTarget::Markdown, options, || {
        render_markdown(&expanded.doc).unwrap()
    })
    .unwrap();
    let loss = report
        .losses
        .iter()
        .find(|loss| loss.code == "markdown-container-flattened")
        .unwrap();
    assert_eq!(loss.file.as_deref(), Some("c.crv"));
    assert_eq!(loss.pos.as_ref().unwrap().start_line, 4);
    let sites = loss.included_by.as_ref().unwrap();
    assert_eq!(
        sites
            .iter()
            .map(|site| (site.file.as_str(), site.line, site.column))
            .collect::<Vec<_>>(),
        [("a.crv", 1, 1), ("b.crv", 1, 1)]
    );
    let strict = with_render_loss_report(
        RenderTarget::Markdown,
        CheckedRenderOptions {
            strict: true,
            ..options
        },
        || render_markdown(&expanded.doc).unwrap(),
    )
    .unwrap_err();
    assert_eq!(strict.losses, report.losses);
}

#[test]
fn top_level_loss_has_no_include_fields() {
    let source = ":::\nx\n:::\n";
    let doc = parse_with_options(source, &Options::default().with_positions(true));
    let report = with_render_loss_report(
        RenderTarget::Markdown,
        CheckedRenderOptions::default(),
        || render_markdown(&doc).unwrap(),
    )
    .unwrap();
    assert!(!report.losses.is_empty());
    for loss in report.losses {
        assert!(loss.file.is_none());
        assert!(loss.included_by.is_none());
        assert!(loss.pos.unwrap().file.is_none());
    }
}
