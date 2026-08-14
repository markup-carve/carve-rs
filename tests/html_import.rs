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

#[test]
fn shared_contract_fixtures_match() {
    let root = Path::new("tests/spec/tests/html-import");
    for entry in fs::read_dir(root).unwrap() {
        let dir = entry.unwrap().path();
        if !dir.is_dir() {
            continue;
        }
        let html = fs::read_to_string(dir.join("input.html")).unwrap();
        let expected = fs::read_to_string(dir.join("expected.crv")).unwrap();
        let expected_report: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join("expected.report.json")).unwrap())
                .unwrap();
        let result = html_to_carve(&html, &HtmlImportOptions::default()).unwrap();
        assert_eq!(result.value, expected, "{}", dir.display());
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
            .map(|d| match d.code {
                HtmlImportDiagnosticCode::ElementDropped => "element-dropped",
                HtmlImportDiagnosticCode::ElementUnwrapped => "element-unwrapped",
                HtmlImportDiagnosticCode::AttributeDropped => "attribute-dropped",
                HtmlImportDiagnosticCode::StyleUnmapped => "style-unmapped",
                HtmlImportDiagnosticCode::TableDegraded => "table-degraded",
                HtmlImportDiagnosticCode::RawPreserved => "raw-preserved",
                HtmlImportDiagnosticCode::DiagnosticsTruncated => "diagnostics-truncated",
            })
            .collect::<Vec<_>>();
        assert_eq!(actual_codes, expected_codes, "{}", dir.display());
    }
}

/// PART 9 §4a, carve#1159. The renderer emits a quote's attribution as a
/// `<footer>` inside the `<blockquote>`, so an importer that read it as an
/// ordinary second paragraph made the engine's own HTML un-round-trippable.
#[test]
fn a_trailing_footer_in_a_quote_is_its_attribution() {
    let result = html_to_carve(
        "<blockquote><p>To be</p><footer>Hamlet</footer></blockquote>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert_eq!(result.value, "> To be\n^ Hamlet\n");
}

/// A quote has ONE attribution, so a second footer cannot join it. The LAST is
/// the one this renderer emits and the one an author puts after the quoted
/// text; the earlier footer stays an ordinary block rather than being dropped.
#[test]
fn the_last_footer_is_the_attribution_and_the_others_stay() {
    let result = html_to_carve(
        "<blockquote><footer>First</footer><p>To be</p><footer>Hamlet</footer></blockquote>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert_eq!(result.value, "> First\n>\n> To be\n^ Hamlet\n");
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

        let exotic = html_to_carve("<p><kbd dir=\"rtl\">k</kbd></p>", &options).unwrap();
        assert_eq!(exotic.value, "[k]{kbd}\n", "{mode:?}");
        assert_eq!(
            exotic
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
    assert_eq!(block.value, "``` js\nx()\n```\n");
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
