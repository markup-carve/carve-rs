//! A frontmatter closer may carry a trailing run of spaces and tabs.
//!
//! PART 1 says so in the note on the delimiter itself: "PART 2 drops that
//! trailing `whitespace` (spaces and tabs), so `---<TAB>` and `---<SP><TAB>`
//! are bare frontmatter delimiters". The run is not content, so the line is a
//! delimiter and the block closes on it.
//!
//! carve-rs read the closer by searching for the substring `"\n---\n"`, which
//! cannot see a run at all. A document whose closer was `---<SP><TAB>` lost its
//! frontmatter ENTIRELY: `fmt` escaped the opener into literal text and read the
//! closer as a thematic break, and `migrate --from markdown` returned
//! `## ---yaml title: Hi`, a setext heading (carve-rs#2407).
//!
//! THE CLAUSE DRAWS A BOUNDARY, and the fix has to stop at it. A tab BEFORE a
//! format token is not this run: the format slot's terminal is a space alone,
//! because A TAB IS SYNTAX ONLY IN THE LEADING RUN, so `---<TAB>yaml` is no
//! typed opener (watched in `frontmatter_opener_slot_is_a_space`). And a form
//! feed is CONTENT, not whitespace the line ending drops, so `---<FF>` closes
//! nothing. Both are controls here: a fix that admits them is worse than the
//! bug it removes.
//!
//! THREE PRODUCERS. `split_frontmatter` (the parse path), `raw_frontmatter`
//! (the `fmt` path) and `leading_frontmatter_block` (the Markdown importer) each
//! decide where the block ends, and all three carried the substring search.

use carve::{markdown_to_carve, parse, to_carve, to_html};

const CLOSERS: [(&str, &str); 5] = [
    ("bare", "---"),
    ("space", "--- "),
    ("tab", "---\t"),
    ("space then tab", "--- \t"),
    ("two tabs", "---\t\t"),
];

fn assert_block(label: &str, source: &str, format: &str) {
    let document = parse(source);
    let raw = document
        .frontmatter_raw
        .as_ref()
        .unwrap_or_else(|| panic!("{label}: no frontmatter block"));
    assert_eq!(raw.format, format, "{label}: wrong format token");
    assert_eq!(raw.content, "title: Hi", "{label}: wrong block content");
    assert_eq!(
        document.frontmatter.get("title").map(String::as_str),
        Some("Hi"),
        "{label}: the map lost the key"
    );
    assert_eq!(
        to_carve(source),
        format!("---{format}\ntitle: Hi\n---\n\nBody\n"),
        "{label}: fmt did not write the canonical block"
    );
}

#[test]
fn every_admitted_closer_closes_a_typed_block() {
    for (label, closer) in CLOSERS {
        let source = format!("---yaml\ntitle: Hi\n{closer}\nBody\n");
        assert_block(&format!("typed, {label}"), &source, "yaml");
    }
}

#[test]
fn every_admitted_closer_closes_a_bare_block() {
    // A bare opener writes back as `---yaml`, the parser's own default for it.
    for (label, closer) in CLOSERS {
        let source = format!("---\ntitle: Hi\n{closer}\nBody\n");
        assert_block(&format!("bare, {label}"), &source, "yaml");
    }
}

#[test]
fn the_importer_keeps_the_block_instead_of_a_setext_heading() {
    for (label, closer) in CLOSERS {
        let markdown = format!("---yaml\ntitle: Hi\n{closer}\nBody\n");
        let carve = markdown_to_carve(&markdown);
        assert_eq!(
            carve, "---yaml\ntitle: Hi\n---\n\nBody\n",
            "{label}: the importer lost the block"
        );
        // The imported source is a `fmt` fixed point, which is the only reason
        // the wrong parse went unnoticed: it was a fixed point of the wrong
        // document.
        assert_eq!(to_carve(&carve), carve, "{label}: not a fmt fixed point");
    }
}

#[test]
fn the_closer_run_does_not_leak_into_the_body() {
    let document = parse("---yaml\ntitle: Hi\n--- \t\nBody\n");
    assert_eq!(document.children.len(), 1, "the body is one block");
    let html = to_html("---yaml\ntitle: Hi\n--- \t\nBody\n");
    assert!(html.contains("Body"), "the body vanished: {html}");
    assert!(!html.contains("<hr"), "the closer became a rule: {html}");
    assert!(!html.contains("title: Hi"), "metadata leaked: {html}");
}

/// CONTROL. The clause's own boundary: a tab before the format token leaves the
/// line out of the delimiter test entirely, and no widening of the TRAILING run
/// may reach it.
#[test]
fn a_tab_before_a_format_token_still_opens_nothing() {
    let source = "---\tyaml\ntitle: Hi\n---\nBody\n";
    assert!(
        parse(source).frontmatter_raw.is_none(),
        "a tab in the format slot opened a block"
    );
    let html = to_html(source);
    assert!(html.contains("title: Hi"), "the metadata line vanished");
}

/// CONTROL. A form feed is content, so it is not the run PART 2 drops and the
/// block never closes. The narrowest version of the fix is `[' ', '\t']`;
/// anything reaching for `char::is_whitespace` or a `trim()` fails here.
#[test]
fn a_form_feed_closes_nothing() {
    for (label, closer) in [("form feed", "---\u{c}"), ("vertical tab", "---\u{b}")] {
        let source = format!("---yaml\ntitle: Hi\n{closer}\nBody\n");
        assert!(
            parse(&source).frontmatter_raw.is_none(),
            "{label}: closed the block"
        );
        assert!(
            markdown_to_carve(&source)
                .lines()
                .next()
                .is_none_or(|line| line != "---yaml"),
            "{label}: the importer closed the block"
        );
    }
}
