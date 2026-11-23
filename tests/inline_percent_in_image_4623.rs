use carve::api::{Carve, Options};

fn html(source: &str) -> String {
    Carve::new().render(source, &Options::default()).unwrap()
}

#[test]
fn percent_in_an_image_that_never_forms_is_a_comment() {
    assert_eq!(html("x ![a %% b"), "<p>x ![a</p>");
    assert_eq!(html("x ![a %% ] b](u)"), "<p>x ![a</p>");
    assert_eq!(
        html("x ![a %% b](u)"),
        "<p>x <img src=\"u\" alt=\"a %% b\"></p>"
    );
}
