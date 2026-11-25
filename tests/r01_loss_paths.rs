//! Render-loss paths come from one walk of the tree, so many losses stay
//! linear; each path is the one the per-loss search gave.
use std::process::Command;

fn paths(source: &str, tag: &str) -> Vec<String> {
    let dir = std::env::temp_dir().join(format!("carve-rs-r01-paths-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.crv"), source).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args([
            "render",
            "--mode",
            "static",
            "--extensions",
            r#"["carve.fencedRender.mermaid"]"#,
            "--report-losses",
            "losses.json",
            "a.crv",
        ])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report = std::fs::read_to_string(dir.join("losses.json")).unwrap();
    let mut found = Vec::new();
    let mut rest = report.as_str();
    while let Some(at) = rest.find("\"path\":\"") {
        rest = &rest[at + 8..];
        let end = rest.find('"').unwrap();
        found.push(rest[..end].to_string());
        rest = &rest[end..];
    }
    found
}

#[test]
fn fallback_and_destination_losses_keep_their_paths() {
    let source = "```mermaid\nA\n```\n\n- item\n\n  ```mermaid\n  B\n  ```\n\n> [x](javascript:a) and ![i](javascript:b)\n\n| [y](javascript:c) |\n";
    assert_eq!(
        paths(source, "mixed"),
        [
            "/children/0",
            "/children/1/children/0/children/1",
            "/children/2/children/0/children/0/href",
            "/children/2/children/0/children/2/src",
            "/children/3/children/0/children/0/children/0/children/0/children/0/href",
        ]
    );
}

#[test]
fn every_one_of_many_losses_has_its_path() {
    let source = "```mermaid\nA\n```\n\n".repeat(300);
    let found = paths(&source, "many");
    assert_eq!(found.len(), 100, "the record keeps maxLosses entries");
    for (i, path) in found.iter().enumerate() {
        assert_eq!(path, &format!("/children/{i}"));
    }
}
