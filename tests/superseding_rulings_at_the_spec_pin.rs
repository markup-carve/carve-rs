//! Superseding rulings of 1 November 2026, as the spec model renders them.

use carve::{lint_carve, to_carve, to_html};

fn html(source: &str) -> String {
    to_html(source).trim().to_string()
}

fn ext(body: &str) -> String {
    format!("<span class=\"ext-foo\">{body}</span>")
}

#[test]
fn an_extension_closes_at_bracket_depth_zero() {
    for (source, out) in [
        (":foo[[x](u) y]", ext("<a href=\"u\">x</a> y")),
        (":foo[a [b] c]", ext("a [b] c")),
        (":foo[a [b [c] d] e]", ext("a [b [c] d] e")),
        (":foo[[x]]", ext("[x]")),
        (":foo[a]b]", format!("{}b]", ext("a"))),
        (":foo[a [b c] d", format!("{} d", ext("a [b c"))),
        (":foo[[[a] b] c", format!("{} b] c", ext("[[a"))),
        (":foo[a\\]b]", ext("a]b")),
        (":foo[a\\[b] c]", format!("{} c]", ext("a[b"))),
        (":foo[a\\\\]b]", format!("{}b]", ext("a\\"))),
    ] {
        assert_eq!(html(source), format!("<p>{out}</p>"), "{source}");
    }
}

#[test]
fn an_arrow_needs_its_exact_run() {
    for (source, out) in [
        ("a<---b", "a&lt;—b"),
        ("<--->", "&lt;—&gt;"),
        ("a--->b", "a—&gt;b"),
        ("a<----b", "a&lt;––b"),
        ("a <--- b", "a &lt;— b"),
        ("a ---> b", "a ---&gt; b"),
        ("a<--b", "a←b"),
        ("a-->b", "a→b"),
        ("a<-->b", "a↔b"),
        ("<-->-", "↔-"),
    ] {
        assert_eq!(html(source), format!("<p>{out}</p>"), "{source}");
    }
}

#[test]
fn single_hyphen_arrows_and_c_and_r_are_text() {
    assert_eq!(
        html("a<-b x->y a<->b"),
        "<p>a&lt;-b x-&gt;y a&lt;-&gt;b</p>"
    );
    assert_eq!(
        html("(a), (b) and (c) (r) (tm)"),
        "<p>(a), (b) and (c) (r) ™</p>"
    );
}

#[test]
fn a_mention_or_tag_name_holds_a_letter() {
    assert_eq!(html("Paid @5 and #123"), "<p>Paid @5 and #123</p>");
    assert!(html("#1a").contains("class=\"tag\""));
    assert!(html("@a1").contains("class=\"mention\""));
}

#[test]
fn a_marker_between_an_items_columns_reads_as_at_the_marker_column() {
    assert_eq!(
        html("- a\n * b\n"),
        "<ul>\n  <li>a</li>\n</ul>\n<ul>\n  <li>b</li>\n</ul>"
    );
    assert_eq!(
        html("- a\n - b\n"),
        "<ul>\n  <li>a</li>\n  <li>b</li>\n</ul>"
    );
    assert!(html("- a\n - [ ] b\n").contains("<ul class=\"task-list\">"));
}

#[test]
fn a_letter_or_roman_marker_with_a_dot_needs_two_spaces() {
    assert_eq!(html("e. g. foo\n"), "<p>e. g. foo</p>");
    assert_eq!(
        html("a.  one\nb.  two\n"),
        "<ol type=\"a\">\n  <li>one</li>\n  <li>two</li>\n</ol>"
    );
    assert_eq!(
        html("a) x\n"),
        "<ol type=\"a\" data-delim=\")\">\n  <li>x</li>\n</ol>"
    );
    assert_eq!(to_carve("a.  one\nb.  two\n"), "a.  one\nb.  two\n");
    assert_eq!(to_carve("iv.  x\n    y\n"), "iv.  x\n     y\n");
}

#[test]
fn a_code_fence_with_invalid_metadata_keeps_its_language() {
    assert_eq!(
        html("```js {1,3}\na\n```\n\ntext *x*\n\n```py title=\"t\"\nb\n```\n\nafter\n"),
        "<pre><code class=\"language-js\">a\n</code></pre>\n<p>text <strong>x</strong></p>\n\
         <pre><code class=\"language-py\">b\n</code></pre>\n<p>after</p>"
    );
    assert_eq!(
        html("```js foo``` is inline\n"),
        "<p><code>js foo</code> is inline</p>"
    );
    assert!(html("``` {.x}\na\n```\n").starts_with("<p>"));
    assert!(html("```js, x\na\n```\n").starts_with("<p>"));
}

#[test]
fn a_recovered_fence_reports_fence_title_syntax() {
    for (source, column, hint) in [
        ("```raw html\nx\n```\n", 1, Some("```=html")),
        ("> ```raw latex\n> x\n> ```\n", 3, Some("```=latex")),
        ("- ```raw html\n  x\n  ```\n", 3, Some("```=html")),
        ("```js {1,3}\na\n```\n", 1, None),
    ] {
        let findings = lint_carve(source);
        assert_eq!(findings.len(), 1, "{source}: {findings:?}");
        assert_eq!(findings[0].rule, "fence-title-syntax");
        assert_eq!((findings[0].line, findings[0].column), (1, column));
        if let Some(hint) = hint {
            assert!(findings[0].message.contains(hint), "{source}");
        }
    }
    for source in [
        "```raw\nx\n```\n",
        "```raw \"t\"\nx\n```\n",
        "```=html\nx\n```\n",
    ] {
        assert!(lint_carve(source).is_empty(), "{source}");
    }
}
