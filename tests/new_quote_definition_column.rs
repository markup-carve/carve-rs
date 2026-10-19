use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    for html in [
        to_html(source),
        to_html_with_options(source, &Options::default().with_positions(true)),
    ] {
        assert_eq!(
            html.trim_end_matches('\n'),
            expected.trim_end_matches('\n'),
            "{source:?}"
        );
    }
}

#[test]
fn exact_html() {
    check("- > :: t\n> : d\n>   [^f]: n\ntail", "<ul>\n  <li>\n    <blockquote>\n      <dl>\n        <dt>t</dt>\n      </dl>\n    </blockquote>\n  </li>\n</ul>\n<blockquote><p>: d\n[^f]: n\ntail</p></blockquote>\n");
    check("- > > - para\n> >   [^f]: n", "<ul>\n  <li>\n    <blockquote>\n      <blockquote>\n        <ul>\n          <li>para</li>\n        </ul>\n      </blockquote>\n    </blockquote>\n  </li>\n</ul>\n<blockquote>\n  <blockquote><p>[^f]: n</p></blockquote>\n</blockquote>\n");
}
