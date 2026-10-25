use super::{is_escaped, mask_code_and_destinations, read_djot_attributes};
use crate::{HtmlImportSeverity, MigrationConfidence, MigrationDiagnostic, MigrationFidelity};

pub(super) fn recover(source: &str) -> (String, Vec<MigrationDiagnostic>) {
    if !source.contains('{') {
        return (source.to_owned(), Vec::new());
    }
    let mask = mask_code_and_destinations(source);
    let mut output = String::with_capacity(source.len());
    let mut losses = Vec::new();
    let mut copied = 0;
    let mut line = 1;
    let mut prior = 0;
    let candidates = cached_regex!(r"\{([^{}\n]*)\}").unwrap();
    let cross_reference = cached_regex!(r"#[^#\s]*#").unwrap();
    let token_re = cached_regex!(
        r#"#[^\s.#{}]+|\.[^\s.#{}]+|[A-Za-z][A-Za-z0-9_-]*=(?:"(?:\\.|[^"])*"|[^\s{}]+)|[^\s{}]+"#
    )
    .unwrap();
    let shorthand = cached_regex!(r"^[#.][A-Za-z0-9_][\w-]*$").unwrap();
    for matched in candidates.captures_iter(source) {
        let whole = matched.get(0).unwrap();
        let at = whole.start();
        line += source[prior..at].bytes().filter(|b| *b == b'\n').count();
        prior = at;
        if mask.as_bytes().get(at) != Some(&b'{')
            || is_escaped(source.as_bytes(), at)
            || read_djot_attributes(source, at, false, false).is_some()
        {
            continue;
        }
        let inner = &matched[1];
        if inner.contains(['\0', '\\', '<', '>'])
            || cross_reference.is_match(inner)
            || !inner.starts_with(|ch: char| {
                ch.is_whitespace() || ch == '.' || ch == '#' || ch.is_ascii_alphabetic()
            })
            || !inner.contains(['.', '#'])
        {
            continue;
        }
        let tokens: Vec<_> = token_re.find_iter(inner).map(|m| m.as_str()).collect();
        if tokens.is_empty()
            || tokens.iter().any(|token| {
                !token.starts_with(['#', '.'])
                    && read_djot_attributes(&format!("{{{token}}}"), 0, false, false).is_none()
            })
        {
            continue;
        }
        let mut kept = Vec::new();
        let mut rejected = Vec::new();
        for token in tokens {
            let valid = read_djot_attributes(&format!("{{{token}}}"), 0, false, false).is_some()
                && (!token.starts_with(['#', '.']) || shorthand.is_match(token));
            if valid {
                kept.push(token);
            } else {
                rejected.push(if token.starts_with('#') {
                    "id"
                } else if token.starts_with('.') {
                    "class"
                } else {
                    token.split('=').next().unwrap_or(token)
                });
            }
        }
        if rejected.is_empty() && kept.join(" ") == inner.trim() {
            continue;
        }
        let replacement = format!("{{{}}}", kept.join(" "));
        if read_djot_attributes(&replacement, 0, false, false).is_none() {
            continue;
        }
        output.push_str(&source[copied..at]);
        output.push_str(&replacement);
        copied = whole.end();
        for name in rejected {
            losses.push(MigrationDiagnostic {
                code: "djot-attribute-dropped".into(),
                message: format!(
                    "Djot attribute {name} could not be represented in Carve and was dropped."
                ),
                severity: HtmlImportSeverity::Warning,
                fidelity: MigrationFidelity::Dropped,
                confidence: MigrationConfidence::Exact,
                path: Some(format!("line:{line}")),
            });
        }
    }
    if copied == 0 {
        return (source.to_owned(), losses);
    }
    output.push_str(&source[copied..]);
    (output, losses)
}
