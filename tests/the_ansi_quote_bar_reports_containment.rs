//! markup-carve/carve#1689: the ANSI blockquote bar reports CONTAINMENT, not
//! node kind. Everything a quote contains carries it, so the ANSI reader is
//! never told a block was unquoted where the HTML says it was.
//!
//! WHY THESE FIXTURES CAN FAIL. Before the ruling the bar was applied by each
//! block's own match arm, and only three ever asked for it (paragraph,
//! admonition title, div label). So every assertion below that expects a bar on
//! a heading, a code block, a list or a promoted image failed on the previous
//! implementation, and the two-spellings-agree assertion failed because the
//! flush spelling had no bar at all while the indented one did.
//!
//! The blank-line assertion is the NEAR MISS: prefixing a quote's whole
//! rendered body indiscriminately would draw a gutter through the space between
//! its blocks and past its end. It is the one shape a naive reading of this fix
//! would also change, and it must not.

fn strip(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            while let Some(&n) = chars.peek() {
                chars.next();
                if n == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[test]
fn both_spellings_of_a_lone_quoted_image_get_the_same_bar() {
    // Identical HTML, different trees: `block_quote > image` against
    // `block_quote > paragraph > image`. Spec corpus category
    // 411-a-lone-indented-image-is-a-paragraph-and-its-html-cannot-say-so.
    let flush = strip(&carve::to_ansi("> ![Apollo](a.jpg)\n"));
    let indented = strip(&carve::to_ansi(">   ![Apollo](a.jpg)\n"));

    // Asserting BOTH spellings is the point of the ruling: a test on the flush
    // case alone cannot show that the two now agree.
    assert_eq!(flush, indented, "flush={flush:?} indented={indented:?}");
    assert_eq!(flush.trim_end(), "│ [img: Apollo]", "{flush:?}");
}

#[test]
fn a_quoted_heading_gets_the_bar_on_its_underline_too() {
    let out = strip(&carve::to_ansi("> # Heading\n"));
    let lines: Vec<&str> = out.lines().filter(|l| !l.is_empty()).collect();

    assert_eq!(lines.len(), 2, "{out:?}");
    for line in &lines {
        assert!(line.starts_with("│ "), "{line:?}");
    }
}

#[test]
fn a_quoted_code_block_gets_the_bar_on_every_payload_line() {
    let out = strip(&carve::to_ansi("> ```\n> alpha\n> beta\n> ```\n"));
    let lines: Vec<&str> = out.lines().filter(|l| !l.is_empty()).collect();

    assert_eq!(lines.len(), 2, "{out:?}");
    for line in &lines {
        assert!(line.starts_with("│ "), "{line:?}");
    }
    assert!(out.contains("alpha"), "{out:?}");
}

#[test]
fn the_bar_sits_outside_a_quoted_list_marker() {
    // The old design prefixed the item's PARAGRAPH, so the bullet - added by the
    // list renderer afterwards - landed to the LEFT of the bar and the output
    // read `• │ item`. Containment puts the quote outermost.
    let out = strip(&carve::to_ansi("> - item\n"));

    assert!(out.starts_with("│ "), "{out:?}");
    assert!(out.find('│') < out.find('•'), "{out:?}");
}

#[test]
fn nested_quotes_compose_one_bar_per_level() {
    let out = strip(&carve::to_ansi("> > nested\n"));
    assert!(out.starts_with("│ │ "), "{out:?}");
}

#[test]
fn the_blank_line_between_two_quoted_blocks_stays_bare() {
    // Near miss: the shape a naive "prefix the whole body" fix would also
    // change. A bar here would draw a gutter through the gap and past the end.
    let out = strip(&carve::to_ansi("> one\n>\n> two\n"));
    let barred = out.split('\n').filter(|l| l.starts_with("│ ")).count();

    assert_eq!(barred, 2, "{out:?}");
    for line in out.split('\n') {
        if !line.starts_with("│ ") {
            assert_eq!(line, "", "{out:?}");
        }
    }
}

#[test]
fn an_unquoted_heading_and_code_block_have_no_bar_at_all() {
    // Control: the bar tracks containment, so outside a quote there is none.
    assert!(!strip(&carve::to_ansi("# Heading\n")).contains('│'));
    assert!(!strip(&carve::to_ansi("```\ncode\n```\n")).contains('│'));
}
