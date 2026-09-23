//! A non-UTF-8 argument is a usage error, not a panic.
#![cfg(unix)]

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::process::Command;

fn run(args: &[&OsStr]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(args)
        .output()
        .expect("spawn carve binary");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn a_non_utf8_path_is_rejected_cleanly() {
    let latin1 = OsStr::from_bytes(b"caf\xe9.md");
    for args in [
        vec![
            OsStr::new("migrate"),
            OsStr::new("--from"),
            OsStr::new("markdown"),
            latin1,
        ],
        vec![OsStr::new("lint"), latin1],
        vec![latin1],
    ] {
        let (code, stderr) = run(&args);
        assert_eq!(code, 2, "stderr: {stderr}");
        assert!(stderr.contains("not valid UTF-8"), "stderr: {stderr}");
        assert!(!stderr.contains("panicked"), "stderr: {stderr}");
    }
}
