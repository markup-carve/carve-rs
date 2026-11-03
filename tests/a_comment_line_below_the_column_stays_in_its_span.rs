//! A comment-shaped line below an item's column inside an open `%%%` span (a
//! line block's span or a comment block's) is that span's payload, and its exact
//! run the closer (PART 0 S1, PART 9 §28, markup-carve/carve#4994 and #4987).
//! The next line that misses the item ends the span and the item.

use carve::{to_html, to_html_with_options, Options};

fn html(src: &str) -> String {
    let facade = to_html(src);
    let positioned = to_html_with_options(src, &Options::default().with_positions(true));
    assert_eq!(facade, positioned, "the two paths disagree on {src:?}");
    facade
}

fn assert_html(src: &str, expected: &str) {
    let normalize = |h: &str| h.split_whitespace().collect::<Vec<_>>().join(" ");
    assert_eq!(normalize(&html(src)), normalize(expected), "on {src:?}");
}

const LINE_BLOCK_ITEM: &str = r#"<ul>
  <li>
    <div class="line-block">

    </div>
  </li>
</ul>"#;

#[test]
fn the_ticket_reproductions() {
    assert_html(
        "- ::: |\n  %%%\n %%%\n\n  %%%\nz\n",
        &format!("{LINE_BLOCK_ITEM}\n<p>z</p>"),
    );
    assert_html(
        "- ::: |\n  %%%\n%%%%\n ```\n  %%%\nz\n",
        &format!("{LINE_BLOCK_ITEM}\n<pre><code> %%%\nz\n</code></pre>"),
    );
    assert_html(
        "1. ::: |\n   %%%\n  %%%%\n - b\n   %%%\nz\n",
        r#"<ol>
  <li>
    <div class="line-block">

    </div>
  </li>
</ol>
<ul>
  <li>b</li>
</ul>
<p>z</p>"#,
    );
    // carve#4987.
    assert_html(
        "- ::: |\n  %%%%\n %%%\n ```\n  %%%%\nz\n",
        &format!("{LINE_BLOCK_ITEM}\n<pre><code> %%%%\nz\n</code></pre>"),
    );
}

#[test]
fn a_line_block_in_the_item_body_and_a_comment_block_read_the_same() {
    assert_html(
        "- a\n\n  ::: |\n  %%%\n %%%%\n%% c\nz\n",
        r#"<ul>
  <li>a
    <div class="line-block">

    </div>
  </li>
</ul>
<p>z</p>"#,
    );
    assert_html(
        "- a\n\n  %%%\n %%%\n\n  %%%\nz\n",
        "<ul>\n  <li>a</li>\n</ul>\n<p>z</p>",
    );
    assert_html(
        "- a\n\n  %%%\n%%%%\n ```\n  %%%\nz\n",
        "<ul>\n  <li>a</li>\n</ul>\n<pre><code> %%%\nz\n</code></pre>",
    );
}

/// Below the column the line reads as it does at the host's column.
#[test]
fn a_comment_line_reads_alike_below_and_at_the_column() {
    let hosts: &[(&[&str], usize)] = &[
        (&["- ::: |"], 2),
        (&["1. ::: |"], 3),
        (&["- - ::: |"], 4),
        (&["- a", "", "  ::: |"], 2),
        (&["- a", ""], 2),
        (&["- x", "  - ::: |"], 4),
    ];
    let followers: &[&[&str]] = &[&["z"], &[" ```"], &[" - b"], &["%% c"], &[":::"], &[]];
    let mut checked = 0;
    for (host, col) in hosts {
        for run in [3, 4] {
            for middle in [3, 4, 5] {
                for blank in [false, true] {
                    for follower in followers {
                        let doc = |at: usize| {
                            let mut lines: Vec<String> =
                                host.iter().map(|l| l.to_string()).collect();
                            lines.push(format!("{}{}", " ".repeat(*col), "%".repeat(run)));
                            lines.push(format!("{}{}", " ".repeat(at), "%".repeat(middle)));
                            if blank {
                                lines.push(String::new());
                            }
                            if follower.is_empty() {
                                lines.push(format!("{}{}", " ".repeat(*col), "%".repeat(run)));
                            } else {
                                lines.extend(follower.iter().map(|l| l.to_string()));
                            }
                            lines.push("z".into());
                            lines.join("\n") + "\n"
                        };
                        let at_column = html(&doc(*col));
                        for below in [0, 1, col - 1] {
                            let source = doc(below);
                            assert_eq!(html(&source), at_column, "on {source:?}");
                            checked += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(checked, 1296);
}
