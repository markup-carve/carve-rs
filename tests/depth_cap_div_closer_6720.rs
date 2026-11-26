//! carve#6720: at the block depth cap a bare `:::` line still closes an outer div; only openers are capped (C2-B-190).

use carve::api::{Carve, Options};

fn html(source: &str) -> String {
    Carve::new()
        .render(source, &Options::default())
        .unwrap()
        .trim_end()
        .to_owned()
}

#[test]
fn the_closer_ends_the_div_at_depth_200() {
    let src = format!(":::\n{}a\n:::\nafter\n", "> ".repeat(199));
    let out = html(&src);
    assert!(out.contains("<p>a</p>"), "{out}");
    assert!(out.ends_with("</div>\n<p>after</p>"), "{out}");
}
