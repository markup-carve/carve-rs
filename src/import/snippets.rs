//! C2-H-215 to C2-H-217: the snippet carriers of a `roundtrip` render, read
//! back as snippet uses.

use super::dom::Id;
use super::{Code, Cx};
use serde_json::{Value, json};

struct Carrier {
    n: String,
    block: bool,
    name: String,
}

/// `N:MODE:$NAME`, `N` a decimal ordinal without a leading zero.
fn parse(value: &str) -> Option<Carrier> {
    let (n, rest) = value.split_once(':')?;
    let (mode, name) = rest.split_once(':')?;
    let name = name.strip_prefix('$')?;
    let ordinal = n.bytes().all(|b| b.is_ascii_digit()) && !n.starts_with('0') && !n.is_empty();
    let block = match mode {
        "inline" => false,
        "block" => true,
        _ => return None,
    };
    (ordinal && crate::snippet::is_name(name)).then(|| Carrier {
        n: n.to_string(),
        block,
        name: name.to_string(),
    })
}

impl Cx {
    /// C2-H-217: the scope carrier beside an accepted carrier reports the lost include.
    pub(crate) fn snippet_scope(&mut self, id: Id) {
        let Some(scope) = self.dom.attr(id, "data-carve-snippet-scope") else {
            return;
        };
        if scope != "include" {
            return self.ignored_carrier(
                id,
                "data-carve-snippet-scope",
                "the value is not include",
            );
        }
        self.report(
            id,
            Code::SnippetScopeLoss,
            "The snippet came from an included file; its definition reads back into this document",
        );
    }

    /// C2-H-186: the use an element with `data-carve-snippet` stands for, in a slot of `block`.
    fn accepted(&mut self, id: Id, block: bool) -> Option<Carrier> {
        let value = self.dom.attr(id, "data-carve-snippet")?.to_string();
        let carrier = parse(&value);
        let fault = match &carrier {
            None => Some("the value is not N:MODE:$NAME".to_string()),
            Some(c) if c.block != block => Some(format!(
                "the mode {} does not match a slot of {}",
                if c.block { "block" } else { "inline" },
                if block { "blocks" } else { "inline content" }
            )),
            Some(c)
                if self.probe_accepted.as_ref().map_or_else(
                    || self.snippet_ordinals.contains(&c.n),
                    |accepted| !accepted.contains(&id),
                ) =>
            {
                Some("another carrier holds the same ordinal".to_string())
            }
            Some(_) => None,
        };
        if let Some(fault) = fault {
            self.ignored_carrier(id, "data-carve-snippet", &fault);
            if self.dom.has(id, "data-carve-snippet-scope") {
                self.ignored_carrier(
                    id,
                    "data-carve-snippet-scope",
                    "no snippet carrier is read here",
                );
            }
            return None;
        }
        let carrier = carrier?;
        self.snippet_ordinals.insert(carrier.n.clone());
        if let Some(accepted) = self.accepted_snippet_ids.as_mut() {
            accepted.insert(id);
        }
        self.snippet_scope(id);
        Some(carrier)
    }

    pub(crate) fn inline_snippet(&mut self, id: Id) -> Option<Vec<Value>> {
        if !self.dom.has(id, "data-carve-snippet") {
            return None;
        }
        let c = self.accepted(id, false)?;
        Some(vec![
            json!({"type": "snippet_use", "name": c.name, "mode": "inline"}),
        ])
    }

    pub(crate) fn block_snippet(&mut self, id: Id) -> Option<Vec<Value>> {
        if !self.dom.has(id, "data-carve-snippet") {
            return None;
        }
        if !self.dom.is(id, "p") {
            self.ignored_carrier(
                id,
                "data-carve-snippet",
                "a block use is read only from a paragraph",
            );
            return None;
        }
        let c = self.accepted(id, true)?;
        Some(vec![
            json!({"type": "snippet_use", "name": c.name, "mode": "block"}),
        ])
    }

    /// C2-H-217: a note reference whose `data-carve-snippet-mark` names an accepted use is dropped.
    pub(crate) fn dropped_mark(&mut self, id: Id) -> bool {
        let Some(mark) = self.dom.attr(id, "data-carve-snippet-mark") else {
            return false;
        };
        if self.snippet_ordinals.contains(mark) {
            return true;
        }
        self.ignored_carrier(
            id,
            "data-carve-snippet-mark",
            "the value is no ordinal of a snippet use",
        );
        false
    }
}
