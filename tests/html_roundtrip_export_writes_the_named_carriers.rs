//! Roundtrip export writes the PART 10 §13a carriers, and an import consumes
//! them (the engine half of markup-carve/carve#2894).
//!
//! The carriers were MEASURED against carve-php, the only implementation that
//! had them, with its `data-djot-` prefix normalized to the ruled `data-carve-`
//! (markup-carve/carve#2902 question 0). Every expected string below is that
//! engine's byte output under `setRoundTripMode(true)`, which is what makes
//! these assertions a parity gate rather than a restatement of this code.
//!
//! One inventory row has no assertion here because carve-rs has no datum behind
//! it: `data-carve-src` holds verbatim block source nothing retains.
//! `data-carve-col-widths` used to sit beside it; the parser now records the
//! separator row as `table.separatorWidths` (markup-carve/carve#2894), and its
//! own round trip is measured in
//! `a_table_carries_the_separator_row_it_was_written_with`.

use carve::{
    html_to_ast, html_to_carve, to_html, to_html_with_options, to_html_with_options_and_report,
    CheckedRenderOptions, HtmlImportMode, HtmlImportOptions, Options,
};

fn rt(source: &str) -> String {
    to_html_with_options(source, &Options::default().with_roundtrip(true))
}

fn back(html: &str, mode: HtmlImportMode) -> String {
    html_to_carve(
        html,
        &HtmlImportOptions {
            mode,
            ..HtmlImportOptions::default()
        },
    )
    .expect("the import holds")
    .value
}

fn safe(html: &str) -> String {
    back(html, HtmlImportMode::Safe)
}

// ---------------------------------------------------------------- off by default

#[test]
fn the_option_is_off_by_default_and_the_default_bytes_do_not_move() {
    assert!(!Options::default().roundtrip);
    let source = "* a\n\n***\n\nEscaped \\* star.\n";
    assert_eq!(
        to_html(source),
        to_html_with_options(source, &Options::default())
    );
    assert!(!to_html(source).contains("data-marker"));
    assert!(!to_html(source).contains("data-char"));
    assert!(!to_html(source).contains("data-carve-"));
}

#[test]
fn turning_it_off_explicitly_is_the_default_bytes() {
    let source = "* a\n\n***\n";
    assert_eq!(
        to_html_with_options(source, &Options::default().with_roundtrip(false)),
        to_html(source)
    );
}

// ---------------------------------------------------------------- the inventory

#[test]
fn a_non_default_bullet_carries_its_marker() {
    assert!(rt("* a\n* b\n").contains(r#"<ul data-marker="*">"#));
    // The DEFAULT needs no spelling (§13a), so a `-` list's bytes do not move.
    assert_eq!(rt("- a\n"), to_html("- a\n"));
}

#[test]
fn a_separator_row_carries_its_dash_runs() {
    assert!(rt("| a | b |\n|:-----|---:|\n").contains(r#"<table data-carve-col-widths="5,3">"#));
    // A native `|=` header wrote no separator row, so there is nothing to carry.
    assert_eq!(rt("|= a |= b |\n"), to_html("|= a |= b |\n"));
}

#[test]
fn a_non_default_break_character_carries_itself() {
    assert!(rt("***\n").contains(r#"<hr data-char="*">"#));
    assert!(rt("___\n").contains(r#"<hr data-char="_">"#));
    assert_eq!(rt("---\n"), to_html("---\n"));
}

#[test]
fn an_authored_heading_id_says_it_was_authored() {
    let html = rt("{#chosen}\n# Explicit\n");
    assert!(
        html.contains(r#"<section id="chosen" data-carve-explicit-id="1">"#),
        "{html}"
    );
    // A GENERATED id carries nothing: the carrier is what tells them apart.
    assert!(!rt("# Generated\n").contains("data-carve-explicit-id"));
}

#[test]
fn an_authored_id_rides_the_heading_where_no_section_wraps_it() {
    let html = to_html_with_options(
        "{#chosen}\n# Explicit\n",
        &Options::default().with_roundtrip(true).with_sections(false),
    );
    assert!(html.contains(r#"<h1 data-carve-explicit-id="1""#), "{html}");
}

#[test]
fn a_reference_link_and_image_carry_the_label_they_resolved_by() {
    let html = rt("[t][lbl] ![a][img]\n\n[lbl]: /dest \"T\"\n[img]: /pic.png\n");
    assert!(
        html.contains(r#"<a href="/dest" title="T" data-carve-ref="lbl">t</a>"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<img src="/pic.png" alt="a" data-carve-ref="img">"#),
        "{html}"
    );
    // A DIRECT link resolved by nothing and carries no label.
    assert!(!rt("[t](/u)\n").contains("data-carve-ref"));
}

#[test]
fn an_autolink_says_it_was_one() {
    assert!(rt("<https://example.com/a>\n")
        .contains(r#"<a href="https://example.com/a" data-carve-autolink="1">"#));
    assert!(!rt("[https://example.com/a](https://example.com/a)\n").contains("data-carve-autolink"));
}

#[test]
fn a_footnote_carries_its_label_on_both_halves() {
    let html = rt("x[^note]\n\n[^note]: body\n");
    assert!(
        html.contains(r#"role="doc-noteref" data-carve-footnote-label="note""#),
        "{html}"
    );
    assert!(
        html.contains(r#"<li id="fn1" data-carve-footnote-label="note">"#),
        "{html}"
    );
}

#[test]
fn an_inline_footnote_says_so_on_its_endnote() {
    let html = rt("x.^[inline body]\n");
    assert!(
        html.contains(r#"<li id="fn1" data-carve-inline-footnote="1">"#),
        "{html}"
    );
    // A LABELLED endnote is not an inline one.
    assert!(!rt("x[^a]\n\n[^a]: body\n").contains("data-carve-inline-footnote"));
}

#[test]
fn a_raw_inline_carries_its_format_and_holds_its_content() {
    let html = rt("x `<b>h</b>`{=html} y `a<b`{=latex} z\n");
    assert!(
        html.contains(r#"<span data-carve-raw="html">&lt;b&gt;h&lt;/b&gt;</span>"#),
        "{html}"
    );
    // A format HTML has no spelling for: the content is ESCAPED, as carve-php
    // writes it, where the default render drops the node entirely.
    assert!(
        html.contains(r#"<span data-carve-raw="latex">a&lt;b</span>"#),
        "{html}"
    );
    assert!(!to_html("x `a<b`{=latex} z\n").contains("a&lt;b"));
}

fn roundtrip_losses(source: &str) -> carve::RenderResult<String> {
    to_html_with_options_and_report(
        source,
        &Options::default().with_roundtrip(true),
        CheckedRenderOptions::default(),
    )
    .expect("a dropped raw format is not an abort")
}

/// The carrier restores the node on import; the target still did not write the
/// format, so the row is reported with the option on (markup-carve/carve#2902
/// ruling 7, extended to the inline form).
#[test]
fn a_carried_raw_inline_still_reports_its_dropped_format() {
    let report = roundtrip_losses("a `\\x`{=tex} b\n");
    assert_eq!(
        report.value,
        r#"<p>a <span data-carve-raw="tex">\x</span> b</p>"#
    );
    assert_eq!(report.total_losses, 1, "{:?}", report.losses);
    let row = &report.losses[0];
    assert_eq!(row.code, "raw-format-dropped");
    assert_eq!(row.format.as_deref(), Some("tex"));
    assert_eq!(row.node_type, carve::RawNodeType::Inline);

    // Raw HTML is written or escaped, never dropped, so it owes no row.
    let html = roundtrip_losses("a `<b>`{=html} b\n");
    assert!(
        html.value.contains(r#"data-carve-raw="html""#),
        "{}",
        html.value
    );
    assert_eq!(html.total_losses, 0, "{:?}", html.losses);
}

#[test]
fn a_raw_block_under_roundtrip_reports_its_dropped_format() {
    let report = roundtrip_losses("a\n\n```=tex\n\\x\n```\n\nb\n");
    assert_eq!(report.total_losses, 1, "{:?}", report.losses);
    let row = &report.losses[0];
    assert_eq!(row.code, "raw-format-dropped");
    assert_eq!(row.format.as_deref(), Some("tex"));
    assert_eq!(row.node_type, carve::RawNodeType::Block);
}

#[test]
fn an_escaped_character_is_carried_bare_and_one_at_a_time() {
    // The presence carrier is written BARE, which is what carve-php writes and
    // what §13a's "the empty value or `1`" admits.
    let html = rt("Escaped \\* star and \\_ rule.\n");
    assert!(html.contains("<span data-carve-escaped>*</span>"), "{html}");
    assert!(html.contains("<span data-carve-escaped>_</span>"), "{html}");
    // ONE ESCAPE PER ELEMENT (ruling question 4): two escapes are two spans.
    assert_eq!(html.matches("data-carve-escaped").count(), 2);
}

#[test]
fn the_abbreviation_definitions_ride_a_template() {
    let html = rt("*[HTML]: HyperText Markup Language\n\nUse HTML here.\n");
    assert!(
        html.contains(
            "<template data-carve-abbreviations>*[HTML]: HyperText Markup Language</template>"
        ),
        "{html}"
    );
    assert!(!rt("No abbreviation here.\n").contains("data-carve-abbreviations"));
}

// ---------------------------------------------------------------- collisions

#[test]
fn a_carrier_wins_a_collision_and_the_authored_attribute_is_reported() {
    // RULED question 1. carve-php emitted BOTH, which is invalid HTML, with the
    // precedence differing per carrier.
    let source = "{data-char=\"_\"}\n***\n";
    let html = rt(source);
    assert_eq!(html.matches("data-char").count(), 1, "{html}");
    assert!(html.contains(r#"<hr data-char="*">"#), "{html}");

    let report = to_html_with_options_and_report(
        source,
        &Options::default().with_roundtrip(true),
        CheckedRenderOptions::default(),
    )
    .expect("a dropped attribute is not an abort");
    let row = report
        .losses
        .iter()
        .find(|loss| loss.code == "carrier-attribute-dropped")
        .unwrap_or_else(|| panic!("{:?}", report.losses));
    assert_eq!(row.attribute.as_deref(), Some("data-char"));
    assert!(row.format.is_none());
    assert!(row.message.contains("data-char"), "{}", row.message);
}

#[test]
fn a_colliding_attribute_is_kept_where_no_carrier_takes_its_name() {
    // The drop is the CARRIER's, not the name's: with the option off, the
    // authored attribute is the author's and stays.
    let html = to_html("{data-char=\"_\"}\n***\n");
    assert!(html.contains(r#"data-char="_""#), "{html}");
}

#[test]
fn an_explicit_id_collision_drops_the_authored_copy_on_the_heading() {
    let html = to_html_with_options(
        "{#chosen data-carve-explicit-id=\"no\"}\n# Explicit\n",
        &Options::default().with_roundtrip(true).with_sections(false),
    );
    assert_eq!(html.matches("data-carve-explicit-id").count(), 1, "{html}");
    assert!(html.contains(r#"data-carve-explicit-id="1""#), "{html}");
}

#[test]
fn a_marker_collision_on_a_list_drops_the_authored_copy() {
    let html = rt("{data-marker=\"#\"}\n* a\n");
    assert_eq!(html.matches("data-marker").count(), 1, "{html}");
    assert!(html.contains(r#"data-marker="*""#), "{html}");
}

// ---------------------------------------------------------------- import

#[test]
fn the_carriers_come_back_through_an_import() {
    let source = "* star\n* second\n\n***\n\nEscaped \\* star and `a<b`{=latex} raw.\n\n\
                  An autolink <https://example.com/a> here.\n";
    assert_eq!(safe(&rt(source)), source);
}

#[test]
fn the_deprecated_prefix_still_reads() {
    // A document exported by a released carve-php carries `data-djot-*`, and
    // §13a's rename rule keeps reading the old spelling.
    let html = r#"<p><span data-djot-raw="latex">a&lt;b</span></p>"#;
    assert_eq!(safe(html), "`a<b`{=latex}\n");
    let autolink = r#"<p><a href="https://example.com/a" data-djot-autolink="1">https://example.com/a</a></p>"#;
    assert_eq!(safe(autolink), "<https://example.com/a>\n");
}

#[test]
fn a_reference_label_reaches_the_tree() {
    let doc = html_to_ast(
        &rt("[t][lbl]\n\n[lbl]: /dest\n"),
        &HtmlImportOptions::default(),
    )
    .expect("the import holds")
    .value;
    let json = carve::to_json(&doc);
    assert!(
        json.contains("\"ref\": \"lbl\"") || json.contains("\"ref\":\"lbl\""),
        "{json}"
    );
}

#[test]
fn a_nested_container_keeps_its_carriers() {
    let html = rt("::: note\n* star\n\n***\n:::\n");
    assert!(html.contains(r#"<ul data-marker="*">"#), "{html}");
    assert!(html.contains(r#"<hr data-char="*">"#), "{html}");
    let round = safe(&html);
    assert!(round.contains("* star"), "{round}");
    assert!(round.contains("***"), "{round}");
}

#[test]
fn every_task_state_survives_the_cycle() {
    // §13a does not carry these - §11 writes `data-task-state` on every render -
    // so this is the control that roundtrip export neither gates nor duplicates
    // a clause of its own.
    let source = "- [ ] open\n- [x] done\n- [-] dropped\n- [_] held\n- [>] moved\n- [?] unsure\n";
    assert_eq!(safe(&rt(source)), source);
    assert_eq!(safe(&to_html(source)), source);
}

// ---------------------------------------------------------------- negative

#[test]
fn a_reserved_name_never_comes_back_as_an_authored_attribute() {
    // RULED question 5. carve-php's `safe` mode returned `<hr data-char="_">`
    // as `{data-char=_}` plus `---`, so the carrier reappeared as content.
    // Only the attribute path, deliberately: an element Carve cannot express at
    // all takes `roundtrip` mode's RAW-HTML preservation, which is that mode's
    // own trust setting (§13b) rather than a reserved name becoming an authored
    // attribute, so each case here is an element the import rewrites.
    for html in [
        r#"<hr data-char="_">"#,
        r#"<p><a href="/u" data-carve-ref="lbl">t</a></p>"#,
        r#"<p><a href="/u" data-djot-ref="lbl">t</a></p>"#,
        r#"<pre data-carve-src="x"><code>y</code></pre>"#,
        r#"<ul data-carve-col-widths="2,2"><li>a</li></ul>"#,
        r#"<ul><li data-carve-inline-footnote="1">x</li></ul>"#,
        r#"<ul><li data-djot-footnote-label="note">x</li></ul>"#,
    ] {
        for mode in [HtmlImportMode::Safe, HtmlImportMode::Roundtrip] {
            let out = back(html, mode);
            assert!(!out.contains("data-carve-"), "{mode:?}: {out}");
            assert!(!out.contains("data-djot-"), "{mode:?}: {out}");
            assert!(!out.contains("data-char"), "{mode:?}: {out}");
        }
    }
}

#[test]
fn a_malformed_carrier_is_not_this_engines_to_reassemble() {
    // A value outside the row's enumeration is not one the clause writes, so a
    // reader does not invent a node from it. Each case reconstructs NOTHING and
    // still never writes the reserved name out.
    for html in [
        // A break character the enumeration does not admit.
        r#"<hr data-char="=">"#,
        // A bullet that is the default, or not a bullet at all.
        r##"<ul data-marker="#"><li>a</li></ul>"##,
        // A multi-character escape, which would reassemble as `\a\b`.
        r#"<p><span data-carve-escaped>ab</span></p>"#,
        // A format name no info string could spell.
        r#"<p><span data-carve-raw="x y">z</span></p>"#,
    ] {
        let out = safe(html);
        assert!(!out.contains("data-carve-"), "{out}");
        assert!(!out.contains("data-char"), "{out}");
        assert!(!out.contains("data-marker"), "{out}");
    }
    assert_eq!(safe(r#"<hr data-char="=">"#), "---\n");
    assert_eq!(safe(r#"<p><span data-carve-escaped>ab</span></p>"#), "ab\n");
}

#[test]
fn contradictory_metadata_does_not_build_two_nodes() {
    // An anchor claiming to be both an autolink and a reference: the autolink
    // is its own NODE TYPE, so it wins the element and the reference label has
    // nowhere to go rather than producing a link beside it.
    let out = safe(r#"<p><a href="/u" data-carve-autolink="1" data-carve-ref="lbl">/u</a></p>"#);
    assert_eq!(out, "</u>\n");
    assert!(!out.contains("lbl"), "{out}");
}

#[test]
fn an_autolink_carrier_without_a_destination_builds_no_autolink() {
    let out = safe(r#"<p><a data-carve-autolink="1">text</a></p>"#);
    assert!(!out.contains("data-carve-"), "{out}");
}

#[test]
fn a_denied_destination_is_still_denied_under_roundtrip_export() {
    // §13b: enabling export never relaxes a destination check.
    let html = rt("[t](javascript:alert(1))\n");
    assert!(!html.contains("javascript:"), "{html}");
}

#[test]
fn export_does_not_select_an_import_mode() {
    // §13b: finding a carrier is not a reason to trust the document. `safe`
    // does not take the `data-carve-src` payload, and it does not keep the
    // raw HTML `roundtrip` preserves either.
    let html = r#"<pre data-carve-src="``` js&#10;x&#10;```"><code class="language-js">x
</code></pre>"#;
    let out = safe(html);
    assert!(!out.contains("data-carve-src"), "{out}");
    assert!(out.contains("```js"), "{out}");
    // The export option lives on the RENDER options and reaches no importer.
    let defaults = HtmlImportOptions::default();
    assert_eq!(defaults.mode, HtmlImportMode::Safe);
}

// ---------------------------------------------------------------- the CLI

mod cli {
    use std::io::Write;
    use std::process::{Command, Stdio};

    fn run(args: &[&str], input: &str) -> (bool, String, String) {
        let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn carve binary");
        if let Err(error) = child
            .stdin
            .take()
            .expect("stdin")
            .write_all(input.as_bytes())
        {
            if error.kind() != std::io::ErrorKind::BrokenPipe {
                panic!("write stdin: {error}");
            }
        }
        let out = child.wait_with_output().expect("wait");
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    #[test]
    fn the_flag_turns_the_html_carriers_on() {
        let (ok, stdout, stderr) = run(&["--html", "--roundtrip"], "***\n");
        assert!(ok, "{stderr}");
        assert!(stdout.contains(r#"<hr data-char="*">"#), "{stdout}");
    }

    #[test]
    fn the_flag_selects_the_markdown_carriers_too() {
        // §13: the TARGET decides the carrier, so one flag reaches both.
        let (ok, with, stderr) = run(&["--markdown", "--roundtrip"], "::: note\nx\n:::\n");
        assert!(ok, "{stderr}");
        assert!(with.contains("<!-- carve: ::: note -->"), "{with}");
        let (_, without, _) = run(&["--markdown"], "::: note\nx\n:::\n");
        assert_ne!(with, without);
    }

    #[test]
    fn the_removed_flag_is_unknown() {
        let flag = "--carry-markers";
        let (ok, stdout, stderr) = run(&["--markdown", flag], "");
        assert!(!ok);
        assert!(stdout.is_empty(), "{stdout}");
        assert!(
            stderr.contains(&format!("carve: unknown option: {flag}")),
            "{stderr}"
        );
    }

    #[test]
    fn a_target_with_no_carrier_refuses_the_flag() {
        let input = "***\n".repeat(65_536);
        for target in ["--plain", "--ansi", "--carve", "--json"] {
            let (ok, _, stderr) = run(&[target, "--roundtrip"], &input);
            assert!(!ok, "{target}");
            assert!(
                stderr.contains("--roundtrip requires"),
                "{target}: {stderr}"
            );
        }
    }

    #[test]
    fn the_flag_is_off_unless_asked_for() {
        let (ok, stdout, stderr) = run(&["--html"], "***\n");
        assert!(ok, "{stderr}");
        assert!(!stdout.contains("data-char"), "{stdout}");
    }
}
