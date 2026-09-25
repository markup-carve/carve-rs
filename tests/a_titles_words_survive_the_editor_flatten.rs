//! A container title reaches the ProseMirror bridge as a plain string, and the
//! flatten used to emit only the title's top-level text runs: `::: toc "A *b* c"`
//! rode the wire as `A  c`, the word gone and the two spaces around it left
//! standing (carve-rs#1944).
//!
//! The ruling is authorship-and-location rather than node kind. An inline
//! contributes the text the author typed IN THIS TITLE - the markup goes, the
//! words stay - and contributes nothing where the text is generated or lives
//! elsewhere: a minted footnote marker, a citation label from the bibliography,
//! a crossref's resolved number.
//!
//! Every case runs on `::: toc` and on `::: note`, because the two share the
//! flatten and carve-rs#1941's control is that they answer alike.

use serde_json::Value;

/// The title string the bridge writes, for the one container in `src`.
///
/// `None` where no title attribute was written at all, which is how upstream
/// spells an empty projection.
fn projected(src: &str) -> Option<String> {
    let pm = carve::to_prosemirror(&carve::parse(src));
    let doc: Value = serde_json::from_str(&pm.json).expect("the bridge writes JSON");
    let attrs = &doc["content"][0]["attrs"];
    attrs
        .get("title")
        .or_else(|| attrs.get("carveAdmonitionTitle"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// The same, for a title that Carve 0.1 source cannot spell.
fn projected_wire(container: &str, kind: &str, title: &str) -> Option<String> {
    let wire = format!(
        "{{\"type\":\"document\",\"srcByteLength\":0,\"children\":[{{\"type\":\"{container}\",\
         \"kind\":\"{kind}\",\"title\":{title},\"children\":[]}}]}}"
    );
    let doc = carve::from_json(&wire).expect("the wire document decodes");
    let pm = carve::to_prosemirror(&doc);
    let parsed: Value = serde_json::from_str(&pm.json).expect("the bridge writes JSON");
    let attrs = &parsed["content"][0]["attrs"];
    attrs
        .get("title")
        .or_else(|| attrs.get("carveAdmonitionTitle"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn titled(kind: &str, title: &str) -> String {
    format!("::: {kind} \"{title}\"\n:::\n")
}

#[test]
fn the_word_inside_the_markup_survives_and_leaves_no_gap() {
    for kind in ["toc", "note"] {
        let title = projected(&titled(kind, "A *b* c")).expect("a title was written");
        // The gap is the bug, not a cosmetic residue: `A  c` is what says the
        // surrounding runs were emitted and the middle one dropped.
        assert_eq!(title, "A b c", "{kind}: the flatten lost the word");
    }
}

#[test]
fn every_spellable_inline_kind_answers_the_ruling() {
    // `A <x> c` throughout, so a kind that contributes nothing shows the gap
    // and a kind that contributes shows exactly where its words landed.
    let cases: &[(&str, &str, &str)] = &[
        ("/b/", "A b c", "emphasis"),
        ("*b*", "A b c", "strong"),
        ("/*b*/", "A b c", "the combined token"),
        ("_b_", "A b c", "underline"),
        ("~b~", "A b c", "strike"),
        ("=b=", "A b c", "highlight"),
        ("{^b^}", "A b c", "superscript"),
        ("{,b,}", "A b c", "subscript"),
        ("`b`", "A b c", "code"),
        ("[b](x)", "A b c", "a link's label"),
        ("![b](x)", "A b c", "an image's alt text"),
        ("[b]{.k}", "A b c", "a span"),
        ("{+b+}", "A b c", "a critic insertion"),
        ("{-b-}", "A b c", "a critic deletion"),
        ("[b [d](y) e](x)", "A b d e c", "a label two levels down"),
        // Generated, or living elsewhere.
        ("[^1]", "A  c", "a footnote reference is a minted marker"),
        (
            "^[b]",
            "A  c",
            "an inline footnote's body is not in the title",
        ),
        ("</#s>", "A  c", "a crossref resolves against the target"),
        (":smile:", "A  c", "a symbol's glyph comes from the map"),
        ("$`b`", "A  c", "math is a formula, not words"),
        ("`b`{=html}", "A  c", "a raw inline is one target's markup"),
        ("!`b`", "A  c", "a literal inline"),
        ("@u", "A  c", "a mention resolves elsewhere"),
        ("#t", "A  c", "a tag resolves elsewhere"),
        ("{~o~>n~}", "A  c", "a critic substitution"),
        ("{#cm#}", "A  c", "a critic comment is not the title"),
        ("<https://x.test>", "A  c", "an autolink"),
    ];
    for kind in ["toc", "note"] {
        for (spelling, want, why) in cases {
            let src = titled(kind, &format!("A {spelling} c"));
            assert_eq!(
                projected(&src).as_deref(),
                Some(*want),
                "{kind}: {spelling} ({why})"
            );
        }
    }
}

#[test]
fn an_interchange_only_kind_answers_the_ruling() {
    const TEXT_B: &str = "[{\"type\":\"text\",\"value\":\"b\"}]";
    for (container, kind) in [("directive", "toc"), ("admonition", "note")] {
        // Small caps has children, so its words stay like any other wrapper.
        assert_eq!(
            projected_wire(
                container,
                kind,
                &format!("[{{\"type\":\"small_caps\",\"children\":{TEXT_B}}}]")
            )
            .as_deref(),
            Some("b"),
            "{container}: small caps lost its words"
        );
        // A ruby's base and its annotation are both authored, but they are not a
        // reading sequence - `base(annotation)` is how the writer spells one it
        // cannot keep, so only the base is the title's words. Upstream has no
        // ruby case at all: `inlinePlainText` tests `node.children` and a ruby
        // carries `pairs`.
        assert_eq!(
            projected_wire(
                container,
                kind,
                &format!(
                    "[{{\"type\":\"ruby\",\"pairs\":[{{\"base\":{TEXT_B},\"annotation\":\
                     [{{\"type\":\"text\",\"value\":\"ann\"}}]}}]}}]"
                )
            )
            .as_deref(),
            Some("b"),
            "{container}: the ruby base is the title's words"
        );
        // A caption number is minted by the renderer.
        assert_eq!(
            projected_wire(container, kind, "[{\"type\":\"caption_number\",\"n\":2}]").as_deref(),
            Some(""),
            "{container}: a minted number reached the title"
        );
    }
}

#[test]
fn a_title_with_no_child_bearing_inline_is_unchanged() {
    // The control for the recursion: these titles held no inline to descend
    // into, so they answered correctly before the fix and must still.
    for kind in ["toc", "note"] {
        assert_eq!(projected(&titled(kind, "A b c")).as_deref(), Some("A b c"));
        assert_eq!(
            projected(&titled(kind, "A -- c")).as_deref(),
            Some("A \u{2013} c"),
            "{kind}: the smart glyph left the title"
        );
    }
}

#[test]
fn a_footnote_only_title_projects_to_the_empty_string() {
    for kind in ["toc", "note"] {
        let title = projected(&titled(kind, "[^1]")).expect("a title attribute was written");
        assert_eq!(title, "", "{kind}: a marker was minted into the title");
    }
}

#[test]
fn the_words_ride_back_out_through_the_bridge() {
    // Both halves are load-bearing: the outbound flatten has to carry the words
    // and the inbound half has to read them back as the opener's title rather
    // than as an authored `title` attribute.
    for kind in ["toc", "note"] {
        let pm = carve::to_prosemirror(&carve::parse(&titled(kind, "A *b* c")));
        let back = carve::from_prosemirror(&pm.json).expect("the payload decodes");
        assert_eq!(
            carve::render_carve(&back).expect("the way back writes"),
            format!("::: {kind} \"A b c\"\n\n:::\n"),
            "{kind}: the round trip lost the words or renamed the title"
        );
    }
}
