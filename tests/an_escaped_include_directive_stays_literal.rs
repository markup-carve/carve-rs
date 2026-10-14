//! A backslash makes the opening brace literal, so `\{{ path }}` is no
//! directive (CARVE-P7-004, spec PART 6).

use std::cell::RefCell;

use carve::{
    expand_includes, parse_with_options, render_carve, render_html, to_html, IncludeDenial,
    IncludeOptions, IncludeResolved, IncludeResolver, Options,
};

#[derive(Default)]
struct Child(RefCell<Vec<String>>);

impl IncludeResolver for Child {
    fn resolve(
        &self,
        path: &str,
        _ctx: &carve::IncludeContext<'_>,
    ) -> Result<IncludeResolved, IncludeDenial> {
        self.0.borrow_mut().push(path.to_string());
        if path == "c.crv" {
            Ok(IncludeResolved::with_id("body\n".to_string(), path))
        } else {
            Err(IncludeDenial::NotFound)
        }
    }
}

struct Expanded {
    html: String,
    calls: Vec<String>,
    dependencies: usize,
    warnings: usize,
}

fn expand(source: &str) -> Expanded {
    let resolver = Child::default();
    let doc = parse_with_options(source, &Options::default().with_positions(true));
    let result = expand_includes(doc, source, &IncludeOptions::new().with_resolver(&resolver));
    Expanded {
        html: render_html(&result.doc).unwrap(),
        calls: resolver.0.take(),
        dependencies: result.dependencies.len(),
        warnings: result.warnings.len(),
    }
}

fn write(source: &str) -> String {
    render_carve(&parse_with_options(source, &Options::default())).unwrap()
}

#[test]
fn an_escaped_directive_is_not_expanded() {
    for source in [
        "\\{{ c.crv }}",
        "See \\{{ c.crv }} here.",
        "See \\{{ c.crv #pick @shift:1 }} here.",
    ] {
        let result = expand(source);
        assert_eq!(result.html, to_html(source), "{source}");
        assert!(result.calls.is_empty(), "{source}: {:?}", result.calls);
        assert_eq!(result.dependencies, 0, "{source}");
        assert_eq!(result.warnings, 0, "{source}");
    }
}

#[test]
fn a_live_directive_after_an_escaped_one_still_expands() {
    let result = expand("See \\{{ a {{ c.crv }} here.");
    assert_eq!(result.calls, ["c.crv"]);
    assert_eq!(result.html.trim_end(), "<p>See {{ a body here.</p>");
}

#[test]
fn a_directive_after_an_escaped_lone_brace_expands() {
    let result = expand("See \\{{{ c.crv }} here.");
    assert_eq!(result.calls, ["c.crv"]);
    assert_eq!(result.html.trim_end(), "<p>See {body here.</p>");
}

#[test]
fn the_writer_keeps_the_escape() {
    for source in [
        "\\{{ c.crv }}",
        "See \\{{ c.crv }} here.",
        "{{ \\{{ c.crv }}",
    ] {
        let written = write(source);
        assert_eq!(written, format!("{source}\n"), "{source}");
        assert!(expand(&written).calls.is_empty(), "{source}");
    }
}

#[test]
fn the_writer_keeps_a_live_directive_after_an_escaped_one() {
    let source = "See \\{{ a {{ c.crv }} here.";
    let written = write(source);
    assert_eq!(written, format!("{source}\n"));
    assert_eq!(expand(&written).calls, ["c.crv"]);
}

#[test]
fn a_directive_opening_on_the_second_of_three_braces_expands() {
    let result = expand("See {{{ c.crv }} here.");
    assert_eq!(result.calls, ["c.crv"]);
    assert_eq!(result.html.trim_end(), "<p>See {body here.</p>");
}

#[test]
fn an_escaped_closing_brace_is_no_closer() {
    for source in [
        "{{ c.crv \\}}",
        "See {{ c.crv \\}} here.",
        "{{ c.crv }\\}",
        "See {{ c.crv }\\} here.",
    ] {
        let result = expand(source);
        assert_eq!(result.html, to_html(source), "{source}");
        assert!(result.calls.is_empty(), "{source}: {:?}", result.calls);
        let written = write(source);
        assert!(expand(&written).calls.is_empty(), "{source} -> {written}");
    }
}
