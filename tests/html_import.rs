use carve::{
    html_to_ast, html_to_carve, HtmlImportDiagnosticCode, HtmlImportMode, HtmlImportOptions,
};
use std::fs;
use std::path::Path;

#[test]
fn imports_through_the_canonical_writer() {
    let result = html_to_carve(
        "<h1>Hello <em>world</em></h1><p>A <a href=\"https://example.com\">link</a>.</p>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert_eq!(
        result.value,
        "# Hello /world/\n\nA [link](https://example.com).\n"
    );
    assert!(result.report.diagnostics.is_empty());
}

#[test]
fn active_content_and_loss_are_reported() {
    let result = html_to_ast(
        "<p onclick=\"evil()\">safe<script>alert(1)</script><span title=\"lost\"> text</span></p>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert_eq!(
        result
            .report
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec![
            HtmlImportDiagnosticCode::AttributeDropped,
            HtmlImportDiagnosticCode::ElementDropped,
        ]
    );
}

#[test]
fn semantic_mode_keeps_portable_attributes() {
    let options = HtmlImportOptions {
        mode: HtmlImportMode::Semantic,
        ..Default::default()
    };
    let result = html_to_carve("<p id=\"lead\" class=\"intro\">Text</p>", &options).unwrap();
    assert!(result.value.contains("{#lead .intro}"));
}

#[test]
fn roundtrip_mode_preserves_unknown_markup_as_raw_html() {
    let options = HtmlImportOptions {
        mode: HtmlImportMode::Roundtrip,
        ..Default::default()
    };
    // `<ruby>` rather than `<kbd>`: this test needs an element Carve cannot
    // express, and `<kbd>` stopped being one (carve#1140).
    let result = html_to_carve("<p><ruby>x</ruby></p>", &options).unwrap();
    assert!(result.value.contains("`<ruby>x</ruby>`{=html}"));
    assert_eq!(
        result.report.diagnostics[0].code,
        HtmlImportDiagnosticCode::RawPreserved
    );
}

/// Shared html-import fixtures whose pinned golden and this engine disagree,
/// each naming the ruling that explains the gap.
///
/// EVERY ENTRY FAILS IN BOTH DIRECTIONS, the same arrangement `AHEAD_OF_PIN`
/// keeps in `tests/corpus.rs` and `tests/optional_corpus.rs`, and the same one
/// carve-js keeps in `test/html-import-conformance.test.ts`. The third column
/// is what this engine writes TODAY, so a change to that output is caught
/// exactly as the fixture would have caught it; and the value must still DIFFER
/// from the fixture, so an entry the fixture has caught up with FAILS and is
/// deleted in the commit that moves the pin.
///
/// An entry is therefore a statement about the ENGINE with a date on it. It is
/// not a skip: a skip would go green whether or not the output moved, which is
/// how a gate stops being able to fail.
///
/// AN ENTRY SKIPS THE TREE AND THE REPORT TOO, and the third column pins only
/// the source exit. A clause that moves the written source usually moves the
/// rows that describe it, and a second and third recorded value would be two
/// more things to keep current; the engine tests for the ruling pin those
/// directly instead.
const AHEAD_OF_PIN: &[(&str, &str, &str)] = &[
    (
        "empty-definition-description",
        "an empty description body is written `: {empty}` (markup-carve/carve#1827)",
        ":: term\n: {empty}\n",
    ),
    (
        "empty-definition-description-not-last",
        "the sentinel keeps the list whole, so nothing splits (markup-carve/carve#1827)",
        ":: t1\n: {empty}\n:: t2\n: d2\n",
    ),
];

/// The two fields that record WHERE a node was written rather than what it is.
///
/// Every fixture is absent both by construction - they are a property of the
/// INPUT, not of the import - so the published tree is compared without them,
/// exactly as the spec's own reading over these same fixtures does
/// (`tests/html-import-contract.check.mjs`) and as carve-js does in
/// `test/html-import-conformance.test.ts`.
fn without_locations(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(without_locations).collect())
        }
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .filter(|(key, _)| key.as_str() != "pos" && key.as_str() != "srcByteLength")
                .map(|(key, inner)| (key.clone(), without_locations(inner)))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[test]
fn shared_contract_fixtures_match() {
    let root = Path::new("tests/spec/tests/html-import");
    // COLLECTED, NOT ASSERTED IN THE LOOP. An `assert_eq!` inside the walk stops
    // at the FIRST mismatching fixture, so every later one is not passing - it
    // never ran. Moving the pin surfaced five mismatches at once and the loop
    // could only ever name one of them, which is the same shape as driving each
    // assertion on its own.
    let mut mismatches: Vec<String> = Vec::new();
    let mut dirs: Vec<_> = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    for dir in dirs {
        let name = dir.file_name().unwrap().to_str().unwrap().to_string();
        let html = fs::read_to_string(dir.join("input.html")).unwrap();
        let expected = fs::read_to_string(dir.join("expected.crv")).unwrap();
        let expected_report: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join("expected.report.json")).unwrap())
                .unwrap();
        let expected_ast: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join("expected.ast.json")).unwrap())
                .unwrap();
        let result = html_to_carve(&html, &HtmlImportOptions::default()).unwrap();
        if let Some((_, reason, current)) =
            AHEAD_OF_PIN.iter().find(|(fixture, _, _)| *fixture == name)
        {
            if result.value != *current {
                mismatches.push(format!(
                    "{name}: AHEAD_OF_PIN says this engine writes {current:?} ({reason}), \
                     and it writes {:?} - update the entry or delete it",
                    result.value
                ));
            }
            if result.value == expected {
                mismatches.push(format!(
                    "{name}: matches the fixture now - delete its AHEAD_OF_PIN entry"
                ));
            }
            continue;
        }
        if result.value != expected {
            mismatches.push(format!(
                "{name}\n  expected: {expected:?}\n  actual:  {:?}",
                result.value
            ));
            continue;
        }
        // AND THE TREE THE OTHER EXIT PUBLISHES. This loop read `expected.crv`
        // and the report and never `expected.ast.json`, so the tree half of
        // every shared fixture was unchecked in this repository - which is how
        // an `#id` slot rode the published tree from `303354d` with nothing
        // able to see it (carve-rs#1357).
        //
        // `html_to_ast` RATHER THAN `result`: the two exits are different
        // objects, and it is the published one the fixture is a statement about
        // (markup-carve/carve#1616). Reading the tree off the writing exit
        // would compare a fixture against an intermediate nobody publishes -
        // and would have gone green on exactly the defect this comparison was
        // added to catch, since the slot is correct on that side.
        let published = html_to_ast(&html, &HtmlImportOptions::default()).unwrap();
        let actual_ast: serde_json::Value =
            serde_json::from_str(&carve::ast_json::to_json(&published.value)).unwrap();
        if without_locations(&actual_ast) != without_locations(&expected_ast) {
            mismatches.push(format!(
                "{name} tree\n  expected: {}\n  actual:  {}",
                serde_json::to_string(&without_locations(&expected_ast)).unwrap(),
                serde_json::to_string(&without_locations(&actual_ast)).unwrap()
            ));
            continue;
        }
        let expected_codes = expected_report["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["code"].as_str().unwrap())
            .collect::<Vec<_>>();
        let actual_codes = result
            .report
            .diagnostics
            .iter()
            // The vocabulary's own spelling, not a third copy of the table:
            // with a copy here the fixtures compared this test's idea of the
            // codes against the shared ones and left the engine's own spelling
            // unpinned.
            .map(|d| d.code.as_str())
            .collect::<Vec<_>>();
        if actual_codes != expected_codes {
            mismatches.push(format!(
                "{name} diagnostics\n  expected: {expected_codes:?}\n  actual:  {actual_codes:?}"
            ));
            continue;
        }
        // Every OTHER field the fixture states, compared too. A fixture
        // diagnostic is a MINIMUM match - it may leave `path` out, and most do -
        // but a field it does state is the shared contract's answer for that
        // field, and a field nobody compares is a field each engine answers on
        // its own. `path` is the one that happened to: three engines invented
        // three rootings and the loop that was supposed to catch it read only
        // the codes (markup-carve/carve#1257). `message` and `severity` were
        // unread here for the same reason and are pinned with it.
        for (index, expected_diagnostic) in expected_report["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            let actual = &result.report.diagnostics[index];
            let at = format!("{name} diagnostic {index}");
            if let Some(path) = expected_diagnostic["path"].as_str() {
                if actual.path.as_deref() != Some(path) {
                    mismatches.push(format!("{at} path: {path:?} != {:?}", actual.path));
                }
            }
            if let Some(message) = expected_diagnostic["message"].as_str() {
                if actual.message != message {
                    mismatches.push(format!("{at} message: {message:?} != {:?}", actual.message));
                }
            }
            if let Some(severity) = expected_diagnostic["severity"].as_str() {
                if actual.severity.as_str() != severity {
                    mismatches.push(format!(
                        "{at} severity: {severity:?} != {:?}",
                        actual.severity.as_str()
                    ));
                }
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "shared html-import contract mismatch(es):\n{}",
        mismatches.join("\n")
    );
}

/// A `AHEAD_OF_PIN` entry naming a fixture the shared tree does not have
/// is an entry nothing walks, so it can never fail and never be deleted.
#[test]
fn behind_the_ruling_names_only_fixtures_that_exist() {
    let root = Path::new("tests/spec/tests/html-import");
    let present: Vec<String> = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .map(|path| path.file_name().unwrap().to_str().unwrap().to_string())
        .collect();
    let orphaned: Vec<&str> = AHEAD_OF_PIN
        .iter()
        .map(|(fixture, _, _)| *fixture)
        .filter(|fixture| !present.iter().any(|name| name == fixture))
        .collect();
    assert!(
        orphaned.is_empty(),
        "AHEAD_OF_PIN names fixture(s) the shared tree does not have: {orphaned:?}"
    );
}

/// PART 12 §16, the three rules a diagnostic `path` follows, asserted one at a
/// time. An end-to-end path pins all three at once and stays green when two of
/// them are right, which is the state this engine was in: its index basis and
/// its traversal already agreed with the shared convention and only its ROOT
/// did not (markup-carve/carve#1257).
///
/// The convention: rooted at the imported fragment, `[n]` counting among ALL
/// child nodes (text included), naming the importer's own traversal rather than
/// the parsed DOM. It is a human-readable locator, not an XPath expression.
mod diagnostic_path {
    use super::*;

    fn paths(html: &str) -> Vec<String> {
        html_to_ast(html, &HtmlImportOptions::default())
            .unwrap()
            .report
            .diagnostics
            .iter()
            .map(|d| d.path.clone().unwrap_or_default())
            .collect()
    }

    /// ROOT. The parser's synthesized `<html>`/`<head>`/`<body>` name the
    /// parser, not the input, so they are not in the path - whether the input
    /// spelled them or not.
    #[test]
    fn a_path_is_rooted_at_the_fragment_not_at_the_document() {
        assert_eq!(paths("<p onclick=\"x()\">t</p>"), vec!["/p[1]"]);
        assert_eq!(
            paths("<html><body><p onclick=\"x()\">t</p></body></html>"),
            vec!["/p[1]"],
            "an input that spells the wrappers itself gets the same path as the bare fragment"
        );
        assert_eq!(
            paths("<!DOCTYPE html><html><body><p onclick=\"x()\">t</p></body></html>"),
            vec!["/p[1]"],
            "a doctype is not a node of the fragment and does not take an index"
        );
    }

    /// The synthesized `<head>` is what made the wrapper prefix read
    /// `body[2]` rather than `body[1]`, and the head's own content is numbered
    /// into the same run as the body's - one fragment, one sequence.
    #[test]
    fn a_head_is_neither_a_segment_nor_a_gap_in_the_numbering() {
        assert_eq!(
            paths(
                "<html><head><title>T</title></head><body><p onclick=\"x()\">t</p></body></html>"
            ),
            vec!["/title[1]", "/p[2]"]
        );
    }

    /// INDEX BASIS. `[n]` is the position among ALL child nodes. A text node
    /// takes a number; counting elements only would make both of these `[1]`.
    #[test]
    fn an_index_counts_every_child_node_including_text() {
        assert_eq!(paths("lead text<p onclick=\"x()\">t</p>"), vec!["/p[2]"]);
        assert_eq!(
            paths("<p>lead <em>e</em> <kbd onclick=\"x()\">K</kbd></p>"),
            vec!["/p[1]/kbd[4]"],
            "text, em, text, kbd - the kbd is the 4th node, not the 2nd element"
        );
        assert_eq!(
            paths("<!-- c --><p onclick=\"x()\">t</p>"),
            vec!["/p[2]"],
            "a comment is a node of the fragment and takes an index"
        );
    }

    /// TRAVERSAL. The path names the importer's walk, not the parsed DOM:
    /// `<thead>`/`<tbody>` are flattened away and rows are numbered across the
    /// whole table. Naming the DOM would give `/table[1]/tbody[2]/tr[1]/td[1]`.
    #[test]
    fn a_table_path_names_the_traversal_and_not_the_dom() {
        assert_eq!(
            paths(
                "<table><thead><tr><th>h</th></tr></thead>\
                 <tbody><tr><td onclick=\"x()\">c</td></tr></tbody></table>"
            ),
            vec!["/table[1]/tr[2]/td[1]"]
        );
        assert_eq!(
            paths("<table><tr><td onclick=\"x()\">c</td></tr></table>"),
            vec!["/table[1]/tr[1]/td[1]"],
            "the tbody the parser inserts for a bare row is flattened the same way"
        );
        assert_eq!(
            paths(
                "<table><tr><td>a</td><td>b</td><td rowspan=\"2\">c</td></tr>\
                 <tr><td>d</td></tr></table>"
            ),
            vec!["/table[1]/tr[2]"],
            "a diagnostic about a ROW, not a cell, is rooted and numbered the same way"
        );
    }

    /// The wrappers carry no attributes into a diagnostic either: an element
    /// that is not part of the fragment cannot be the subject of one.
    #[test]
    fn a_wrapper_element_is_not_a_diagnostic_subject() {
        assert!(paths("<html><body onclick=\"x()\"><p>t</p></body></html>").is_empty());
        assert!(paths("<html onclick=\"x()\"><body><p>t</p></body></html>").is_empty());
    }

    /// THE TWO RULES ARE ONE RULE (markup-carve/carve#1554). A wrapper the
    /// importer added prints no step, and an index counts among the children of
    /// the parent the step it prints SITS UNDER - so a bare inline run wrapped
    /// in a synthesized paragraph is numbered among the BODY children, never
    /// among the nodes of the wrapper.
    ///
    /// This engine applied the first rule and not the second, which made the
    /// index name a parent no step spells. It was invisible for as long as the
    /// shared `math-block-and-mathml` fixture encoded the same mistake: this
    /// engine and carve-js agreed with it and carve-php, which followed the
    /// clause, was the only one red. The tell was one diagnostic later in the
    /// SAME document - `/p[3]/math[2]` counts the two siblings `/math[1]` did
    /// not.
    ///
    /// Not a math rule, which is why `<kbd>` leads.
    #[test]
    fn a_wrapped_inline_run_is_numbered_where_its_step_is_printed() {
        assert_eq!(
            paths("<p>z</p><kbd onclick=\"x()\">K</kbd>"),
            vec!["/kbd[2]"],
            "the kbd is the second BODY child, not the first child of the paragraph around it"
        );
        assert_eq!(
            paths("<hr><math alttext=\"a\"></math>"),
            vec!["/math[2]"],
            "a block sibling that wraps nothing still takes its index"
        );
        assert_eq!(
            paths("<p>z</p><p>y</p><math alttext=\"a\"></math>"),
            vec!["/math[3]"]
        );
        assert_eq!(
            paths("<p>z</p>lead text<math alttext=\"a\"></math>"),
            vec!["/math[3]"],
            "the run's own leading text node is a child of the body and counts"
        );
        assert_eq!(
            paths("<div><p>z</p><kbd onclick=\"x()\">K</kbd></div>"),
            vec!["/div[1]/kbd[2]"]
        );
        assert_eq!(
            paths("<blockquote><p>z</p><kbd onclick=\"x()\">K</kbd></blockquote>"),
            vec!["/blockquote[1]/kbd[2]"]
        );
    }

    /// The same rule, at the four sites that lift a child OUT of the list they
    /// walk. Rebuilding an index from what is left renumbers every sibling past
    /// the hole, and a step spelled without an index at all - `figcaption`, as
    /// this engine wrote it - says nothing about which one it was.
    #[test]
    fn a_lifted_child_does_not_renumber_the_siblings_it_leaves() {
        assert_eq!(
            paths("<figure><figcaption>c</figcaption><img src=\"i.png\" onclick=\"x()\"></figure>"),
            vec!["/figure[1]/img[2]"],
            "the caption comes out of the child list; the image does not move"
        );
        assert_eq!(
            paths("<figure>\n<img src=\"i.png\">\n<figcaption>c <kbd onclick=\"x()\">K</kbd></figcaption>\n</figure>"),
            vec!["/figure[1]/figcaption[4]/kbd[2]"],
            "a pretty-printed figure puts its caption fourth, and the step says so"
        );
        assert_eq!(
            paths("<details><summary>s</summary><p onclick=\"x()\">b</p></details>"),
            vec!["/details[1]/p[2]"],
            "the summary is lifted out of the body, not out of the numbering"
        );
        assert_eq!(
            paths("<dl>\n<dt>t</dt>\n<dd onclick=\"x()\">d</dd>\n</dl>"),
            vec!["/dl[1]/dd[4]"],
            "the walk collects only dt and dd; the index still counts every child"
        );
        assert_eq!(
            paths("<dl><div><dt>t</dt><dd onclick=\"x()\">d</dd></div></dl>"),
            vec!["/dl[1]/div[1]/dd[2]"],
            "the group wrapper is the author's element and keeps its step"
        );
    }
}

/// A footer inside a quote is ordinary quoted block content.
#[test]
fn a_trailing_footer_in_a_quote_stays_quoted_content() {
    let result = html_to_carve(
        "<blockquote><p>To be</p><footer>Hamlet</footer></blockquote>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert_eq!(result.value, "> To be\n>\n> Hamlet\n");
}

/// Multiple footers are ordinary blocks and none are dropped.
#[test]
fn every_footer_in_a_quote_stays_quoted_content() {
    let result = html_to_carve(
        "<blockquote><footer>First</footer><p>To be</p><footer>Hamlet</footer></blockquote>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert_eq!(result.value, "> First\n>\n> To be\n>\n> Hamlet\n");
}

/// The slot holds INLINE content, so a footer carrying blocks does not fit it.
/// Flattening one would run its paragraphs together with no separator, so it
/// stays ordinary quoted content instead - every word survives, which is the
/// better answer when the shape cannot be represented. carve-js and carve-php
/// agree byte for byte.
#[test]
fn a_footer_carrying_blocks_stays_quoted_content() {
    let result = html_to_carve(
        "<blockquote><p>quote</p><footer><p>By <strong>A</strong></p><p>Work</p></footer></blockquote>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert_eq!(result.value, "> quote\n>\n> By *A*\n>\n> Work\n");
}

/// PART 10 §T9 gives every `th` a `scope` from its POSITION, so importing that
/// value back would write this engine's own output in as if the author had
/// typed it. A value the default cannot explain is a different thing: `colgroup`
/// and `rowgroup` have no marker spelling and no positional derivation, so an
/// authored one is the only way to get them and dropping it is lossy
/// (carve-rs#944).
#[test]
fn an_authored_table_cell_scope_survives_the_import() {
    let result = html_to_carve(
        "<table><thead><tr><th scope=\"colgroup\">A</th></tr></thead><tbody><tr><td>1</td></tr></tbody></table>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert!(
        result.value.contains("scope=colgroup"),
        "the authored scope was dropped: {}",
        result.value
    );
}

#[test]
fn a_scope_that_only_restates_the_positional_default_is_dropped() {
    // The other half, and the one that must not regress: `col` on a cell in the
    // head-row run is exactly what the renderer emits from position, so keeping
    // it would round-trip the generator's own output back into the source.
    for html in [
        "<table><thead><tr><th scope=\"col\">A</th></tr></thead><tbody><tr><td>1</td></tr></tbody></table>",
        "<table><tbody><tr><th scope=\"row\">A</th><td>1</td></tr></tbody></table>",
    ] {
        let result = html_to_carve(html, &HtmlImportOptions::default()).unwrap();
        assert!(
            !result.value.contains("scope"),
            "a positional scope was imported: {}",
            result.value
        );
    }
}

// PART 9 §10 / carve#1140. `<kbd>Tab</kbd>` imported as `Tab`: the element was
// unwrapped and the loss recorded, and an `<abbr title="X">` lost the expansion
// along with the tag. Carve spells all seven exactly, so they map to the
// compact span attribute the ruling settles on.

/// Core's three (PART 9 §9) render back as the element unconditionally.
#[test]
fn the_three_core_semantic_elements_import_as_the_span_attribute() {
    for (html, carve) in [
        ("<p>Press <kbd>Tab</kbd></p>", "Press [Tab]{kbd}\n"),
        (
            "<p><abbr title=\"HyperText\">HTML</abbr></p>",
            "[HTML]{abbr=HyperText}\n",
        ),
        (
            "<p><time datetime=\"2026-01-01\">today</time></p>",
            "[today]{time=2026-01-01}\n",
        ),
    ] {
        let result = html_to_carve(html, &HtmlImportOptions::default()).unwrap();
        assert_eq!(result.value, carve, "{html}");
    }
}

/// The four the `SemanticSpan` extension carries map the same way. They are the
/// extension's names, so a CORE render gives the attribute rather than the
/// element - which the round-trip test below states rather than implies.
#[test]
fn the_four_extension_semantic_elements_import_as_the_span_attribute() {
    for (html, carve) in [
        ("<p><samp>out</samp></p>", "[out]{samp}\n"),
        ("<p><var>v</var></p>", "[v]{var}\n"),
        ("<p><cite>Dune</cite></p>", "[Dune]{cite}\n"),
        (
            "<p><dfn title=\"Cascading Style Sheets\">CSS</dfn></p>",
            "[CSS]{dfn=\"Cascading Style Sheets\"}\n",
        ),
    ] {
        let result = html_to_carve(html, &HtmlImportOptions::default()).unwrap();
        assert_eq!(result.value, carve, "{html}");
    }
}

/// A name that carries a value takes it from the attribute HTML spells it in -
/// `title` for `abbr` and `dfn`, `datetime` for `time` - and the attribute is
/// CONSUMED rather than left behind as a second key saying the same thing.
#[test]
fn a_semantic_name_takes_its_value_from_the_attribute_that_carries_it() {
    for (html, carve) in [
        ("<p><abbr title=\"x\">A</abbr></p>", "[A]{abbr=x}\n"),
        ("<p><dfn title=\"x\">D</dfn></p>", "[D]{dfn=x}\n"),
        ("<p><time datetime=\"x\">t</time></p>", "[t]{time=x}\n"),
    ] {
        let result = html_to_carve(html, &HtmlImportOptions::default()).unwrap();
        assert_eq!(result.value, carve, "{html}");
        assert!(
            !result.value.contains("title") && !result.value.contains("datetime"),
            "the source attribute survived as a duplicate key: {}",
            result.value
        );
    }
}

/// A name with no value, or an element omitting the attribute it would carry
/// one in, gives the bare boolean.
#[test]
fn a_semantic_name_without_a_value_gives_the_bare_boolean() {
    for (html, carve) in [
        ("<p><abbr>HTML</abbr></p>", "[HTML]{abbr}\n"),
        ("<p><dfn>d</dfn></p>", "[d]{dfn}\n"),
        ("<p><time>t</time></p>", "[t]{time}\n"),
        ("<p><kbd>k</kbd></p>", "[k]{kbd}\n"),
    ] {
        let result = html_to_carve(html, &HtmlImportOptions::default()).unwrap();
        assert_eq!(result.value, carve, "{html}");
    }
}

/// A leftover `id`, `class` or `data-*` rides the same span, which is what the
/// attribute block already does elsewhere.
#[test]
fn leftover_attributes_ride_the_same_span() {
    let result = html_to_carve(
        "<p><abbr class=\"x\" id=\"z\" title=\"y\">A</abbr></p>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert_eq!(result.value, "[A]{#z .x abbr=y}\n");
}

/// The compact form nests, so nested elements do too - which the `:name[…]`
/// spelling could not have done.
#[test]
fn a_nested_semantic_element_nests() {
    let result = html_to_carve(
        "<p><kbd><kbd>Ctrl</kbd>+<kbd>C</kbd></kbd></p>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert_eq!(result.value, "[[Ctrl]{kbd}+[C]{kbd}]{kbd}\n");
}

/// The loss stops being reported because there is no longer one. `<time>` is
/// the load-bearing case: its `datetime` was diagnosed as an unsupported
/// attribute one step before the element was unwrapped.
#[test]
fn a_mapped_semantic_element_reports_no_loss() {
    for html in [
        "<p>Press <kbd>Tab</kbd></p>",
        "<p><abbr title=\"HyperText\">HTML</abbr></p>",
        "<p><time datetime=\"2026-01-01\">today</time></p>",
        "<p><samp>out</samp></p>",
        "<p><var>v</var></p>",
        "<p><cite>C</cite></p>",
        "<p><dfn>d</dfn></p>",
    ] {
        let result = html_to_ast(html, &HtmlImportOptions::default()).unwrap();
        assert_eq!(
            result
                .report
                .diagnostics
                .iter()
                .map(|d| d.code)
                .collect::<Vec<_>>(),
            Vec::<HtmlImportDiagnosticCode>::new(),
            "{html}"
        );
    }
}

/// All three modes map them, with no mode branch. `roundtrip` is not "preserve
/// everything verbatim" - it raw-preserves only what Carve CANNOT express - so
/// putting the seven with the other mapped elements settles every mode at once.
/// The consequence, stated rather than hidden: an exotic attribute on one of
/// the seven is now diagnosed as dropped in `roundtrip` instead of riding along
/// inside raw HTML, which is exactly the treatment `<mark>` and `<em>` get.
#[test]
fn every_mode_maps_the_seven_identically() {
    for mode in [
        HtmlImportMode::Safe,
        HtmlImportMode::Semantic,
        HtmlImportMode::Roundtrip,
    ] {
        let options = HtmlImportOptions {
            mode,
            ..Default::default()
        };
        let result = html_to_carve(
            "<p><kbd>Tab</kbd> <abbr title=\"H\">A</abbr> <time datetime=\"D\">t</time> <samp>o</samp> <var>v</var> <cite>c</cite> <dfn>d</dfn></p>",
            &options,
        )
        .unwrap();
        assert_eq!(
            result.value,
            "[Tab]{kbd} [A]{abbr=H} [t]{time=D} [o]{samp} [v]{var} [c]{cite} [d]{dfn}\n",
            "{mode:?}"
        );
        assert!(result.report.diagnostics.is_empty(), "{mode:?}");

        // `dir` rides ALONGSIDE the marker rather than being dropped: the
        // importer keeps every attribute Carve can hold, and a compact
        // semantic span's attribute slot holds one (carve-rs#1060).
        let exotic = html_to_carve("<p><kbd dir=\"rtl\">k</kbd></p>", &options).unwrap();
        assert_eq!(exotic.value, "[k]{dir=rtl kbd}\n", "{mode:?}");
        assert!(exotic.report.diagnostics.is_empty(), "{mode:?}");

        // The one name the marker cannot share is its OWN. Dropping it is
        // reported rather than silently overwritten by the empty marker value.
        let collision = html_to_carve("<p><kbd kbd=\"lit\">k</kbd></p>", &options).unwrap();
        assert_eq!(collision.value, "[k]{kbd}\n", "{mode:?}");
        assert_eq!(
            collision
                .report
                .diagnostics
                .iter()
                .map(|d| d.code)
                .collect::<Vec<_>>(),
            vec![HtmlImportDiagnosticCode::AttributeDropped],
            "{mode:?}"
        );
    }
}

/// The tier consequence, shown side by side rather than implied away. A core
/// name round-trips byte for byte; an extension name comes back as an ordinary
/// attribute until `SemanticSpan` is registered. That is still strictly better
/// than the unwrap it replaces, where the semantic was discarded outright
/// rather than surviving as an attribute a reader can recover.
#[test]
fn a_core_name_round_trips_and_an_extension_name_becomes_an_attribute() {
    use carve::extensions::semantic_span::SemanticSpan;

    let core = html_to_carve("<p>Press <kbd>Tab</kbd></p>", &HtmlImportOptions::default()).unwrap();
    assert_eq!(core.value, "Press [Tab]{kbd}\n");
    assert_eq!(
        carve::to_html(&core.value).trim(),
        "<p>Press <kbd>Tab</kbd></p>"
    );

    let ext = html_to_carve("<p><samp>out</samp></p>", &HtmlImportOptions::default()).unwrap();
    assert_eq!(ext.value, "[out]{samp}\n");
    assert_eq!(
        carve::to_html(&ext.value).trim(),
        "<p><span samp=\"\">out</span></p>",
        "core render"
    );
    let extension = SemanticSpan;
    let mut options = carve::Options::default();
    options.extensions.push(&extension);
    assert_eq!(
        carve::to_html_with_options(&ext.value, &options).trim(),
        "<p><samp>out</samp></p>",
        "render with SemanticSpan registered"
    );
}

/// The three carve-outs the ruling names, each of which must NOT change.
#[test]
fn mark_inline_code_and_a_code_block_are_left_alone() {
    let mark = html_to_carve("<p><mark>m</mark></p>", &HtmlImportOptions::default()).unwrap();
    assert_eq!(mark.value, "=m=\n");
    let code = html_to_carve("<p><code>c</code></p>", &HtmlImportOptions::default()).unwrap();
    assert_eq!(code.value, "`c`\n");
    let block = html_to_carve(
        "<pre><code class=\"language-js\">x()</code></pre>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert_eq!(block.value, "```js\nx()\n```\n");
}

/// An EXPLICITLY empty value is the bare boolean too, which the compact form
/// cannot tell from an absent one: `[A]{abbr=""}` and `[A]{abbr}` both render
/// `<abbr>A</abbr>`, so only the escape hatch `[A]{abbr title=""}` could carry
/// it. That spelling is deliberately not produced here. carve-js takes the same
/// value (`attr(node, source) ?? ''`), and the three engines have to agree byte
/// for byte before the shared fixtures land, so a rule only this engine has
/// would be the divergence rather than the fix. It is not a regression either:
/// before this change the element AND the title were both lost. Pinned so the
/// choice is stated rather than accidental.
#[test]
fn an_explicitly_empty_value_is_the_bare_boolean() {
    for (html, carve) in [
        ("<p><abbr title=\"\">A</abbr></p>", "[A]{abbr}\n"),
        ("<p><time datetime=\"\">t</time></p>", "[t]{time}\n"),
    ] {
        let result = html_to_carve(html, &HtmlImportOptions::default()).unwrap();
        assert_eq!(result.value, carve, "{html}");
    }
}

/// `<table><caption>` is how HTML captions a table, and it is what pandoc emits
/// for every captioned table. The row walk looks only for `tr`, so the caption
/// element was skipped and its text left the document with no diagnostic at all.
/// The slot was already there: `Table::caption` exists, the parser fills it, and
/// Carve spells it `^ text` after the rows (carve-js#1071 is the same gap).
#[test]
fn a_table_keeps_its_own_caption_on_import() {
    let result = html_to_carve(
        "<table><caption>Fruit prices</caption><thead><tr><th>A</th></tr></thead><tbody><tr><td>1</td></tr></tbody></table>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert!(
        result.value.contains("^ Fruit prices"),
        "the table caption was dropped: {}",
        result.value
    );
    assert!(
        result.report.diagnostics.is_empty(),
        "a representable caption should report nothing: {:?}",
        result.report.diagnostics
    );
}

/// PART 9 §16a: the endnotes `<section>` is a DERIVED WRAPPER, so unwrapping it
/// reports nothing - but only what the property actually reaches goes quiet.
///
/// The suppression has to be per-value, not per-element. Silencing the element
/// row and the attribute rows together would take the author's own attributes
/// down with the renderer's, which is the failure the clause names outright.
mod a_derived_endnotes_section {
    use super::*;

    const NOTES: &str = "<hr><ol><li><p>Note text.</p></li></ol>";

    fn codes(html: &str) -> Vec<HtmlImportDiagnosticCode> {
        html_to_ast(html, &HtmlImportOptions::default())
            .unwrap()
            .report
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect()
    }

    #[test]
    fn a_wholly_derived_one_reports_nothing() {
        let html =
            format!("<section role=\"doc-endnotes\" aria-label=\"Footnotes\">{NOTES}</section>");
        assert_eq!(codes(&html), Vec::new());
    }

    #[test]
    fn an_authored_class_on_it_is_still_reported() {
        let html = format!(
            "<section role=\"doc-endnotes\" aria-label=\"Footnotes\" class=\"mine\">{NOTES}</section>"
        );
        assert_eq!(
            codes(&html),
            vec![HtmlImportDiagnosticCode::AttributeDropped]
        );
    }

    #[test]
    fn a_name_no_default_matches_is_still_reported() {
        // Rendered with a German labels map, so the value is not one this
        // importer can rebuild: it is indistinguishable from an authored name
        // and is reported rather than silently dropped.
        let html =
            format!("<section role=\"doc-endnotes\" aria-label=\"Fussnoten\">{NOTES}</section>");
        assert_eq!(
            codes(&html),
            vec![HtmlImportDiagnosticCode::AttributeDropped]
        );
    }

    #[test]
    fn a_section_that_is_not_the_endnotes_one_still_reports_its_unwrap() {
        // The control: without it, the suppression could be reading "section"
        // alone and nothing here would notice.
        let html = "<section role=\"region\" aria-label=\"X\"><p>N.</p></section>";
        assert_eq!(
            codes(html),
            vec![
                HtmlImportDiagnosticCode::ElementUnwrapped,
                HtmlImportDiagnosticCode::AttributeDropped,
                HtmlImportDiagnosticCode::AttributeDropped,
            ]
        );
    }
}
