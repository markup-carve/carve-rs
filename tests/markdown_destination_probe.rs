//! The Markdown writer probes the destination it will actually emit.
//!
//! It normalizes a destination on the way out (it strips control characters,
//! and its consumer decodes character references), so probing the authored form
//! and normalizing afterwards let the writer manufacture a live `javascript:`
//! URL out of one the probe had already dismissed
//! (`markup-carve/carve-rs#806`).

fn markdown(source: &str) -> String {
    let doc = carve::parse(source);
    carve::render_markdown(&doc).expect("markdown renders")
}

fn ansi(source: &str) -> String {
    let doc = carve::parse(source);
    carve::render_ansi(&doc).expect("ansi renders")
}

/// U+007F and the C1 range are dropped by the writer's own strip, so the probe
/// has to see them gone. Built from an escape, never pasted.
fn smuggled(codepoint: u32) -> String {
    let hidden = char::from_u32(codepoint).expect("a valid scalar value");
    let source = format!("[t](java{hidden}script:alert1)\n");
    assert!(
        source.contains(hidden),
        "the probe character was lost before the test could use it"
    );
    source
}

#[test]
fn a_del_or_c1_control_does_not_smuggle_a_denied_scheme() {
    for codepoint in [0x7f, 0x80, 0x9f] {
        let out = markdown(&smuggled(codepoint));
        assert!(
            !out.contains("javascript:"),
            "U+{codepoint:04X} smuggled a live scheme into Markdown: {out}"
        );
        assert!(
            out.contains("[t]()"),
            "U+{codepoint:04X} should leave a blanked destination, got {out}"
        );
    }
}

/// The ANSI target already got this right, by stripping before it probes. It is
/// the reason the fix is a reordering rather than a new decision, so it is
/// pinned here: whatever else changes, these two targets agree.
#[test]
fn the_ansi_target_still_refuses_the_same_input() {
    for codepoint in [0x7f, 0x80, 0x9f] {
        let out = ansi(&smuggled(codepoint));
        assert!(
            !out.contains("javascript:"),
            "U+{codepoint:04X} reached the ANSI output: {out}"
        );
        assert!(
            out.contains("()"),
            "the ANSI target should still show an empty parenthetical, got {out}"
        );
    }
}

/// A character reference is decoded by the consumer, so what it decodes to has
/// to be what was probed. The emitted ampersand is escaped, which decodes back
/// to the authored bytes rather than to a scheme.
#[test]
fn a_character_reference_does_not_smuggle_a_denied_scheme() {
    for source in [
        "[t](&#106;avascript:alert1)\n",
        "[t](&#x6A;avascript:alert1)\n",
        "[t](javascript&colon;alert1)\n",
        "[t](javascript&#58;alert1)\n",
        "![t](&#106;avascript:alert1)\n",
    ] {
        let out = markdown(source);
        assert!(
            !out.contains("(&#") && !out.contains("&colon;") && !out.contains("&#58;"),
            "an undecoded reference survived into the destination: {out}"
        );
        assert!(
            out.contains("&amp;"),
            "the reference-opening ampersand should be escaped: {out}"
        );
    }
}

/// CONTROL: an ordinary destination is emitted byte-for-byte. An ampersand that
/// opens nothing is not a character reference, and a query string must survive
/// intact - percent-encoding it was the tempting fix and it is the wrong one.
#[test]
fn an_ordinary_destination_is_untouched() {
    let out = markdown("[a](http://x/?a=1&b=2)\n\n[c](mailto:x@y.z)\n\n![i](p.png \"t\")\n");
    assert!(out.contains("[a](http://x/?a=1&b=2)"), "{out}");
    assert!(out.contains("[c](mailto:x@y.z)"), "{out}");
    assert!(out.contains("![i](p.png \"t\")"), "{out}");
}

/// CONTROL: the denylist itself still works on a plainly authored scheme, and
/// still lets an ordinary one through.
#[test]
fn the_denylist_still_decides_the_plain_cases() {
    assert!(markdown("[t](javascript:alert1)\n").contains("[t]()"));
    assert!(markdown("[t](vbscript:x)\n").contains("[t]()"));
    assert!(markdown("[t](https://example.org/)\n").contains("(https://example.org/)"));
}
