use std::io::Write;
use std::process::{Command, Stdio};

fn run(args: &[&str], input: &str) -> (i32, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
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
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    (
        output.status.code().unwrap(),
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

#[test]
fn import_help_describes_each_accepted_engine_flag_once() {
    let (status, stdout, stderr) = run(&["import", "--help"], "");
    assert_eq!(status, 0);
    assert_eq!(stderr, "");
    assert_eq!(
        stdout,
        concat!(
            "Usage: carve import [FLAG...] [FILE...]\n",
            "  --adapter generic|tiptap|prosemirror|ckeditor|tinymce|word|google-docs\n",
            "  --allow-loss code\n",
            "  --directory path\n",
            "  --from html|markdown|gfm|djot\n",
            "  --help switch\n",
            "  --list-table switch\n",
            "  --max-render-losses count\n",
            "  --output path\n",
            "  --raw-html switch\n",
            "  --report-losses path\n",
            "  --strict-losses switch\n",
            "  --version switch\n",
        )
    );
}
