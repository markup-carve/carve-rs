//! An HTML import keeps every attribute Carve can hold (carve-rs#1060).
//!
//! The importer's attribute policy was a KEEP list: `data-*` plus a handful of
//! named cases survived and everything else was dropped, so `aria-label` and an
//! author's own `foo="bar"` were lost. Maintainer ruling on
//! `markup-carve/carve-php#1337`: retention is the correct behavior, and
//! carve-js shipped its half as `markup-carve/carve-js#1157`. Dropping
//! `aria-label` is an accessibility regression applied silently and in bulk to
//! exactly the documents an importer runs on, and Carve's attribute syntax can
//! hold the pair, so it was a choice rather than a limitation.
//!
//! The policy is a REFUSAL list now, and the refusals are DERIVED: the importer
//! calls `is_dangerous_attr_name`, the PART 9 §25 name filter the HTML renderer
//! already applies, rather than spelling a second `starts_with("on")` that can
//! drift away from it.
//!
//! EVERY ASSERTION HERE READS THE EMITTED CARVE SOURCE, never rendered HTML.
//! The renderer strips `on*` on the way out, so a render assertion passes while
//! the source is dirty - `assert_the_render_would_have_hidden_it` below is that
//! trap, pinned so nobody re-introduces the weaker check.

use carve::{html_to_carve, to_html, HtmlImportMode, HtmlImportOptions};

fn imported(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .unwrap()
        .value
}

/// The measured table from the report: the middle band flips to kept, the two
/// ends stay where all three engines already agreed.
#[test]
fn the_middle_band_is_kept_and_the_two_ends_are_unchanged() {
    assert_eq!(
        imported("<blockquote aria-label=\"note\">x</blockquote>"),
        "{aria-label=note}\n> x\n"
    );
    assert_eq!(
        imported("<blockquote foo=\"bar\">x</blockquote>"),
        "{foo=bar}\n> x\n"
    );
    // Already kept before, and still kept: the widening did not disturb it.
    assert_eq!(
        imported("<blockquote data-x=\"1\">x</blockquote>"),
        "{data-x=1}\n> x\n"
    );
    // Already stripped before, and still stripped.
    for html in [
        "<blockquote onclick=\"e()\">x</blockquote>",
        "<blockquote style=\"color:red\">x</blockquote>",
    ] {
        assert_eq!(imported(html), "> x\n", "{html}");
    }
}

/// The refusal is the RENDERER'S filter, not a second list beside it. `srcdoc`
/// and `formaction` are the two names that separate the two spellings: a
/// hand-written `starts_with("on")` keeps them, `is_dangerous_attr_name`
/// refuses them, and the importer now answers the same way the renderer does.
#[test]
fn the_two_sinks_only_the_shared_filter_knows_are_refused() {
    for name in ["srcdoc", "formaction"] {
        let html = format!("<blockquote {name}=\"x\">q</blockquote>");
        assert_eq!(imported(&html), "> q\n", "{name} survived the import");
    }
}

/// `srcset` is REFUSED ON THE WAY IN, and it is the one refusal here that is
/// not derived.
///
/// This is the IMPORTER declining to admit a list-valued URL attribute the
/// keep list never reached, which is a separate decision from what the
/// renderer does with one an author wrote by hand. The §25 half is
/// `markup-carve/carve#1320`, now ruled and implemented - the value sanitizer
/// probes a URL-list value at every candidate rather than at its head, see
/// `tests/a_url_list_attribute_is_probed_at_every_candidate.rs`. The refusal
/// here neither waits on that nor duplicates it: admitting the attribute would
/// be widening retention, and that is its own call.
#[test]
fn a_list_valued_url_attribute_is_refused_rather_than_carried() {
    let source =
        imported("<img src=\"a.png\" alt=\"a\" srcset=\"safe.png 1x, javascript:alert(1) 2x\">");
    assert!(
        !source.contains("srcset") && !source.contains("javascript:"),
        "the smuggled URL reached the source: {source}"
    );
    assert_eq!(source, "![a](a.png)\n");
}

/// THE TRAP THIS FILE EXISTS TO AVOID. The renderer strips `on*` on output, so
/// asserting on rendered HTML says nothing about what the import kept. Shown
/// with a document whose SOURCE is deliberately dirty: the render is clean
/// either way, so only the source assertion can fail.
#[test]
fn assert_the_render_would_have_hidden_it() {
    let dirty = "{onclick=\"steal()\"}\n> q\n";
    assert!(
        !to_html(dirty).contains("onclick"),
        "the renderer stopped stripping handlers, and this file's premise with it"
    );
    assert!(
        dirty.contains("onclick"),
        "the source a render assertion would have called clean"
    );
}

/// THE ADVERSARY. Names in nobody's enumeration, on every element category the
/// importer routes differently, in both non-roundtrip modes, asserting the
/// value never reaches the emitted Carve source.
///
/// A sentinel VALUE rather than the name is what is searched for: a name can
/// appear in prose or in a marker by coincidence, a sentinel cannot.
#[test]
fn no_active_attribute_reaches_the_source_on_any_element_category() {
    // `%ATTR%` is replaced by the attribute under test.
    let shapes = [
        "<p %ATTR%>t</p>",
        "<div %ATTR%><p>t</p></div>",
        "<blockquote %ATTR%>t</blockquote>",
        "<h2 %ATTR%>t</h2>",
        "<hr %ATTR%>",
        "<pre %ATTR%><code>t</code></pre>",
        "<ul %ATTR%><li>t</li></ul>",
        "<ul><li %ATTR%>t</li></ul>",
        "<ol %ATTR%><li>t</li></ol>",
        "<dl %ATTR%><dt>a</dt><dd>b</dd></dl>",
        "<dl><dt %ATTR%>a</dt><dd>b</dd></dl>",
        "<dl><dt>a</dt><dd %ATTR%>b</dd></dl>",
        "<table %ATTR%><tr><td>a</td></tr></table>",
        "<table><tr %ATTR%><td>a</td></tr></table>",
        "<table><tr><td %ATTR%>a</td></tr></table>",
        "<table><tr><th %ATTR%>a</th></tr></table>",
        "<table><tbody %ATTR%><tr><td>a</td></tr></tbody></table>",
        "<table><caption %ATTR%>c</caption><tr><td>a</td></tr></table>",
        "<details %ATTR%><summary>s</summary><p>b</p></details>",
        "<details><summary %ATTR%>s</summary><p>b</p></details>",
        "<figure %ATTR%><img src=\"a.png\" alt=\"a\"><figcaption>c</figcaption></figure>",
        "<figure><img src=\"a.png\" alt=\"a\"><figcaption %ATTR%>c</figcaption></figure>",
        "<section %ATTR%><p>t</p></section>",
        "<article %ATTR%><p>t</p></article>",
        "<p><em %ATTR%>t</em></p>",
        "<p><strong %ATTR%>t</strong></p>",
        "<p><del %ATTR%>t</del></p>",
        "<p><ins %ATTR%>t</ins></p>",
        "<p><mark %ATTR%>t</mark></p>",
        "<p><sub %ATTR%>t</sub></p>",
        "<p><sup %ATTR%>t</sup></p>",
        "<p><code %ATTR%>t</code></p>",
        "<p><a href=\"u\" %ATTR%>t</a></p>",
        "<p><img src=\"a.png\" alt=\"a\" %ATTR%></p>",
        "<p><span %ATTR%>t</span></p>",
        "<p><q %ATTR%>t</q></p>",
        "<p><abbr title=\"T\" %ATTR%>t</abbr></p>",
        "<p><time datetime=\"D\" %ATTR%>t</time></p>",
        "<p><kbd %ATTR%>t</kbd></p>",
        "<p><samp %ATTR%>t</samp></p>",
        "<p><var %ATTR%>t</var></p>",
        "<p><cite %ATTR%>t</cite></p>",
        "<p><dfn %ATTR%>t</dfn></p>",
        "<p>a<br %ATTR%>b</p>",
        "<p><small %ATTR%>t</small></p>",
        "<p><bdi %ATTR%>t</bdi></p>",
    ];
    // Handler names no enumeration lists, mixed case included, plus the two
    // sinks and `style`. None of these has a bare spelling in any keep list.
    let attrs = [
        "onclick=\"PWNED\"",
        "onmouseover=\"PWNED\"",
        "onfocus=\"PWNED\"",
        "onpointerdown=\"PWNED\"",
        "onanimationstart=\"PWNED\"",
        "ontransitionend=\"PWNED\"",
        "OnClIcK=\"PWNED\"",
        "ONFOCUS=\"PWNED\"",
        "srcdoc=\"PWNED\"",
        "formaction=\"PWNED\"",
        "style=\"background:url(PWNED)\"",
        "srcset=\"a.png 1x, javascript:PWNED 2x\"",
    ];
    let mut checked = 0usize;
    for mode in [HtmlImportMode::Safe, HtmlImportMode::Semantic] {
        let options = HtmlImportOptions {
            mode,
            ..Default::default()
        };
        for shape in shapes {
            for attr in attrs {
                let html = shape.replace("%ATTR%", attr);
                let source = html_to_carve(&html, &options).unwrap().value;
                assert!(
                    !source.contains("PWNED"),
                    "{mode:?} leaked into the source\n  input:  {html}\n  source: {source}"
                );
                checked += 1;
            }
        }
    }
    assert_eq!(checked, shapes.len() * attrs.len() * 2);
}

/// The same sweep with a HARMLESS attribute, so the one above cannot pass by
/// dropping everything. Each shape must carry the sentinel through to the
/// source somewhere, OR say in the report that it could not.
#[test]
fn a_harmless_attribute_is_kept_or_named_never_silently_dropped() {
    let shapes = [
        "<p aria-label=\"KEPT\">t</p>",
        "<div aria-label=\"KEPT\"><p>t</p></div>",
        "<blockquote aria-label=\"KEPT\">t</blockquote>",
        "<h2 aria-label=\"KEPT\">t</h2>",
        "<ul aria-label=\"KEPT\"><li>t</li></ul>",
        "<ul><li aria-label=\"KEPT\">t</li></ul>",
        "<table><tr><td aria-label=\"KEPT\">a</td></tr></table>",
        "<p><em aria-label=\"KEPT\">t</em></p>",
        "<p><a href=\"u\" aria-label=\"KEPT\">t</a></p>",
        "<p><img src=\"a.png\" alt=\"a\" aria-label=\"KEPT\"></p>",
        "<p><span aria-label=\"KEPT\">t</span></p>",
        "<p><kbd aria-label=\"KEPT\">t</kbd></p>",
        // These three have nowhere to put it, and each says so.
        "<p>a<br aria-label=\"KEPT\">b</p>",
        "<section aria-label=\"KEPT\"><p>t</p></section>",
        "<p><small aria-label=\"KEPT\">t</small></p>",
    ];
    for shape in shapes {
        let result = html_to_carve(shape, &HtmlImportOptions::default()).unwrap();
        let named = result
            .report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("aria-label"));
        assert!(
            result.value.contains("aria-label") || named,
            "silently dropped\n  input:  {shape}\n  source: {}",
            result.value
        );
    }
}

/// An unwrapped element takes its attributes with it, and SAYS SO. The keep
/// list refused most of these names inside the policy and reported them there,
/// so widening retention without a report at the unwrap would have converted a
/// reported loss into a silent one - the opposite of what the ticket asks for.
#[test]
fn an_unwrapped_element_names_the_attributes_it_could_not_carry() {
    for html in [
        "<section role=\"region\"><p>t</p></section>",
        "<p><small dir=\"rtl\">t</small></p>",
        "<p>a<br clear=\"all\">b</p>",
    ] {
        let result = html_to_carve(html, &HtmlImportOptions::default()).unwrap();
        assert!(
            result
                .report
                .diagnostics
                .iter()
                .any(|d| d.message.starts_with("Dropped")),
            "no loss reported for {html}"
        );
    }
}

/// A name with NO BARE SPELLING in Carve attribute syntax is dropped and named,
/// not kept and quietly rewritten. `escape_attr_key` strips every character the
/// writer's identifier rule rejects, so keeping `xlink:href` would have emitted
/// `xlinkhref` and the document would claim an attribute nobody wrote.
#[test]
fn a_name_the_writer_would_rewrite_is_refused_instead() {
    for name in ["xlink:href", "1foo", "~onclick"] {
        let html = format!("<blockquote {name}=\"u\">q</blockquote>");
        let result = html_to_carve(&html, &HtmlImportOptions::default()).unwrap();
        assert_eq!(result.value, "> q\n", "{name} reached the source");
        assert!(
            !result.value.contains("xlinkhref"),
            "{name} was rewritten rather than refused"
        );
    }
}

/// A compact semantic span's MARKER owns its key, so an attribute of the same
/// name has no room. The keep list hid this by refusing `cite` on everything
/// but a `<blockquote>`; the widening makes it reachable, so it is named rather
/// than silently overwritten by the marker's empty value.
#[test]
fn the_marker_of_a_semantic_span_does_not_silently_eat_a_same_named_attribute() {
    let result = html_to_carve(
        "<p><cite cite=\"https://x.example\">c</cite></p>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert_eq!(result.value, "[c]{cite}\n");
    assert!(
        result
            .report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("cite")),
        "the collision was not reported: {:?}",
        result.report.diagnostics
    );
}

/// A link's and an image's `title` is READ into the destination slot, so it must
/// not ALSO be stored as a key. The keep list spelled this as
/// `title && tag != "a" && tag != "img"`; the refusal list has to spell it in
/// the consumed set instead, or the attribute comes out twice.
#[test]
fn a_title_a_slot_already_holds_is_not_spelled_twice() {
    assert_eq!(
        imported("<p><a href=\"u\" title=\"t\">x</a></p>"),
        "[x](u \"t\")\n"
    );
    assert_eq!(
        imported("<p><img src=\"a.png\" alt=\"a\" title=\"t\"></p>"),
        "![a](a.png \"t\")\n"
    );
    // On anything else there is no slot, so the key IS the spelling.
    assert_eq!(imported("<p title=\"t\">x</p>"), "{title=t}\nx\n");
}

/// `lang` comes back in the shorthand the language has for it rather than as an
/// ordinary key, which is what carve-js does too.
#[test]
fn a_language_tag_uses_the_shorthand() {
    assert_eq!(imported("<p lang=\"fr\">x</p>"), "{:fr}\nx\n");
}
