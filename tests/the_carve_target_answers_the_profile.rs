//! `--carve` is a profiled target like every other one (carve-rs#1191).
//!
//! `to_carve` carries no `Options`, so the profile was never asked about on the
//! parse path: an over-cap document came back in full at exit 0, and a raw HTML
//! block a `minimal` profile removes from every other output was written
//! straight back out. The ingest path had already answered the same flags the
//! other way - `--from-json --carve --profile minimal` degrades that block -
//! so one target gave two answers depending on how the document arrived.
//!
//! Every assertion here is paired with the same input run WITHOUT `--profile`.
//! Asserting only "the raw block is gone" would stay green on a build whose
//! writer had simply stopped emitting raw blocks at all, which is a different
//! and worse bug.

use std::io::Write;
use std::process::{Command, Output, Stdio};

const RAW_HTML_BLOCK: &str = "```=html\n<script>alert(1)</script>\n```\n";
const FRONTMATTER_DOC: &str = "---\ntitle: Secret\n---\n\nbody\n";

fn run(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn carve binary");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("write stdin");
    child.wait_with_output().expect("wait carve binary")
}

fn stdout_of(args: &[&str], input: &str) -> String {
    let out = run(args, input);
    assert!(
        out.status.success(),
        "{args:?} exited {:?}: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("stdout is UTF-8")
}

/// The feature filter. A `minimal` profile denies raw blocks, so the writer
/// must not hand the payload back.
#[test]
fn a_denied_raw_block_does_not_come_back_out_of_the_writer() {
    let filtered = stdout_of(&["--carve", "--profile", "minimal"], RAW_HTML_BLOCK);
    assert!(
        !filtered.contains("```=html"),
        "the writer reproduced the raw block a `minimal` profile denies: {filtered:?}"
    );

    // The pair. Without the profile the SAME input keeps its raw block, so the
    // assertion above is about the profile and not about the writer having
    // stopped emitting raw blocks.
    let unfiltered = stdout_of(&["--carve"], RAW_HTML_BLOCK);
    assert!(
        unfiltered.contains("```=html"),
        "without a profile the writer must still reproduce the raw block: {unfiltered:?}"
    );
}

/// The two branches of the CLI reach the same document, so they must reach the
/// same answer. `--from-json` already filtered before `render_carve`; the parse
/// path did not.
#[test]
fn the_parse_path_and_the_ingest_path_agree_on_the_same_document() {
    let json = stdout_of(&["--json"], RAW_HTML_BLOCK);
    let from_source = stdout_of(&["--carve", "--profile", "minimal"], RAW_HTML_BLOCK);
    let from_json = stdout_of(&["--from-json", "--carve", "--profile", "minimal"], &json);
    assert_eq!(
        from_source, from_json,
        "the same document under the same profile serialized two different ways \
         depending on whether it arrived as source or as an encoded tree"
    );
}

/// Frontmatter is the case the tree cannot answer on its own: the writer holds
/// the RAW block in a local and reproduces those bytes, so the filter's own
/// strip never reaches it. A denied block that came back verbatim would leak
/// every `title:` / `author:` value the strip exists to remove.
#[test]
fn denied_frontmatter_is_not_reproduced_from_the_raw_block() {
    let filtered = stdout_of(&["--carve", "--profile", "minimal"], FRONTMATTER_DOC);
    assert!(
        !filtered.contains("Secret"),
        "the writer reproduced frontmatter a `minimal` profile denies: {filtered:?}"
    );
    assert!(
        filtered.contains("body"),
        "the document's body went missing with its frontmatter: {filtered:?}"
    );

    let unfiltered = stdout_of(&["--carve"], FRONTMATTER_DOC);
    assert!(
        unfiltered.contains("Secret"),
        "without a profile the writer must still reproduce frontmatter: {unfiltered:?}"
    );
}

/// The library entry point, not only the CLI. A host that calls the crate
/// directly gets the same refusal.
#[test]
fn the_library_sibling_refuses_an_over_cap_document() {
    let profile = carve::Profile::minimal();
    let cap = profile.max_length();
    assert!(cap > 0, "the minimal profile is expected to carry a cap");
    let options = carve::Options::default().with_profile(profile);

    let over_cap = "x".repeat(cap + 1);
    let err = carve::try_to_carve_with_options(&over_cap, &options)
        .expect_err("an over-cap document must be refused");
    assert!(
        err.to_string().contains("max_length_exceeded"),
        "the refusal does not say why: {err}"
    );

    // The near miss, one byte under. Without it this test would pass against a
    // sibling that refused everything.
    let at_cap = format!("{}\n", "x".repeat(cap - 1));
    assert_eq!(at_cap.len(), cap);
    let rendered =
        carve::try_to_carve_with_options(&at_cap, &options).expect("a document AT the cap renders");
    assert!(
        !rendered.trim().is_empty(),
        "a document at the cap rendered nothing"
    );
}

/// `to_carve` is unchanged. The sibling is additive: the no-options entry point
/// still carries no profile and still reproduces what the author wrote.
#[test]
fn the_no_options_entry_point_is_unchanged() {
    assert_eq!(
        carve::to_carve(RAW_HTML_BLOCK),
        carve::to_carve_with_options(RAW_HTML_BLOCK, &carve::Options::default()),
        "the no-options entry point and a default-options call must agree"
    );
    assert!(
        carve::to_carve(RAW_HTML_BLOCK).contains("```=html"),
        "`to_carve` must still reproduce a raw block"
    );
}
