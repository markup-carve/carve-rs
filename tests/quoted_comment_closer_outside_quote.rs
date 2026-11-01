use carve::{to_html, try_to_html_with_options, Options};

#[test]
fn corpus_888_closes_matched_comments_before_quote_ownership() {
    let cases = [
        (
            include_str!("spec/tests/corpus/888-a-comment-span-in-a-quote-closes-at-an-unmarked-closer.crv"),
            include_str!("spec/tests/corpus/888-a-comment-span-in-a-quote-closes-at-an-unmarked-closer.html"),
        ),
        (
            include_str!("spec/tests/corpus/888-a-comment-span-in-a-quote-closes-at-an-unmarked-closer-2.crv"),
            include_str!("spec/tests/corpus/888-a-comment-span-in-a-quote-closes-at-an-unmarked-closer-2.html"),
        ),
        (
            include_str!("spec/tests/corpus/888-a-comment-span-in-a-quote-closes-at-an-unmarked-closer-3.crv"),
            include_str!("spec/tests/corpus/888-a-comment-span-in-a-quote-closes-at-an-unmarked-closer-3.html"),
        ),
        (
            include_str!("spec/tests/corpus/888-a-comment-span-in-a-quote-closes-at-an-unmarked-closer-4.crv"),
            include_str!("spec/tests/corpus/888-a-comment-span-in-a-quote-closes-at-an-unmarked-closer-4.html"),
        ),
        (
            include_str!("spec/tests/corpus/888-a-comment-span-in-a-quote-closes-at-an-unmarked-closer-5.crv"),
            include_str!("spec/tests/corpus/888-a-comment-span-in-a-quote-closes-at-an-unmarked-closer-5.html"),
        ),
        (
            include_str!("spec/tests/corpus/888-a-comment-span-in-a-quote-closes-at-an-unmarked-closer-6.crv"),
            include_str!("spec/tests/corpus/888-a-comment-span-in-a-quote-closes-at-an-unmarked-closer-6.html"),
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(to_html(source).trim(), expected.trim(), "{source}");
        assert_eq!(
            try_to_html_with_options(source, &Options::default().with_positions(true))
                .unwrap()
                .trim(),
            expected.trim(),
            "{source}"
        );
    }
}

#[test]
fn an_unmarked_closer_hides_a_definition_within_the_comment_span() {
    let source = "> %%%\n> [r]: /url\n%%%\n\nSee [r][].\n";
    let expected = "<blockquote>\n\n</blockquote>\n<p>See [r][].</p>";
    assert_eq!(to_html(source).trim(), expected);
}
