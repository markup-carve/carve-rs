//! Extensions §13.3 and §13.5, ruled on markup-carve/carve-php#1537 and stated
//! in the spec by markup-carve/carve#1504. Both rules bind BOTH constructs, so
//! every case here runs against Tabs and CodeGroup alike: §13 exists to stop
//! the two renderers drifting, and a rule tested on one of them is a rule that
//! can.
//!
//! §13.3 - the generated control is `type="button"`. A `<button>` with no
//! `type` is a SUBMIT button, so a tab set inside a `<form>` submitted the form
//! when a tab was activated, instead of switching panels.
//!
//! §13.5 - exactly one item is selected: the first one the document marks
//! `{selected}`, and the first item where it marks none. Later marks are
//! IGNORED, and over-specifying is not an error - no diagnostic, because §13
//! has no diagnostic channel and the document is redundant rather than wrong.
//!
//! This engine behaved exactly as carve-php and carve-js did on both halves
//! (measured against `1149591` before the fix), so neither was a carve-rs
//! divergence; the ports are markup-carve/carve-php#1550 and
//! markup-carve/carve-js#1287.

use carve::{CodeGroup, CodeGroupOptions, Options, Tabs, TabsMode, TabsOptions};

/// Marks the SECOND and THIRD items, never the first.
///
/// That is the whole design of the fixture. Marking the first as well would
/// make first-wins indistinguishable from the default-the-first branch, and a
/// document where the last mark is also the winner cannot tell first-wins from
/// last-wins. Only a MIDDLE winner separates the ruling from both of the rules
/// it was chosen over.
///
/// It is corpus case 48/49's document byte for byte - one document, two modes.
const TABS_TWO_MARKED: &str = ":::: tabs
::: tab [First]
Content one.
:::

{selected}
::: tab [Second]
Content two.
:::

{selected}
::: tab [Third]
Content three.
:::
::::
";

const CODE_GROUP_TWO_MARKED: &str = "::: code-group
``` js [Node]
console.log(1)
```

{selected}
``` python [Py]
print(1)
```

{selected}
``` ruby [Rb]
puts 1
```
:::
";

const TABS_UNMARKED: &str = ":::: tabs
::: tab [First]
Content one.
:::

::: tab [Second]
Content two.
:::
::::
";

const CODE_GROUP_UNMARKED: &str = "::: code-group
``` js [Node]
console.log(1)
```

``` python [Py]
print(1)
```
:::
";

fn tabs_html(source: &str, mode: TabsMode) -> String {
    let ext = Tabs::with_options(TabsOptions {
        mode,
        ..TabsOptions::default()
    });
    carve::to_html_with_options(source, &Options::new().with_extension(&ext))
}

fn code_group_html(source: &str, mode: TabsMode) -> String {
    let ext = CodeGroup::with_options(CodeGroupOptions {
        mode,
        ..CodeGroupOptions::default()
    });
    carve::to_html_with_options(source, &Options::new().with_extension(&ext))
}

/// One renderer per row: the label, the document, the render, and the id
/// prefix the set generates. Every §13 case below walks BOTH rows.
fn both_constructs(
    tabs_source: &'static str,
    code_group_source: &'static str,
    mode: TabsMode,
) -> Vec<(&'static str, String, &'static str)> {
    vec![
        ("tabs", tabs_html(tabs_source, mode), "tabset-1"),
        (
            "code group",
            code_group_html(code_group_source, mode),
            "codegroup-1",
        ),
    ]
}

/// §13.3: EVERY generated control says `type="button"`.
///
/// Asserted as an absence too. The positive alone passes an engine that writes
/// the attribute on the selected control and leaves the rest bare, which is the
/// shape a "fix the example in the docs" change produces.
#[test]
fn every_aria_control_is_a_type_button() {
    for (name, out, _) in both_constructs(TABS_UNMARKED, CODE_GROUP_UNMARKED, TabsMode::Aria) {
        assert_eq!(
            out.matches("<button type=\"button\" role=\"tab\"").count(),
            2,
            "{name}: {out}"
        );
        assert!(
            !out.contains("<button role=\"tab\""),
            "{name} left a control without the type: {out}"
        );
    }
}

/// The `css` mode has no button to fix, and gains none.
///
/// Its control is an `<input type="radio">`, which already says what it is.
/// Without this the rule could be read as "tab sets emit buttons now".
#[test]
fn the_css_mode_still_emits_no_button_at_all() {
    for (name, out, _) in both_constructs(TABS_UNMARKED, CODE_GROUP_UNMARKED, TabsMode::Css) {
        assert!(!out.contains("<button"), "{name}: {out}");
        assert_eq!(out.matches("type=\"radio\"").count(), 2, "{name}: {out}");
    }
}

/// §13.5 in `aria` mode: the FIRST mark wins, in both constructs.
///
/// The count is the assertion that fails today: two marks gave two
/// `aria-selected="true"` tabs, a shape a single-select `tablist` has no state
/// for. The `tabindex` half goes with it - a tab that is not selected is out of
/// the tab order, so an unfixed engine also left two normal tab stops in the
/// set.
#[test]
fn the_first_mark_wins_in_aria_mode() {
    for (name, out, set) in both_constructs(TABS_TWO_MARKED, CODE_GROUP_TWO_MARKED, TabsMode::Aria)
    {
        assert_eq!(
            out.matches("aria-selected=\"true\"").count(),
            1,
            "{name}: {out}"
        );
        assert_eq!(
            out.matches("aria-selected=\"false\"").count(),
            2,
            "{name}: {out}"
        );
        assert_eq!(out.matches("tabindex=\"-1\"").count(), 2, "{name}: {out}");

        // The winner is the SECOND item: not the first, which is what the
        // default would have chosen, and not the third, which is what last-wins
        // would have.
        assert!(
            out.contains(&format!("id=\"{set}-tab-2\" aria-selected=\"true\"")),
            "{name}: {out}"
        );
        assert!(
            out.contains(&format!("id=\"{set}-tab-1\" aria-selected=\"false\"")),
            "{name}: {out}"
        );
        assert!(
            out.contains(&format!("id=\"{set}-tab-3\" aria-selected=\"false\"")),
            "{name}: {out}"
        );

        // ...and the reveal follows the selection: two panels hidden, one not.
        assert_eq!(out.matches(" hidden>").count(), 2, "{name}: {out}");
    }
}

/// §13.5 in `css` mode, on the SAME document, selecting the SAME item.
///
/// This is the half that makes the ruling a ruling. A radio group cannot have
/// two checked members - the browser resolves it to one whatever the markup
/// says - so `css` never rendered the over-specified document differently, and
/// first-wins was chosen because it is what the `css` default already does with
/// `checked`. If the two modes could disagree about which tab opens, there
/// would be no reason to prefer it.
#[test]
fn the_first_mark_wins_in_css_mode_too() {
    for (name, out, set) in both_constructs(TABS_TWO_MARKED, CODE_GROUP_TWO_MARKED, TabsMode::Css) {
        assert_eq!(out.matches(" checked>").count(), 1, "{name}: {out}");
        let checked = out
            .lines()
            .find(|line| line.contains(" checked>"))
            .unwrap_or_default();
        assert!(
            checked.contains(&format!("id=\"{set}-tab-2\"")),
            "{name} checked the wrong control: {out}"
        );
    }
}

/// Marking NOTHING still opens the first item, in both modes and both
/// constructs.
///
/// The default branch and the first-wins branch are ONE statement now, so this
/// is the case that would break if the collapse were written as "drop every
/// mark after the first" without the fallback.
#[test]
fn an_unmarked_set_still_opens_its_first_item() {
    let cases: Vec<(&str, String, &str)> = vec![
        (
            "tabs aria",
            tabs_html(TABS_UNMARKED, TabsMode::Aria),
            "aria-selected=\"true\"",
        ),
        (
            "tabs css",
            tabs_html(TABS_UNMARKED, TabsMode::Css),
            " checked>",
        ),
        (
            "code group aria",
            code_group_html(CODE_GROUP_UNMARKED, TabsMode::Aria),
            "aria-selected=\"true\"",
        ),
        (
            "code group css",
            code_group_html(CODE_GROUP_UNMARKED, TabsMode::Css),
            " checked>",
        ),
    ];

    for (name, out, needle) in cases {
        assert_eq!(out.matches(needle).count(), 1, "{name}: {out}");
        // The FIRST control carries it: the marker appears before the second
        // control's id does.
        assert!(
            out.find(needle) < out.find("-tab-2\""),
            "{name} opened something other than the first item: {out}"
        );
    }
}

/// Over-specifying is NOT an error: no panic, no diagnostic, no marker in the
/// output. §13 has no diagnostic channel and the document is redundant, not
/// wrong. The ignored item renders like any other.
#[test]
fn over_specifying_is_not_diagnosed() {
    for (name, out, set) in both_constructs(TABS_TWO_MARKED, CODE_GROUP_TWO_MARKED, TabsMode::Aria)
    {
        assert!(
            out.contains(&format!("id=\"{set}-tab-3\"")),
            "{name}: {out}"
        );
        assert!(!out.contains("data-error"), "{name}: {out}");
        assert!(!out.contains("carve-error"), "{name}: {out}");
        assert!(
            out.contains("Content three.") || out.contains("puts 1"),
            "{name}: {out}"
        );
    }
}

/// Corpus cases 48 and 49, byte for byte.
///
/// AHEAD OF THE PINNED CORPUS, deliberately, so the bytes are inlined.
///
/// `48-tabs-aria-single-selection` and `49-tabs-css-single-selection` land with
/// markup-carve/carve#1504, and the spec submodule this engine pins
/// (`d164b12`, 45 optional cases) predates them - it predates `46`/`47` too. A
/// test that read them off disk would panic on a missing file, and
/// `AHEAD_OF_PIN` in `tests/optional_corpus.rs` cannot hold them either: its
/// own `ahead_of_pin_names_only_cases_the_manifest_states` guard REFUSES a slug
/// the pinned manifest does not state. So the fixtures are inlined from spec
/// main, and the pin bump that catches up deletes this test in favor of the
/// corpus runner reaching the files.
///
/// ONE DOCUMENT, TWO MODES, because a rule whose content is "the two modes
/// agree" is not pinned by either mode alone.
#[test]
fn corpus_case_48_the_aria_render() {
    assert_eq!(
        tabs_html(TABS_TWO_MARKED, TabsMode::Aria).trim(),
        r#"<div class="tabs" role="tablist" aria-label="Tabs">
<button type="button" role="tab" id="tabset-1-tab-1" aria-selected="false" aria-controls="tabset-1-panel-1" class="tabs-label" tabindex="-1">First</button>
<button type="button" role="tab" id="tabset-1-tab-2" aria-selected="true" aria-controls="tabset-1-panel-2" class="tabs-label">Second</button>
<button type="button" role="tab" id="tabset-1-tab-3" aria-selected="false" aria-controls="tabset-1-panel-3" class="tabs-label" tabindex="-1">Third</button>
<div role="tabpanel" id="tabset-1-panel-1" aria-labelledby="tabset-1-tab-1" class="tabs-panel" hidden>
<p>Content one.</p>
</div>
<div role="tabpanel" id="tabset-1-panel-2" aria-labelledby="tabset-1-tab-2" class="tabs-panel">
<p>Content two.</p>
</div>
<div role="tabpanel" id="tabset-1-panel-3" aria-labelledby="tabset-1-tab-3" class="tabs-panel" hidden>
<p>Content three.</p>
</div>
</div>"#
    );
}

#[test]
fn corpus_case_49_the_css_render_of_the_same_document() {
    assert_eq!(
        tabs_html(TABS_TWO_MARKED, TabsMode::Css).trim(),
        r#"<div class="tabs" role="group" aria-label="Tabs">
<input type="radio" name="tabset-1" id="tabset-1-tab-1" class="tabs-radio">
<label for="tabset-1-tab-1" class="tabs-label">First</label>
<input type="radio" name="tabset-1" id="tabset-1-tab-2" class="tabs-radio" checked>
<label for="tabset-1-tab-2" class="tabs-label">Second</label>
<input type="radio" name="tabset-1" id="tabset-1-tab-3" class="tabs-radio">
<label for="tabset-1-tab-3" class="tabs-label">Third</label>
<div class="tabs-panel" role="group" aria-label="First">
<p>Content one.</p>
</div>
<div class="tabs-panel" role="group" aria-label="Second">
<p>Content two.</p>
</div>
<div class="tabs-panel" role="group" aria-label="Third">
<p>Content three.</p>
</div>
</div>"#
    );
}
