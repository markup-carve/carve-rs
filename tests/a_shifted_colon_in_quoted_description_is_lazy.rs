//! A colon line between an item's marker and content columns stays lazy
//! inside an open quoted description (PART 0 S4, markup-carve/carve#5364).

use carve::to_html;

#[test]
fn shifted_colon_continues_the_open_description() {
    for description in ["  : d", "    : d"] {
        let source = format!("- a\n  > +\n  :: t\n{description}\n : d\n");
        let html = to_html(&source);
        assert!(html.contains("<dd>d\n: d</dd>"), "{source:?}: {html}");
        assert_eq!(html.matches("<dd>").count(), 1, "{source:?}: {html}");
    }
}
