//! A description item that follows a nested closed code fence opens a NEW item,
//! rather than being absorbed into the one above (markup-carve/carve#1970).
//!
//! A code fence opened on a description body's list-marker lead owns the
//! flush-left lines below it as verbatim body - it stays open because a
//! flush-left line cannot close a fence nested past the marker. But when the
//! fence's content and its closer are written AT or PAST the body's content
//! column (the form `carve fmt` re-emits, and the form carve-js and carve-php
//! read), the closer closes the fence, and the following `:: ` term at column 0
//! must start a new entry. This engine kept the lead-fence ownership alive for
//! the whole body and absorbed that term as a paragraph in the first `<dd>`;
//! carve#1970 ruled it two entries, matching the oracle, carve-js and carve-php.
//!
//! ORACLE: the executable spec (`scripts/spec/layout.mjs` + `html.mjs`) reads
//! both the source (corpus `455-...-4`) and the blank-less re-emission as two
//! description entries.

use carve::{to_carve, to_html};

fn flat(source: &str) -> String {
    to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

const TWO_ENTRIES: &str = "<dl> <dt>t</dt> <dd> <ul> <li> \
     <pre><code class=\"language-x\">code </code></pre> </li> </ul> </dd> \
     <dt>t2</dt> <dd>plain</dd> </dl>";

#[test]
fn the_blank_less_closed_fence_form_reads_as_two_entries() {
    // The re-emitted form: the fence is CLOSED by an indented closer, and there
    // is no separating blank before the second term. This was the sole engine
    // that folded `:: t2` / `: plain` into the first entry.
    assert_eq!(
        flat(":: t\n: - ```x\n    code\n    ```\n:: t2\n: plain\n"),
        TWO_ENTRIES
    );
}

#[test]
fn the_source_unterminated_form_reads_as_two_entries_too() {
    // Corpus `455-...-4`: the nested lead fence has no closer, so its flush-left
    // body runs to the blank line, which ends it; `:: t2` then opens the second
    // entry. This already read correctly and must stay put.
    assert_eq!(
        flat(":: t\n: - ``` x\ncode\n\n:: t2\n: plain\n"),
        TWO_ENTRIES
    );
}

#[test]
fn the_writer_re_emits_the_blank_less_form_and_it_round_trips() {
    // `carve fmt` no longer inserts a separating blank before the second term:
    // with the parser reading the closed-fence form as two entries, the blank
    // was a workaround for an absorption that no longer happens, and dropping it
    // converges this writer with carve-js and carve-php. The re-emission still
    // re-reads to the same two entries.
    let source = ":: t\n: - ``` x\ncode\n\n:: t2\n: plain\n";
    let formatted = to_carve(source);
    assert_eq!(
        formatted,
        ":: t\n: - ```x\n    code\n    ```\n:: t2\n: plain\n"
    );
    assert_eq!(flat(&formatted), flat(source));
}
