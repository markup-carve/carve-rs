//! `tools/check-engine-pin.py` is watched failing, once per assertion it makes
//! (markup-carve/carve-rs#771).
//!
//! A guard nobody has watched fail is not evidence of anything, and the specific
//! way this class of guard dies is documented at markup-carve/carve#755: it
//! keeps passing after it has stopped measuring. The two shapes of that here are
//!
//!   - a distance-only check, which asserts nothing once the pin reaches the tip
//!     of the engine. Every assertion below except `pin_age` is exercised with
//!     the pin sitting EXACTLY ON the engine tip, so a healthy pin is not what
//!     silences them;
//!   - a reader that finds no pin and reports success. `pin_present` is asserted
//!     against a manifest with no engine dependency at all.
//!
//! The fixtures are throwaway git repositories built here rather than the real
//! carve-rs, so the cases are hermetic: "a revision that does not exist" and "a
//! revision that is not on main" are constructed, not waited for.

use std::path::{Path, PathBuf};
use std::process::Command;

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("carve-rs-pin-guard-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("engine")).unwrap();
        std::fs::create_dir_all(root.join("binding")).unwrap();
        let f = Fixture { root };
        f.build_engine();
        f
    }

    fn engine(&self) -> PathBuf {
        self.root.join("engine")
    }

    fn binding(&self) -> PathBuf {
        self.root.join("binding")
    }

    fn git(&self, dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .current_dir(dir)
            .args(args)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
            .output()
            .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// A two-commit `main` plus one commit on a side branch that never merged.
    fn build_engine(&self) {
        let e = self.engine();
        self.git(&e, &["init", "--quiet", "--initial-branch=main"]);
        for n in ["one", "two"] {
            std::fs::write(e.join(n), n).unwrap();
            self.git(&e, &["add", "."]);
            self.git(&e, &["commit", "--quiet", "-m", n]);
        }
        self.git(&e, &["checkout", "--quiet", "-b", "side"]);
        std::fs::write(e.join("side"), "side").unwrap();
        self.git(&e, &["add", "."]);
        self.git(&e, &["commit", "--quiet", "-m", "side"]);
        self.git(&e, &["checkout", "--quiet", "main"]);
    }

    fn tip(&self) -> String {
        self.git(&self.engine(), &["rev-parse", "main"])
    }

    fn unmerged(&self) -> String {
        self.git(&self.engine(), &["rev-parse", "side"])
    }

    fn write(&self, rel: &str, body: &str) {
        let path = self.binding().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    /// A Cargo pin in the shape the three Cargo bindings use: the dependency key
    /// is not "carve-lang", the package rename is, and the lock repeats the
    /// revision in its own `source` line.
    fn cargo_pin(&self, manifest_rev: &str, lock_rev: &str) {
        self.write(
            "Cargo.toml",
            &format!(
                "[package]\nname = \"a-binding\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
                 [dependencies]\n\
                 carve_rs = {{ package = \"carve-lang\", git = \"https://github.com/markup-carve/carve-rs\", rev = \"{manifest_rev}\" }}\n"
            ),
        );
        self.write(
            "Cargo.lock",
            &format!(
                "version = 3\n\n[[package]]\nname = \"a-binding\"\nversion = \"0.1.0\"\n\n\
                 [[package]]\nname = \"carve-lang\"\nversion = \"0.1.1\"\n\
                 source = \"git+https://github.com/markup-carve/carve-rs?rev={lock_rev}#{lock_rev}\"\n"
            ),
        );
    }

    fn run(&self, extra: &[&str]) -> (i32, String) {
        let engine = self.engine();
        let mut args: Vec<String> = vec![
            "tools/check-engine-pin.py".into(),
            "--engine".into(),
            engine.to_string_lossy().into_owned(),
        ];
        args.extend(extra.iter().map(|s| s.to_string()));
        let out = Command::new("python3")
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args(&args)
            .output()
            .expect("python3 must be on PATH");
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        (out.status.code().unwrap_or(-1), text)
    }

    fn cargo_args(&self) -> Vec<String> {
        vec![
            "--form".into(),
            "cargo".into(),
            "--manifest".into(),
            self.binding()
                .join("Cargo.toml")
                .to_string_lossy()
                .into_owned(),
            "--lock".into(),
            self.binding()
                .join("Cargo.lock")
                .to_string_lossy()
                .into_owned(),
        ]
    }

    fn run_cargo(&self, extra: &[&str]) -> (i32, String) {
        let mut a = self.cargo_args();
        a.extend(extra.iter().map(|s| s.to_string()));
        let refs: Vec<&str> = a.iter().map(String::as_str).collect();
        self.run(&refs)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn assert_fails_with(out: (i32, String), check: &str) {
    let (code, text) = out;
    assert_eq!(code, 1, "expected an assertion failure, got:\n{text}");
    assert!(
        text.contains(check),
        "expected the `{check}` assertion to be the one that failed:\n{text}"
    );
}

// ---------------------------------------------------------------------------
// the healthy case, so a red result below means something
// ---------------------------------------------------------------------------

#[test]
fn a_pin_on_the_tip_passes() {
    let f = Fixture::new("healthy");
    let tip = f.tip();
    f.cargo_pin(&tip, &tip);
    let (code, text) = f.run_cargo(&[]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("main is 0 commit(s) ahead of it"), "{text}");
    assert!(text.contains("every assertion holds"), "{text}");
}

// ---------------------------------------------------------------------------
// THE PROPERTY THE TICKET ASKS FOR: these all run at ZERO drift
// ---------------------------------------------------------------------------

/// The lockfile was regenerated and the manifest was not, with the manifest
/// sitting exactly on the engine tip. Nothing about the DISTANCE is wrong here -
/// a distance-only gate is silent on this document, and this is the case that
/// proves the guard still works when the pin is healthy.
#[test]
fn a_lock_that_disagrees_with_its_manifest_fails_at_zero_drift() {
    let f = Fixture::new("lockdrift");
    let tip = f.tip();
    let older = f.git(&f.engine(), &["rev-parse", "main~1"]);
    f.cargo_pin(&tip, &older);
    assert_fails_with(f.run_cargo(&[]), "lock_agrees");
}

/// The reader finds no engine dependency at all. Reporting success here is the
/// exact defect this replaces: three bindings looked unguarded because anyone
/// grepping for "carve" in a manifest found only the binding's own package.
#[test]
fn a_manifest_with_no_engine_dependency_fails_rather_than_passing() {
    let f = Fixture::new("nopin");
    f.write(
        "Cargo.toml",
        "[package]\nname = \"a-binding\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\ncarve = \"0.1\"\n",
    );
    f.write("Cargo.lock", "version = 3\n");
    assert_fails_with(f.run_cargo(&[]), "pin_present");
}

/// A git dependency with no `rev` at all: every build resolves whatever landed
/// since, which is the state carve-wasm's own README records having left.
#[test]
fn a_branch_tracking_dependency_fails_at_zero_drift() {
    let f = Fixture::new("branchdep");
    f.write(
        "Cargo.toml",
        "[package]\nname = \"a-binding\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
         [dependencies]\ncarve_rs = { package = \"carve-lang\", git = \"https://github.com/markup-carve/carve-rs\" }\n",
    );
    f.write("Cargo.lock", "version = 3\n");
    assert_fails_with(f.run_cargo(&[]), "pin_present");
}

/// The lock resolves the engine under the wrong package name. `carve-lang` is
/// the published name; `carve` is the binding's own module in three of the four
/// repositories, so a lock naming `carve` is reading something else.
#[test]
fn a_lock_naming_the_wrong_package_fails_at_zero_drift() {
    let f = Fixture::new("wrongpkg");
    let tip = f.tip();
    f.cargo_pin(&tip, &tip);
    f.write(
        "Cargo.lock",
        &format!(
            "version = 3\n\n[[package]]\nname = \"carve\"\nversion = \"0.1.1\"\n\
             source = \"git+https://github.com/markup-carve/carve-rs?rev={tip}#{tip}\"\n"
        ),
    );
    assert_fails_with(f.run_cargo(&[]), "lock_agrees");
}

/// An abbreviated revision resolves against a local checkout and then never
/// matches the 40-hex the lockfile writes.
#[test]
fn an_abbreviated_revision_fails_at_zero_drift() {
    let f = Fixture::new("abbrev");
    let tip = f.tip();
    f.cargo_pin(&tip[..12], &tip[..12]);
    assert_fails_with(f.run_cargo(&[]), "pin_well_formed");
}

// ---------------------------------------------------------------------------
// the three failures the ticket asks to be demonstrated
// ---------------------------------------------------------------------------

#[test]
fn a_revision_that_does_not_exist_fails() {
    let f = Fixture::new("ghost");
    let ghost = "0".repeat(40);
    f.cargo_pin(&ghost, &ghost);
    assert_fails_with(f.run_cargo(&[]), "revision_exists");
}

#[test]
fn a_revision_that_is_not_on_main_fails() {
    let f = Fixture::new("unmerged");
    let side = f.unmerged();
    f.cargo_pin(&side, &side);
    assert_fails_with(f.run_cargo(&[]), "revision_on_branch");
}

// ---------------------------------------------------------------------------
// the rev-file form (carve-go)
// ---------------------------------------------------------------------------

#[test]
fn a_rev_file_pin_is_read_and_checked() {
    let f = Fixture::new("revfile");
    let tip = f.tip();
    f.write("internal/wasm/REV", &format!("{tip}\n"));
    let rev = f.binding().join("internal/wasm/REV");
    let (code, text) = f.run(&["--form", "rev-file", "--file", &rev.to_string_lossy()]);
    assert_eq!(code, 0, "{text}");

    std::fs::write(&rev, format!("{}\n{}\n", f.tip(), f.unmerged())).unwrap();
    assert_fails_with(
        f.run(&["--form", "rev-file", "--file", &rev.to_string_lossy()]),
        "pin_present",
    );
}

#[test]
fn a_missing_rev_file_fails() {
    let f = Fixture::new("norevfile");
    let rev = f.binding().join("internal/wasm/REV");
    assert_fails_with(
        f.run(&["--form", "rev-file", "--file", &rev.to_string_lossy()]),
        "pin_present",
    );
}

/// The revision has to DESCRIBE the committed artifact, not merely sit beside
/// it. This assertion is only made when a digest is recorded alongside; without
/// one there is nothing to compare, and asserting anyway would be the check that
/// cannot fail.
#[test]
fn an_artifact_that_does_not_match_its_digest_fails_at_zero_drift() {
    let f = Fixture::new("artifact");
    let tip = f.tip();
    f.write("internal/wasm/REV", &format!("{tip}\n"));
    f.write("internal/wasm/carve.wasm", "the bytes that were committed");
    f.write("internal/wasm/carve.wasm.sha256", &"a".repeat(64));
    let rev = f.binding().join("internal/wasm/REV");
    let art = f.binding().join("internal/wasm/carve.wasm");
    let dig = f.binding().join("internal/wasm/carve.wasm.sha256");
    assert_fails_with(
        f.run(&[
            "--form",
            "rev-file",
            "--file",
            &rev.to_string_lossy(),
            "--artifact",
            &art.to_string_lossy(),
            "--artifact-digest",
            &dig.to_string_lossy(),
        ]),
        "artifact_digest",
    );
}

// ---------------------------------------------------------------------------
// the lag report is a number; only AGE can fail the job
// ---------------------------------------------------------------------------

/// A pin 26 commits behind - the distance the three Cargo bindings carried when
/// markup-carve/carve-rs#771 was written - passes, because commit count is not
/// the subject. carve-rs merges continuously, so a count-based gate would be red
/// from the moment any PR opens and unclearable by the action it recommends.
#[test]
fn a_pin_behind_by_commits_is_reported_and_not_failed() {
    let f = Fixture::new("behind");
    let older = f.git(&f.engine(), &["rev-parse", "main~1"]);
    f.cargo_pin(&older, &older);
    let (code, text) = f.run_cargo(&[]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("main is 1 commit(s) ahead of it"), "{text}");
}

#[test]
fn an_over_age_pin_fails_and_a_young_one_does_not() {
    let f = Fixture::new("age");
    let older = f.git(&f.engine(), &["rev-parse", "main~1"]);
    f.cargo_pin(&older, &older);
    // The fixture's two commits are seconds apart, so any positive limit clears
    // it; a zero-day limit is what the age gate looks like when it bites.
    let (code, text) = f.run_cargo(&["--max-age-days", "30"]);
    assert_eq!(code, 0, "{text}");

    // Re-commit the tip with a far-future date so the pin is provably old.
    std::fs::write(f.engine().join("three"), "three").unwrap();
    f.git(&f.engine(), &["add", "."]);
    let out = Command::new("git")
        .current_dir(f.engine())
        .args([
            "commit",
            "--quiet",
            "-m",
            "three",
            "--date",
            "2030-01-01T00:00:00Z",
        ])
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
        .env("GIT_COMMITTER_DATE", "2030-01-01T00:00:00Z")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_fails_with(f.run_cargo(&["--max-age-days", "30"]), "pin_age");
}
