use std::io::Write;
use std::process::{Command, Stdio};

fn cli(args: &[&str], input: &str) -> (i32, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(args)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .stdout(Stdio::null())
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
        String::from_utf8(output.stderr).unwrap(),
    )
}

#[test]
fn sibling_after_a_stacked_first_block_stays_a_sibling() {
    assert_eq!(
        carve::to_html("- - +\n  x\n- sibling\n"),
        "<ul>\n  <li>\n    <ul>\n      <li></li>\n    </ul>\n    x\n  </li>\n  <li>sibling</li>\n</ul>"
    );
}

#[test]
fn cli_error_classes_have_status_two_and_a_stable_prefix() {
    let missing = "/tmp/carve-rs-fx11-no-such-file.crv";
    for (args, input, category) in [
        (vec!["--unknown"], "", "usage"),
        (vec!["--profile", "nope"], "", "usage"),
        (vec!["migrate", "--from", "nope"], "", "usage"),
        (vec!["--from-json", "--html"], "{bad", "ast-json-invalid"),
        (vec!["--html", missing], "", "input-unreadable"),
    ] {
        let (status, stderr) = cli(&args, input);
        assert_eq!(status, 2, "{args:?}: {stderr}");
        assert!(
            stderr.starts_with(&format!("carve[{category}]:")),
            "{args:?}: {stderr}"
        );
    }
}
