//! A line block's body is verse: nothing inside one is claimed (PART 9 §23).
//!
//! The definition prepass kept a definition-shaped verse line - that was
//! carve-rs#491, and it fixed the content loss - but went on REGISTERING it,
//! deliberately, because whether any engine should was still open (carve#557).
//! carve#574 answered it: the line renders and defines nothing.

fn html(source: &str) -> String {
    carve::to_html(source)
        .replace('\n', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn a_definition_in_verse_does_not_resolve_elsewhere() {
    let out = html("::: |\n[d]: /u\n:::\n\nsee [d][]\n");

    assert!(out.contains("[d]: /u"), "the verse line vanished: {out}");
    assert!(
        !out.contains("href=\"/u\""),
        "the definition registered: {out}"
    );
}

#[test]
fn a_definition_after_the_verse_still_resolves() {
    let out = html("::: |\nverse\n:::\n\n[d]: /u\n\nsee [d][]\n");

    assert!(out.contains("href=\"/u\""), "{out}");
}

#[test]
fn a_wider_verse_fence_closes_on_its_own_width() {
    let out = html(":::: |\n[d]: /u\n:::\nstill verse\n::::\n\nsee [d][]\n");

    assert!(!out.contains("href=\"/u\""), "{out}");
}

#[test]
fn a_footnote_definition_in_verse_stays_literal() {
    let out = html("::: |\n[^f]: t\n:::\n");

    assert!(out.contains("[^f]: t"), "{out}");
    assert!(!out.contains("doc-endnotes"), "{out}");
}

#[test]
fn ownership_discovery_does_not_parse_verse_inlines_twice() {
    use carve::{CarveExtension, InlineMatch, InlineNode, MatcherContext, Options};
    use std::cell::Cell;

    struct Counter(Cell<usize>);
    impl CarveExtension for Counter {
        fn name(&self) -> &'static str {
            "verse-counter"
        }
        fn match_inline(
            &self,
            text: &str,
            pos: usize,
            _ctx: &MatcherContext<'_>,
        ) -> Option<InlineMatch> {
            text.get(pos..)?.strip_prefix("§token")?;
            self.0.set(self.0.get() + 1);
            Some(InlineMatch {
                node: InlineNode::text("matched"),
                end: pos + "§token".len(),
            })
        }
    }

    let counter = Counter(Cell::new(0));
    let options = Options::new().with_extension(&counter);
    let out = carve::to_html_with_options(
        "§token\n\n::: |\n§token\n[r]: /hidden\n:::\n\n[t][r]\n",
        &options,
    );
    assert!(out.contains("matched"), "{out}");
    assert!(!out.contains("href=\"/hidden\""), "{out}");
    assert_eq!(counter.0.get(), 2);
}

#[test]
fn indented_comment_closer_in_verse() {
    assert_eq!(carve::to_html("::: |\n%%%\n:::\n  %%%\nkept\n:::\n\n[r]: /url\n\n[t][r]\n"), "<div class=\"line-block\">\n  <p><br>\n:::<br>\n&nbsp;&nbsp;%%%<br>\nkept</p>\n</div>\n<p><a href=\"/url\">t</a></p>");
}

#[test]
fn indented_comment_closer_in_div() {
    assert_eq!(
        carve::to_html("::: container\n%%%\n:::\n  %%%\nkept\n:::\n\n[r]: /url\n\n[t][r]\n"),
        "<div class=\"container\">\n  <p>kept</p>\n</div>\n<p><a href=\"/url\">t</a></p>"
    );
}
