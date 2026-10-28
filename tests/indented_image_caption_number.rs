use carve::{parse, parse_with_options, render_html, to_html, to_html_with_options, Options};

#[test]
fn indented_images_number_the_first_bare_caption_marker() {
    for suffix in ["", "\n[^note]: text\n\nSee[^note]\n"] {
        for spaces in 1..=3 {
            let source = format!("{}![a](/u)\n^ Figure #\n{suffix}", " ".repeat(spaces));
            let positioned = Options::default().with_positions(true);
            for html in [
                to_html(&source),
                to_html_with_options(&source, &Options::default()),
                to_html_with_options(&source, &positioned),
                render_html(&parse(&source)).unwrap(),
                render_html(&parse_with_options(&source, &positioned)).unwrap(),
            ] {
                assert!(
                    html.contains("<figcaption>Figure 1</figcaption>"),
                    "{source:?}: {html}"
                );
            }
        }
    }
}
