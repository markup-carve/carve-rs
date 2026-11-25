//! carve#6611: a valid block whose value starts where a failed outer block's value run ended is one atom printed as written (C2-I-035, C2-I-036).

use carve::api::{Carve, Options};

fn html(source: &str) -> String {
    Carve::new()
        .render(source, &Options::default())
        .unwrap()
        .trim_end()
        .to_owned()
}

#[test]
fn the_inner_block_keeps_straight_quotes() {
    assert_eq!(html("{v={k=\"a\"}\n"), "<p>{v={k=\"a\"}</p>");
    assert_eq!(html("{v={k=''}\n"), "<p>{v={k=''}</p>");
}
