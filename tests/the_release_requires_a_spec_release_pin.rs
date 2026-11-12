//! `scripts/check-spec-pin-is-tag.sh` passes only a spec pin that a `0.x.y` tag
//! on the spec remote names, and reports an unreachable remote as exit 2.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn scratch() -> PathBuf {
    let n = NEXT.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("spec-pin-tag-{}-{n}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn git(cwd: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["-c", "user.email=t@example.invalid", "-c", "user.name=t"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// A bare spec remote with one release tag; returns (remote, released, after).
fn spec(root: &Path, tag: &str) -> (PathBuf, String, String) {
    let bare = root.join("spec.git");
    let work = root.join("spec-work");
    git(root, &["init", "-q", "--bare", bare.to_str().unwrap()]);
    git(root, &["init", "-q", "-b", "main", work.to_str().unwrap()]);
    git(&work, &["commit", "-q", "--allow-empty", "-m", "released"]);
    let released = git(&work, &["rev-parse", "HEAD"]);
    git(&work, &["tag", "-a", tag, "-m", tag]);
    git(
        &work,
        &["commit", "-q", "--allow-empty", "-m", "after the release"],
    );
    let after = git(&work, &["rev-parse", "HEAD"]);
    git(
        &work,
        &["push", "-q", bare.to_str().unwrap(), "main", "--tags"],
    );
    (bare, released, after)
}

fn engine(root: &Path, pin: &str) -> PathBuf {
    let dir = root.join(format!("engine-{}", &pin[..8]));
    fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    fs::write(
        dir.join(".gitmodules"),
        "[submodule \"tests/spec\"]\n\tpath = tests/spec\n\turl = https://example.invalid/carve.git\n",
    )
    .unwrap();
    git(&dir, &["add", ".gitmodules"]);
    git(
        &dir,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{pin},tests/spec"),
        ],
    );
    git(&dir, &["commit", "-q", "-m", "pin"]);
    dir
}

fn check(dir: &Path, remote: &Path) -> (i32, String) {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/check-spec-pin-is-tag.sh");
    let out = Command::new("bash")
        .arg(script)
        .arg("tests/spec")
        .current_dir(dir)
        .env("SPEC_REMOTE", remote)
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code().unwrap_or(-1), text)
}

#[test]
fn a_pin_on_a_spec_release_tag_passes() {
    let root = scratch();
    let (remote, released, _) = spec(&root, "0.1.14");
    let (status, text) = check(&engine(&root, &released), &remote);
    assert_eq!(status, 0, "{text}");
    assert!(text.contains("is spec release 0.1.14"), "{text}");
}

#[test]
fn a_pin_between_two_spec_releases_is_refused() {
    let root = scratch();
    let (remote, _, after) = spec(&root, "0.1.14");
    let (status, text) = check(&engine(&root, &after), &remote);
    assert_eq!(status, 1, "{text}");
    assert!(
        text.contains("is not a published spec release tag"),
        "{text}"
    );
}

#[test]
fn an_unreachable_remote_cannot_check() {
    let root = scratch();
    let (_, released, _) = spec(&root, "0.1.14");
    let (status, text) = check(&engine(&root, &released), &root.join("no-such.git"));
    assert_eq!(status, 2, "{text}");
    assert!(text.contains("CANNOT CHECK"), "{text}");
}

#[test]
fn the_release_workflow_runs_the_check() {
    let workflow = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/release.yml"),
    )
    .unwrap();
    assert!(workflow.contains("./scripts/check-spec-pin-is-tag.sh tests/spec"));
}
