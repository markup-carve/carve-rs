//! THE LAZY FRAME NEVER REACHES THE READER (markup-carve/carve-rs#1538).
//!
//! `collect_blockquote_body` hands a lazily-folded line to the quote's own parse
//! behind a `U+0000 'L' U+0000` frame. That frame is pipeline state, not text:
//! any of it reaching rendered output is a sentinel-injection hazard, and would
//! put a control character on the page.
//!
//! WHY THIS FILE GENERATES ITS INPUTS. The corpus cannot catch this class. The
//! executable spec shipped the same frame into rendered code text and its own
//! corpus-wide sentinel check could not fail for the shape that broke it - no
//! corpus input opens a fence on a marker line and then drops below the content
//! column (carve, `oracle-framing-never-leaks.test.mjs`). A guard whose inputs
//! are a fixed list only guards the list. So this enumerates the AXES - which
//! construct, at which column, with the lazy line at which column - and lets the
//! product generate shapes nobody thought to write down.
//!
//! It is deliberately indifferent to what the right rendering IS. Several of
//! these shapes are genuinely undecided across the engines; the assertion is
//! only that the answer contains no framing.

use carve::to_html;

/// Bodies kept VERBATIM: a frame in one of these reaches the reader as text
/// rather than being consumed by a paragraph builder. These four are the whole
/// class in this engine - `CodeBlock`, `RawBlock` and `Comment` are the only
/// nodes holding a raw `content: String`, and the line block is the only other
/// construct that keeps its body's lines.
const VERBATIM: &[(&str, &str, &str)] = &[
    ("code fence", "```", "```"),
    ("raw block", "```=html", "```"),
    ("line block", "::: |", ":::"),
    ("comment fence", "%%%", "%%%"),
];

/// Any sentinel character, not just the frame: a partial frame is still a leak.
fn sentinels(html: &str) -> bool {
    html.contains('\u{0000}')
}

fn check(name: &str, doc: &str, failures: &mut Vec<String>) {
    let html = match std::panic::catch_unwind(|| to_html(doc)) {
        Ok(html) => html,
        Err(_) => {
            failures.push(format!("{name}: PANICKED on {doc:?}"));
            return;
        }
    };
    if sentinels(&html) {
        failures.push(format!(
            "{name}: framing leaked\n  in  {doc:?}\n  out {html:?}"
        ));
    }
}

/// The product: every verbatim construct, opened at every column a container can
/// put it at, with the lazy line at every column around it.
#[test]
fn no_generated_shape_leaks_the_frame() {
    let mut failures = Vec::new();
    let mut generated = 0usize;
    // Hosts that leave an open paragraph a line can fold into, and put the
    // construct at a different content column each time.
    let hosts = ["> ", "> - ", "> - - ", "> > ", "> > - ", "> - > "];
    for host in hosts {
        for (name, open, close) in VERBATIM {
            // The lazy line's own column, from flush-left to past the content
            // column of the deepest host above.
            for lazy_col in 0..8 {
                let pad = " ".repeat(lazy_col);
                // With and without a closer: an unterminated construct runs to
                // the end of its container, which is a different collection path.
                for closed in [false, true] {
                    let mut doc = format!("{host}{open}\n{pad}body\n");
                    if closed {
                        doc.push_str(&format!("{host}{close}\n"));
                    }
                    doc.push_str("tail\n");
                    check(name, &doc, &mut failures);
                    generated += 1;
                    // The opener on the MARKER line - the exact shape that broke
                    // the executable spec's own check.
                    let doc = format!("{host}x\n{host}{open}\n{pad}body\n{host}{close}\ntail\n");
                    check(name, &doc, &mut failures);
                    generated += 1;
                }
            }
        }
    }
    assert!(
        generated >= 500,
        "generator produced only {generated} shapes"
    );
    assert!(
        failures.is_empty(),
        "{} of {generated} generated shapes leaked the frame:\n{}",
        failures.len(),
        failures
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// NON-VERBATIM constructs too. These go through a paragraph or inline builder,
/// so they are the sites that must strip on the TEXT path rather than the
/// verbatim one - and a miss there is just as visible.
#[test]
fn no_text_path_leaks_the_frame() {
    let mut failures = Vec::new();
    let mut generated = 0usize;
    let followers = [
        "text",
        "# h",
        "> q",
        "- m",
        ":: t",
        ":  d",
        "| a |",
        "---",
        "^ cap",
        "{.k}",
        "%% c",
        "1. o",
        "![i](/i.png)",
        "*[A]: b",
        "[a]: /u",
        "[^f]: n",
        "::: note",
        ":::",
        "+",
        "\\",
    ];
    for host in ["> ", "> - ", "> - - ", "> > - ", "> - :: t"] {
        for follower in followers {
            for lazy_col in 0..8 {
                let doc = format!("{host}x\n{}{follower}\ntail\n", " ".repeat(lazy_col));
                check("text path", &doc, &mut failures);
                generated += 1;
            }
        }
    }
    assert!(
        generated >= 500,
        "generator produced only {generated} shapes"
    );
    assert!(
        failures.is_empty(),
        "{} of {generated} generated shapes leaked the frame:\n{}",
        failures.len(),
        failures
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// PREREQUISITE 3, PINNED. The frame survives only because `normalize_source`
/// runs ONCE at the document entry while container bodies are re-parsed through
/// `parse_blocks*`, which does not re-normalize. A refactor that moved
/// normalization into the nested parse would replace the frame's NULs with
/// U+FFFD and break the mechanism with NO other symptom - the lazy line would
/// silently regain a column. This asserts the frame still does its job at depth.
#[test]
fn the_frame_survives_a_deeply_nested_reparse() {
    // At six containers deep the body has been rebuilt and re-parsed many times
    // over. A marker-shaped lazy line must still be TEXT, not a new list.
    let doc = "> - - - - - x\n  - m\ntail\n";
    let html = to_html(doc);
    assert!(!sentinels(&html), "framing leaked: {html:?}");
    assert!(
        !html.contains("<li>m</li>"),
        "the lazy marker re-opened a list at depth, so the frame did not survive: {html:?}"
    );
}

/// A DOCUMENT CANNOT FORGE THE FRAME. Every U+0000 in the input becomes U+FFFD
/// before the first line is read, so the frame stays unforgeable - the
/// prerequisite the executable spec had to add for the same reason (carve#1523).
#[test]
fn a_document_cannot_forge_the_frame() {
    for forged in [
        "\u{0000}L\u{0000}- m\n",
        "> - x\n\u{0000}L\u{0000}- m\n",
        "```\n\u{0000}L\u{0000}x\n```\n",
    ] {
        let html = to_html(forged);
        assert!(!sentinels(&html), "a forged frame survived: {html:?}");
    }
}
