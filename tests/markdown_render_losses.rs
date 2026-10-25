use carve::{
    to_markdown_with_report, try_to_markdown_with_options, with_render_loss_report,
    CheckedRenderOptions, Options, RawNodeType, RenderTarget,
};
use std::io::Write;
use std::process::{Command, Stdio};

fn codes(source: &str) -> Vec<&'static str> {
    to_markdown_with_report(source, CheckedRenderOptions::default())
        .unwrap()
        .losses
        .iter()
        .map(|loss| loss.code)
        .collect()
}

#[test]
fn markdown_reports_each_unrepresentable_kind() {
    assert_eq!(codes(":::\np\n:::\n"), ["markdown-container-flattened"]);
    assert_eq!(codes("{.wide}\n# H\n"), ["markdown-attributes-dropped"]);
    assert_eq!(
        codes("|= H |\n| a |\n^ cap\n"),
        ["markdown-caption-flattened"]
    );
    assert_eq!(
        codes("::: figure\n![one](a.png)\n^ cap\n:::\n"),
        ["markdown-caption-flattened"]
    );
    assert_eq!(
        codes("```=html\n<b>x</b>\n```\n\n`<b>`{=html}\n"),
        ["raw-html-escaped", "raw-html-escaped"]
    );
}

#[test]
fn markdown_reports_effective_fence_title_without_language() {
    for source in [
        "``` \"notes.txt\"\nbody\n```\n",
        "{title=\"from attributes\"}\n``` \"from header\"\nbody\n```\n",
    ] {
        let report = to_markdown_with_report(source, CheckedRenderOptions::default()).unwrap();
        let loss = report
            .losses
            .iter()
            .find(|loss| loss.code == "fence-title-dropped")
            .expect("dropped title must be reported");
        assert_eq!(
            loss.message,
            "Dropped a code fence title without a language in Markdown"
        );
        assert_eq!(loss.node_type, RawNodeType::Block);
        assert_eq!(loss.target, RenderTarget::Markdown);
        assert!(loss.pos.is_some());
    }
    assert!(!codes("```rust \"notes.txt\"\nbody\n```\n").contains(&"fence-title-dropped"));
    let options = Options::default().with_roundtrip(true);
    let report = with_render_loss_report(
        RenderTarget::Markdown,
        CheckedRenderOptions::default(),
        || try_to_markdown_with_options("``` \"notes.txt\"\nbody\n```\n", &options).unwrap(),
    )
    .unwrap();
    assert!(!report
        .losses
        .iter()
        .any(|loss| loss.code == "fence-title-dropped"));
}

#[test]
fn markdown_loss_fields_and_source_order_are_preserved() {
    let source = ":::\np\n:::\n\n{.dangling}\n";
    let report = to_markdown_with_report(source, CheckedRenderOptions::default()).unwrap();
    assert_eq!(
        report
            .losses
            .iter()
            .map(|loss| loss.code)
            .collect::<Vec<_>>(),
        [
            "markdown-container-flattened",
            "markdown-attributes-dropped"
        ]
    );
    assert_eq!(report.total_losses, 2);
    assert_eq!(report.losses[0].target, RenderTarget::Markdown);
    assert_eq!(report.losses[0].node_type, RawNodeType::Block);
    assert_eq!(report.losses[0].pos.as_ref().unwrap().start_line, 1);
    assert_eq!(report.losses[1].pos.as_ref().unwrap().start_line, 5);
    assert_eq!(
        report.losses[1].message,
        "Dropped an unattached block attribute while rendering Markdown"
    );

    let bounded = to_markdown_with_report(
        source,
        CheckedRenderOptions {
            strict: false,
            max_losses: 1,
        },
    )
    .unwrap();
    assert_eq!(bounded.total_losses, 2);
    assert!(bounded.truncated);
    assert_eq!(bounded.losses[0].code, "markdown-container-flattened");
    let strict = to_markdown_with_report(
        source,
        CheckedRenderOptions {
            strict: true,
            max_losses: 1,
        },
    )
    .unwrap_err();
    assert_eq!(strict.total_losses, 2);

    let earlier_dangling = "::: note\n{.lost}\n:::\n\n:::\np\n:::\n";
    let bounded = to_markdown_with_report(
        earlier_dangling,
        CheckedRenderOptions {
            strict: false,
            max_losses: 1,
        },
    )
    .unwrap();
    assert_eq!(bounded.total_losses, 2);
    assert_eq!(bounded.losses[0].code, "markdown-attributes-dropped");
    assert_eq!(bounded.losses[0].pos.as_ref().unwrap().start_line, 2);
}

#[test]
fn roundtrip_carriers_preserve_the_three_structural_kinds() {
    let options = Options::default().with_positions(true).with_roundtrip(true);
    let source = "{.wide}\n:::\np\n:::\n\n|= H |\n| a |\n^ cap\n\n```=html\n<b>x</b>\n```\n";
    let report = with_render_loss_report(
        RenderTarget::Markdown,
        CheckedRenderOptions::default(),
        || try_to_markdown_with_options(source, &options).unwrap(),
    )
    .unwrap();
    assert_eq!(
        report
            .losses
            .iter()
            .map(|loss| loss.code)
            .collect::<Vec<_>>(),
        ["raw-html-escaped"]
    );
}

#[test]
fn cli_accepts_the_new_codes_and_allows_an_actual_container_loss() {
    for code in [
        "markdown-container-flattened",
        "markdown-caption-flattened",
        "markdown-attributes-dropped",
        "fence-title-dropped",
        "raw-html-escaped",
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_carve"))
            .args(["--plain", "--allow-loss", code])
            .output()
            .unwrap();
        assert!(output.status.success(), "{code}: {:?}", output.stderr);
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args([
            "--markdown",
            "--strict-losses",
            "--allow-loss",
            "markdown-container-flattened",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b":::\np\n:::\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "p\n");
}
