use serde_json::Map;
use std::fmt;

use crate::ast::Document;
use crate::ast_json::{from_json, parse_value, value_to_json, Json};

use super::{schema_map, SchemaMap};

type Object = Map<String, Json>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProseMirrorError {
    message: String,
}

impl ProseMirrorError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ProseMirrorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ProseMirrorError {}

pub fn from_prosemirror(json: &str) -> Result<Document, ProseMirrorError> {
    let input = parse_value(json).map_err(|e| ProseMirrorError::new(e.to_string()))?;
    let mut reader = Reader { map: schema_map() };
    let wire = reader.document(&input)?;
    from_json(&value_to_json(&wire)).map_err(|e| ProseMirrorError::new(e.to_string()))
}

struct Reader {
    map: &'static SchemaMap,
}

impl Reader {
    fn resolve(&self, name: &str) -> Option<(String, usize)> {
        // Two Carve types can claim one ProseMirror name - `carveDiv` for
        // both `div` and `admonition`, `link` for both `link` and `autolink` -
        // and which one this returns therefore depends on map iteration order,
        // which differs between engines. That is deliberately NOT arbitrated
        // here. Every colliding pair is handled by a single match arm that
        // decides from the node's own state, so the answer this gives for such
        // a name does not change the result.
        for (ty, names) in &self.map.names {
            if let Some(index) = names.iter().position(|n| n == name) {
                return Some((ty.clone(), index));
            }
            if self
                .map
                .accepts
                .get(ty)
                .is_some_and(|v| v.iter().any(|n| n == name))
            {
                return Some((ty.clone(), 0));
            }
        }
        None
    }

    fn document(&mut self, value: &Json) -> Result<Json, ProseMirrorError> {
        let obj = object_ref(value, "payload root")?;
        let name = string_field(obj, "type")?;
        if self.resolve(name).as_ref().map(|v| v.0.as_str()) != Some("document") {
            return Err(ProseMirrorError::new(
                "The payload root must be a ProseMirror document node",
            ));
        }
        let mut children = Vec::new();
        for child in array_field(obj, "content") {
            let child_obj = object_ref(child, "document child")?;
            let child_name = string_field(child_obj, "type")?;
            let (ty, _) = self.known(child_name)?;
            if ty == "frontmatter" {
                let a = attrs_obj(child_obj);
                children.push(node(
                    "frontmatter",
                    [
                        ("format", string_json(a, "format", "yaml")),
                        ("content", string_json(a, "content", "")),
                    ],
                ));
            } else if ty == "footnote" {
                let mut n = self.container(child_obj, "footnote", "children", false)?;
                if let Json::Object(ref mut o) = n {
                    o.insert(
                        "label".into(),
                        string_json(attrs_obj(child_obj), "label", ""),
                    );
                }
                children.push(n);
            } else {
                children.push(self.block(child)?);
            }
        }
        Ok(node(
            "document",
            [
                ("children", Json::Array(children)),
                ("srcByteLength", Json::from(0)),
            ],
        ))
    }

    fn known(&self, name: &str) -> Result<(String, usize), ProseMirrorError> {
        if let Some(resolved) = self.resolve(name) {
            return Ok(resolved);
        }
        // A preservation node is part of the wire, not an unknown name: it is
        // what a bridge writes for a construct its editor schema has no node
        // for, and its `carveSource` is the construct verbatim. This engine
        // never writes one and cannot yet read one, so it says which of the two
        // it is rather than reporting the name as unrecognized.
        if self.map.preservation.contains(name) {
            return Err(ProseMirrorError::new(format!(
                "`{name}` preserves Carve source this engine cannot yet read back"
            )));
        }
        Err(ProseMirrorError::new(format!(
            "Unknown ProseMirror node or mark type `{name}`"
        )))
    }

    fn block(&mut self, value: &Json) -> Result<Json, ProseMirrorError> {
        let obj = object_ref(value, "block")?;
        let name = string_field(obj, "type")?;
        let (ty, flavor) = self.known(name)?;
        let a = attrs_obj(obj);
        match ty.as_str() {
            "paragraph" => {
                // A lone image is a BLOCK image in Carve, not a paragraph
                // wrapping one, so the wrapper the editor had to add comes off
                // again. The exception is an image whose reference never
                // resolved: it renders as literal text, which is inline
                // content and needs its paragraph. Unwrapping it produced a
                // bare `![moon][gone]` outside any block.
                let content = array_field(obj, "content");
                if content.len() == 1 {
                    let only = object_ref(&content[0], "paragraph child")?;
                    if let Some((mapped, flavor)) = self.resolve(string_field(only, "type")?) {
                        let ia = attrs_obj(only);
                        let unresolved = string_opt(ia, "carveRef").is_some()
                            && string_opt(ia, "src").unwrap_or_default().is_empty();
                        if mapped == "image" && !unresolved {
                            return self.inline_atom(only, "image", flavor);
                        }
                    }
                }
                self.container(obj, &ty, "children", true)
            }
            "heading" => {
                let mut n = self.container(obj, &ty, "children", true)?;
                insert(&mut n, "level", number_json(a, "level", 1));
                Ok(n)
            }
            "code_block" => {
                let mut n = with_attrs(
                    node(
                        &ty,
                        [
                            ("content", Json::String(text_content(obj))),
                            ("lang", optional_string(a, "language")),
                            ("header", optional_string(a, "carveFenceTitle")),
                            ("label", optional_string(a, "carveFenceLabel")),
                        ],
                    ),
                    a,
                );
                remove_nulls(&mut n);
                Ok(n)
            }
            "thematic_break" => {
                let mut n = with_attrs(
                    node(&ty, [("marker", optional_string(a, "carveMarker"))]),
                    a,
                );
                remove_nulls(&mut n);
                Ok(n)
            }
            "block_quote" | "div" | "line_block" | "admonition" => {
                // `admonition` is not a container type of its own: an
                // admonition that turns out to have no kind IS a div.
                let container_ty = if ty == "admonition" {
                    "div"
                } else {
                    ty.as_str()
                };
                if container_ty == "div" && flavor == 0 {
                    if let Some(kind) = admonition_kind(a).map(str::to_owned) {
                        return self.admonition(obj, &kind);
                    }
                }
                let mut n = self.container(obj, container_ty, "children", false)?;
                if container_ty == "block_quote" {
                    insert(&mut n, "fenced", optional_bool(a, "carveFenced"));
                }
                if container_ty == "div" {
                    add_flavor_class(&mut n, flavor);
                    // The div's visible heading. `container` only carries
                    // attributes, and the label is a field of its own.
                    if let Json::Object(fields) = &mut n {
                        if let Some(label) = string_opt(a, "label") {
                            fields.insert("label".into(), Json::String(label.into()));
                        }
                    }
                }
                Ok(n)
            }
            "list" => self.list(obj, flavor),
            "table" => self.table(obj),
            "definition_list" => self.definition_list(obj),
            "figure" => self.figure(obj),
            "raw_block" => Ok(node(
                &ty,
                [
                    ("format", string_json(a, "format", "")),
                    ("content", Json::String(text_content(obj))),
                ],
            )),
            "comment" => Ok(node(
                &ty,
                [
                    ("block", bool_json(a, "block", true)),
                    // A payload that does not carry the flag is the `%%`
                    // spelling, which is what an editor that never saw a
                    // delimited comment would have produced.
                    ("delimited", bool_json(a, "delimited", false)),
                    ("content", Json::String(text_content(obj))),
                ],
            )),
            "link_reference_definition" => {
                let authored = title_is_authored(a);
                let mut n = with_attrs(
                    node(
                        &ty,
                        [
                            ("label", string_json(a, "label", "")),
                            ("href", string_json(a, "href", "")),
                            ("title", structural_title(a, authored)),
                        ],
                    ),
                    &attr_run(a, authored),
                );
                remove_nulls(&mut n);
                Ok(n)
            }
            "section" => self.container(obj, "div", "children", false),
            _ => Err(ProseMirrorError::new(format!(
                "ProseMirror type `{name}` is not valid in a block position"
            ))),
        }
    }

    /// An admonition, however its ProseMirror name resolved.
    ///
    /// Both `carveDiv` and the profile-vocabulary `admonition` entry land here.
    /// They used to build it twice, and the copies drifted: only one filtered
    /// `title` out of the attribute pass, so an admonition that reached the
    /// editor through the other one came back wearing a stray `title="..."` on
    /// the rendered element.
    fn admonition(&mut self, obj: &Object, kind: &str) -> Result<Json, ProseMirrorError> {
        let a = attrs_obj(obj);
        let children = array_field(obj, "content")
            .iter()
            .map(|v| self.block(v))
            .collect::<Result<Vec<_>, _>>()?;
        // The stamped opener title first; a payload from an editor that does
        // not stamp it falls back to `title`, which is also where an authored
        // title attribute lives - the ambiguity the stamp exists to remove.
        //
        // Which one supplied it decides whether `title` survives the attribute
        // pass. Taken as the opener, it must not ALSO become an authored
        // attribute, or an unstamped payload comes back with the words rendered
        // twice: once as the visible title and once as `title="..."` on the
        // element.
        let stamped = string_opt(a, "carveAdmonitionTitle");
        let from_title_attr = stamped.is_none() && string_opt(a, "title").is_some();
        let title = stamped
            .or_else(|| string_opt(a, "title"))
            .map(|s| Json::Array(vec![node("text", [("value", Json::String(s.into()))])]))
            .unwrap_or(Json::Null);
        let consumed: &[&str] = if from_title_attr {
            &["carveAdmonitionTitle", "title"]
        } else {
            &["carveAdmonitionTitle"]
        };
        // The kind is the opener word, and the outbound side appends it to the
        // classes because that is where an admonition's kind lives once it is
        // rendered. Leaving it there on the way back writes it twice: once as
        // `::: note` and once as an authored `{.note}` the author never typed.
        let mut n = with_attrs(
            node(
                "admonition",
                [
                    ("kind", Json::String(kind.into())),
                    ("title", title),
                    ("label", optional_string(a, "label")),
                    ("children", Json::Array(children)),
                ],
            ),
            &without(&without_class_word(a, kind), consumed),
        );
        remove_nulls(&mut n);
        Ok(n)
    }

    fn container(
        &mut self,
        obj: &Object,
        ty: &str,
        field: &str,
        inline: bool,
    ) -> Result<Json, ProseMirrorError> {
        let children = if inline {
            self.inlines(array_field(obj, "content"))?
        } else {
            array_field(obj, "content")
                .iter()
                .map(|v| self.block(v))
                .collect::<Result<_, _>>()?
        };
        Ok(with_attrs(
            node(ty, [(field, Json::Array(children))]),
            attrs_obj(obj),
        ))
    }

    fn list(&mut self, obj: &Object, flavor: usize) -> Result<Json, ProseMirrorError> {
        let a = attrs_obj(obj);
        let mut items = Vec::new();
        for item in array_field(obj, "content") {
            let io = object_ref(item, "list item")?;
            let (ity, _) = self.known(string_field(io, "type")?)?;
            if ity != "list_item" {
                return Err(ProseMirrorError::new(
                    "A list may contain only mapped list items",
                ));
            }
            let mut n = self.container(io, "list_item", "children", false)?;
            let checked = optional_bool(attrs_obj(io), "checked");
            // A payload is an editor's, not a parser's: it can carry any pair.
            // One that contradicts `checked` is dropped rather than trusted,
            // because `checked` is the attribute tiptap itself maintains.
            let state = match optional_string(attrs_obj(io), "carveTaskState") {
                Json::String(s)
                    if checked == Json::Bool(false)
                        && matches!(s.as_str(), " " | "-" | "_" | ">" | "?") =>
                {
                    Json::String(s)
                }
                _ => Json::Null,
            };
            insert(&mut n, "checked", checked);
            insert(&mut n, "taskState", state);
            items.push(n);
        }
        let ordered = flavor == 1;
        let mut n = with_attrs(
            node(
                "list",
                [
                    ("ordered", Json::Bool(ordered)),
                    ("tight", bool_json(a, "tight", true)),
                    ("items", Json::Array(items)),
                    (
                        "start",
                        if ordered && bool_value(a, "carveListStartExplicit", false) {
                            number_json(a, "start", 1)
                        } else {
                            Json::Null
                        },
                    ),
                    ("bareMarker", optional_bool(a, "carveBareMarker")),
                    ("olType", optional_string(a, "carveListStyle")),
                    ("delim", optional_string(a, "carveListMarker")),
                    (
                        "bulletChar",
                        if !ordered {
                            optional_string(a, "carveListMarker")
                        } else {
                            Json::Null
                        },
                    ),
                ],
            ),
            a,
        );
        remove_nulls(&mut n);
        Ok(n)
    }

    fn table(&mut self, obj: &Object) -> Result<Json, ProseMirrorError> {
        let mut rows = Vec::new();
        let mut caption = None;
        for child in array_field(obj, "content") {
            let co = object_ref(child, "table child")?;
            let (ty, _) = self.known(string_field(co, "type")?)?;
            if ty == "caption" {
                caption = Some(Json::Array(self.inlines(array_field(co, "content"))?));
                continue;
            }
            if ty != "table_row" {
                return Err(ProseMirrorError::new(
                    "A table may contain only a caption and rows",
                ));
            }
            let mut cells = Vec::new();
            for cell in array_field(co, "content") {
                let ce = object_ref(cell, "table cell")?;
                let (cty, flavor) = self.known(string_field(ce, "type")?)?;
                if cty != "table_cell" {
                    return Err(ProseMirrorError::new("A table row may contain only cells"));
                }
                let ca = attrs_obj(ce);
                let mut cn = with_attrs(
                    node(
                        "table_cell",
                        [
                            ("header", Json::Bool(flavor == 1)),
                            ("children", Json::Array(self.table_cell_inlines(ce)?)),
                            ("align", optional_string(ca, "alignment")),
                            ("valign", optional_string(ca, "verticalAlignment")),
                            (
                                "span",
                                match string_opt(ca, "carveSpanMarker") {
                                    Some("^") => Json::String("rowspan".into()),
                                    Some("<") => Json::String("colspan".into()),
                                    _ => Json::Null,
                                },
                            ),
                        ],
                    ),
                    ca,
                );
                remove_nulls(&mut cn);
                cells.push(cn);
            }
            rows.push(with_attrs(
                node("table_row", [("cells", Json::Array(cells))]),
                attrs_obj(co),
            ));
        }
        let mut n = with_attrs(
            node(
                "table",
                [
                    ("rows", Json::Array(rows)),
                    ("caption", caption.unwrap_or(Json::Null)),
                ],
            ),
            attrs_obj(obj),
        );
        remove_nulls(&mut n);
        Ok(n)
    }

    fn table_cell_inlines(&mut self, obj: &Object) -> Result<Vec<Json>, ProseMirrorError> {
        let content = array_field(obj, "content");
        if content.len() == 1 {
            if let Json::Object(p) = &content[0] {
                if self
                    .resolve(string_field(p, "type")?)
                    .as_ref()
                    .map(|v| v.0.as_str())
                    == Some("paragraph")
                {
                    return self.inlines(array_field(p, "content"));
                }
            }
        }
        self.inlines(content)
    }

    fn definition_list(&mut self, obj: &Object) -> Result<Json, ProseMirrorError> {
        let mut items = Vec::new();
        for child in array_field(obj, "content") {
            let co = object_ref(child, "definition child")?;
            let (ty, _) = self.known(string_field(co, "type")?)?;
            if ty != "definition_term" && ty != "definition_description" {
                return Err(ProseMirrorError::new("Invalid definition-list child"));
            }
            // A term holds inline content, a description holds blocks; both put
            // it under `children`.
            items.push(self.container(co, &ty, "children", ty == "definition_term")?);
        }
        let mut n = node("definition_list", [("items", Json::Array(items))]);
        // §17 L7, `const: true` in PART 12 §8: written back only when the
        // outbound side stamped it, so an absent attribute stays absent rather
        // than becoming an explicit `false` the schema does not name.
        insert(&mut n, "loose", optional_bool(attrs_obj(obj), "loose"));
        Ok(with_attrs(n, attrs_obj(obj)))
    }

    fn figure(&mut self, obj: &Object) -> Result<Json, ProseMirrorError> {
        let mut target = None;
        let mut caption = Json::Array(Vec::new());
        let mut short = Json::Null;
        for child in array_field(obj, "content") {
            let co = object_ref(child, "figure child")?;
            let (ty, _) = self.known(string_field(co, "type")?)?;
            if ty == "caption" {
                let v = Json::Array(self.inlines(array_field(co, "content"))?);
                if bool_value(attrs_obj(co), "short", false) {
                    short = v
                } else {
                    caption = v
                };
            } else {
                target = Some(
                    if ty == "paragraph" && array_field(co, "content").len() == 1 {
                        let only = &array_field(co, "content")[0];
                        let oo = object_ref(only, "figure target")?;
                        if self
                            .resolve(string_field(oo, "type")?)
                            .as_ref()
                            .map(|v| v.0.as_str())
                            == Some("image")
                        {
                            self.inline_atom(oo, "image", 0)?
                        } else {
                            self.block(child)?
                        }
                    } else {
                        self.block(child)?
                    },
                );
            }
        }
        let mut n = with_attrs(
            node(
                "figure",
                [
                    (
                        "target",
                        target.ok_or_else(|| ProseMirrorError::new("Figure needs a target"))?,
                    ),
                    ("caption", caption),
                    ("shortCaption", short),
                ],
            ),
            attrs_obj(obj),
        );
        remove_nulls(&mut n);
        Ok(n)
    }

    fn inlines(&mut self, values: &[Json]) -> Result<Vec<Json>, ProseMirrorError> {
        let mut out = Vec::new();
        for value in values {
            let obj = object_ref(value, "inline")?;
            let name = string_field(obj, "type")?;
            if Some(name) == self.map.mark_carrier.as_deref() {
                let built = vec![self.empty_mark(obj)?];
                self.apply_marks(obj, built, &mut out)?;
                continue;
            }
            let (ty, flavor) = self.known(name)?;
            let built = if ty == "text" {
                vec![node(
                    "text",
                    [(
                        "value",
                        Json::String(string_opt(obj, "text").unwrap_or("").into()),
                    )],
                )]
            } else {
                vec![self.inline_atom(obj, &ty, flavor)?]
            };
            self.apply_marks(obj, built, &mut out)?;
        }
        Ok(out)
    }

    fn apply_marks(
        &mut self,
        obj: &Object,
        mut built: Vec<Json>,
        out: &mut Vec<Json>,
    ) -> Result<(), ProseMirrorError> {
        if let Some(Json::Array(marks)) = obj.get("marks") {
            for mark in marks.iter().rev() {
                let mo = object_ref(mark, "mark")?;
                let mn = string_field(mo, "type")?;
                let (mty, _) = self.known(mn)?;
                let inner = built;
                built = vec![self.wrap_mark(&mty, attrs_obj(mo), inner)?];
            }
        }
        append_merged(out, built);
        Ok(())
    }

    /// The atom a mark with no content arrives as, read back as that mark.
    ///
    /// `markType` is the ProseMirror name, so it resolves through the same map
    /// the outbound side wrote it from, and `markAttrs` is the mark's own
    /// attribute map - absent where the mark had none.
    fn empty_mark(&mut self, obj: &Object) -> Result<Json, ProseMirrorError> {
        let a = attrs_obj(obj);
        let mark_type = string_opt(a, "markType").ok_or_else(|| {
            ProseMirrorError::new("An empty-mark carrier needs the ProseMirror name it stands for")
        })?;
        let (ty, _) = self.known(mark_type)?;
        let mark_attrs = match a.get("markAttrs") {
            Some(Json::Object(o)) => o.clone(),
            _ => Object::new(),
        };
        self.wrap_mark(&ty, &mark_attrs, Vec::new())
    }

    fn wrap_mark(
        &self,
        ty: &str,
        a: &Object,
        children: Vec<Json>,
    ) -> Result<Json, ProseMirrorError> {
        let n = match ty {
            "code" => with_attrs(
                node(ty, [("value", Json::String(wire_plain_text(&children)))]),
                a,
            ),
            "critic_comment" => node(ty, [("text", Json::String(wire_plain_text(&children)))]),
            "abbreviation" => node(
                ty,
                [
                    ("abbr", Json::String(wire_plain_text(&children))),
                    ("expansion", string_json(a, "title", "")),
                ],
            ),
            "link" | "autolink" => {
                let href = string_json(a, "href", "");
                if bool_value(a, "carveAutolink", false) {
                    // `with_attrs` like every other branch: an autolink can
                    // carry an attribute block too, and building the node bare
                    // dropped `<https://example.com>{.ext}` back to a classless
                    // autolink.
                    with_attrs(
                        node(
                            "autolink",
                            [
                                ("href", href),
                                ("text", Json::String(wire_plain_text(&children))),
                            ],
                        ),
                        a,
                    )
                } else {
                    let authored = title_is_authored(a);
                    let mut n = with_attrs(
                        node(
                            "link",
                            [
                                ("href", href),
                                ("children", Json::Array(children)),
                                ("title", structural_title(a, authored)),
                                ("ref", optional_string(a, "carveRef")),
                                ("rawRef", optional_string(a, "carveRawRef")),
                            ],
                        ),
                        &attr_run(a, authored),
                    );
                    remove_nulls(&mut n);
                    n
                }
            }
            "strong" | "emphasis" | "underline" | "strike" | "highlight" | "subscript"
            | "superscript" | "insert" | "delete" | "span" => {
                with_attrs(node(ty, [("children", Json::Array(children))]), a)
            }
            _ => {
                return Err(ProseMirrorError::new(format!(
                    "ProseMirror mark maps to unsupported Carve type `{ty}`"
                )))
            }
        };
        Ok(n)
    }

    fn inline_atom(
        &mut self,
        obj: &Object,
        ty: &str,
        flavor: usize,
    ) -> Result<Json, ProseMirrorError> {
        let a = attrs_obj(obj);
        let n = match ty {
            "hard_break" => node(ty, []),
            "image" => {
                let authored = title_is_authored(a);
                with_attrs(
                    node(
                        ty,
                        [
                            ("src", string_json(a, "src", "")),
                            ("alt", string_json(a, "alt", "")),
                            ("title", structural_title(a, authored)),
                            ("ref", optional_string(a, "carveRef")),
                            ("rawRef", optional_string(a, "carveRawRef")),
                        ],
                    ),
                    &attr_run(a, authored),
                )
            }
            "math" => with_attrs(
                node(
                    ty,
                    [
                        ("display", bool_json(a, "display", false)),
                        ("content", string_json(a, "src", "")),
                    ],
                ),
                a,
            ),
            "mention" => {
                if flavor == 1 {
                    node("tag", [("name", string_json(a, "id", ""))])
                } else {
                    node(ty, [("user", string_json(a, "id", ""))])
                }
            }
            "raw_inline" => node(
                ty,
                [
                    ("format", string_json(a, "format", "")),
                    ("content", string_json(a, "content", "")),
                ],
            ),
            "literal_inline" => {
                with_attrs(node(ty, [("content", string_json(a, "content", ""))]), a)
            }
            "symbol" => with_attrs(node(ty, [("name", string_json(a, "name", ""))]), a),
            "heading_ref" => node(ty, [("target", string_json(a, "target", ""))]),
            "substitution" => node(
                ty,
                [
                    ("oldText", string_json(a, "oldText", "")),
                    ("newText", string_json(a, "newText", "")),
                ],
            ),
            "inline_footnote" => with_attrs(
                node(
                    ty,
                    [(
                        "inline",
                        Json::Array(self.inlines(array_field(obj, "content"))?),
                    )],
                ),
                a,
            ),
            "footnote_ref" => with_attrs(node(ty, [("id", optional_string(a, "label"))]), a),
            "comment" => node(
                "comment",
                [
                    ("block", Json::Bool(false)),
                    ("delimited", bool_json(a, "delimited", false)),
                    ("content", string_json(a, "content", "")),
                ],
            ),
            "inline_extension" => {
                let source = string_opt(a, "carveSource").unwrap_or("");
                with_attrs(
                    node(
                        ty,
                        [
                            ("name", Json::String(source.trim_start_matches(':').into())),
                            (
                                "content",
                                Json::Array(self.inlines(array_field(obj, "content"))?),
                            ),
                        ],
                    ),
                    a,
                )
            }
            _ => {
                return Err(ProseMirrorError::new(format!(
                    "ProseMirror type maps to unsupported inline Carve type `{ty}`"
                )))
            }
        };
        let mut n = n;
        remove_nulls(&mut n);
        Ok(n)
    }
}

fn object_ref<'a>(v: &'a Json, what: &str) -> Result<&'a Object, ProseMirrorError> {
    if let Json::Object(o) = v {
        Ok(o)
    } else {
        Err(ProseMirrorError::new(format!("{what} must be an object")))
    }
}
fn string_field<'a>(o: &'a Object, k: &str) -> Result<&'a str, ProseMirrorError> {
    string_opt(o, k)
        .ok_or_else(|| ProseMirrorError::new(format!("Every ProseMirror node needs a string {k}")))
}
fn string_opt<'a>(o: &'a Object, k: &str) -> Option<&'a str> {
    match o.get(k) {
        Some(Json::String(s)) => Some(s),
        _ => None,
    }
}
fn attrs_obj(o: &Object) -> &Object {
    match o.get("attrs") {
        Some(Json::Object(a)) => a,
        _ => empty_object(),
    }
}
/// A copy of the attributes with one occurrence of `word` gone from `class`.
///
/// The LAST occurrence, because that is the one the outbound side appended; an
/// author who really wrote `{.note}` on a `::: note` keeps their own copy.
fn without_class_word(o: &Object, word: &str) -> Object {
    let mut out = o.clone();
    let Some(Json::String(class)) = o.get("class") else {
        return out;
    };
    let mut parts: Vec<&str> = class.split_whitespace().collect();
    let Some(position) = parts.iter().rposition(|part| *part == word) else {
        return out;
    };
    parts.remove(position);
    if parts.is_empty() {
        out.remove("class");
    } else {
        out.insert("class".into(), Json::String(parts.join(" ")));
    }
    out
}
fn without(o: &Object, keys: &[&str]) -> Object {
    o.iter()
        .filter(|(k, _)| !keys.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}
fn empty_object() -> &'static Object {
    static E: std::sync::OnceLock<Object> = std::sync::OnceLock::new();
    E.get_or_init(Object::new)
}
fn array_field<'a>(o: &'a Object, k: &str) -> &'a [Json] {
    match o.get(k) {
        Some(Json::Array(a)) => a,
        _ => &[],
    }
}
fn node<const N: usize>(ty: &str, fields: [(&str, Json); N]) -> Json {
    let mut o = Object::new();
    o.insert("type".into(), Json::String(ty.into()));
    for (k, v) in fields {
        o.insert(k.into(), v);
    }
    Json::Object(o)
}
fn insert(n: &mut Json, k: &str, v: Json) {
    if !matches!(v, Json::Null) {
        if let Json::Object(o) = n {
            o.insert(k.into(), v);
        }
    }
}
fn remove_nulls(n: &mut Json) {
    if let Json::Object(o) = n {
        o.retain(|_, v| !matches!(v, Json::Null));
    }
}
fn string_json(o: &Object, k: &str, d: &str) -> Json {
    Json::String(string_opt(o, k).unwrap_or(d).into())
}
fn optional_string(o: &Object, k: &str) -> Json {
    string_opt(o, k)
        .map(|s| Json::String(s.into()))
        .unwrap_or(Json::Null)
}
fn number_json(o: &Object, k: &str, d: i64) -> Json {
    match o.get(k) {
        Some(value @ Json::Number(_)) => value.clone(),
        _ => Json::from(d),
    }
}
fn bool_value(o: &Object, k: &str, d: bool) -> bool {
    match o.get(k) {
        Some(Json::Bool(v)) => *v,
        _ => d,
    }
}
fn bool_json(o: &Object, k: &str, d: bool) -> Json {
    Json::Bool(bool_value(o, k, d))
}
fn optional_bool(o: &Object, k: &str) -> Json {
    match o.get(k) {
        Some(Json::Bool(v)) => Json::Bool(*v),
        _ => Json::Null,
    }
}
fn text_content(o: &Object) -> String {
    array_field(o, "content")
        .iter()
        .filter_map(|v| {
            if let Json::Object(x) = v {
                string_opt(x, "text")
            } else {
                None
            }
        })
        .collect()
}
fn with_attrs(mut n: Json, a: &Object) -> Json {
    let mut id = None;
    let mut classes = Vec::new();
    let mut kv = Object::new();
    for (k, v) in a {
        match (k.as_str(), v) {
            ("id", Json::String(s)) => id = Some(s.clone()),
            ("class", Json::String(s)) => {
                classes = s.split_whitespace().map(str::to_owned).collect()
            }
            (_, Json::String(s)) if !is_structural_attr(k) => {
                kv.insert(k.clone(), Json::String(s.clone()));
            }
            _ => {}
        }
    }
    if id.is_some() || !classes.is_empty() || !kv.is_empty() {
        let mut ao = Object::new();
        // The run is replayed in the order it was WRITTEN in, and an editor is
        // free to have changed the document since: a slot the run names that is
        // now gone is skipped by the writer, and an attribute the run does not
        // name - a class the editor toggled on, an id it assigned - is still an
        // attribute and goes after the ones the run does name. Leaving it out
        // of `order` deleted it: `carveAttrOrder: ["k", "#id"]` plus a class
        // wrote `{k=v #i}` and the class was gone.
        //
        // With no run to replay the three appends run in sequence and produce
        // the canonical `#id .class key="val"` - which is also why the id is
        // appended before the classes rather than after.
        let authored: Vec<&str> = match a.get("carveAttrOrder") {
            Some(Json::Array(v)) => v
                .iter()
                .filter_map(|s| match s {
                    Json::String(s) => Some(s.as_str()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        let mut order: Vec<Json> = authored
            .iter()
            .map(|s| Json::String((*s).to_owned()))
            .collect();
        if let Some(v) = id {
            ao.insert("id".into(), Json::String(v));
            if !authored.contains(&"#id") {
                order.push(Json::String("#id".into()));
            }
        }
        if !classes.is_empty() {
            ao.insert(
                "classes".into(),
                Json::Array(classes.into_iter().map(Json::String).collect()),
            );
            if !authored.contains(&".class") {
                order.push(Json::String(".class".into()));
            }
        }
        if !kv.is_empty() {
            for key in kv.keys() {
                if !authored.contains(&key.as_str()) {
                    order.push(Json::String(key.clone()));
                }
            }
            ao.insert("keyValues".into(), Json::Object(kv));
        }
        ao.insert("order".into(), Json::Array(order));
        insert(&mut n, "attrs", Json::Object(ao));
    }
    n
}
/// Whether the attribute run names `title`, which is to say the wire's `title`
/// holds an AUTHORED `{title=...}` attribute rather than the structural title
/// slot.
///
/// The two spellings share one wire field, and this run is the only record of
/// which one the author typed. The outbound side keeps that record true - where
/// the structural slot wins the field it drops `title` from the run - so reading
/// it here is what stops `[z](safe.html){title="T"}` returning as
/// `[z](safe.html "T")` (carve-rs#1105).
fn title_is_authored(a: &Object) -> bool {
    matches!(a.get("carveAttrOrder"), Some(Json::Array(v))
        if v.iter().any(|s| matches!(s, Json::String(s) if s == "title")))
}

/// The structural title slot, with the old overloaded `title` as a fallback.
fn structural_title(a: &Object, authored: bool) -> Json {
    if a.contains_key("carveLinkTitle") {
        optional_string(a, "carveLinkTitle")
    } else if authored {
        Json::Null
    } else {
        optional_string(a, "title")
    }
}

/// The attribute run: the wire attributes minus `title`, unless the run claims
/// it - in which case `title` is one of the attributes and stays.
fn attr_run(a: &Object, authored: bool) -> Object {
    if authored {
        without(a, &["carveLinkTitle"])
    } else {
        without(a, &["carveLinkTitle", "title"])
    }
}

fn is_structural_attr(k: &str) -> bool {
    matches!(
        k,
        "alt"
            | "alignment"
            | "verticalAlignment"
            | "carveAttrOrder"
            | "carveAutolink"
            | "carveAdmonitionKind"
            | "carveAdmonitionTitle"
            | "carveBareMarker"
            | "carveFenceLabel"
            | "carveFenceTitle"
            | "carveHeadingRef"
            | "carveLinkTitle"
            | "carveListMarker"
            | "carveListStartExplicit"
            | "carveListStyle"
            | "carveMarker"
            | "carveRawRef"
            | "carveRef"
            | "carveSource"
            | "carveSpanMarker"
            | "carveTaskState"
            | "checked"
            | "class"
            | "colspan"
            | "content"
            | "display"
            | "format"
            | "href"
            | "id"
            | "integral"
            | "items"
            | "label"
            | "language"
            | "level"
            | "name"
            | "newText"
            | "oldText"
            | "raw"
            | "rowspan"
            | "short"
            | "src"
            | "loose"
            | "start"
            | "suppressAuthor"
            | "target"
            | "tight"
    )
}
fn admonition_kind(a: &Object) -> Option<&str> {
    // The outbound side stamps the kind, because it is free text and a class
    // cannot carry the distinction: `::: footnotes` is an admonition whose kind
    // is `footnotes`, and guessing from a list of known words turned it into a
    // plain div and lost its placement.
    if let Some(kind) = string_opt(a, "carveAdmonitionKind") {
        return Some(kind);
    }
    // A payload from an editor that does not stamp it still has to work, and
    // there the class is all there is. The list is the built-in vocabulary, so
    // an unstamped payload keeps the common kinds and loses the rest - which is
    // why the stamp exists.
    string_opt(a, "class")?.split_whitespace().find(|v| {
        matches!(
            *v,
            "note" | "tip" | "important" | "warning" | "caution" | "danger"
        )
    })
}
fn add_flavor_class(n: &mut Json, flavor: usize) {
    let class = match flavor {
        1 => "tabs",
        2 => "tab",
        _ => return,
    };
    if let Json::Object(o) = n {
        let attrs = o
            .entry(String::from("attrs"))
            .or_insert_with(|| Json::Object(Object::new()));
        if let Json::Object(a) = attrs {
            a.insert(
                "classes".into(),
                Json::Array(vec![Json::String(class.into())]),
            );
        }
    }
}
fn wire_plain_text(nodes: &[Json]) -> String {
    fn walk(v: &Json, s: &mut String) {
        if let Json::Object(o) = v {
            if let Some(Json::String(x)) = o.get("value").or_else(|| o.get("text")) {
                s.push_str(x)
            }
            for k in ["children", "content"] {
                if let Some(Json::Array(a)) = o.get(k) {
                    for v in a {
                        walk(v, s)
                    }
                }
            }
        }
    }
    let mut s = String::new();
    for n in nodes {
        walk(n, &mut s)
    }
    s
}
fn append_merged(out: &mut Vec<Json>, mut built: Vec<Json>) {
    for n in built.drain(..) {
        if let Some(last) = out.last_mut() {
            if merge_same(last, &n) {
                continue;
            }
        }
        out.push(n)
    }
}
fn merge_same(a: &mut Json, b: &Json) -> bool {
    let (Json::Object(ao), Json::Object(bo)) = (a, b) else {
        return false;
    };
    if ao.get("type") != bo.get("type") || ao.get("attrs") != bo.get("attrs") {
        return false;
    }
    if let (Some(Json::Array(ac)), Some(Json::Array(bc))) =
        (ao.get_mut("children"), bo.get("children"))
    {
        // An EMPTY mark is a construct of its own, not half of a run: merging
        // it into a neighbour leaves one node and deletes the other. Merging
        // exists to rejoin text an editor split in two, and text is what makes
        // the two halves one run - so EITHER side being empty refuses it, not
        // only both.
        //
        // Both-empty was the shape the carrier was written for (`[]{.x}[]{.x}`).
        // One-empty is the same loss and was still merged: `[]{.x}[a]{.x}`,
        // `[](/u)[a](/u)` and `{++}{++a++}` each came back with the empty
        // construct gone, in silence - the mark had a carrier by then, so
        // nothing reported it dropped.
        if ac.is_empty() || bc.is_empty() {
            return false;
        }
        ac.extend(bc.clone());
        true
    } else {
        false
    }
}
