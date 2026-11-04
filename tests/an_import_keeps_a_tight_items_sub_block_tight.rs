//! A blank above an item's sub-block loosens the list (carve#5015), so an import
//! or a Markdown roundtrip writes one only where the source item was loose.

use carve::{migrate_djot, migrate_markdown, to_html, to_markdown_with_options, Options};

fn roundtrip(source: &str) -> String {
    let markdown = to_markdown_with_options(source, &Options::default().with_roundtrip(true));
    migrate_markdown(&markdown).value
}

#[test]
fn a_roundtrip_keeps_each_items_looseness() {
    for source in [
        "- x\n  :::\n  a\n  :::\n",
        "- Item.\n  :: Term\n  : Definition.\n",
        "- Item.\n\n  :: Term\n  : Definition.\n",
        "> - outer\n>   :: T\n>     : D\n",
        "- host\n\n  i.  outer one\n\n      a.  inner one\n  ii.  outer two\n",
        "> i.  outer\n>\n>     - inner\n> ii.  peer\n",
    ] {
        let imported = roundtrip(source);
        assert_eq!(
            to_html(&imported),
            to_html(source),
            "{source:?} => {imported:?}"
        );
    }
}

#[test]
fn a_djot_sub_list_after_a_blank_stays_in_a_tight_item() {
    for (source, carve) in [
        ("- outer\n\n  - inner\n", "- outer\n  - inner\n"),
        (
            "- outer\n\n\n  - inner\n- peer\n",
            "- outer\n  - inner\n- peer\n",
        ),
        ("> - outer\n>\n>   - inner\n", "> - outer\n>   - inner\n"),
        (
            "> - x\n>\n>   : t\n>\n>     a\n",
            "> - x\n>   :: t\n>   :  a\n",
        ),
        // A blank above a later block of the item is Djot's too.
        ("- a\n\n  # h\n\n  - c\n", "- a\n\n  # h\n\n  - c\n"),
    ] {
        assert_eq!(migrate_djot(source).value, carve, "{source:?}");
    }
}
