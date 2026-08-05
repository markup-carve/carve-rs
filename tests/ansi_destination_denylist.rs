//! The ANSI target blanks a destination PART 9 §25 denies.
//!
//! §25 binds "EVERY TARGET THAT EMITS A RESOLVABLE URL, not only to the HTML
//! renderer", and gives the reason: a scheme blanked in one target and passed
//! through in another is not blocked, it is deferred by one step. This writer
//! printed the destination verbatim in its parenthetical - `click
//! (javascript:alert(1))` - where the Markdown writer already emitted
//! `[click]()`. Every current terminal emulator autolinks a URL in its output and
//! hands it to the OS handler on click, which is that one step (carve-rs#651,
//! carve#765). All three engines agreed, so it was a design rather than a defect
//! in one of them - the same shape as the Markdown bypass carve#385 fixed.
//!
//! THE DESTINATION IS BLANKED, NOT THE PARENTHETICAL DROPPED: §25 says to emit an
//! EMPTY value, and the empty parenthetical distinguishes "withheld" from "the
//! author wrote none".
//!
//! THE LINK TEXT IS UNTOUCHED, here as in every target. A denied autolink has the
//! URL as its text, so it still shows those characters - HTML shows them too,
//! inside `href=""`. Blanking there would edit the author's words rather than a
//! destination, and a test below pins that it does not happen.
//!
//! NO NEW COPY of the denylist: this calls `escape::sanitize_url`, the same
//! function the HTML and Markdown paths use. A local list of four schemes in one
//! writer is what let the OS protocol-handler class through in the first place.

/// ANSI output with the SGR sequences removed, so a case reads as text.
fn ansi(source: &str) -> String {
    let out = carve::to_ansi(source);
    let mut clean = String::with_capacity(out.len());
    let mut chars = out.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // Drop through the terminating byte of the CSI sequence.
            for inner in chars.by_ref() {
                if inner.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        clean.push(c);
    }
    clean.trim().to_string()
}

#[test]
fn every_denied_scheme_family_is_blanked() {
    for source in [
        "[a](javascript:alert(1))\n",
        "[a](vbscript:x)\n",
        "[a](data:text/html,x)\n",
        "[a](file:///etc/passwd)\n",
        "[a](ms-msdt:x)\n",
        "[a](search-ms:x)\n",
    ] {
        assert_eq!(ansi(source), "a ()", "{source:?} leaked its destination");
    }
}

#[test]
fn the_scheme_check_is_case_insensitive() {
    // A reader lowercases before resolving, so `JAVASCRIPT:` reaching the
    // terminal would defeat the check entirely.
    assert_eq!(ansi("[a](JAVASCRIPT:alert(1))\n"), "a ()");
}

#[test]
fn obfuscating_whitespace_is_stripped_before_the_scheme_is_read() {
    // The shape corpus 121 pins for HTML. An inline `(...)` destination cannot
    // begin with whitespace at all, so the probe never mattered there - a
    // reference DEFINITION can, and that path reaches this target.
    let source = "[click][a]\n\n[a]: \u{202f}javascript:alert(1)\n";
    assert_eq!(ansi(source), "click ()");
}

#[test]
fn an_ordinary_destination_is_untouched() {
    // The boundary that matters most: this must not blank what a terminal reader
    // actually wants to see.
    assert_eq!(ansi("[a](https://ok.test)\n"), "a (https://ok.test)");
    assert_eq!(ansi("[a](/local/path)\n"), "a (/local/path)");
    assert_eq!(ansi("[a](mailto:x@y.test)\n"), "a (mailto:x@y.test)");
}

#[test]
fn a_fragment_still_shows_no_parenthetical() {
    // Unchanged, and pinned because the fix touches the same condition.
    assert_eq!(ansi("[a](#frag)\n"), "a");
}

#[test]
fn a_denied_autolink_gains_no_empty_parenthetical() {
    // The trap. An autolink's text IS its destination, so no parenthetical was
    // ever shown; deciding from the SANITIZED destination rather than the
    // authored one produces `javascript:alert(1) ()`.
    assert_eq!(ansi("<javascript:alert(1)>\n"), "javascript:alert(1)");
}

#[test]
fn an_image_is_unaffected() {
    // It never printed a destination.
    assert_eq!(ansi("![i](ms-msdt:x)\n"), "[img: i]");
}

#[test]
fn no_target_passes_the_scheme_through() {
    // The property §25 is actually about, asserted across the targets rather than
    // only on this one.
    let source = "[a](javascript:alert(1))\n";
    assert!(!carve::to_html(source).contains("javascript:"));
    assert!(!carve::to_markdown(source).contains("javascript:"));
    assert!(!carve::to_plain_text(source).contains("javascript:"));
    assert!(!carve::to_ansi(source).contains("javascript:"));
}
