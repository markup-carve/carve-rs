//! CARVE-P11-063, PART 11 §10s: under the opt-in carrier mode the Markdown
//! target brackets every ELEMENT-LESS CONTAINER with an HTML comment carrying
//! its Carve opener and closer verbatim, so an export and a re-import return
//! the container.
//!
//! THE TWO CONTROLS ARE THE POINT: with the mode off the emitted bytes are the
//! ones this target emits today, and a document holding no element-less
//! container gains no comment either way. A carrier that moved the default
//! output would be a breaking change rather than an opt-in mode.
//!
//! An attributed container's attributes live on the line ABOVE, not on the
//! opener: PART 4 is strict that an opener line carries no inline `{...}`
//! attributes, so a conformant engine drops `::: wrapper {.fancy}`'s brace block
//! at parse time and no writer can carry what the tree does not hold. The
//! attribute line takes a marker of its own.
//!
//! Every expectation here is byte-identical to carve-js and carve-php.

use carve::Options;

fn markdown(source: &str, carry: bool) -> String {
    carve::to_markdown_with_options(source, &Options::default().with_carry_markers(carry))
}

/// The set measured on this engine: a tab set and each panel, a code-group set,
/// a named div plain and attributed, a div nested in one, a columns container
/// and each column, a disclosure, a spoiler, a composite figure group wrapper,
/// a typeless div carrying a label, the `-->` escape in a title and in a label,
/// and an admonition of every kind, bare and titled.
fn carried() -> Vec<(String, String, String)> {
    let mut cases: Vec<(String, String, String)> = vec![
        (
            "a tab set and each of its panels".into(),
            "::: tabs\n:::: tab [Overview]\nFirst panel.\n::::\n\n:::: tab [Install]\nSecond panel.\n::::\n:::\n".into(),
            concat!(
                "<!-- carve: ::: tabs -->\n<!-- carve: :::: tab [Overview] -->\n**Overview**\n\nFirst panel.\n\n",
                "<!-- carve: :::: -->\n<!-- carve: :::: tab [Install] -->\n**Install**\n\nSecond panel.\n\n",
                "<!-- carve: :::: -->\n<!-- carve: ::: -->\n",
            )
            .into(),
        ),
        (
            "a code-group set and its panel".into(),
            "::: code-group\n:::: tab [sh]\n```sh\nx\n```\n::::\n:::\n".into(),
            concat!(
                "<!-- carve: ::: code-group -->\n<!-- carve: :::: tab [sh] -->\n**sh**\n\n```sh\nx\n```\n\n",
                "<!-- carve: :::: -->\n<!-- carve: ::: -->\n",
            )
            .into(),
        ),
        (
            "a named div, whose name is what today drops".into(),
            "::: wrapper\nA generic div.\n:::\n".into(),
            "<!-- carve: ::: wrapper -->\nA generic div.\n\n<!-- carve: ::: -->\n".into(),
        ),
        (
            // An attributed container is TWO Carve lines, so it is two markers.
            "a named div carrying attributes".into(),
            "{.fancy #w}\n::: wrapper\nA generic div.\n:::\n".into(),
            "<!-- carve: {.fancy #w} -->\n<!-- carve: ::: wrapper -->\nA generic div.\n\n<!-- carve: ::: -->\n".into(),
        ),
        (
            "a div nested in a named div".into(),
            "::: outer\n:::: inner\nx\n::::\n:::\n".into(),
            "<!-- carve: ::: outer -->\n<!-- carve: :::: inner -->\nx\n\n<!-- carve: :::: -->\n<!-- carve: ::: -->\n".into(),
        ),
        (
            "a columns container and each column".into(),
            "::: columns\n:::: column\nA\n::::\n\n:::: column\nB\n::::\n:::\n".into(),
            concat!(
                "<!-- carve: ::: columns -->\n<!-- carve: :::: column -->\nA\n\n<!-- carve: :::: -->\n",
                "<!-- carve: :::: column -->\nB\n\n<!-- carve: :::: -->\n<!-- carve: ::: -->\n",
            )
            .into(),
        ),
        (
            "a disclosure".into(),
            "::: details \"Open me\"\nHidden.\n:::\n".into(),
            "<!-- carve: ::: details \"Open me\" -->\n**Open me**\n\nHidden.\n\n<!-- carve: ::: -->\n".into(),
        ),
        (
            "a spoiler".into(),
            "::: spoiler\nHidden.\n:::\n".into(),
            "<!-- carve: ::: spoiler -->\nHidden.\n\n<!-- carve: ::: -->\n".into(),
        ),
        (
            "a composite figure group wrapper".into(),
            "::: figure\n![a](a.png)\n:::\n".into(),
            "<!-- carve: ::: figure -->\n![a](a.png)\n\n<!-- carve: ::: -->\n".into(),
        ),
        (
            "a typeless div carrying a label".into(),
            "::: [First]\nx\n:::\n".into(),
            "<!-- carve: ::: [First] -->\n**First**\n\nx\n\n<!-- carve: ::: -->\n".into(),
        ),
        (
            // THE ESCAPE, the only spelling in the payload that is not Carve
            // source read back verbatim. It rides a quoted title, which PART 4
            // admits and which has no escape of its own.
            "a payload carrying the comment terminator".into(),
            "::: note \"a --> b\"\nBody.\n:::\n".into(),
            "<!-- carve: ::: note \"a --\\> b\" -->\n**a \u{2192} b**\n\nBody.\n\n<!-- carve: ::: -->\n".into(),
        ),
        (
            // A backslash already sitting where the escape would put one grows
            // by one, so the transform reverses exactly.
            "a payload already carrying a backslash before that bracket".into(),
            "::: note \"a --\\> b\"\nBody.\n:::\n".into(),
            "<!-- carve: ::: note \"a --\\\\> b\" -->\n**a --\\> b**\n\nBody.\n\n<!-- carve: ::: -->\n".into(),
        ),
        (
            "a label carrying the comment terminator".into(),
            "::: wrapper [a --> b]\nx\n:::\n".into(),
            "<!-- carve: ::: wrapper [a --\\> b] -->\n**a --> b**\n\nx\n\n<!-- carve: ::: -->\n".into(),
        ),
    ];
    for kind in [
        "note", "tip", "warning", "danger", "info", "success", "example", "quote",
    ] {
        cases.push((
            format!("an admonition of kind {kind}"),
            format!("::: {kind}\nAn admonition body.\n:::\n"),
            format!("<!-- carve: ::: {kind} -->\nAn admonition body.\n\n<!-- carve: ::: -->\n"),
        ));
        cases.push((
            format!("an admonition of kind {kind} carrying a title"),
            format!("::: {kind} \"A title\"\nAn admonition body.\n:::\n"),
            format!(
                "<!-- carve: ::: {kind} \"A title\" -->\n**A title**\n\nAn admonition body.\n\n<!-- carve: ::: -->\n"
            ),
        ));
    }

    cases
}

#[test]
fn the_carrier_mode_writes_the_opener_verbatim() {
    let mut wrong = Vec::new();
    for (name, carve, carrier) in carried() {
        let got = markdown(&carve, true);
        if got != carrier {
            wrong.push(format!("{name}\n  want {carrier:?}\n   got {got:?}"));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of the measured containers wrote the wrong bytes:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

#[test]
fn the_carrier_mode_round_trips_the_container() {
    let mut wrong = Vec::new();
    for (name, carve, carrier) in carried() {
        let got = carve::markdown_to_carve(&carrier);
        if got != carve {
            wrong.push(format!("{name}\n  want {carve:?}\n   got {got:?}"));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of the measured containers did not come back:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

#[test]
fn the_mode_off_emits_exactly_the_bytes_this_target_emits_today() {
    for (name, carve, _) in carried() {
        let off = markdown(&carve, false);
        assert!(
            !off.contains("<!-- carve:"),
            "{name}: the mode off wrote a marker: {off:?}"
        );
    }
    assert_eq!(
        markdown(
            "::: tabs\n:::: tab [Overview]\nFirst panel.\n::::\n\n:::: tab [Install]\nSecond panel.\n::::\n:::\n",
            false
        ),
        "**Overview**\n\nFirst panel.\n\n**Install**\n\nSecond panel.\n"
    );
    assert_eq!(
        markdown("::: note\nAn admonition body.\n:::\n", false),
        "An admonition body.\n"
    );
    assert_eq!(
        markdown("::: wrapper\nA generic div.\n:::\n", false),
        "A generic div.\n"
    );
}

#[test]
fn a_document_with_no_element_less_container_gains_no_comment_either_way() {
    let plain = "# Head\n\nA paragraph with *bold* text.\n\n- one\n- two\n";
    let expected = "# Head\n\nA paragraph with **bold** text.\n\n- one\n- two\n";
    for carry in [false, true] {
        let out = markdown(plain, carry);
        assert!(
            !out.contains("<!-- carve:"),
            "a document with nothing to carry gained a marker (carry={carry}): {out:?}"
        );
        assert_eq!(out, expected, "carry={carry}");
    }
}

/// A container this target DOES spell is not element-less and takes no marker:
/// an attributes-only opener is a paragraph, and a list table is a pipe table.
#[test]
fn a_container_this_target_spells_takes_no_marker() {
    for source in [
        "::: {.warning}\nx\n:::\n",
        "{header-rows=1}\n::: list-table\n- - A\n  - B\n- - one\n  - x\n:::\n",
    ] {
        let out = markdown(source, true);
        assert!(
            !out.contains("<!-- carve:"),
            "{source:?} took a marker: {out:?}"
        );
    }
}

/// A host that prefixes its lines takes no marker yet: the comment would sit at
/// the host's content column or behind its `>`, where the import does not read
/// it, so it would be written and never read back. The container degrades there
/// exactly as it does with the mode off (markup-carve/carve#2850).
#[test]
fn a_container_in_a_prefixed_host_takes_no_marker() {
    for source in [
        "- item\n\n  ::: note\n  Body.\n  :::\n",
        "> ::: note\n> Body.\n> :::\n",
    ] {
        let on = markdown(source, true);
        assert!(
            !on.contains("<!-- carve:"),
            "{source:?} took a marker: {on:?}"
        );
        assert_eq!(on, markdown(source, false), "{source:?}");
        // And no marker written means no damage claimed on the way back.
        let report = carve::migrate_markdown(&on);
        assert!(
            !report
                .report
                .diagnostics
                .iter()
                .any(|row| row.code == "carrier-markers-damaged"),
            "{source:?} claimed damage: {:?}",
            report.report.diagnostics
        );
    }
}

/// A MARKER INSIDE A FENCED CODE BLOCK IS NOT A MARKER. A code block's payload
/// is verbatim content, so a page documenting the mode holds marker-shaped lines
/// that record no container; lifting one rewrites the sample inside the fence.
#[test]
fn a_marker_shaped_line_inside_a_fenced_code_block_is_left_alone() {
    let source =
        "Text.\n\n```md\n<!-- carve: ::: note -->\nBody.\n<!-- carve: ::: -->\n```\n\nTail.\n";
    let result = carve::migrate_markdown(source);
    assert_eq!(result.value, source);
    assert!(
        !result
            .report
            .diagnostics
            .iter()
            .any(|row| row.code == "carrier-markers-damaged"),
        "a fenced sample was read as a damaged set: {:?}",
        result.report.diagnostics
    );
}

/// A DAMAGED SET IS NEVER GUESSED: a marker deleted, two reordered or a set left
/// unbalanced imports as ordinary Markdown with exactly one diagnostic.
#[test]
fn a_damaged_set_imports_as_plain_markdown_plus_one_diagnostic() {
    for source in [
        "Body.\n<!-- carve: ::: -->\n",
        "<!-- carve: ::: -->\nBody.\n<!-- carve: ::: note -->\n",
        "<!-- carve: ::: note -->\n<!-- carve: ::: wrapper -->\nBody.\n<!-- carve: ::: -->\n",
    ] {
        let result = carve::migrate_markdown(source);
        let damaged: Vec<_> = result
            .report
            .diagnostics
            .iter()
            .filter(|row| row.code == "carrier-markers-damaged")
            .collect();
        assert_eq!(
            damaged.len(),
            1,
            "{source:?} owes exactly one diagnostic: {:?}",
            result.report.diagnostics
        );
        assert_eq!(damaged[0].fidelity.as_str(), "degraded", "{source:?}");
        assert_eq!(damaged[0].confidence.as_str(), "fallback", "{source:?}");
        // Never a guess: no container is reconstructed, and the markers come
        // back as the raw HTML they are.
        assert!(
            !result.value.lines().any(|line| line.starts_with(":::")),
            "{source:?} reconstructed a container: {:?}",
            result.value
        );
        assert!(
            result.value.contains("```=html"),
            "{source:?} lost the markers instead of keeping them as raw HTML: {:?}",
            result.value
        );
    }
}

#[test]
fn a_sound_set_reports_no_damage() {
    let result =
        carve::migrate_markdown("<!-- carve: ::: note -->\nBody.\n\n<!-- carve: ::: -->\n");
    assert!(
        !result
            .report
            .diagnostics
            .iter()
            .any(|row| row.code == "carrier-markers-damaged"),
        "{:?}",
        result.report.diagnostics
    );
    assert_eq!(result.value, "::: note\nBody.\n:::\n");
}
