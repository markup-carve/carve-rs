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

/// A HOST THAT PREFIXES ITS LINES CARRIES. The marker stands at the host's
/// content column or behind its `>`, and the import finds it there because
/// WHICH LINES ARE MARKERS NOW COMES FROM A PARSE: `pulldown_cmark` reports a
/// marker that is block content as an HTML block whatever prefix its line
/// carries, and one inside a code construct as code text
/// (markup-carve/carve#2850).
///
/// Every shape here is byte-exact and HTML-equal to its source. The two-level
/// and quote-in-item sources are spelled TIGHT on purpose: a blank line between
/// an outer item and its nested list is lost by this target whether a container
/// is involved or not, so a loose spelling would measure that instead of this.
fn prefixed_hosts() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        (
            "a container in a list item",
            "- Item.\n\n  ::: note\n  Body.\n  :::\n",
            "- Item.\n\n  <!-- carve: ::: note -->\n  Body.\n\n  <!-- carve: ::: -->\n",
        ),
        (
            "a container two list levels in",
            "- Outer.\n  - Inner.\n\n    ::: note\n    Body.\n    :::\n",
            "- Outer.\n  - Inner.\n\n    <!-- carve: ::: note -->\n    Body.\n\n    <!-- carve: ::: -->\n",
        ),
        (
            "a container in a block quote",
            "> ::: note\n> Body.\n> :::\n",
            "> <!-- carve: ::: note -->\n> Body.\n>\n> <!-- carve: ::: -->\n",
        ),
        (
            "a container in a block quote in a list item",
            "- Item.\n  > ::: note\n  > Body.\n  > :::\n",
            "- Item.\n  > <!-- carve: ::: note -->\n  > Body.\n  >\n  > <!-- carve: ::: -->\n",
        ),
        (
            // THE OPENER SHARES THE ITEM'S MARKER LINE here, so the placeholder
            // the import lifts the marker to stands behind a `-`. Leaving the
            // token in the output would be corruption rather than a missed
            // restore, which is why the prefix is read as whatever precedes the
            // token and not as a character class.
            "a container opening a list item",
            "- ::: note\n  Body.\n  :::\n",
            "- <!-- carve: ::: note -->\n  Body.\n\n  <!-- carve: ::: -->\n",
        ),
        (
            // AND ITS BODY BEGINS WITH A LIST, so the closer's placeholder is a
            // lazy continuation of that inner item's paragraph and the written
            // Carve puts it at the inner content column. A closer stands at its
            // OPENER's column, which is what brings it back.
            "a container opening a list item, holding a list",
            "- ::: note\n  - one\n  - two\n  :::\n",
            "- <!-- carve: ::: note -->\n  - one\n  - two\n  <!-- carve: ::: -->\n",
        ),
        (
            "an empty container opening a list item",
            "- ::: note\n  :::\n",
            "- <!-- carve: ::: note -->\n  <!-- carve: ::: -->\n",
        ),
        (
            // TWO ITEMS OPEN ON ONE LINE here. The parse takes every container
            // prefix off on its own, which is the whole reason the reading
            // moved there: a line scan would have had to count them by hand.
            "a container opening two list items at once",
            "- - ::: note\n    Body.\n    :::\n",
            "- - <!-- carve: ::: note -->\n    Body.\n\n    <!-- carve: ::: -->\n",
        ),
        (
            // A SIBLING ITEM AFTER THE CLOSER MUST NOT GO LOOSE. A closer and
            // what follows take a blank line between them where they are
            // siblings; the item below belongs to the host above the container,
            // and a blank there would wrap `next` in a `<p>`.
            "a container opening a list item, with a sibling item after it",
            "- ::: note\n  - one\n  - two\n  :::\n- next\n",
            "- <!-- carve: ::: note -->\n  - one\n  - two\n  <!-- carve: ::: -->\n- next\n",
        ),
    ]
}

/// A CONTAINER OPENING A TASK ITEM comes back too, and it is the one host whose
/// marker is not an HTML BLOCK: a task marker's `[ ] ` is inline content, so an
/// HTML block cannot begin after it and the parse reports the opener as an
/// inline span. It is admitted because it IS the whole line, host prefix aside,
/// which is what keeps a marker-shaped span inside running text out.
///
/// NOT byte-exact, and not because of the carrier mode: this target's import
/// drops the checkbox off a task item with a LOOSE body whether a container is
/// involved or not - `- [ ] XYZ\n  Body.\n\n  TAIL\n` comes back as `- XYZ`
/// on an unchanged build. So the container is what is asserted here.
#[test]
fn a_container_opening_a_task_item_comes_back() {
    let source = "- [ ] ::: note\n  Body.\n  :::\n";
    let carrier = markdown(source, true);
    assert_eq!(
        carrier,
        "- [ ] <!-- carve: ::: note -->\n  Body.\n\n  <!-- carve: ::: -->\n"
    );
    let result = carve::migrate_markdown(&carrier);
    assert_eq!(result.value, "- ::: note\n  Body.\n  :::\n");
    assert!(
        !result.value.contains("=html") && !result.value.contains("<!-- carve:"),
        "the container did not come back: {:?}",
        result.value
    );
    assert!(
        !result
            .report
            .diagnostics
            .iter()
            .any(|row| row.code == "carrier-markers-damaged"),
        "a sound set in a task item was called damaged: {:?}",
        result.report.diagnostics
    );
}

#[test]
fn a_container_in_a_prefixed_host_takes_a_marker_at_its_hosts_column() {
    let mut wrong = Vec::new();
    for (name, carve, carrier) in prefixed_hosts() {
        let got = markdown(carve, true);
        if got != carrier {
            wrong.push(format!("{name}\n  want {carrier:?}\n   got {got:?}"));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of the prefixed hosts wrote the wrong bytes:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

#[test]
fn a_container_in_a_prefixed_host_comes_back() {
    let mut wrong = Vec::new();
    for (name, carve, carrier) in prefixed_hosts() {
        let got = carve::markdown_to_carve(carrier);
        if got != carve {
            wrong.push(format!("{name}\n  want {carve:?}\n   got {got:?}"));
        }
        assert!(
            !got.contains("CARVECARRIER"),
            "{name}: the placeholder reached the output: {got:?}"
        );
    }
    assert!(
        wrong.is_empty(),
        "{} of the prefixed hosts did not come back:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

/// The bytes are the point, but so is the MEANING: a round trip that returns
/// the same text under a different element passes a byte comparison.
#[test]
fn a_prefixed_hosts_round_trip_renders_the_same_html() {
    let html = |source: &str| carve::to_html(source);
    for (name, carve, carrier) in prefixed_hosts() {
        assert_eq!(
            html(carve),
            html(&carve::markdown_to_carve(carrier)),
            "{name}"
        );
    }
}

/// And a damaged set inside a prefixed host is still never guessed at.
#[test]
fn a_damaged_set_inside_a_prefixed_host_reports_and_reconstructs_nothing() {
    for source in [
        "- Item.\n\n  Body.\n\n  <!-- carve: ::: -->\n",
        "- Item.\n\n  <!-- carve: ::: -->\n  Body.\n\n  <!-- carve: ::: note -->\n",
        "> <!-- carve: ::: note -->\n> <!-- carve: ::: wrapper -->\n> Body.\n>\n> <!-- carve: ::: -->\n",
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
        assert!(
            !result
                .value
                .lines()
                .any(|line| line.trim_start_matches([' ', '>']).starts_with(":::")),
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

/// A TABLE CELL STILL CARRIES NOTHING, and needs no guard to: this target
/// flattens a cell to one line and the container's body with it, so there is no
/// line for a marker to stand on (markup-carve/carve#2856).
#[test]
fn a_container_in_a_table_cell_takes_no_marker_either_way() {
    let source = concat!(
        "{header-rows=1}\n::: list-table\n- - A\n  - B\n",
        "- - cell one\n  - ::: note\n    Body.\n    :::\n:::\n",
    );
    let on = markdown(source, true);
    assert!(!on.contains("<!-- carve:"), "{on:?}");
    assert_eq!(on, markdown(source, false));
    assert!(on.contains("| cell one | Body. |"), "{on:?}");
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

/// AND A VERBATIM RUN AT A PREFIXED COLUMN IS STILL VERBATIM. Reading a marker
/// through a host prefix is what made these reachable: a flat scan cannot tell
/// a marker at a list item's content column from code text in that item, and
/// four of these five sit at a column such a scan would have admitted. The
/// parse answers all five without a guard of its own.
#[test]
fn a_marker_shaped_line_in_a_verbatim_run_is_left_alone_at_any_column() {
    for source in [
        // An indented code block at top level.
        "Text.\n\n    <!-- carve: ::: note -->\n    Body.\n    <!-- carve: ::: -->\n",
        // A fence at a list item's content column.
        "- Item.\n\n  ```md\n  <!-- carve: ::: note -->\n  Body.\n  <!-- carve: ::: -->\n  ```\n",
        // An indented code block inside a list item.
        "- Item.\n\n      <!-- carve: ::: note -->\n      Body.\n      <!-- carve: ::: -->\n",
        // A fence indented past three columns, which is code text, not a fence.
        "Text.\n\n     ```md\n     <!-- carve: ::: note -->\n     ```\n",
        // An inline code span, which is not a line of its own at all.
        "Text with `<!-- carve: ::: note -->` in it.\n",
    ] {
        let result = carve::migrate_markdown(source);
        assert!(
            !result
                .value
                .lines()
                .any(|line| line.trim_start_matches([' ', '>']).starts_with(":::")),
            "{source:?} fabricated a container: {:?}",
            result.value
        );
        assert!(
            result.value.contains("<!-- carve: ::: note -->"),
            "{source:?} lost the verbatim marker text: {:?}",
            result.value
        );
        assert!(
            !result
                .report
                .diagnostics
                .iter()
                .any(|row| row.code == "carrier-markers-damaged"),
            "{source:?} was read as a damaged set: {:?}",
            result.report.diagnostics
        );
    }
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
