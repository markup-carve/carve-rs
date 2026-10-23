use super::*;

pub fn migration_raw_heading_ids(source: &str) -> HashMap<String, usize> {
    if !source.contains("=html") {
        return HashMap::new();
    }
    fn visit(value: &Value, ids: &mut HashMap<String, usize>) {
        match value {
            Value::Object(object) => {
                if matches!(
                    object.get("type").and_then(Value::as_str),
                    Some("raw_inline" | "raw_block")
                ) && object.get("format").and_then(Value::as_str) == Some("html")
                {
                    if let Some(content) = object.get("content").and_then(Value::as_str) {
                        let dom = html5ever::parse_document(RcDom::default(), Default::default())
                            .one(content);
                        let mut pending = vec![dom.document.clone()];
                        while let Some(node) = pending.pop() {
                            for child in node.children.borrow().iter() {
                                if let NodeData::Element { name, attrs, .. } = &child.data {
                                    if matches!(
                                        &*name.local,
                                        "section" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                                    ) {
                                        for attr in attrs
                                            .borrow()
                                            .iter()
                                            .filter(|attr| &*attr.name.local == "id")
                                        {
                                            *ids.entry(attr.value.to_string()).or_default() += 1;
                                        }
                                    }
                                }
                                pending.push(child.clone());
                            }
                        }
                    }
                }
                for (key, child) in object {
                    if key != "attrs" && key != "position" {
                        visit(child, ids);
                    }
                }
            }
            Value::Array(items) => {
                for item in items {
                    visit(item, ids);
                }
            }
            _ => {}
        }
    }
    let document: Value =
        serde_json::from_str(&crate::to_json(&crate::parse(source))).expect("parser emitted JSON");
    let mut ids = HashMap::new();
    visit(&document, &mut ids);
    ids
}

pub(super) fn rewrite_same_file_heading_links(
    source: String,
) -> Result<String, crate::RenderCarveError> {
    Ok(rewrite_same_file_heading_links_reported(source, &HashMap::new())?.value)
}

/// A case-only fragment rewrite: the destination before it, and the old and new fragment.
pub(crate) struct FragmentFixup {
    pub(crate) destination: String,
    pub(crate) from: String,
    pub(crate) to: String,
}

/// `unfixed` is the same rendering with the original fragments, so assessment sees only the conversion.
pub(crate) struct FragmentRewrite {
    pub(crate) value: String,
    pub(crate) unfixed: String,
    pub(crate) fixups: Vec<FragmentFixup>,
}

/// `authored` maps a converted destination back to the one the Markdown link
/// wrote (CARVE-P2-031).
pub(crate) fn rewrite_same_file_heading_links_reported(
    source: String,
    authored: &HashMap<String, String>,
) -> Result<FragmentRewrite, crate::RenderCarveError> {
    if !source.contains("](#") {
        return Ok(FragmentRewrite {
            unfixed: source.clone(),
            value: source,
            fixups: Vec::new(),
        });
    }
    let html = crate::to_html(&source);
    let mut raw_heading_ids = migration_raw_heading_ids(&source);
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
    let mut exact = HashSet::new();
    let mut aliases: HashMap<String, Option<String>> = HashMap::new();
    let mut pending = vec![dom.document.clone()];
    while let Some(node) = pending.pop() {
        for child in node.children.borrow().iter() {
            if let NodeData::Element { name, attrs, .. } = &child.data {
                if &*name.local == "a" {
                    for attr in attrs
                        .borrow()
                        .iter()
                        .filter(|attr| &*attr.name.local == "name")
                    {
                        exact.insert(attr.value.to_string());
                    }
                }
                for attr in attrs
                    .borrow()
                    .iter()
                    .filter(|attr| &*attr.name.local == "id")
                {
                    let id = attr.value.to_string();
                    exact.insert(id.clone());
                    if let Some(count) = raw_heading_ids.get_mut(&id) {
                        if *count > 0 {
                            *count -= 1;
                            continue;
                        }
                    }
                    if id.rsplit_once('-').is_some_and(|(_, number)| {
                        !number.is_empty()
                            && !number.starts_with('0')
                            && number != "1"
                            && number.bytes().all(|byte| byte.is_ascii_digit())
                    }) {
                        continue;
                    }
                    let heading_section = &*name.local == "section" && child.children.borrow().iter().any(|nested| {
                        matches!(&nested.data, NodeData::Element { name, .. } if matches!(&*name.local, "h1" | "h2" | "h3" | "h4" | "h5" | "h6"))
                    });
                    let nested_heading =
                        matches!(&*name.local, "h1" | "h2" | "h3" | "h4" | "h5" | "h6");
                    if heading_section || nested_heading {
                        let key = id.to_lowercase();
                        aliases
                            .entry(key)
                            .and_modify(|old| {
                                if old.as_ref() != Some(&id) {
                                    *old = None;
                                }
                            })
                            .or_insert(Some(id));
                    }
                }
            }
            pending.push(child.clone());
        }
    }
    fn decode(value: &str) -> Option<String> {
        let mut bytes = Vec::new();
        let mut index = 0;
        while index < value.len() {
            if value.as_bytes()[index] == b'%' {
                let hex = std::str::from_utf8(value.as_bytes().get(index + 1..index + 3)?).ok()?;
                bytes.push(u8::from_str_radix(hex, 16).ok()?);
                index += 3;
            } else {
                bytes.push(value.as_bytes()[index]);
                index += 1;
            }
        }
        String::from_utf8(bytes).ok()
    }
    fn encode(value: &str) -> String {
        encode_markdown_destination(value, false)
    }
    fn visit(
        value: &mut Value,
        exact: &HashSet<String>,
        aliases: &HashMap<String, Option<String>>,
        fixups: &mut Vec<FragmentFixup>,
        authored: &HashMap<String, String>,
    ) {
        match value {
            Value::Object(object) => {
                if object.get("type").and_then(Value::as_str) == Some("link")
                    && !object.contains_key("ref")
                {
                    if let Some(href) = object.get("href").and_then(Value::as_str) {
                        if let Some(raw) = href.strip_prefix('#') {
                            if let Some(fragment) = decode(raw) {
                                if !exact.contains(&fragment) {
                                    if let Some(Some(id)) = aliases.get(&fragment.to_lowercase()) {
                                        let to = encode(id);
                                        let written =
                                            authored.get(href).map_or(href, String::as_str);
                                        fixups.push(FragmentFixup {
                                            destination: written.to_owned(),
                                            from: written
                                                .strip_prefix('#')
                                                .unwrap_or(raw)
                                                .to_owned(),
                                            to: to.clone(),
                                        });
                                        object
                                            .insert("href".into(), Value::String(format!("#{to}")));
                                    }
                                }
                            }
                        }
                    }
                }
                for (key, child) in object.iter_mut() {
                    if key != "attrs" && key != "position" {
                        visit(child, exact, aliases, fixups, authored);
                    }
                }
            }
            Value::Array(items) => {
                for item in items {
                    visit(item, exact, aliases, fixups, authored);
                }
            }
            _ => {}
        }
    }
    let mut document: Value =
        serde_json::from_str(&crate::to_json(&crate::parse(&source))).expect("parser emitted JSON");
    let original = document.clone();
    let mut fixups = Vec::new();
    visit(&mut document, &exact, &aliases, &mut fixups, authored);
    if fixups.is_empty() {
        return Ok(FragmentRewrite {
            unfixed: source.clone(),
            value: source,
            fixups,
        });
    }
    let render = |document: &Value| -> Result<String, crate::RenderCarveError> {
        let document = crate::from_json(&document.to_string()).expect("parser emitted AST JSON");
        let result = crate::render_carve(&document)?;
        Ok(if source.ends_with('\n') {
            result
        } else {
            result.trim_end_matches('\n').to_string()
        })
    };
    Ok(FragmentRewrite {
        value: render(&document)?,
        unfixed: render(&original)?,
        fixups,
    })
}
