//! A tab after a marker ladder's outer item prefix is measured in that item's
//! frame. Its virtual column determines ownership; positions retain source bytes.

use carve::{from_json, parse_with_options, render_html, to_html, to_json, Options};
use serde_json::Value;

const SECOND_PARAGRAPH: &str = "- - 10. a\n\n  \t  b\n";
const FENCED_CHILD: &str = "- 1. a\n  \t```\n  \tx\n  \t```\n";
const QUOTED_LADDER: &str = "> - - 10. a\n>   \t  > b\n> \n> - z\n";
const TABBED_PLUS: &str = "- - 10. a\n  \t+\n";
const TABBED_LITERAL_PLUS: &str = "- - 10. a\n\n  \t  +\n";

#[test]
fn tabbed_continuations_stay_in_the_deepest_item() {
    assert_eq!(
        to_html(SECOND_PARAGRAPH),
        "<ul>\n  <li>\n    <ul>\n      <li>\n        <ol start=\"10\">\n          <li><p>a</p>\n            <p>b</p>\n          </li>\n        </ol>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
    assert_eq!(
        to_html(FENCED_CHILD),
        "<ul>\n  <li>\n    <ol>\n      <li>a\n        <pre><code>x\n</code></pre>\n      </li>\n    </ol>\n  </li>\n</ul>"
    );
    assert_eq!(
        to_html(QUOTED_LADDER),
        "<blockquote>\n  <ul>\n    <li>\n      <ul>\n        <li>\n          <ol start=\"10\">\n            <li>a\n              <blockquote><p>b</p></blockquote>\n            </li>\n          </ol>\n        </li>\n      </ul>\n    </li>\n    <li><p>z</p></li>\n  </ul>\n</blockquote>"
    );
    assert_eq!(
        to_html(TABBED_PLUS),
        "<ul>\n  <li>\n    <ul>\n      <li>\n        <ol start=\"10\">\n          <li>a</li>\n        </ol>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
    assert_eq!(
        to_html(TABBED_LITERAL_PLUS),
        "<ul>\n  <li>\n    <ul>\n      <li>\n        <ol start=\"10\">\n          <li><p>a</p>\n            <p>+</p>\n          </li>\n        </ol>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn ast_positions_and_json_roundtrip_keep_the_authored_line() {
    let parsed = parse_with_options(SECOND_PARAGRAPH, &Options::default().with_positions(true));
    let json = to_json(&parsed);
    let value: Value = serde_json::from_str(&json).unwrap();
    let inner = &value["children"][0]["items"][0]["children"][0]["items"][0]["children"][0];
    assert_eq!(inner["type"], "list");
    assert_eq!(inner["tight"], false);
    assert_eq!(inner["pos"]["startOffset"], 4);
    assert_eq!(inner["pos"]["endOffset"], 17);
    assert_eq!(inner["items"][0]["children"][1]["pos"]["startOffset"], 16);
    let decoded = from_json(&json).unwrap();
    assert_eq!(render_html(&decoded).unwrap(), to_html(SECOND_PARAGRAPH));
    assert_eq!(to_json(&decoded), json);
}
