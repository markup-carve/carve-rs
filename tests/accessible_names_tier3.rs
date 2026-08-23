//! carve#1468 / carve#1469: a Tier-3 extension that writes an element writes its
//! accessible NAME too. Each shape below had a role, or a visible label on its
//! parts, and nothing a reader could use to tell the whole from the next one.

use carve::extensions::{
    CodeGroup, FencedRender, FencedRenderOptions, HeadingPermalinks, HeadingPermalinksOptions,
    Index, TableOfContents, TableOfContentsOptions, Tabs, TabsMode, TocPlacement,
};
use carve::label_default;
use carve::Options;

fn html(src: &str, ext: &dyn carve::CarveExtension) -> String {
    let mut o = Options::new();
    o.extensions.push(ext);
    carve::to_html_with_options(src, &o)
}

fn html_with_labels(src: &str, ext: &dyn carve::CarveExtension, key: &str, value: &str) -> String {
    let mut o = Options::new();
    o.extensions.push(ext);
    o.labels.insert(key.to_string(), value.to_string());
    carve::to_html_with_options(src, &o)
}

#[test]
fn a_lone_index_back_link_is_named_by_label_and_term() {
    let out = html("A :index[widget] here.\n\n::: index\n:::\n", &Index::new());
    assert!(
        out.contains(
            "<a href=\"#idx-widget-1\" class=\"index-backref\" aria-label=\"Back to widget\">\u{21a9}</a>"
        ),
        "{out}"
    );
}

/// The whole point: an index entry carries ONE back-link per occurrence, so
/// without the ordinal a reader meets a row of identical unnamed arrows. PART 9
/// §16's rule is mirrored - the name is the label plus what the link VISIBLY
/// says - so the ordinal appears in both (WCAG 2.5.3).
#[test]
fn the_kth_back_link_is_numbered_visibly_and_in_its_name() {
    let out = html(
        "A :index[widget] and :index[widget] again.\n\n::: index\n:::\n",
        &Index::new(),
    );
    assert!(
        out.contains("aria-label=\"Back to widget 1\">\u{21a9}<sup>1</sup></a>"),
        "{out}"
    );
    assert!(
        out.contains("aria-label=\"Back to widget 2\">\u{21a9}<sup>2</sup></a>"),
        "{out}"
    );
}

#[test]
fn the_tab_set_is_named_without_inventing_roles_the_css_mode_cannot_honor() {
    let src = ":::: tabs\n\n::: tab [One]\na\n:::\n\n::::\n";
    let out = html(src, &Tabs::new());
    assert!(
        out.contains("<div class=\"tabs\" role=\"group\" aria-label=\"Tabs\">"),
        "{out}"
    );
}

#[test]
fn aria_mode_keeps_tablist_and_gains_the_missing_name() {
    let src = ":::: tabs\n\n::: tab [One]\na\n:::\n\n::::\n";
    let opts = carve::extensions::TabsOptions {
        mode: TabsMode::Aria,
        ..Default::default()
    };
    let out = html(src, &Tabs::with_options(opts));
    assert!(
        out.contains("<div class=\"tabs\" role=\"tablist\" aria-label=\"Tabs\">"),
        "{out}"
    );
}

#[test]
fn the_code_group_names_itself_rather_than_redirecting_to_tabs() {
    let src = "::: code-group\n\n``` php [PHP]\n1\n```\n\n:::\n";
    let out = html(src, &CodeGroup::new());
    assert!(
        out.contains("<div class=\"code-group\" role=\"group\" aria-label=\"Code examples\">"),
        "{out}"
    );
}

#[test]
fn a_diagram_fence_is_an_image_with_a_name() {
    let out = html(
        "``` mermaid\ngraph TD;\n```\n",
        &FencedRender::new("mermaid"),
    );
    assert!(
        out.contains("<pre class=\"mermaid\" role=\"img\" aria-label=\"mermaid\">"),
        "{out}"
    );
}

/// An `img` with no accessible name is SKIPPED, which is worse than the source
/// being read out - so an empty label removes the role as well.
#[test]
fn role_and_name_are_written_together_or_not_at_all() {
    let opts = FencedRenderOptions {
        label: String::new(),
        ..FencedRenderOptions::new(
            vec!["mermaid".to_string()],
            None,
            None,
            carve::extensions::ContentMode::Text,
        )
    };
    let out = html(
        "``` mermaid\ngraph TD;\n```\n",
        &FencedRender::with_options(opts),
    );
    assert!(out.contains("<pre class=\"mermaid\">"), "{out}");
    assert!(!out.contains("role=\"img\""), "{out}");
}

/// The author who cared enough to NAME the fence is exactly the one who must not
/// lose the role: without it the source is still announced as prose.
#[test]
fn an_authored_name_still_gets_the_role() {
    let out = html(
        "{aria-label=\"Deploy flow\"}\n``` mermaid\ngraph TD;\n```\n",
        &FencedRender::new("mermaid"),
    );
    assert!(out.contains("aria-label=\"Deploy flow\""), "{out}");
    assert!(out.contains("role=\"img\""), "{out}");
    assert!(!out.contains("aria-label=\"mermaid\""), "{out}");
}

#[test]
fn the_authors_own_name_wins_under_any_ascii_casing() {
    let src = "{ARIA-LABEL=\"Mine\"}\n:::: tabs\n\n::: tab [One]\na\n:::\n\n::::\n";
    let out = html(src, &Tabs::new());
    assert!(out.contains("ARIA-LABEL=\"Mine\""), "{out}");
    assert!(!out.contains("aria-label=\"Tabs\""), "{out}");
}

/// ONE labels map localizes every engine-written string. With a per-extension
/// option as the only spelling, switching a document to German meant finding
/// several call sites and silently missing one; PART 9 §16a forbids making a
/// host configure the same text twice.
#[test]
fn one_labels_map_reaches_every_extension() {
    let out = html_with_labels(
        "A :index[Gerät] hier.\n\n::: index\n:::\n",
        &Index::new(),
        "indexBackref",
        "Zurück zu",
    );
    assert!(out.contains("aria-label=\"Zurück zu Gerät\""), "{out}");

    let out = html_with_labels(
        ":::: tabs\n\n::: tab [Eins]\na\n:::\n\n::::\n",
        &Tabs::new(),
        "tabsGroup",
        "Registerkarten",
    );
    assert!(out.contains("aria-label=\"Registerkarten\""), "{out}");

    let out = html_with_labels(
        "::: code-group\n\n``` php [PHP]\n1\n```\n\n:::\n",
        &CodeGroup::new(),
        "codeGroup",
        "Codebeispiele",
    );
    assert!(out.contains("aria-label=\"Codebeispiele\""), "{out}");
}

#[test]
fn an_extension_option_overrides_the_map_for_that_instance() {
    let out = html_with_labels(
        "A :index[widget] here.\n\n::: index\n:::\n",
        &Index::new().with_backref_label("Explicit"),
        "indexBackref",
        "Zurück zu",
    );
    assert!(out.contains("aria-label=\"Explicit widget\""), "{out}");
    assert!(!out.contains("Zurück zu"), "{out}");
}

// THE OTHER HALF OF THE ADMISSION RULE (markup-carve/carve#1510, ruled in
// markup-carve/carve#1520).
//
// Everything above checks that a DOCUMENTED key reaches the output. The rest of
// this file checks the opposite direction. Extensions §1.5 used to say every
// extension-written string with a fixed English default has a key in the
// render's `labels` map, and two strings satisfied that sentence with no key:
// the heading-permalink label, default `Permalink`, and the table-of-contents
// summary, default `Table of Contents`, visible whenever a collapsible
// disclosure is on. §1.5 was narrowed rather than the map grown - a string the
// extension already exposes as an OPTION is configured there, and it does not
// get both spellings - and PART 9 §16a's note recording the question as open
// became that rule.
//
// ASSERTING THE ABSENCE ALONE CANNOT FAIL FOR THE RIGHT REASON. A key nothing
// implements is inert whether the rule is honored or the string was simply
// forgotten. So the permalink is measured three ways: the documented default
// renders, the map key changes NOTHING, and the extension option DOES reach the
// output. Only the third separates "configured elsewhere" from "not
// configurable at all", which is the state §1.5 says a string must not be in.

const HEADING: &str = "# One\n\nbody\n";

/// The `aria-label` this engine writes on a permalink anchor, whatever it says.
fn permalink_label(html: &str) -> Option<&str> {
    let rest = html.split_once("class=\"permalink\" aria-label=\"")?.1;
    rest.split_once('"').map(|(value, _)| value)
}

/// Assertion one. Without it the two below could both hold on a probe that
/// finds nothing at all in either render.
#[test]
fn the_permalink_label_renders_its_documented_english_default() {
    let out = html(HEADING, &HeadingPermalinks::new());
    assert_eq!(permalink_label(&out), Some("Permalink"), "{out}");
}

/// Assertion two: the map key is inert, which is what "no key" means
/// observationally.
#[test]
fn the_permalink_label_is_not_read_from_the_labels_map() {
    let out = html_with_labels(
        HEADING,
        &HeadingPermalinks::new(),
        "headingPermalink",
        "Sentinel-headingPermalink",
    );
    assert_eq!(permalink_label(&out), Some("Permalink"), "{out}");
}

/// Assertion three, the one that makes assertion two answerable: the string IS
/// configurable, on the extension that writes it.
#[test]
fn the_permalink_label_is_read_from_the_extension_option() {
    let opts = HeadingPermalinksOptions {
        aria_label: "Option-headingPermalink".into(),
        ..Default::default()
    };
    let out = html(HEADING, &HeadingPermalinks::with_options(opts));
    assert_eq!(
        permalink_label(&out),
        Some("Option-headingPermalink"),
        "{out}"
    );
}

/// The negative half, and the assertion that goes red if someone later adds the
/// key the rule refuses. `label_default` is this engine's whole `labels`
/// vocabulary; a name it does not answer for has no key at all.
#[test]
fn neither_option_only_name_is_in_the_labels_vocabulary() {
    for key in ["headingPermalink", "tocSummary"] {
        assert_eq!(
            label_default(key),
            "",
            "{key} has a labels default, and Extensions §1.5 says a string its extension \
             exposes as an option does not get a key as well"
        );
    }

    // The control: a name the rule DOES admit answers, so the loop above is
    // measuring the vocabulary rather than a function that answers nothing.
    assert_eq!(label_default("indexBackref"), "Back to");
}

/// THE SUMMARY DOES NOT EXIST IN THIS ENGINE YET, so it is pinned as a tripwire
/// rather than as a row.
///
/// carve-js and carve-php wrap the table of contents in a `<details>` whose
/// `<summary>` carries the string; this engine has no `collapsible` option and
/// writes the nav unwrapped, so there is no string here to configure
/// either way. That satisfies the rule vacuously, which is a weaker thing than
/// the permalink above satisfies it. When the disclosure is ported, this
/// assertion goes red - and whoever ports it has to come back here and give
/// `tocSummary` the three-assertion treatment, on the extension option, never
/// as a `labels` key.
#[test]
fn the_table_of_contents_writes_no_summary_to_configure() {
    let out = html("::: toc\n:::\n\n# One\n\nbody\n", &TableOfContents::new());
    // A nav WAS rendered - the control on the two assertions below, which a
    // render producing nothing at all would satisfy vacuously. On the open TAG
    // rather than `<nav class="toc">`, because `css_class` is an option and the
    // nav now carries a name after it.
    assert!(out.contains("<nav "), "{out}");
    // The open TAG, not the exact `<summary>` string: a ported disclosure that
    // put a class on the element would slip past the closed form and leave the
    // tripwire green while the string it guards had arrived.
    assert!(!out.contains("<details"), "{out}");
    assert!(!out.contains("<summary"), "{out}");
}

// THE TABLE-OF-CONTENTS NAV SAYS WHAT IT IS CALLED (Extensions §8b.1, ruled in
// markup-carve/carve#1547 closing markup-carve/carve#1509).
//
// `<nav>` is a navigation landmark unconditionally - unlike `<section>`, which
// maps to `generic` until it is named - so an unnamed one is an entry in a
// reader's landmark list reading only "navigation". A page holds more than one
// the moment both TOC extensions are registered, a document writes `::: toc`
// twice, or a site template contributes its own, and unnamed they are
// indistinguishable. That is the defect; a single anonymous nav is only how it
// starts.
//
// AUTHORED, so it gets a `labels` key rather than an extension option: the
// directive's content is empty and nothing on the page names the nav, so there
// is no string to derive from; `Table of contents` is ordinary English rather
// than the class word `toc` an abbreviation-expanding reader would hear spelled
// out; and no configuration put an `aria-label` on this nav before, so §1.5's
// "unless the extension already exposes it as an option" does not fire.

const HEADINGS: &str = "# One\n\n## Two\n\nbody\n";
const PLACED: &str = "::: toc\n:::\n\n# One\n\n## Two\n\nbody\n";

/// The `aria-label` on the element the extension emitted, whatever it says.
///
/// NOT a substring search for `<nav class="toc" aria-label="..."`:
/// `TableOfContentsOptions::css_class` is configurable, so a probe keyed on the
/// class passes for the wrong reason the moment a host sets it - and every
/// other named element in this file writes the same attribute, so the attribute
/// alone does not identify what was measured either. This reads the open tag of
/// the element under test and takes the name off it.
fn name_on(open_tag_of: &str, html: &str) -> Option<String> {
    let start = html.find(open_tag_of)?;
    let tag = &html[start..start + html[start..].find('>')?];
    let rest = tag.split_once(" aria-label=\"")?.1;
    rest.split_once('"').map(|(value, _)| value.to_string())
}

fn nav_name(html: &str) -> Option<String> {
    name_on("<nav", html)
}

/// THE THREE-ASSERTION STANDARD (markup-carve/carve#1511): the documented
/// default reaches the output, the map entry CHANGES it, and a row for a key
/// that already worked before this ruling is driven through the same harness -
/// without which a probe finding nothing in either render satisfies the first
/// two vacuously.
fn label_backed(row: &str, labels: &[(&str, &str)]) -> Option<String> {
    let mut o = Options::new();
    for (k, v) in labels {
        o.labels.insert((*k).to_string(), (*v).to_string());
    }
    match row {
        "tabs" => {
            let ext = Tabs::new();
            o.extensions.push(&ext);
            let out =
                carve::to_html_with_options(":::: tabs\n\n::: tab [One]\na\n:::\n\n::::\n", &o);
            name_on("<div class=\"tabs\"", &out)
        }
        "injected" => {
            let ext = TableOfContents::new();
            o.extensions.push(&ext);
            nav_name(&carve::to_html_with_options(HEADINGS, &o))
        }
        _ => {
            let ext = TocPlacement::new();
            o.extensions.push(&ext);
            nav_name(&carve::to_html_with_options(PLACED, &o))
        }
    }
}

const LABEL_ROWS: [(&str, &str, &str); 3] = [
    ("placement", "tocNav", "Table of contents"),
    ("injected", "tocNav", "Table of contents"),
    // The control: a key that already worked, through the same harness.
    ("tabs", "tabsGroup", "Tabs"),
];

/// Assertion one. Without it the assertion below could hold on a render where
/// the probe finds nothing at all.
#[test]
fn each_label_backed_name_renders_its_documented_english_default() {
    for (row, _, default) in LABEL_ROWS {
        assert_eq!(label_backed(row, &[]).as_deref(), Some(default), "{row}");
    }
}

/// Assertion two: the map entry CHANGES it, which is what having a key means
/// observationally - and what a hard-coded English string cannot do.
#[test]
fn each_label_backed_name_is_read_from_the_labels_map() {
    for (row, key, _) in LABEL_ROWS {
        let sentinel = format!("Sentinel-{key}");
        assert_eq!(
            label_backed(row, &[(key, &sentinel)]).as_deref(),
            Some(sentinel.as_str()),
            "{row}"
        );
    }
}

#[test]
fn the_key_is_declared_with_its_documented_default() {
    assert_eq!(label_default("tocNav"), "Table of contents");
}

/// §8b.3 makes the nav fragment the cross-impl contract, and a name chosen
/// per-extension is the one change that would break byte-identity between the
/// two extensions that write it.
#[test]
fn both_extensions_write_the_same_nav_byte_for_byte() {
    for labels in [None, Some("Inhaltsverzeichnis")] {
        let nav_of = |src: &str, ext: &dyn carve::CarveExtension| -> String {
            let mut o = Options::new();
            o.extensions.push(ext);
            if let Some(value) = labels {
                o.labels.insert("tocNav".to_string(), value.to_string());
            }
            let out = carve::to_html_with_options(src, &o);
            let start = out.find("<nav").expect("a nav");
            let end = out.find("</nav>").expect("a nav close") + 6;
            out[start..end].to_string()
        };
        assert_eq!(
            nav_of(PLACED, &TocPlacement::new()),
            nav_of(HEADINGS, &TableOfContents::new()),
        );
    }
}

/// A name the AUTHOR wrote outranks the label and nothing is added beside it -
/// §1.5's existing precedence, since §8b.1 already carries the attribute line
/// onto the nav. The match is on the attribute NAME, ASCII-case-insensitively
/// (§16a, the shapes carve#1468 closed), and this engine echoes the author's own
/// spelling back, so a case-sensitive test would write a second name next to
/// theirs.
#[test]
fn an_authored_nav_name_wins_under_any_ascii_casing() {
    for spelling in ["aria-label", "ARIA-LABEL", "Aria-Label"] {
        let src = format!("{{{spelling}=\"Chapters\"}}\n{PLACED}");
        let out = html(&src, &TocPlacement::new());
        assert!(out.contains(&format!("{spelling}=\"Chapters\"")), "{out}");
        assert!(!out.contains("Table of contents"), "{out}");
        assert_eq!(
            out.to_ascii_lowercase().matches("aria-label=").count(),
            1,
            "{out}"
        );
    }
}

#[test]
fn an_empty_entry_suppresses_the_nav_name_entirely() {
    let out = html_with_labels(PLACED, &TocPlacement::new(), "tocNav", "");
    assert!(out.contains("<nav class=\"toc\">"), "{out}");
    assert_eq!(nav_name(&out), None, "{out}");
}

#[test]
fn the_nav_name_is_escaped_where_it_lands() {
    let out = html_with_labels(
        PLACED,
        &TocPlacement::new(),
        "tocNav",
        "A \"quoted\" & <angled>",
    );
    assert_eq!(
        nav_name(&out).as_deref(),
        Some("A &quot;quoted&quot; &amp; &lt;angled&gt;"),
        "{out}"
    );
}

/// THE CLASS IS AN OPTION, so the name has to be found without it. A test
/// grepping for `<nav class="toc" aria-label=` would pass here for the wrong
/// reason (markup-carve/carve-rs#1249).
#[test]
fn the_name_is_on_the_nav_whatever_its_class_is_called() {
    let opts = TableOfContentsOptions {
        css_class: "contents".into(),
        ..Default::default()
    };
    let out = html(HEADINGS, &TableOfContents::with_options(opts));
    assert!(!out.contains("class=\"toc\""), "{out}");
    assert_eq!(
        nav_name(&out).as_deref(),
        Some("Table of contents"),
        "{out}"
    );
}

/// THE DEGRADED NAV IS STILL A LANDMARK. `::: toc` renders an EMPTY `<nav>`
/// when no heading falls in its window, and again once the cumulative byte
/// budget that bounds K blocks by N headings is exhausted. The budget bounds
/// the ENTRY LIST, not the element's identity - and the empty nav is exactly
/// where an unnamed landmark is least distinguishable, because there is no link
/// text to read instead.
#[test]
fn an_empty_nav_is_named_too() {
    let out = html("::: toc\n:::\n\nplain paragraph\n", &TocPlacement::new());
    assert!(
        out.contains("<nav class=\"toc\" aria-label=\"Table of contents\"></nav>"),
        "{out}"
    );
}

#[test]
fn a_nav_the_byte_budget_degraded_keeps_its_name() {
    let mut src = "::: toc\n:::\n\n".repeat(5000);
    for i in 0..50 {
        src.push_str(&format!("# Heading number {i} with length\n\n"));
    }
    let out = html(&src, &TocPlacement::new());
    let degraded = out.matches("></nav>").count();
    // The budget IS reached - without this the assertion below passes on a
    // render where nothing degraded at all.
    assert!(degraded > 0, "nothing degraded");
    assert_eq!(
        out.matches("<nav class=\"toc\" aria-label=\"Table of contents\"></nav>")
            .count(),
        degraded,
        "a degraded nav lost its name"
    );
}
