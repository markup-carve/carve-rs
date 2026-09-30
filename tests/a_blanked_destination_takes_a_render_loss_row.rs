//! A destination PART 9 §25 blanks owes one render-loss row.
//!
//! §25 blanks a denied URL scheme on every clickable sink, and until
//! markup-carve/carve#2679 no clause said what the render direction owed for it.
//! Every engine blanked the destination and reported nothing, so a checked
//! render was silent about the one link a consumer of untrusted input most wants
//! told about. Measured on `main` before this change: `[x](javascript:alert(1))`
//! reported `total_losses == 0` on all five targets.
//!
//! The code is `destination-denied`, it lives in the CARVE-P2-024 render channel
//! (markup-carve/carve#2681), and the row is REPORTING ONLY: `href=""` stays
//! exactly as it was. Half of the assertions below are on the emitted value,
//! because the easy mistake is to "fix" the blanking while adding the row.
//!
//! WHICH TARGETS OWE A ROW follows CARVE-P2-024's existing rule that a loss
//! records what the selected renderer ACTUALLY does, not every node of that kind
//! in the tree. HTML and Markdown emit all three destinations, so all three
//! blank. The ANSI target prints only a LINK's destination, in a parenthetical;
//! an autolink's destination is its own visible text and an image never had one,
//! so nothing is blanked there and nothing is reported. Plain text emits no URL
//! and the canonical Carve writer keeps the destination whole, so neither
//! reports.

use carve::{CheckedRenderOptions, Options, RawNodeType, RenderResult, RenderTarget};

const LINK: &str = "[x](javascript:alert(1))\n";
const AUTOLINK: &str = "<javascript:alert(1)>\n";
const IMAGE: &str = "![x](vbscript:two)\n";
/// The corpus 536 input: one denied link and one denied image in one document.
const BOTH: &str = "[report me](javascript:one) and ![report me too](vbscript:two)\n";

fn options(safe: bool) -> Options<'static> {
    Options::default().with_positions(true).with_raw_html(!safe)
}

/// A checked HTML render under one of the two safe modes.
fn html(source: &str, safe: bool) -> RenderResult<String> {
    let options = options(safe);
    carve::with_render_loss_report(RenderTarget::Html, CheckedRenderOptions::default(), || {
        carve::to_html_with_options(source, &options)
    })
    .expect("a non-strict collection cannot fail")
}

fn losses(source: &str, target: RenderTarget) -> usize {
    let checked = match target {
        RenderTarget::Html => carve::to_html_with_report(source, CheckedRenderOptions::default()),
        RenderTarget::Markdown => {
            carve::to_markdown_with_report(source, CheckedRenderOptions::default())
        }
        RenderTarget::Plain => {
            carve::to_plain_text_with_report(source, CheckedRenderOptions::default())
        }
        RenderTarget::Ansi => carve::to_ansi_with_report(source, CheckedRenderOptions::default()),
        RenderTarget::Carve => carve::to_carve_with_report(source, CheckedRenderOptions::default()),
    };
    checked
        .expect("a non-strict collection cannot fail")
        .total_losses
}

#[test]
fn a_denied_link_destination_takes_one_row_in_both_safe_modes() {
    for safe in [false, true] {
        let checked = html(LINK, safe);
        assert_eq!(checked.total_losses, 1, "safe={safe}");
        let loss = &checked.losses[0];
        assert_eq!(loss.code, "destination-denied");
        assert_eq!(loss.target, RenderTarget::Html);
        assert_eq!(loss.node_type, RawNodeType::Inline);
        assert_eq!(loss.format, None, "a blanked destination has no format");
        assert!(!loss.message.is_empty());
        let pos = loss.pos.as_ref().expect("the sink's source span");
        assert_eq!((pos.start_line, pos.start_column), (1, 1));
    }
}

#[test]
fn a_denied_autolink_destination_takes_one_row_in_both_safe_modes() {
    for safe in [false, true] {
        let checked = html(AUTOLINK, safe);
        assert_eq!(checked.total_losses, 1, "safe={safe}");
        assert_eq!(checked.losses[0].code, "destination-denied");
        assert_eq!(checked.losses[0].node_type, RawNodeType::Inline);
    }
}

#[test]
fn a_denied_image_source_takes_one_row_in_both_safe_modes() {
    for safe in [false, true] {
        let checked = html(IMAGE, safe);
        assert_eq!(checked.total_losses, 1, "safe={safe}");
        assert_eq!(checked.losses[0].code, "destination-denied");
        assert_eq!(checked.losses[0].node_type, RawNodeType::Inline);
    }
}

#[test]
fn both_sinks_in_one_render_take_one_row_each_in_document_order() {
    for safe in [false, true] {
        let checked = html(BOTH, safe);
        assert_eq!(checked.total_losses, 2, "safe={safe}");
        assert_eq!(checked.losses.len(), 2);
        assert!(!checked.truncated);
        for loss in &checked.losses {
            assert_eq!(loss.code, "destination-denied");
        }
        let first = checked.losses[0].pos.as_ref().expect("span");
        let second = checked.losses[1].pos.as_ref().expect("span");
        assert!(
            first.start_offset < second.start_offset,
            "{first:?} then {second:?}"
        );
    }
}

#[test]
fn the_emitted_value_does_not_change() {
    // The whole point of the ruling: the same call now REPORTS what it blanked,
    // and blanks exactly what it blanked before.
    for source in [LINK, AUTOLINK, IMAGE, BOTH] {
        for safe in [false, true] {
            let checked = html(source, safe);
            assert_eq!(
                checked.value,
                carve::to_html_with_options(source, &options(safe)),
                "{source:?} safe={safe}"
            );
            assert!(!checked.value.contains("href=\"javascript:"));
            assert!(!checked.value.contains("src=\"vbscript:"));
        }
    }
    assert!(html(LINK, false).value.contains("<a href=\"\">x</a>"));
    assert!(html(IMAGE, false)
        .value
        .contains("<img src=\"\" alt=\"x\">"));
    // The TEXT is not blanked, here or on any target: a denied autolink's
    // visible text is its URL, and withholding it would edit what the author
    // wrote rather than withhold a destination.
    assert!(html(AUTOLINK, false)
        .value
        .contains("<a href=\"\">javascript:alert(1)</a>"));
}

#[test]
fn an_ordinary_destination_reports_nothing() {
    for source in [
        "[x](https://ok.test)\n",
        "<https://ok.test>\n",
        "![x](/local.png)\n",
        "[x](#frag)\n",
        "[x](mailto:a@b.test)\n",
    ] {
        for safe in [false, true] {
            let checked = html(source, safe);
            assert_eq!(checked.total_losses, 0, "{source:?} safe={safe}");
        }
    }
}

#[test]
fn a_target_reports_the_destinations_it_actually_blanks() {
    use RenderTarget::{Ansi, Carve, Html, Markdown, Plain};
    // (source, html, markdown, plain, ansi, carve)
    for (source, expected) in [
        (
            LINK,
            [(Html, 1), (Markdown, 1), (Plain, 0), (Ansi, 1), (Carve, 0)],
        ),
        // The ANSI target shows an autolink's destination as its own text and
        // never prints a parenthetical for one, so it blanks nothing.
        (
            AUTOLINK,
            [(Html, 1), (Markdown, 1), (Plain, 0), (Ansi, 0), (Carve, 0)],
        ),
        // ANSI renders an image as `[img: alt]` and never emitted a source.
        (
            IMAGE,
            [(Html, 1), (Markdown, 1), (Plain, 0), (Ansi, 0), (Carve, 0)],
        ),
        (
            BOTH,
            [(Html, 2), (Markdown, 2), (Plain, 0), (Ansi, 1), (Carve, 0)],
        ),
    ] {
        for (target, count) in expected {
            assert_eq!(
                losses(source, target),
                count,
                "{source:?} on {}",
                target.as_str()
            );
        }
    }
}

#[test]
fn a_strict_checked_render_refuses_a_document_whose_destination_was_blanked() {
    let error = carve::to_html_with_report(
        BOTH,
        CheckedRenderOptions {
            strict: true,
            ..CheckedRenderOptions::default()
        },
    )
    .expect_err("strict must refuse");
    assert_eq!(error.total_losses, 2);
    assert_eq!(error.totals_by_code.get("destination-denied"), Some(&2));
}
