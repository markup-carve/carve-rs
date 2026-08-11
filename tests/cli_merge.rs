use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

fn fixtures(contents: [&str; 3]) -> Vec<std::path::PathBuf> {
    let fixture = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
    contents
        .iter()
        .enumerate()
        .map(|(index, content)| {
            let path = std::env::temp_dir().join(format!(
                "carve-merge-{}-{fixture}-{index}.crv",
                std::process::id(),
            ));
            fs::write(&path, content).unwrap();
            path
        })
        .collect()
}

#[test]
fn clean_json_merge_uses_the_shared_envelope() {
    let paths = fixtures(["Base.\n", "Ours.\n", "Base.\n\nAdded.\n"]);
    let output = Command::new(env!("CARGO_BIN_EXE_carve"))
        .arg("merge")
        .arg("--json")
        .args(&paths)
        .output()
        .unwrap();
    for path in paths {
        fs::remove_file(path).unwrap();
    }
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.starts_with("{\"ok\":true,\"ast\":"));
    assert!(stdout.contains("\"conflicts\":[]"));
}

#[test]
fn conflict_json_marks_the_deleted_side() {
    let paths = fixtures(["alpha\n\nbeta\n", "alpha\n", "alpha\n\nbeta edited\n"]);
    let output = Command::new(env!("CARGO_BIN_EXE_carve"))
        .arg("merge")
        .arg("--json")
        .args(&paths)
        .output()
        .unwrap();
    for path in paths {
        fs::remove_file(path).unwrap();
    }
    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\"reason\":\"delete-edit\""));
    assert!(stdout.contains("\"deleted\":{\"base\":false,\"ours\":true,\"theirs\":false}"));
}

#[test]
fn merge_help_is_successful() {
    let output = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(["merge", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("usage: carve merge"));
}
