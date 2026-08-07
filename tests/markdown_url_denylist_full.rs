//! A Markdown destination is resolved by whatever renders that Markdown, so a
//! scheme blanked in HTML and passed through there is not blocked -- it is the same
//! sink one step removed (PART 9 section 25, markup-carve/carve#385).
//!
//! The Markdown renderer carried a local copy of the denylist listing four schemes
//! and probed with an ASCII-only filter, so the twenty OS protocol-handler schemes
//! reached the output while the HTML renderer blanked them. Both the set and the
//! probe now come from `escape`.

const NNBSP: &str = "\u{202f}";

fn destination_of(src: &str) -> String {
    let md = carve::to_markdown(src);
    let start = match md.find("](") {
        Some(i) => i + 2,
        None => return "<none>".to_string(),
    };
    let rest = &md[start..];
    match rest.find(')') {
        Some(end) => rest[..end].to_string(),
        None => "<unterminated>".to_string(),
    }
}

fn link_to(url: &str) -> String {
    format!("[click][a]\n\n[a]: {url}\n")
}

#[test]
fn every_dangerous_scheme_is_blanked() {
    for url in [
        "javascript:alert(1)",
        "vbscript:msgbox(1)",
        "data:text/html,<script>x</script>",
        "file:///etc/passwd",
        "ms-msdt:/id PCWDiagnostic",
        "search-ms:query=x",
        "shell:startup",
        "vscode://x",
        "jar:http://x!/",
    ] {
        // Two safe answers, and the second is new: a destination BLANKED by the
        // denylist, or no link at all. `ms-msdt:/id PCWDiagnostic` carries a
        // space, so under carve#911 the definition line is anchored at end of
        // line and what follows the destination makes the production fail - the
        // line is an ordinary paragraph and the reference never resolves, which
        // is strictly stronger than blanking it.
        let dest = destination_of(&link_to(url));
        assert!(
            dest.is_empty() || dest == "<none>",
            "survived: {url} -> {dest}"
        );
        // The guarantee itself, whichever answer the line took: the scheme
        // never reaches a destination.
        let md = carve::to_markdown(&link_to(url));
        assert!(!md.contains(&format!("]({url}")), "survived in {md}");
    }
}

#[test]
fn a_scheme_hidden_behind_unicode_whitespace_is_blanked() {
    let url = format!("{NNBSP}javascript:alert(1)");
    assert_eq!(destination_of(&link_to(&url)), "");
}

#[test]
fn it_agrees_with_the_html_target() {
    // Which is the whole point: one target must not undo the other's blanking.
    for url in ["ms-msdt:/id", "javascript:alert(1)"] {
        let src = link_to(url);
        assert!(
            carve::to_html(&src).contains("href=\"\""),
            "html kept {url}"
        );
        assert_eq!(destination_of(&src), "", "markdown kept {url}");
    }
}

#[test]
fn ordinary_schemes_are_untouched() {
    for url in ["https://example.com/ok", "mailto:a@b.com", "tel:+15551234"] {
        assert_eq!(destination_of(&link_to(url)), url, "blanked: {url}");
    }
}
