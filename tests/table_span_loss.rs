use carve::{CheckedRenderOptions, RenderTarget};

const SPANS: &str = "|= System |= Limit | < |\n| A | Cold | 20 |\n| ^ | Hot | 10 |\n";

#[test]
fn spanning_cells_report_once_per_cell_on_lossy_targets() {
    let options = CheckedRenderOptions::default();
    for (target, result) in [
        (
            RenderTarget::Markdown,
            carve::to_markdown_with_report(SPANS, options).unwrap(),
        ),
        (
            RenderTarget::Plain,
            carve::to_plain_text_with_report(SPANS, options).unwrap(),
        ),
        (
            RenderTarget::Ansi,
            carve::to_ansi_with_report(SPANS, options).unwrap(),
        ),
    ] {
        assert_eq!(result.total_losses, 2, "{target:?}");
        assert_eq!(
            result
                .losses
                .iter()
                .map(|loss| (loss.colspan, loss.rowspan))
                .collect::<Vec<_>>(),
            vec![(Some(2), None), (None, Some(2))]
        );
        for loss in &result.losses {
            assert_eq!(loss.code, "table-span-flattened");
            assert_eq!(loss.target, target);
            assert_eq!(loss.node_type.as_str(), "block");
            assert_eq!(
                loss.message,
                "Flattened a spanning table cell into separate cells"
            );
            assert!(loss.pos.is_some());
        }
    }
    assert_eq!(
        carve::to_html_with_report(SPANS, options)
            .unwrap()
            .total_losses,
        0
    );
    assert_eq!(
        carve::to_carve_with_report(SPANS, options)
            .unwrap()
            .total_losses,
        0
    );
    assert!(carve::to_markdown_with_report(
        SPANS,
        CheckedRenderOptions {
            strict: true,
            ..options
        }
    )
    .is_err());
}

#[test]
fn one_cell_with_both_spans_reports_one_loss() {
    let result = carve::to_markdown_with_report(
        "|= Wide | < |\n| ^ | ^ |\n",
        CheckedRenderOptions::default(),
    )
    .unwrap();
    assert_eq!(result.total_losses, 1);
    assert_eq!(
        (result.losses[0].colspan, result.losses[0].rowspan),
        (Some(2), Some(2))
    );
}

#[test]
fn later_header_row_is_demoted_only_in_markdown() {
    let source = "|= First |\n|= Second |\n| Body |\n";
    let options = CheckedRenderOptions::default();
    let result = carve::to_markdown_with_report(source, options).unwrap();
    assert_eq!(result.total_losses, 1);
    let loss = &result.losses[0];
    assert_eq!(loss.code, "table-header-row-demoted");
    assert_eq!(loss.node_type.as_str(), "block");
    assert_eq!(loss.message, "Wrote a later header row as a body row");
    assert!(loss.pos.is_some());
    assert_eq!(
        carve::to_plain_text_with_report(source, options)
            .unwrap()
            .total_losses,
        0
    );
    assert_eq!(
        carve::to_ansi_with_report(source, options)
            .unwrap()
            .total_losses,
        0
    );
    assert!(carve::to_markdown_with_report(
        source,
        CheckedRenderOptions {
            strict: true,
            ..options
        }
    )
    .is_err());
}

#[test]
fn row_and_cell_losses_precede_cell_content_losses() {
    let source = "|= First | < |\n|= `x`{=latex} | < |\n";
    let result = carve::to_markdown_with_report(source, CheckedRenderOptions::default()).unwrap();
    let codes = result
        .losses
        .iter()
        .map(|loss| loss.code)
        .collect::<Vec<_>>();
    assert_eq!(
        codes,
        [
            "table-span-flattened",
            "table-header-row-demoted",
            "table-span-flattened",
            "raw-format-dropped"
        ]
    );
}
