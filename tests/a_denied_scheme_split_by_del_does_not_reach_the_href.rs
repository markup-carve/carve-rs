//! PART 9 §25, the denied-scheme defense, on the HTML target.
//!
//! `[x](java<DEL>script:alert(1))` reached the rendered `href` with the raw
//! `7f` byte intact, and the image spelling reached `src` the same way, while
//! the plain `javascript:alert(1)` was blanked correctly. This engine and
//! carve-js were byte-identical here; carve-php blanked both
//! (markup-carve/carve-rs#833).
//!
//! The defect was `escape::is_url_probe_skippable`, which read
//! `(c as u32) <= 0x20 || c.is_whitespace() || c == '\u{FEFF}'`. That stops
//! short of DEL and reaches only U+0085 of the C1 block, so a scheme split by
//! any of the rest was invisible to the probe.
//!
//! THE ENGINE ALREADY HAD THIS RIGHT ONE FILE OVER. The ANSI target runs
//! `strip_terminal_controls` - `char::is_control`, which is Cc exactly - over
//! the destination BEFORE handing it to `sanitize_url`, and the Markdown target
//! does the same through `is_not_emitted`. Only HTML reached the probe with the
//! character still in place, which is why only HTML leaked. The fix is the
//! predicate the other two targets were already using, moved to where the probe
//! itself can see it.
//!
//! THIS IS DEFENSE IN DEPTH, NOT A DEMONSTRATED EXECUTION. Whether such a URL
//! resolves depends on whether the consumer's URL parser discards the character
//! before it reads the scheme, and consumers differ.
//!
//! EVERY ASSERTION IS ON BYTES OR CODE POINTS, never on a rendered string. DEL
//! is invisible in terminal output and `java<DEL>script:` reads as
//! `javascript:` in any log - which is exactly how the first report of this
//! defect concluded the character had been normalized away when it had not. A
//! test comparing rendered strings passes against the broken engine.

use carve::{sanitize_svg, SanitizeSvgOptions};

/// DEL, and the whole C1 block. Outside PART 9 §29 by T5, and each one a
/// character some URL consumer discards before it reads a scheme.
fn probe_only_class() -> Vec<char> {
    std::iter::once('\u{7f}')
        .chain((0x80u32..=0x9f).filter_map(char::from_u32))
        .collect()
}

fn split(c: char) -> String {
    format!("java{c}script:alert(1)")
}

fn attr<'a>(html: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("{name}=\"");
    let i = html.find(&needle)? + needle.len();
    let rest = &html[i..];
    let j = rest.find('"')?;
    Some(&rest[..j])
}

fn hex(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn control_the_plain_spelling_was_always_blanked_and_still_is() {
    // A CONTROL. The denylist itself was never broken - only the split form got
    // past - so no mutation of this defect may move this row. If it goes red,
    // the failure is in the denylist and not in the probe class, and this file
    // is looking at the wrong thing.
    assert_eq!(
        attr(&carve::to_html("[x](javascript:alert(1))\n"), "href"),
        Some("")
    );
    assert_eq!(
        attr(&carve::to_html("![a](javascript:alert(1))\n"), "src"),
        Some("")
    );
}

#[test]
fn control_a_nul_split_is_inert_because_the_reader_replaced_it() {
    // Also a CONTROL, for a different reason: U+0000 never reaches the probe.
    // The reader turns it into U+FFFD, and U+FFFD is not a character a URL
    // consumer discards, so the destination stays visibly broken rather than
    // resolving. Widening the probe class does not touch this path, and this
    // row is what says so. It also BOUNDS THE HAZARD: the defect is specific to
    // characters a parser DROPS, not to control characters generally.
    let html = carve::to_html(&format!("[x]({})\n", split('\u{0}')));
    let href = attr(&html, "href").expect("a link");
    assert_eq!(hex(href), hex("java\u{fffd}script:alert(1)"));
    assert!(!hex(href).contains("7f"));
}

#[test]
fn the_del_split_is_blanked_on_href_and_on_src_alike() {
    // THE ROW THAT PROVES THE FIX. Asserted on the hex of the attribute value,
    // because `java<DEL>script:alert(1)` and `javascript:alert(1)` are
    // indistinguishable when printed.
    let s = split('\u{7f}');
    assert_eq!(hex(&s), "6a6176617f7363726970743a616c657274283129");

    assert_eq!(
        attr(&carve::to_html(&format!("[x]({s})\n")), "href"),
        Some("")
    );
    // The image spelling reproduces the defect identically because it reaches
    // the SAME `sanitize_url`. It needed no separate fix, and this row keeps
    // that true if the two paths ever diverge.
    assert_eq!(
        attr(&carve::to_html(&format!("![a]({s})\n")), "src"),
        Some("")
    );
}

#[test]
fn a_leading_del_does_not_hide_the_scheme_either() {
    let html = carve::to_html("[x](\u{7f}javascript:alert(1))\n");
    assert_eq!(attr(&html, "href"), Some(""));
}

#[test]
fn every_character_of_the_probe_only_class_is_seen_through_on_both_attributes() {
    for c in probe_only_class() {
        let s = split(c);
        let link = carve::to_html(&format!("[x]({s})\n"));
        let image = carve::to_html(&format!("![a]({s})\n"));
        // U+0085 terminates the destination for the reader, so the construct is
        // not a link at all and there is no attribute to judge. Everything that
        // DOES parse as a destination must be blanked.
        if let Some(href) = attr(&link, "href") {
            assert_eq!(href, "", "href U+{:04X}", c as u32);
        }
        if let Some(src) = attr(&image, "src") {
            assert_eq!(src, "", "src U+{:04X}", c as u32);
        }
    }
}

#[test]
fn it_is_the_whole_denylist_not_the_script_schemes() {
    // The OS protocol-handler class (CVE-2026-20841) splits the same way, and a
    // fix reaching only `javascript` would be half a fix twice over.
    for scheme in [
        "javascript",
        "vbscript",
        "data",
        "file",
        "ms-msdt",
        "ms-office",
        "search-ms",
        "shell",
        "vscode",
        "jar",
    ] {
        let s = format!("{}\u{7f}{}:payload", &scheme[..2], &scheme[2..]);
        assert_eq!(
            attr(&carve::to_html(&format!("[x]({s})\n")), "href"),
            Some(""),
            "{scheme} must be blanked when split"
        );
    }
}

#[test]
fn the_original_destination_is_what_is_emitted_when_it_is_allowed() {
    // STRIP-THEN-PROBE, not strip-then-emit. The stripped form is a judgement
    // aid and never becomes output: a benign destination keeps its bytes.
    let url = "/a\u{7f}b?q=1";
    let html = carve::to_html(&format!("[x]({url})\n"));
    assert_eq!(hex(attr(&html, "href").expect("a link")), hex(url));
}

#[test]
fn the_class_does_not_overreach() {
    // A scheme cannot be manufactured out of one that was never written: an
    // ordinary character in the scheme position is not stripped.
    let html = carve::to_html("[x](java-script:alert(1))\n");
    assert_eq!(
        hex(attr(&html, "href").expect("a link")),
        hex("java-script:alert(1)")
    );
}

#[test]
fn an_attribute_override_carries_the_same_probe() {
    // `sanitize_attr_value` filters with the same predicate, so widening it
    // reaches the `{background=...}` / `{style=...}` door as well.
    //
    // The benign row comes FIRST and is load-bearing: without it, this test
    // would still pass if the attribute were dropped wholesale by the name
    // filter, and a check that cannot fail is worse than no check.
    assert!(
        carve::to_html("![a](/ok.png){background=\"https://x.example/\"}\n")
            .contains("background=\"https://x.example/\""),
        "the attribute override door must be open for this row to mean anything"
    );
    for c in ['\u{7f}', '\u{80}', '\u{9b}', '\u{9f}', '\u{1}'] {
        let s = split(c);
        let html = carve::to_html(&format!("![a](/ok.png){{background=\"{s}\"}}\n"));
        assert!(
            !html.contains("script:alert"),
            "U+{:04X} must not reach an attribute value",
            c as u32
        );
    }
}

#[test]
fn the_non_html_targets_refuse_it_too() {
    // These two were already correct, through their own pre-strip. The rows are
    // here so a later simplification that removes a pre-strip on the grounds
    // that "the probe handles it" is measured rather than assumed.
    let s = split('\u{7f}');
    let md = carve::to_markdown(&format!("[x]({s})\n"));
    assert!(!md.contains("script:alert"), "markdown: {md:?}");
    assert!(!hex(&md).contains("7f"));
    let ansi = carve::to_ansi(&format!("[x]({s})\n"));
    assert!(!ansi.contains("script:alert"), "ansi: {ansi:?}");
}

#[test]
fn the_svg_sanitizer_carried_the_second_spelling_of_the_same_probe() {
    // `svg_sanitize.rs` keeps its own strip and its own copy of the denylist,
    // and the copy had the same gap. Both opt-in doors are covered, plus the
    // reject-every-absolute-scheme check, which a split defeated OUTRIGHT
    // rather than merely dodging the denylist.
    let links = SanitizeSvgOptions {
        allow_links: true,
        ..Default::default()
    };
    let images = SanitizeSvgOptions {
        allow_external_images: true,
        ..Default::default()
    };
    for c in ['\u{7f}', '\u{80}', '\u{9b}', '\u{9f}'] {
        let s = split(c);
        let link = sanitize_svg(
            &format!("<svg viewBox=\"0 0 10 10\"><a href=\"{s}\"><rect width=\"1\" height=\"1\"/></a></svg>"),
            &links,
        )
        .svg;
        assert!(
            !link.contains("script:alert"),
            "svg a href U+{:04X}: {link:?}",
            c as u32
        );
        let image = sanitize_svg(
            &format!(
                "<svg viewBox=\"0 0 10 10\"><image href=\"{s}\" width=\"1\" height=\"1\"/></svg>"
            ),
            &images,
        )
        .svg;
        assert!(
            !image.contains("script:alert"),
            "svg image href U+{:04X}: {image:?}",
            c as u32
        );
        let paint = sanitize_svg(
            &format!(
                "<svg viewBox=\"0 0 10 10\"><rect fill=\"{s}\" width=\"1\" height=\"1\"/></svg>"
            ),
            &SanitizeSvgOptions::default(),
        )
        .svg;
        assert!(
            !paint.contains("script:alert"),
            "svg fill U+{:04X}: {paint:?}",
            c as u32
        );
    }
}

#[test]
fn control_the_svg_sanitizer_still_allows_a_benign_scheme() {
    // The widened strip did not turn into a reject-everything.
    let links = SanitizeSvgOptions {
        allow_links: true,
        ..Default::default()
    };
    let ok = sanitize_svg(
        "<svg viewBox=\"0 0 10 10\"><a href=\"https://ok.example/\"><rect width=\"1\" height=\"1\"/></a></svg>",
        &links,
    )
    .svg;
    assert!(ok.contains("href=\"https://ok.example/\""), "{ok:?}");
}

#[test]
fn dismissed_site_the_formatter_probe_is_downstream_of_this_one() {
    // `render_carve.rs` carries a THIRD scheme probe, in
    // `dangerous_destination_scheme`, and its skip is leading-only. It is not a
    // second defect and was not widened, because it answers a different
    // question: whether to escape a destination so that REPARSING the formatted
    // source cannot resurrect something the renderer refused. It never handled
    // an interior split for ANY character, U+0001 included. Whatever it lets
    // through is judged again on the way out, which is what this row measures,
    // and if that ever stops being true this row is what says so.
    for c in ['\u{1}', '\u{7f}', '\u{9b}'] {
        let formatted = carve::to_carve(&format!("[x]({})\n", split(c)));
        let html = carve::to_html(&formatted);
        assert_eq!(
            attr(&html, "href"),
            Some(""),
            "U+{:04X} reparsed live: {formatted:?}",
            c as u32
        );
    }
}
