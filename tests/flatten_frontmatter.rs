//! `carve flatten` drops the leading frontmatter of an included file and
//! keeps its later metadata blocks (C2-T-047, CC-1611, carve-rs#4613).

use std::process::Command;

fn flatten(name: &str, files: &[(&str, &str)]) -> (Option<i32>, String) {
    let dir =
        std::env::temp_dir().join(format!("carve-rs-flatten-fm-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (file, text) in files {
        std::fs::write(dir.join(file), text).unwrap();
    }
    let out = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(["flatten", "a.crv"])
        .current_dir(&dir)
        .output()
        .unwrap();
    (out.status.code(), String::from_utf8(out.stdout).unwrap())
}

#[test]
fn cc_1611_keeps_the_later_block() {
    let out = flatten(
        "cc1611",
        &[
            ("a.crv", "{{ b.crv }}\n"),
            ("b.crv", "---\ntitle: B\n---\n\nx\n\n---yaml\nk: v\n---\n"),
        ],
    );
    assert_eq!(out, (Some(0), "x\n\n---yaml\nk: v\n---\n".to_string()));
}

#[test]
fn a_leading_block_alone_is_dropped() {
    let out = flatten(
        "alone",
        &[
            ("a.crv", "{{ b.crv }}\n"),
            ("b.crv", "---\ntitle: B\n---\n\nx\n"),
        ],
    );
    assert_eq!(out, (Some(0), "x\n".to_string()));
}

#[test]
fn nested_includes_drop_their_own_leading_blocks() {
    let out = flatten(
        "nested",
        &[
            ("a.crv", "top\n\n{{ b.crv }}\n"),
            ("b.crv", "---\ntitle: B\n---\n\nb\n\n{{ c.crv }}\n"),
            ("c.crv", "---yaml\ntitle: C\n---\n\nc\n"),
        ],
    );
    assert_eq!(out, (Some(0), "top\n\nb\n\nc\n".to_string()));
}

#[test]
fn the_top_file_frontmatter_stays() {
    let out = flatten(
        "top",
        &[
            ("a.crv", "---\ntitle: A\n---\n\n{{ b.crv }}\n"),
            ("b.crv", "---\ntitle: B\n---\n\nx\n"),
        ],
    );
    assert_eq!(out, (Some(0), "---\ntitle: A\n---\n\nx\n".to_string()));
}

#[test]
fn a_block_after_a_blank_first_line_is_not_leading() {
    let (status, out) = flatten(
        "line2",
        &[
            ("a.crv", "{{ b.crv }}\n"),
            ("b.crv", "\n---yaml\nk: v\n---\n\nx\n"),
        ],
    );
    assert_eq!(status, Some(0));
    assert!(out.contains("k: v\n---\n\nx\n"), "{out}");
}
