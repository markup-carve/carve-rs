//! Disclosure bodies render as a core div body does (carve#6757,
//! carve-rs#4843).

use carve::api::extensions::{self, Bundle};
use carve::api::{Carve, Mode, Options};

fn html_with(source: &str, bundle: Bundle, opts: &Options) -> String {
    let mut carve = Carve::new();
    carve.register(bundle).unwrap();
    carve.render(source, opts).unwrap()
}

fn html(source: &str, bundle: Bundle) -> String {
    html_with(source, bundle, &Options::default())
}

fn static_mode() -> Options {
    Options {
        mode: Mode::Static,
        ..Options::default()
    }
}

#[test]
fn a_wrapped_paragraph_keeps_its_continuation_at_column_zero() {
    assert_eq!(
        html("::: details \"T\"\nx\ny\n:::\n", extensions::details()),
        "<details>\n  <summary>T</summary>\n  <p>x\ny</p>\n</details>"
    );
    assert_eq!(
        html("::: spoiler \"S\"\nx\ny\n:::\n", extensions::spoiler()),
        "<details class=\"spoiler\">\n  <summary>S</summary>\n  <p>x\ny</p>\n</details>"
    );
    assert_eq!(
        html_with(
            "::: spoiler \"S\"\nx\ny\n:::\n",
            extensions::spoiler(),
            &static_mode()
        ),
        "<section class=\"spoiler spoiler-revealed\">\n  <h3 class=\"spoiler-title\">S</h3>\n  <p>x\ny</p>\n</section>"
    );
    assert_eq!(
        html(
            "- item\n\n  ::: details \"N\"\n  a\n  b\n  :::\n",
            extensions::details()
        ),
        "<ul>\n  <li>item\n    <details>\n      <summary>N</summary>\n      <p>a\nb</p>\n    </details>\n  </li>\n</ul>"
    );
}

#[test]
fn a_heading_in_the_body_has_no_section() {
    assert_eq!(
        html("::: details \"T\"\n# H\n:::\n", extensions::details()),
        "<details>\n  <summary>T</summary>\n  <h1 id=\"H\">H</h1>\n</details>"
    );
    assert_eq!(
        html_with(
            "::: spoiler \"S\"\n## H\n\ntext\n:::\n",
            extensions::spoiler(),
            &static_mode()
        ),
        "<section class=\"spoiler spoiler-revealed\">\n  <h3 class=\"spoiler-title\">S</h3>\n  <h2 id=\"H\">H</h2>\n  <p>text</p>\n</section>"
    );
}
