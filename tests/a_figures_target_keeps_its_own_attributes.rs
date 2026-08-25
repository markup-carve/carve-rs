//! A rebuilt figure writes its TARGET's own attribute line, and whatever the
//! merge displaces is declared (ruling markup-carve/carve#1721).
//!
//! THE ID THAT SURVIVED BELONGED TO THE ELEMENT THAT DID NOT. A
//! `<figure id="f">` around a `<table id="g">` was written as `{#f}` over the
//! rows, so the table's own identity was gone from the source, from the
//! re-render and from the report at once - and the only row present described
//! the figure being unspellable, which is a different fact. An id is a link
//! target, so every anchor pointing at that table broke while the document
//! rendered perfectly.
//!
//! BOTH HALVES OR NEITHER. Writing the target's line and forgetting the row
//! would pass an assertion on the emitted Carve alone, so every case below
//! asserts the diagnostics too: the figure's `#f` is displaced by the merge and
//! `attribute-dropped` is what says so. Never resolving a collision by dropping
//! one side in silence is the whole ruling, and a test that watched one side
//! could not see half of it.
//!
//! AND THE RE-RENDER IS THE PROOF THAT IT WORKED. `#g` resolving after a round
//! trip is the property an anchor depends on; a string comparison on the source
//! says only that the bytes moved.
//!
//! THE MERGE IS NOT SYMMETRIC, so the cases separate the three names it treats
//! differently: `id` is a single slot the last line wins, a key-value pair is
//! that slot rule under a name, and `classes` is a set the two lines UNION - so
//! a class is never displaced and never owed a row.
//!
//! WHERE THE MERGED SET LANDS DIFFERS BY ARM, and the ruling is about the VALUE
//! that wins rather than the element it ends up on. A table re-reads as
//! `<table id="g"><caption>`, so the pair sits on the table itself; a quote or a
//! fence re-reads as a figure around them, so it sits on that figure. Both are
//! pinned below, because a message claiming the target ELEMENT keeps its id
//! would be false on two of the three arms.
//!
//! EQUAL VALUES STILL LOSE ONE OF THE TWO, which is why the row names the
//! COLLISION rather than a side. `<figure id="x"><blockquote id="x">` comes back
//! as `<figure id="x"><blockquote>`: the value survives and the target's own
//! attribute does not. Suppressing the row when the values match reads like the
//! obvious simplification and would turn that case into the silent drop this
//! ruling exists to remove, so a case below pins it firing.
//!
//! THE IMAGE ARM IS THE CONTROL. An image writes its attributes inline, after
//! the destination, so the figure's line and the image's braces never meet.
//! Nothing about that arm changes, and the case that says so fails if a later
//! pass makes the collision rule sweep an arm that has no collision.
//!
//! THE `structure-unspellable` SENTENCE MOVED WITH THE FIX. It used to say the
//! written table "carries the caption and the figure's attributes", which this
//! ruling makes false: the table carries its OWN. The text asserted below is
//! the one carve-js and carve-php already share.

use carve::{html_to_carve, render_html, HtmlImportMode, HtmlImportOptions, HtmlImportSeverity};

const MODES: [HtmlImportMode; 3] = [
    HtmlImportMode::Safe,
    HtmlImportMode::Semantic,
    HtmlImportMode::Roundtrip,
];

const DISPLACED_ID: &str = "Info :: attribute-dropped :: Dropped one id on <figure>: the figure and its target both set id, and their two attribute lines merge into a single value";
const UNSPELLABLE: &str = "Warning :: structure-unspellable :: A figure wrapping a table has no Carve spelling; the caption is written on the table, which renders <caption> inside it";

const TABLE: &str = "<figure id=\"f\" class=\"c\"><table id=\"g\" class=\"d\"><tr><td>a</td></tr></table><figcaption>Cap</figcaption></figure>";

fn imported(html: &str, mode: HtmlImportMode) -> (String, Vec<String>) {
    let options = HtmlImportOptions {
        mode,
        ..Default::default()
    };
    let result = html_to_carve(html, &options).expect("imports");
    let rows = result
        .report
        .diagnostics
        .iter()
        .map(|d| {
            let severity = match d.severity {
                HtmlImportSeverity::Info => "Info",
                HtmlImportSeverity::Warning => "Warning",
                HtmlImportSeverity::Error => "Error",
            };
            format!("{severity} :: {} :: {}", d.code.as_str(), d.message)
        })
        .collect();
    (result.value, rows)
}

fn html_of(carve: &str) -> String {
    render_html(&carve::parse(carve)).expect("renders")
}

/// The case the ruling is about. Before the fix this emitted
/// `{#f .c}\n| a |\n^ Cap\n` with `structure-unspellable` and nothing else: the
/// authored `id="g"` was gone and no row named it.
#[test]
fn a_table_target_keeps_its_own_attribute_line_and_the_displaced_id_is_declared() {
    for mode in MODES {
        let (carve, rows) = imported(TABLE, mode);
        assert_eq!(carve, "{#f .c}\n{#g .d}\n| a |\n^ Cap\n", "mode {mode:?}");
        assert_eq!(rows, vec![UNSPELLABLE, DISPLACED_ID], "mode {mode:?}");
    }
}

/// The property, not the bytes: an anchor pointing at `#g` has to resolve after
/// the round trip, and the id the wrapper carried is the one that goes.
#[test]
fn the_re_render_holds_the_tables_own_id_and_not_the_figures() {
    for mode in MODES {
        let (carve, _) = imported(TABLE, mode);
        let html = html_of(&carve);
        assert!(html.contains("id=\"g\""), "mode {mode:?}: {html}");
        assert!(!html.contains("id=\"f\""), "mode {mode:?}: {html}");
        // The classes UNION rather than displacing, so both stay on the table.
        assert!(html.contains("class=\"c d\""), "mode {mode:?}: {html}");
    }
}

/// A quote and a fence stack two block lines the same way the table does, so
/// the same collision reaches them. Both already wrote the target's line; what
/// they did not do was declare the id the merge displaces.
#[test]
fn a_quote_target_declares_the_displaced_id() {
    for mode in MODES {
        let (carve, rows) = imported(
            "<figure id=\"f\" class=\"c\"><blockquote id=\"g\" class=\"d\"><p>a</p></blockquote><figcaption>Cap</figcaption></figure>",
            mode,
        );
        assert_eq!(carve, "{#f .c}\n{#g .d}\n> a\n^ Cap\n", "mode {mode:?}");
        assert_eq!(rows, vec![DISPLACED_ID], "mode {mode:?}");
    }
}

/// Where the merged set lands, pinned so the row's wording stays honest. A quote
/// target re-reads as a figure holding the merged pair, not as a quote holding
/// it - the surviving VALUE is the target's either way, which is what the row
/// says and all it says.
#[test]
fn the_merged_pair_lands_on_the_rebuilt_figure_for_a_quote_target() {
    for mode in MODES {
        let (carve, _) = imported(
            "<figure id=\"f\" class=\"c\"><blockquote id=\"g\" class=\"d\"><p>a</p></blockquote><figcaption>Cap</figcaption></figure>",
            mode,
        );
        let html = html_of(&carve);
        assert!(
            html.contains("<figure id=\"g\" class=\"c d\">"),
            "mode {mode:?}: {html}"
        );
        assert!(html.contains("<blockquote>"), "mode {mode:?}: {html}");
        assert!(!html.contains("id=\"f\""), "mode {mode:?}: {html}");
    }
}

#[test]
fn a_code_block_target_declares_the_displaced_id() {
    for mode in MODES {
        let (carve, rows) = imported(
            "<figure id=\"f\" class=\"c\"><pre id=\"g\" class=\"d\"><code>a</code></pre><figcaption>Cap</figcaption></figure>",
            mode,
        );
        assert_eq!(
            carve, "{#f .c}\n{#g .d}\n```\na\n```\n^ Cap\n",
            "mode {mode:?}"
        );
        assert_eq!(rows, vec![DISPLACED_ID], "mode {mode:?}");
    }
}

/// A key-value pair takes the slot rule under its own name, so a figure and a
/// target setting the same key displaces it.
#[test]
fn a_displaced_key_value_pair_is_declared_by_its_own_name() {
    for mode in MODES {
        let (carve, rows) = imported(
            "<figure data-k=\"1\"><blockquote data-k=\"2\"><p>a</p></blockquote><figcaption>Cap</figcaption></figure>",
            mode,
        );
        assert_eq!(
            carve, "{data-k=1}\n{data-k=2}\n> a\n^ Cap\n",
            "mode {mode:?}"
        );
        assert_eq!(
            rows,
            vec!["Info :: attribute-dropped :: Dropped one data-k on <figure>: the figure and its target both set data-k, and their two attribute lines merge into a single value"],
            "mode {mode:?}"
        );
        assert!(html_of(&carve).contains("data-k=\"2\""), "mode {mode:?}");
    }
}

/// And a key the target does not set is not displaced, so no row is owed.
#[test]
fn a_key_the_target_does_not_set_is_not_displaced() {
    for mode in MODES {
        let (carve, rows) = imported(
            "<figure data-k=\"1\"><blockquote data-j=\"2\"><p>a</p></blockquote><figcaption>Cap</figcaption></figure>",
            mode,
        );
        assert_eq!(
            carve, "{data-k=1}\n{data-j=2}\n> a\n^ Cap\n",
            "mode {mode:?}"
        );
        assert!(rows.is_empty(), "mode {mode:?}: {rows:?}");
    }
}

/// Two attributes of the same name with the SAME value still merge into one, so
/// one of the two elements comes out without it and a row is owed. Measured:
/// `<figure id="x"><blockquote id="x">` re-renders as
/// `<figure id="x"><blockquote>`, so it is the TARGET's that is gone.
#[test]
fn equal_values_still_lose_one_of_the_two_and_the_row_fires() {
    for mode in MODES {
        let (carve, rows) = imported(
            "<figure id=\"x\"><blockquote id=\"x\"><p>a</p></blockquote><figcaption>Cap</figcaption></figure>",
            mode,
        );
        assert_eq!(carve, "{#x}\n{#x}\n> a\n^ Cap\n", "mode {mode:?}");
        assert_eq!(rows, vec![DISPLACED_ID], "mode {mode:?}");
        let html = html_of(&carve);
        assert!(html.contains("<figure id=\"x\">"), "mode {mode:?}: {html}");
        assert!(html.contains("<blockquote>"), "mode {mode:?}: {html}");
    }
}

/// Classes union, so neither side is displaced and neither is owed a row.
/// Reporting one would name a class the output still carries.
#[test]
fn classes_union_and_owe_no_row() {
    for mode in MODES {
        let (carve, rows) = imported(
            "<figure class=\"c\"><table class=\"d\"><tr><td>a</td></tr></table><figcaption>Cap</figcaption></figure>",
            mode,
        );
        assert_eq!(carve, "{.c}\n{.d}\n| a |\n^ Cap\n", "mode {mode:?}");
        assert_eq!(rows, vec![UNSPELLABLE], "mode {mode:?}");
    }
}

/// A target whose wrapper carries nothing is not a collision either, and its
/// attributes were dropped just as silently before the fix.
#[test]
fn a_target_keeps_its_attributes_when_the_figure_carries_none() {
    for mode in MODES {
        let (carve, rows) = imported(
            "<figure><table id=\"g\" class=\"d\"><tr><td>a</td></tr></table><figcaption>Cap</figcaption></figure>",
            mode,
        );
        assert_eq!(carve, "{#g .d}\n| a |\n^ Cap\n", "mode {mode:?}");
        assert_eq!(rows, vec![UNSPELLABLE], "mode {mode:?}");
    }
}

/// THE CONTROL. An image writes its attributes inline, so the two never meet,
/// both ids survive and no row is owed. This passed before the change and has
/// to keep passing after it.
#[test]
fn the_image_arm_where_the_attributes_never_meet_is_unchanged() {
    for mode in MODES {
        let (carve, rows) = imported(
            "<figure id=\"f\" class=\"c\"><img id=\"g\" class=\"d\" src=\"a.png\" alt=\"A\"><figcaption>Cap</figcaption></figure>",
            mode,
        );
        assert_eq!(
            carve, "{#f .c}\n![A](a.png){#g .d}\n^ Cap\n",
            "mode {mode:?}"
        );
        assert!(rows.is_empty(), "mode {mode:?}: {rows:?}");
        let html = html_of(&carve);
        assert!(html.contains("id=\"f\""), "mode {mode:?}: {html}");
        assert!(html.contains("id=\"g\""), "mode {mode:?}: {html}");
    }
}
