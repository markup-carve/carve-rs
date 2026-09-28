//! A COMMENT SPAN'S DELIMITERS PAIR IN EVERY HOST
//! (markup-carve/carve-rs#2113, oracle markup-carve/carve#2503 and #2505).
//!
//! Section 28 pairs the delimiters and indentation is part of neither
//! (markup-carve/carve#2471), so everything between them is payload wherever the
//! closer is written. Three collectors ended their container ON that closer: the
//! description body, the note body and the item collector's dedent. The
//! container's own parse then read an opener with no closer among its lines,
//! which §28 makes one `%%` line comment, so the payload was PUBLISHED while
//! both delimiters were dropped.
//!
//! THE CLOSER'S COLUMN IS NOT A PARAMETER. markup-carve/carve#2484 settled the
//! principle: no reading of a comment block makes the body visible and the
//! markers invisible. Most tests below therefore read against the CONTROL - the
//! same span closed at the opener's own base - rather than against a transcribed
//! string, so the pair keeps testing the rule if the surrounding output moves.
//! Equality alone cannot see a pair that is wrong the same way on both sides, so
//! the payload is a distinctive token and its absence is asserted outright.
//!
//! Expectations cross-checked against the executable spec
//! (`scripts/spec/layout.mjs` plus `scripts/spec/html.mjs`) at
//! markup-carve/carve d4c15e82, run per document.

use carve::{to_html, to_html_with_options, Options};

/// The one token that may never reach the page: the span's payload.
const PAYLOAD: &str = "zpay";

fn both_paths(src: &str) -> String {
    let facade = to_html(src);
    let authoritative = to_html_with_options(src, &Options::default().with_positions(true));
    assert_eq!(
        facade, authoritative,
        "the library path and the position-tracking path disagree on {src:?}"
    );
    facade
}

/// The below-column closer reads like its base-closer control, and neither
/// publishes the payload.
fn reads_like_its_control(below: &str, control: &str) {
    let below_html = both_paths(below);
    let control_html = both_paths(control);
    assert!(
        !control_html.contains(PAYLOAD),
        "the CONTROL published the payload, so the pair cannot arbitrate: \
         {control:?}: {control_html}"
    );
    assert!(
        !below_html.contains(PAYLOAD),
        "the payload reached the page: {below:?}: {below_html}"
    );
    assert_eq!(
        below_html.trim(),
        control_html.trim(),
        "the below-column closer read differently from its base-closer control\n\
         below: {below:?}\ncontrol: {control:?}"
    );
}

#[test]
fn the_reported_description_body_hides_the_payload() {
    // The ticket's own reproducer, verbatim, against the spec's own output.
    assert_eq!(
        both_paths(":: t\n:  head\n\n     %%%\n     a\n%%%\n").trim(),
        concat!("<dl>\n", "  <dt>t</dt>\n", "  <dd>head</dd>\n", "</dl>",),
    );
}

#[test]
fn a_description_body_answers_every_column_in_the_band() {
    // THE COLUMN IS NOT A PARAMETER, so the whole band answers alike. This host
    // needed the guard placed ahead of its own holds-no-paragraph break: at
    // column 0 a later guard was reached first, and columns 1 and 2 were not.
    let control = format!(":: t\n:  head\n\n   %%%\n   {PAYLOAD}\n   %%%\n   z\n");
    for closer in ["", " ", "  "] {
        reads_like_its_control(
            &format!(":: t\n:  head\n\n   %%%\n   {PAYLOAD}\n{closer}%%%\n   z\n"),
            &control,
        );
    }
}

#[test]
fn a_note_body_keeps_the_span_whole() {
    // The ticket reads this host as already hiding the payload. It does at a
    // delimiter REACHING column 2, and it did not at column 0 or 1, so a host
    // that looks correct may be correct only at some columns.
    let control = format!("see[^f]\n\n[^f]: head\n\n  %%%\n  {PAYLOAD}\n  %%%\n");
    for closer in ["", " "] {
        reads_like_its_control(
            &format!("see[^f]\n\n[^f]: head\n\n  %%%\n  {PAYLOAD}\n{closer}%%%\n"),
            &control,
        );
    }
}

#[test]
fn a_list_item_keeps_the_span_whole_and_the_second_span_with_it() {
    reads_like_its_control(
        &format!("- head\n\n    %%%\n    {PAYLOAD}\n%%%\n    %%%\n    b\n    %%%\n\n  tail\n"),
        &format!("- head\n\n    %%%\n    {PAYLOAD}\n    %%%\n    %%%\n    b\n    %%%\n\n  tail\n"),
    );
}

#[test]
fn a_nested_item_keeps_the_span_whole() {
    reads_like_its_control(
        &format!("- o\n  - head\n\n    %%%\n    {PAYLOAD}\n%%%\n%%%\n    b\n    %%%\n"),
        &format!("- o\n  - head\n\n    %%%\n    {PAYLOAD}\n    %%%\n    %%%\n    b\n    %%%\n"),
    );
}

#[test]
fn an_opener_with_no_closer_ahead_still_opens_nothing() {
    // THE CONTROL FOR THE OPPOSITE OVER-CORRECTION. Section 28 gives an opener
    // with no exact-width closer ahead no span at all: it degrades to one line
    // comment and its payload is ordinary content. A predicate that reported a
    // span here would hide text the author wrote.
    for src in [
        format!(":: t\n:  head\n\n   %%%\n   {PAYLOAD}\n"),
        format!("- head\n\n    %%%\n    {PAYLOAD}\n"),
        format!("see[^f]\n\n[^f]: head\n\n  %%%\n  {PAYLOAD}\n"),
    ] {
        let html = both_paths(&src);
        assert!(
            html.contains(PAYLOAD),
            "the payload was hidden: {src:?}: {html}"
        );
    }
}

#[test]
fn a_run_inside_a_code_fence_is_content_and_a_mismatched_width_is_payload() {
    // A code fence's body is opaque, so a `%%%` written in it opens nothing and
    // the real delimiter below is not its closer.
    let in_code = both_paths(&format!(
        "- head\n\n    ```\n    %%%\n    ```\n    %%%\n    {PAYLOAD}\n%%%\n\n  tail\n"
    ));
    assert!(
        in_code.contains("%%%"),
        "the code block lost its own run: {in_code}"
    );
    assert!(!in_code.contains(PAYLOAD), "{in_code}");

    // A run of a mismatched width is payload of the span that is open rather than
    // a delimiter of a new one.
    let mismatched = both_paths(&format!(
        "- head\n\n    %%%%\n    {PAYLOAD}\n%%%\n    b\n%%%%\n\n  tail\n"
    ));
    assert!(!mismatched.contains(PAYLOAD), "{mismatched}");
    assert!(!mismatched.contains(">b<"), "{mismatched}");
}

#[test]
fn a_payload_line_below_the_column_still_ends_the_host() {
    // ONLY A COMMENT-SHAPED LINE IS EXEMPT. Section 24 C3 still hands a plain
    // below-column line to the enclosing parse, so the item ends there and the
    // line is the document's.
    let html = both_paths("- head\n\n    %%%\nplain\n    %%%\n\n  tail\n");
    assert!(html.contains("plain"), "{html}");
}

#[test]
fn an_opener_on_a_marker_line_is_seen_where_a_block_may_begin() {
    // Section 10 I2: a list marker never interrupts, so it opens a container only
    // where a block may begin - and BOTH ways of getting that wrong publish a
    // payload, so each direction has its own live input here.
    //
    // A code fence opened on a MARKER LINE before the span. Read as written the
    // marker line is not a fence, so the `%%%` inside its opaque payload became a
    // phantom opener that claimed the real delimiter below as its closer, and the
    // span it broke was published.
    reads_like_its_control(
        &format!(":: t\n:  head\n\n   - ```\n     %%%\n     ```\n\n   %%%\n   {PAYLOAD}\n%%%\n\n   tail\n"),
        &format!(":: t\n:  head\n\n   - ```\n     %%%\n     ```\n\n   %%%\n   {PAYLOAD}\n   %%%\n\n   tail\n"),
    );
    reads_like_its_control(
        &format!("see[^f]\n\n[^f]: head\n\n  - ```\n    %%%\n    ```\n\n  %%%\n  {PAYLOAD}\n%%%\n"),
        &format!(
            "see[^f]\n\n[^f]: head\n\n  - ```\n    %%%\n    ```\n\n  %%%\n  {PAYLOAD}\n  %%%\n"
        ),
    );

    // The opposite. With no blank above it the same marker-shaped line FOLDS into
    // the open paragraph, so it opens nothing and the `%%%` under it opens a real
    // span. Stripping markers unconditionally invented an opaque body there,
    // which hid that opener and left its payload on the page.
    reads_like_its_control(
        &format!("see[^f]\n\n[^f]: head\n  - ```\n  %%%\n  {PAYLOAD}\n%%%\n"),
        &format!("see[^f]\n\n[^f]: head\n  - ```\n  %%%\n  {PAYLOAD}\n  %%%\n"),
    );
}
