use carve::CheckedRenderOptions;
use serde::Deserialize;

#[derive(Deserialize)]
struct Control {
    source: String,
    markdown: Markdown,
}

#[derive(Deserialize)]
struct Markdown {
    rust: String,
}

#[test]
fn markdown_preserves_nested_emphasis() {
    let controls: Vec<Control> =
        serde_json::from_str(include_str!("fixtures/markdown-nested-emphasis.json")).unwrap();
    assert_eq!(controls.len(), 6444);
    for control in controls {
        let result =
            carve::to_markdown_with_report(&control.source, CheckedRenderOptions::default())
                .unwrap();
        assert_eq!(result.value, control.markdown.rust, "{}", control.source);
        assert!(result.losses.is_empty(), "{}", control.source);
        assert_eq!(result.total_losses, 0, "{}", control.source);
        assert!(!result.truncated, "{}", control.source);
        let ast = carve::parse(&control.source);
        assert_eq!(
            carve::render_markdown(&ast).unwrap(),
            result.value,
            "{}",
            control.source
        );
    }
}

#[test]
fn repair_keeps_complete_ordered_loss_counts() {
    let source = "{*a {/b {*x `omitted`{=latex}*} c/} d*} and `tail`{=typst}";
    let report = carve::to_markdown_with_report(source, CheckedRenderOptions::default()).unwrap();
    assert_eq!(report.value, carve::to_markdown(source));
    assert_eq!(report.total_losses, 2);
    assert_eq!(
        report
            .losses
            .iter()
            .map(|loss| loss.format.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("latex"), Some("typst")]
    );
    assert_eq!(
        report
            .losses
            .iter()
            .map(|loss| loss.pos.as_ref().unwrap().start_column)
            .collect::<Vec<_>>(),
        vec![13, 45]
    );
    let bounded = carve::to_markdown_with_report(
        source,
        CheckedRenderOptions {
            strict: false,
            max_losses: 0,
        },
    )
    .unwrap();
    assert!(bounded.losses.is_empty());
    assert_eq!(bounded.total_losses, 2);
    assert!(bounded.truncated);
    let error = carve::to_markdown_with_report(
        source,
        CheckedRenderOptions {
            strict: true,
            max_losses: 1,
        },
    )
    .unwrap_err();
    assert_eq!(error.total_losses, 2);
    assert_eq!(error.losses.len(), 1);
    assert!(error.truncated);
    assert_eq!(
        carve::to_markdown_with_report("/clean/", CheckedRenderOptions::default())
            .unwrap()
            .total_losses,
        0
    );
}

#[test]
fn markdown_preserves_nested_wrapper_hosts_and_attributes() {
    #[derive(Deserialize)]
    struct HostControl {
        source: String,
        markdown: String,
    }
    let controls: Vec<HostControl> =
        serde_json::from_str(include_str!("fixtures/markdown-nested-emphasis-hosts.json")).unwrap();
    assert_eq!(controls.len(), 16);
    for control in controls {
        let report =
            carve::to_markdown_with_report(&control.source, CheckedRenderOptions::default())
                .unwrap();
        assert_eq!(report.value, control.markdown, "{}", control.source);
        let expected_losses = if control.source.starts_with("::: figure\n") {
            vec!["markdown-caption-flattened"]
        } else {
            vec![]
        };
        assert_eq!(
            report.losses.iter().map(|loss| loss.code).collect::<Vec<_>>(),
            expected_losses,
            "{}",
            control.source
        );
        assert_eq!(
            carve::render_markdown(&carve::parse(&control.source)).unwrap(),
            report.value
        );
    }
}
