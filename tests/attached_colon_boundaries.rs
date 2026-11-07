use carve::{decision_trace, parse, render_html};

fn html(source: &str) -> String {
    render_html(&parse(source)).unwrap()
}

#[test]
fn marker_line_attachment_trace_keeps_its_item_owner() {
    for (opener, family) in [("::: note", "div"), ("```", "code")] {
        let source = format!("- +\n{opener}\n\n  tail\n");
        let trace = decision_trace(&source);
        assert!(
            trace.contains(&format!(
                "\"line\":2,\"step\":\"S5\",\"owner\":\"doc/item:1/{family}:2\""
            )),
            "{source:?}: {trace}"
        );
        assert!(
            !trace
                .lines()
                .find(|line| line.contains("\"line\":2"))
                .unwrap()
                .contains("\"closed\""),
            "{source:?}: {trace}"
        );
        assert!(trace.contains("\"line\":4,\"step\":\"S5\",\"owner\":\"doc/item:1\""));
        assert!(html(&source).contains("<p>tail</p>"));
    }
}

#[test]
fn quoted_host_resumes_after_attached_colon_blank() {
    let source = "> - +\n> ::: note\n>\n>   - q\n";
    assert_eq!(
        html(source),
        "<blockquote>\n  <ul>\n    <li>\n      <aside class=\"admonition note\" aria-label=\"Note\">\n\n      </aside>\n      <ul>\n        <li>q</li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>"
    );
    let trace = decision_trace(source);
    assert!(
        trace.contains("\"line\":4,\"step\":\"S5\",\"owner\":\"doc/quote:1/item:1/item:4\""),
        "{trace}"
    );
}
