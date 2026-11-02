// carve#4966: a line that misses an empty line block's item closes it, so the
// next column-0 line ends whatever that line opened (PART 0 S1, CARVE-P0-006).

#[test]
fn a_block_opened_past_the_closed_line_block_takes_no_lazy_line() {
    let cases = [
        (
            "- - - ::: |\n    ~~~\nb\n",
            "<ul>\n  <li>\n    <ul>\n      <li>\n        <ul>\n          <li>\n            <div class=\"line-block\">\n\n            </div>\n          </li>\n        </ul>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>b</p>",
        ),
        (
            "- - ::: |\n  ~~~\nb\n",
            "<ul>\n  <li>\n    <ul>\n      <li>\n        <div class=\"line-block\">\n\n        </div>\n      </li>\n    </ul>\n    <pre><code></code></pre>\n  </li>\n</ul>\n<p>b</p>",
        ),
        (
            "- - ::: |\n   ```\nb\n",
            "<ul>\n  <li>\n    <ul>\n      <li>\n        <div class=\"line-block\">\n\n        </div>\n      </li>\n    </ul>\n    <pre><code></code></pre>\n  </li>\n</ul>\n<p>b</p>",
        ),
        (
            "- - ::: |\n  ::: x\nb\n",
            "<ul>\n  <li>\n    <ul>\n      <li>\n        <div class=\"line-block\">\n\n        </div>\n      </li>\n    </ul>\n    <div class=\"x\">\n\n    </div>\n  </li>\n</ul>\n<p>b</p>",
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(carve::to_html(source).trim_end(), expected, "{source:?}");
    }
}

#[test]
fn a_stanza_with_content_still_takes_a_lazy_line() {
    assert_eq!(
        carve::to_html("- - ::: |\n    x\nb\n").trim_end(),
        "<ul>\n  <li>\n    <ul>\n      <li>\n        <div class=\"line-block\">\n          <p>x<br>\nb</p>\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}
