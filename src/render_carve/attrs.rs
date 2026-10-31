use super::*;

/// An attribute block after a mention or a tag stays literal, so a tree that
/// carries one has no spelling: `[@a]{.c}` is a span holding a mention and
/// `@a{.c}` leaves the braces as text (ruling markup-carve/carve-php#2083).
pub(super) fn refuse_attributes_on_sigil(attrs: &Option<Attrs>, node_type: &'static str) {
    let carries = attrs.as_ref().is_some_and(|attrs| {
        attrs.id.is_some() || !attrs.classes.is_empty() || !attrs.key_values.is_empty()
    });
    if carries {
        crate::render_carve_error::record_unspellable(
            node_type,
            "an attribute block after a mention or tag reads back as text",
        );
    }
}

pub(crate) fn render_carrier_attrs(attrs: &Option<Attrs>) -> String {
    render_attrs_with_markers(attrs, &['>'], false)
}

pub(crate) fn render_attrs(attrs: &Option<Attrs>) -> String {
    render_attrs_with_markers(attrs, &[], false)
}

pub(super) fn render_inline_attrs(attrs: &Option<Attrs>, ctx: &mut CarveContext) -> String {
    let rendered =
        render_attrs_with_markers(attrs, &ctx.open_kinds, ctx.attribute_bracket_depth > 0);
    for &marker in &ctx.open_kinds {
        if (marker == '=' && rendered.contains("=\""))
            || attrs.as_ref().is_some_and(|attrs| {
                attrs.id.as_ref().is_some_and(|id| id.contains(marker))
                    || attrs.classes.iter().any(|class| class.contains(marker))
                    || attrs
                        .key_values
                        .iter()
                        .any(|(key, value)| key.contains(marker) || value.contains(marker))
            })
        {
            *ctx.attribute_markers.entry(marker).or_default() += 1;
        }
    }
    rendered
}

// A quoted attribute slot shields `~}` from the outer inline closer. Other
// closing pairs, including `+}`, still need their escaping for readback.
fn escape_inline_quoted_attr(value: &str) -> String {
    escape_quoted_run(value, &['"', '|', '{', '}', '[', ']', '`']).replace("~\\}", "~}")
}

/// A row's block sits past the closing `|`, outside the cell cut, so a pipe in
/// a quoted value needs no escape there (PART 11 §2).
pub(super) fn render_row_attrs(attrs: &Option<Attrs>) -> String {
    let rendered = render_attrs(attrs);
    let mut out = String::with_capacity(rendered.len());
    let mut chars = rendered.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('|') => out.push('|'),
                Some(next) => {
                    out.push(ch);
                    out.push(next);
                }
                None => out.push(ch),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

pub(super) fn render_attrs_with_markers(
    attrs: &Option<Attrs>,
    markers: &[char],
    bracketed: bool,
) -> String {
    let conflicts = |value: &str| markers.iter().any(|&marker| value.contains(marker));
    let quote = |value: &str| {
        if (!markers.is_empty() || bracketed)
            && (conflicts(value) || value.contains(['{', '}', '[', ']', '`']))
        {
            format!("\"{}\"", escape_inline_quoted_attr(value))
        } else {
            quote_attr_value(value)
        }
    };
    let Some(attrs) = attrs else {
        return String::new();
    };
    let mut parts = Vec::new();
    let id_as_key = attrs
        .id
        .as_ref()
        .is_some_and(|id| !is_explicit_id_or_class_identifier(id) || conflicts(id));
    let mut seen_keys: Vec<&str> = Vec::new();
    let mut seen_key_set = (attrs.order.len() > 8).then(HashSet::new);
    let emit_id = |parts: &mut Vec<String>| {
        if let Some(id) = &attrs.id {
            if id_as_key {
                parts.push(format!("id={}", quote(id)));
            } else {
                parts.push(format!("#{}", escape_attr_name_value(id)));
            }
        }
    };
    let emit_classes = |parts: &mut Vec<String>| {
        for cls in &attrs.classes {
            // THE SHORTHAND ONLY REACHES WHAT A FENCE WORD REACHES. `.` takes the
            // `explicit_identifier` (carve#2435), so `-col`, `w-1/2` and the empty
            // class have no `.` spelling and take the key-value form the parser
            // now folds back into this same slot (CARVE-P4-007). Written as `.`
            // they were source this engine's own parser reads as a paragraph.
            if crate::parse::is_css_identifier(cls) && !conflicts(cls) {
                parts.push(format!(".{}", escape_attr_name_value(cls)));
            } else {
                parts.push(format!(
                    "class={}",
                    if markers.is_empty() && !bracketed {
                        quoted_attr_value(cls)
                    } else {
                        format!("\"{}\"", escape_inline_quoted_attr(cls))
                    }
                ));
            }
        }
    };
    let emit_key = |parts: &mut Vec<String>, key: &str| {
        if let Some(value) = attrs.key_values.get(key) {
            // EXACT key match, not case-insensitive: `LANG` and `lang` are
            // different attribute names, so folding here rewrote
            // `[x]{LANG=fr}` into `[x]{:fr}` and changed the name, which
            // breaks PART 11 §1 (carve#1137).
            if key == "lang" && is_language_tag(value) {
                parts.push(format!(":{value}"));
            } else if value.is_empty() && is_boolean_attr_name(key) && !conflicts(key) {
                // PART 11 §6c: a value-less attribute comes back as the bare
                // name, which is the production the language has for it. A key
                // needing escaping has no bare spelling to fall back to, and
                // neither does a `_`-first one (carve#1450) -- see
                // `is_boolean_attr_name`.
                parts.push(escape_attr_key(key));
            } else {
                parts.push(format!("{}={}", escape_attr_key(key), quote(value)));
            }
        }
    };
    if attrs.order.is_empty() {
        emit_id(&mut parts);
        emit_classes(&mut parts);
        for key in attrs.key_values.keys() {
            emit_key(&mut parts, key);
        }
    } else {
        for slot in &attrs.order {
            match slot {
                AttrSlot::Id => emit_id(&mut parts),
                AttrSlot::Class => emit_classes(&mut parts),
                AttrSlot::Key(key) => {
                    let unique = if let Some(seen) = &mut seen_key_set {
                        seen.insert(key.as_str())
                    } else if seen_keys.contains(&key.as_str()) {
                        false
                    } else {
                        seen_keys.push(key.as_str());
                        true
                    };
                    if unique {
                        emit_key(&mut parts, key);
                    }
                }
            }
        }
        for key in attrs.key_values.keys() {
            let seen = if let Some(seen) = &seen_key_set {
                seen.contains(key.as_str())
            } else {
                seen_keys.contains(&key.as_str())
            };
            if !seen {
                emit_key(&mut parts, key);
            }
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("{{{}}}", parts.join(" "))
    }
}

pub(super) fn is_language_tag(value: &str) -> bool {
    value.is_empty()
        || value.split('-').all(|subtag| {
            !subtag.is_empty()
                && subtag.len() <= 8
                && subtag.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
}

/// A value in the QUOTED form whatever it holds.
///
/// `unquoted_value` is `(letter | digit | '-' | '_' | '.' | ':')+`, so a class
/// like `w-1/2` has no bare spelling and [`quote_attr_value`] hands one back
/// anyway (carve#2440). The classes that reach the key-value form are exactly
/// the ones `.` cannot spell, so this asks no questions and quotes - which is
/// also the spelling carve#2435 ruled for the importer, `{class="-col"}`.
pub(super) fn quoted_attr_value(value: &str) -> String {
    format!("\"{}\"", escape_quoted_run(value, &['"', '|']))
}

pub(super) fn quote_attr_value(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|ch| !ch.is_whitespace() && !matches!(ch, '"' | '\'' | '\\' | '{' | '}' | '|'))
    {
        value.to_string()
    } else {
        // `\|` is the only pipe a table row's cell cut leaves in place
        // ([CARVE-P2-019]), so a pipe is escaped wherever the value sits.
        format!("\"{}\"", escape_quoted_run(value, &['"', '|']))
    }
}
