use carve::{parse, render_html};

#[test]
fn quoted_notes_own_their_indented_blocks() {
    for prefix in ["> ", "> > "] {
        for gap in ["", prefix.trim_end()] {
            for indent in ["  ", "   ", "\t"] {
                let separator = if gap.is_empty() {
                    String::new()
                } else {
                    format!("{gap}\n")
                };
                let source = format!(
                    "{prefix}r[^n]\n{prefix}\n{prefix}[^n]: p\n{separator}{prefix}{indent}> q\n"
                );
                let html = render_html(&parse(&source)).unwrap();
                let (body, notes) = html.split_once("<section role=\"doc-endnotes\"").unwrap();
                assert!(!body.contains("q</p>"), "{source}: {html}");
                assert!(
                    notes.contains("<blockquote><p>q</p></blockquote>"),
                    "{source}: {html}"
                );
            }
        }
    }
}

#[test]
fn quote_exit_and_below_floor_content_stay_outside_the_note() {
    for source in [
        "> r[^n]\n>\n> [^n]: p\n> > q\n",
        "> r[^n]\n>\n> [^n]: p\n\n  > q\n",
    ] {
        let html = render_html(&parse(source)).unwrap();
        let (body, notes) = html.split_once("<section role=\"doc-endnotes\"").unwrap();
        assert!(body.contains("q</p>"), "{html}");
        assert!(!notes.contains("q</p>"), "{html}");
    }
}

#[test]
fn quoted_note_positions_point_to_authored_content() {
    for prefix in ["> ", "> > "] {
        for indent in ["  ", "   ", "\t"] {
            let source = format!(
                "{prefix}r[^n]\n{prefix}\n{prefix}[^n]: p\n{prefix}\n{prefix}{indent}> café\n"
            );
            let options = carve::Options {
                positions: true,
                ..Default::default()
            };
            let document = carve::parse_with_options(&source, &options);
            let carve::ast::BlockNode::BlockQuote(quote) = &document.footnote_defs["n"][1] else {
                panic!("missing quote")
            };
            let pos = quote.pos.as_ref().unwrap();
            assert_eq!(
                (pos.start_line, pos.start_column),
                (5, prefix.chars().count() + indent.chars().count() + 1)
            );
            let slice: String = source
                .chars()
                .skip(pos.start_offset)
                .take(pos.end_offset - pos.start_offset)
                .collect();
            assert_eq!(slice, "> café");
        }
    }
}
