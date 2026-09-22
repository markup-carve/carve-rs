//! A bbcode formatting tag is spelled the way the Carve writer would spell the
//! tree it describes. carve-js and carve-php pin the same cases, byte for byte.

use carve::{bbcode_to_carve, to_carve, to_html};

fn carve(bbcode: &str) -> String {
    bbcode_to_carve(bbcode).expect("under the input limit")
}

fn html(bbcode: &str) -> String {
    to_html(&carve(bbcode)).trim().to_string()
}

/// Ruling markup-carve/carve-rs#1719: an empty element holds nothing a reader sees.
#[test]
fn an_empty_tag_is_dropped() {
    for tag in ["b", "i", "u", "s", "sup", "sub"] {
        assert_eq!(carve(&format!("a [{tag}][/{tag}] b")), "a  b\n", "{tag}");
    }
    assert_eq!(carve("a [b][i][/i][/b] b"), "a  b\n");
}

#[test]
fn the_tag_keeps_its_meaning() {
    for (bbcode, expected) in [
        ("a [b] x[/b] b", "<p>a <strong> x</strong> b</p>"),
        ("a [b]x [/b] b", "<p>a <strong>x </strong> b</p>"),
        ("a [u]\tx[/u] b", "<p>a <u>\tx</u> b</p>"),
        ("a[b]x[/b]b", "<p>a<strong>x</strong>b</p>"),
        (
            "a [i][b]x[/b][/i] b",
            "<p>a <em><strong>x</strong></em> b</p>",
        ),
        (
            "a [b][i]x[/i][/b] b",
            "<p>a <strong><em>x</em></strong> b</p>",
        ),
        ("a*[b]x[/b] b", "<p>a*<strong>x</strong> b</p>"),
        ("a/[u]x[/u] b", "<p>a/<u>x</u> b</p>"),
        (
            "a [b]x[/b][b]y[/b] b",
            "<p>a <strong>x</strong><strong>y</strong> b</p>",
        ),
        ("a [b]x[b]y[/b]z[/b] b", "<p>a <strong>xyz</strong> b</p>"),
        ("a [b]x[/b][i][/i]y b", "<p>a <strong>x</strong>y b</p>"),
        ("a [b]x[i]y b", "<p>a [b]x[i]y b</p>"),
        ("a [B]x[/B] b", "<p>a <strong>x</strong> b</p>"),
        ("a [b]a[b]x[/b] b", "<p>a [b]a<strong>x</strong> b</p>"),
        ("a{[i]x[/i]} b", "<p>a{<em>x</em>} b</p>"),
        ("*[b]_[/b]", "<p>*<strong>_</strong></p>"),
        ("a \\*[b]x[/b] b", "<p>a \\*<strong>x</strong> b</p>"),
    ] {
        assert_eq!(html(bbcode), expected, "{bbcode}");
    }
}

#[test]
fn a_format_pass_leaves_the_import_alone() {
    for bbcode in [
        "a [b]x[/b]_y b",
        "a*[b]x[/b] b",
        "a [b] x[/b] b",
        "a [i][u]x[/u][/i] b",
        "a{[i]x[/i]} b",
    ] {
        let once = carve(bbcode);
        assert_eq!(to_carve(&once), once, "{bbcode}");
    }
}

#[test]
fn nesting_as_deep_as_the_input_limit_allows() {
    let depth = 10_000;
    let tags = ["b", "i", "u", "s"];
    let open: String = (0..depth).map(|k| format!("[{}]", tags[k % 4])).collect();
    let close: String = (0..depth)
        .map(|k| format!("[/{}]", tags[(depth - 1 - k) % 4]))
        .collect();
    assert_eq!(
        carve(&format!("{open}x{close}")),
        carve("[b][i][u][s]x[/s][/u][/i][/b]")
    );
    // Unclosed tags stay literal, and neither pass recurses on their depth.
    let unclosed = carve(&("[b]".repeat(depth) + "x"));
    assert!(
        unclosed.trim_end().ends_with('x'),
        "{}",
        &unclosed[unclosed.len() - 20..]
    );
}
