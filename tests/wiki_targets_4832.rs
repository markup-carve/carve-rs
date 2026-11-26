//! The `:wiki` target shapes of carve-rs#4832 (C2-X-057, C2-H-161).

use carve::api::extensions::{self, Bundle, WikilinksOptions};
use carve::api::{Carve, Options};

fn html(source: &str, bundle: Bundle) -> String {
    let mut carve = Carve::new();
    carve.register(bundle).unwrap();
    carve.render(source, &Options::default()).unwrap()
}

#[test]
fn wiki_target_shapes() {
    let wiki = || extensions::wikilinks(WikilinksOptions::default());
    assert_eq!(
        html("A :wiki[#only] B", wiki()),
        r##"<p>A <a href="#only" class="wikilink" data-wikilink="">#only</a> B</p>"##
    );
    assert_eq!(html("C :wiki[|disp] D", wiki()), "<p>C disp D</p>");
    assert_eq!(
        html("E :wiki[javascript:alert(1)] F", wiki()),
        r#"<p>E <a href="javascriptalert1" class="wikilink" data-wikilink="javascript:alert(1)">javascript:alert(1)</a> F</p>"#
    );
    assert_eq!(
        html(":wiki[Page]{#i .k data-wikilink=evil}", wiki()),
        r#"<p><a href="page" id="i" class="wikilink k" data-wikilink="Page">Page</a></p>"#
    );
}
