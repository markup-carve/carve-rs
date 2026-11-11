use carve::{
    render_markdown_with_options, try_to_markdown_with_options, MarkdownRenderError, Options,
};
use std::io::Write;
use std::process::{Command, Stdio};

const CORPUS_1023: &str = "> quote\n^ `open\n {.x\n .y}\n ```\n z\n";

#[test]
fn a_roundtrip_caption_propagates_the_carrier_writer_refusal() {
    let options = Options::default().with_roundtrip(true);
    for source in [CORPUS_1023, "```\na\n```\n^ `open\n value`\n"] {
        let doc = carve::parse(source);
        let error = render_markdown_with_options(&doc, &options).unwrap_err();
        assert!(matches!(error, MarkdownRenderError::SourceUnspellable(_)));
        assert_eq!(
            error.to_string(),
            "the Carve renderer cannot spell code: a line of the value starts with whitespace, which the block layer strips"
        );
        assert!(try_to_markdown_with_options(source, &options).is_err());
    }
}

#[test]
fn the_cli_reports_the_refusal_without_panicking() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(["--md", "--roundtrip"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(CORPUS_1023.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "carve: cannot spell code: a line of the value starts with whitespace, which the block layer strips\n"
    );
}

#[test]
fn carrier_refusals_across_1024_caption_variants_are_typed() {
    let options = Options::default().with_roundtrip(true);
    let mut refusals = 0;
    for depth in 0..4 {
        for caption in ["^ `open\n", "^ x `open\n", "^ [x] `open\n", "^ *x* `open\n"] {
            for value in [" x`", "  x`", "\tx`", " {.x\n .y}\n ```\n z"] {
                for trailing in ["", "\n\nend\n", "\n> next\n", "\n- next\n"] {
                    for lead in ["quote\n", "- item\n", "::: note\nbody\n:::\n", "text\n\n"] {
                        let source = format!(
                            "{}{}{}{}{}",
                            "> ".repeat(depth),
                            lead,
                            caption,
                            value,
                            trailing
                        );
                        match render_markdown_with_options(&carve::parse(&source), &options) {
                            Ok(_) => {}
                            Err(MarkdownRenderError::SourceUnspellable(_)) => refusals += 1,
                            Err(error) => panic!("unexpected refusal for {source:?}: {error}"),
                        }
                    }
                }
            }
        }
    }
    assert!(refusals > 0);
}
