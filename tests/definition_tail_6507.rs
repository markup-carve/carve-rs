//! C2-B-083, C2-B-084: link and abbreviation definitions continue on indented lines (carve#6507).
use carve::api::{Carve, Options, WriteOptions};

fn html(input: &str) -> String {
    Carve::new().render(input, &Options::default()).unwrap()
}

#[test]
fn a_link_definition_takes_one_tail_line() {
    let cases = [
        // C2-B-137: a comment-only line between them is not seen.
        (
            "[a]: /u\n%% c\n  \"T\"\n\nSee [a].\n",
            "<p>See <a href=\"/u\" title=\"T\">a</a>.</p>",
        ),
        // C2-B-106: the attribute line above merges before the tail's block.
        (
            "{#i}\n[a]: /u\n  {.c}\n\nSee [a].\n",
            "<p>See <a href=\"/u\" id=\"i\" class=\"c\">a</a>.</p>",
        ),
        // A title line may leave the attribute block to the next line.
        (
            "[a]: /u\n  \"T\"\n  {.c}\nPara\n\nSee [a].\n",
            "<p>Para</p>\n<p>See <a href=\"/u\" title=\"T\" class=\"c\">a</a>.</p>",
        ),
        (
            "[a]: /u\n  \"T\"\n  more\n\nSee [a].\n",
            "<p>more</p>\n<p>See <a href=\"/u\" title=\"T\">a</a>.</p>",
        ),
        (
            "[a]: /u\n\t\"T\"\n\nSee [a].\n",
            "<p>See <a href=\"/u\" title=\"T\">a</a>.</p>",
        ),
        // One attribute block, not glued ones.
        (
            "[a]: /u\n  {.c}{.d}\n\nSee [a].\n",
            "<p>{.c}{.d}</p>\n<p>See <a href=\"/u\">a</a>.</p>",
        ),
        (
            "[a]: /u\n  \"T\" {.c} x\n\nSee [a].\n",
            "<p>“T” {.c} x</p>\n<p>See <a href=\"/u\">a</a>.</p>",
        ),
        (
            "[a]: /u\n\n  \"T\"\n\nSee [a].\n",
            "<p>“T”</p>\n<p>See <a href=\"/u\">a</a>.</p>",
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(html(input), expected, "{input:?}");
    }
}

#[test]
fn every_indented_line_continues_an_abbreviation() {
    let cases = [
        (
            "*[H]: a\n  - b\n  # c\n\nH\n",
            "<p><abbr title=\"a - b # c\">H</abbr></p>",
        ),
        (
            "*[H]: a\n%% c\n  b\n\nH\n",
            "<p><abbr title=\"a b\">H</abbr></p>",
        ),
        (
            "*[H]: a\n\n  b\n\nH\n",
            "<p>b</p>\n<p><abbr title=\"a\">H</abbr></p>",
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(html(input), expected, "{input:?}");
    }
}

#[test]
fn the_writer_puts_the_tail_on_the_definition_line() {
    let carve = Carve::new();
    let cases = [
        ("[a]: /u\n  \"T\" {#x}\n", "{#x}\n[a]: /u \"T\"\n"),
        ("*[H]: a\n  b\n\tc\n", "*[H]: a b c\n"),
    ];
    for (input, expected) in cases {
        let out = carve.format(input, &WriteOptions::default()).unwrap();
        assert_eq!(out, expected, "{input:?}");
        assert_eq!(
            carve.format(&out, &WriteOptions::default()).unwrap(),
            out,
            "{input:?}"
        );
    }
}
