//! C2-I-023, C2-I-024: a line comment inside an image opener that never forms is a comment (carve#6442).
use carve::api::{Carve, Options};

fn html(source: &str) -> String {
    Carve::new().render(source, &Options::default()).unwrap()
}

#[test]
fn a_comment_in_an_image_opener_that_never_forms_is_a_comment() {
    assert_eq!(html("![a\n%% c\n"), "<p>![a</p>");
    assert_eq!(html("![a %% b](a b)\n"), "<p>![a</p>");
    assert_eq!(html("![[](u) x\n%% c\n"), "<p>![<a href=\"u\"></a> x</p>");
}
