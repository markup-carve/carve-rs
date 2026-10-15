//! markup-carve/carve-js#3144: header roles the Markdown writer moves or drops.

use std::io::Write;
use std::process::{Command, Stdio};

use carve::CheckedRenderOptions;

const HOISTED: &str = "| A | B |\n|= C |= D |\n";
const PARTIAL: &str = "| A |= B |\n| C | D |\n";
const HEADER_FIRST: &str = "|= A |= B |\n| C | D |\n";

fn codes(source: &str) -> Vec<&'static str> {
    carve::to_markdown_with_report(source, CheckedRenderOptions::default())
        .unwrap()
        .losses
        .iter()
        .map(|loss| loss.code)
        .collect()
}

#[test]
fn a_header_row_below_body_rows_is_hoisted_and_reported() {
    let result = carve::to_markdown_with_report(HOISTED, CheckedRenderOptions::default()).unwrap();
    assert_eq!(result.value, "| C | D |\n| --- | --- |\n| A | B |\n");
    assert_eq!(result.total_losses, 1);
    let loss = &result.losses[0];
    assert_eq!(loss.code, "table-header-row-hoisted");
    assert_eq!(loss.node_type.as_str(), "block");
    assert_eq!(loss.message, "Moved a header row above earlier body rows");
    assert_eq!(loss.pos.as_ref().map(|pos| pos.start_line), Some(2));
}

#[test]
fn each_header_cell_of_a_partly_header_row_is_reported() {
    let result = carve::to_markdown_with_report(
        "| A |= B |= E |\n| C | D | F |\n",
        CheckedRenderOptions::default(),
    )
    .unwrap();
    assert_eq!(
        result
            .losses
            .iter()
            .map(|loss| loss.code)
            .collect::<Vec<_>>(),
        ["table-header-cell-demoted", "table-header-cell-demoted"]
    );
    assert_eq!(
        result.losses[0].message,
        "Wrote a header cell as a body cell"
    );
    assert_eq!(
        result
            .losses
            .iter()
            .map(|loss| loss.pos.as_ref().map(|pos| pos.start_column))
            .collect::<Vec<_>>(),
        [Some(6), Some(11)]
    );
    assert_eq!(codes(PARTIAL), ["table-header-cell-demoted"]);
    assert_eq!(
        codes("|= A |= B |\n|= C | D |\n"),
        ["table-header-cell-demoted"]
    );
}

#[test]
fn a_table_whose_header_row_comes_first_reports_nothing() {
    assert!(codes(HEADER_FIRST).is_empty());
    assert!(codes("| A | B |\n| C | D |\n").is_empty());
}

#[test]
fn targets_that_keep_header_roles_report_nothing() {
    let options = CheckedRenderOptions::default();
    for source in [HOISTED, PARTIAL] {
        assert_eq!(
            carve::to_html_with_report(source, options)
                .unwrap()
                .total_losses,
            0
        );
        assert_eq!(
            carve::to_carve_with_report(source, options)
                .unwrap()
                .total_losses,
            0
        );
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
    }
}

#[test]
fn a_rows_loss_precedes_its_cells_and_a_cells_demotion_precedes_its_span() {
    assert_eq!(
        codes("| A |\n|= B | < |\n|= C | D |\n"),
        [
            "table-header-row-hoisted",
            "table-span-flattened",
            "table-header-cell-demoted"
        ]
    );
}

fn render_markdown(args: &[&str], stdin: &str) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(["--markdown", "--strict-losses"])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn strict_losses_fails_on_both_codes_unless_each_is_allowed() {
    for (source, code) in [
        (HOISTED, "table-header-row-hoisted"),
        (PARTIAL, "table-header-cell-demoted"),
    ] {
        let strict = render_markdown(&[], source);
        assert!(!strict.status.success(), "{code}");
        assert!(strict.stdout.is_empty(), "{code}");
        assert!(
            String::from_utf8_lossy(&strict.stderr).contains(code),
            "{code}"
        );
        let allowed = render_markdown(&["--allow-loss", code], source);
        assert!(allowed.status.success(), "{code}: {:?}", allowed.stderr);
        assert!(!allowed.stdout.is_empty(), "{code}");
    }
    assert!(render_markdown(&[], HEADER_FIRST).status.success());
}
