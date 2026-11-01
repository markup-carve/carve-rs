//! `to_html` and the positioned route the CLI takes read one source the same
//! way (carve-rs#3675). CARVE-P9-051: a comment below the content column closes
//! the item, not the list.

use carve::{to_html, try_to_html_with_options, Options};

const ONE_LIST: &str = "<ul>\n  <li><p>a</p></li>\n  <li><p>b</p></li>\n</ul>";

#[test]
fn comments_between_two_items_leave_one_list_on_every_route() {
    for source in [
        "- a\n\n%% c\n%% d\n\n- b\n",
        "- a\n\n%% c\n\n- b\n",
        "- a\n\n%% c\n- b\n",
        "- a\n\n%% c\n%% d\n%% e\n\n- b\n",
    ] {
        assert_eq!(to_html(source).trim(), ONE_LIST, "{source}");
        let positioned = Options::default().with_positions(true);
        assert_eq!(
            try_to_html_with_options(source, &positioned)
                .unwrap()
                .trim(),
            ONE_LIST,
            "{source}"
        );
    }
}
