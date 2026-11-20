//! What one run of the writer holds: the lines written so far, the escapes and
//! spellings the reparse asked for (C2-W-089), the sidecar facts and the losses.

use super::node::{N, id, ty};
use crate::hash::{AddrMap, AddrSet};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

/// C2-L-020: where one continuation row broke the content of a cell.
#[derive(Debug, Clone)]
pub struct Wrap {
    pub line: i64,
    pub path: String,
    pub offset: usize,
    pub row: Option<i64>,
}

/// The facts of the sidecar that the writer reads (C2-L-046), by node.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub bare: Option<bool>,
    pub fenced: Option<bool>,
    pub hard_break_opener: Option<bool>,
    pub continuation_lines: Option<i64>,
    pub wraps: Option<Vec<Wrap>>,
    pub raw_ref: Option<String>,
    pub option_spelling: Option<BTreeMap<String, String>>,
    pub source_format: Option<String>,
    pub header_rows: Option<i64>,
}

/// One node that did not read back, and how far the writer is in the ways to make it form.
#[derive(Debug, Clone, Default)]
pub struct Attempt<'t> {
    pub step: usize,
    pub escapes: Vec<(N<'t>, usize)>,
}

/// One entry of the record of the writer, before its path is known.
#[derive(Debug, Clone)]
pub struct Loss {
    pub code: &'static str,
    pub message: String,
    pub node: Option<usize>,
    pub step: Option<String>,
}

/// A div matcher of an enabled extension (C2-X-008).
#[derive(Debug, Clone)]
pub struct Matcher {
    pub form: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct ProbeResult<'t> {
    pub found: Option<bool>,
    pub taken: Vec<(N<'t>, usize)>,
}

/// The state a probe of the repair swaps in and out (write-check).
#[derive(Debug, Default)]
pub struct Spelling<'t> {
    pub escapes: AddrMap<usize, Vec<usize>>,
    pub attempts: AddrMap<usize, Attempt<'t>>,
    pub losses: Vec<Loss>,
    pub lost: AddrSet<usize>,
    pub lost_keys: AddrMap<usize, Vec<String>>,
}

pub struct Writer<'t> {
    pub pool: &'t typed_arena::Arena<Value>,
    pub matchers: Vec<Matcher>,
    pub extensions: Vec<String>,
    pub lines: Vec<String>,
    pub lead: Option<String>,
    pub bare_dot_lead: bool,
    pub sp: Spelling<'t>,
    pub forms: AddrMap<usize, u32>,
    pub starts: Vec<(N<'t>, usize)>,
    pub start_index: AddrMap<usize, usize>,
    pub facts: Option<AddrMap<usize, Facts>>,
    pub wrap_points: Option<AddrMap<usize, Vec<usize>>>,
    pub wrap_starts: Option<AddrSet<usize>>,
    pub align_tables: bool,
    pub blank_first: bool,
    pub colon_runs: AddrMap<usize, usize>,
    pub kinds_below: AddrMap<usize, u8>,
    pub abbreviations: Vec<(String, String)>,
    pub link_keys: HashSet<String>,
    pub parsed: Option<Value>,
    pub prewritten: Option<usize>,
    pub parses: usize,
    pub probes: HashMap<String, ProbeResult<'t>>,
    pub node_ids: AddrMap<usize, usize>,
    pub stand_ins: AddrMap<usize, HashMap<String, N<'t>>>,
    pub line_opens: HashMap<String, bool>,
    pub work_chars: usize,
    pub work_limit: usize,
    /// The verbatim references written (C2-W-075): their backslashes are the author's.
    pub raw_refs: Vec<String>,
}

impl<'t> Writer<'t> {
    pub fn new(pool: &'t typed_arena::Arena<Value>, align_tables: bool) -> Self {
        Self {
            pool,
            matchers: Vec::new(),
            extensions: Vec::new(),
            lines: Vec::new(),
            lead: None,
            bare_dot_lead: false,
            sp: Spelling::default(),
            forms: AddrMap::default(),
            starts: Vec::new(),
            start_index: AddrMap::default(),
            facts: None,
            wrap_points: None,
            wrap_starts: None,
            align_tables,
            blank_first: false,
            colon_runs: AddrMap::default(),
            kinds_below: AddrMap::default(),
            abbreviations: Vec::new(),
            link_keys: HashSet::new(),
            parsed: None,
            prewritten: None,
            parses: 0,
            probes: HashMap::new(),
            node_ids: AddrMap::default(),
            stand_ins: AddrMap::default(),
            line_opens: HashMap::new(),
            work_chars: 0,
            work_limit: usize::MAX,
            raw_refs: Vec::new(),
        }
    }

    pub fn fact(&self, n: &Value) -> Option<&Facts> {
        self.facts.as_ref()?.get(&id(n))
    }

    pub fn form(&self, n: &Value) -> u32 {
        self.forms.get(&id(n)).copied().unwrap_or(0)
    }

    pub fn escapes_of(&self, n: &Value) -> &[usize] {
        self.sp.escapes.get(&id(n)).map_or(&[], Vec::as_slice)
    }

    pub fn has_escapes(&self, n: &Value) -> bool {
        self.sp.escapes.contains_key(&id(n))
    }

    pub fn is_lost(&self, n: &Value) -> bool {
        self.sp.lost.contains(&id(n))
    }

    pub fn clear_starts(&mut self) {
        self.starts.clear();
        self.start_index.clear();
    }

    /// Adds `at` to the escapes of `n`, keeping them sorted and unique.
    pub fn add_escape(&mut self, n: &Value, at: usize) {
        let list = self.sp.escapes.entry(id(n)).or_default();
        if let Err(place) = list.binary_search(&at) {
            list.insert(place, at);
        }
    }

    /// A text node that stands in for markup in a probe, kept per anchor and role.
    pub fn stand_in(&mut self, anchor: &Value, role: &str, value: &str) -> N<'t> {
        let name = format!("{role} {value}");
        let pool = self.pool;
        let roles = self.stand_ins.entry(id(anchor)).or_default();
        if let Some(known) = roles.get(&name) {
            return known;
        }
        let node: &'t Value = pool.alloc(serde_json::json!({"type": "text", "value": value}));
        roles.insert(name, node);
        node
    }

    pub fn node_id(&mut self, n: &Value) -> usize {
        let next = self.node_ids.len();
        *self.node_ids.entry(id(n)).or_insert(next)
    }
}

/// The columns a line can have in a place of `Writer::starts`.
pub const PLACE_COLUMNS: usize = 8192;

/// Notes where the paragraph `node` starts, for the test of its lines after a parse (C2-W-092).
pub fn note_place<'t>(w: &mut Writer<'t>, node: N<'t>, prefix: &str) {
    let head = w.lead.as_deref().unwrap_or(prefix);
    if !head.is_ascii() || head.len() >= PLACE_COLUMNS {
        return;
    }
    let place = w.lines.len() * PLACE_COLUMNS + head.len();
    if let Some(&i) = w.start_index.get(&id(node)) {
        if let Some(slot) = w.starts.get_mut(i) {
            slot.1 = place;
        }
    } else {
        w.start_index.insert(id(node), w.starts.len());
        w.starts.push((node, place));
    }
}

pub fn trim_end(text: &str) -> &str {
    text.trim_end_matches([' ', '\t'])
}

/// Writes one line behind the pending lead or `prefix`. C2-W-021: an empty
/// line has no spaces of its prefix either.
pub fn emit(w: &mut Writer<'_>, prefix: &str, text: &str) {
    let head = w.lead.take().unwrap_or_else(|| prefix.to_string());
    w.bare_dot_lead = false;
    if text.is_empty() {
        w.lines.push(trim_end(&head).to_string());
    } else {
        w.lines.push(head + text);
    }
}

/// Writes the lines of `text`: the first behind the lead or `prefix`, the others behind `prefix` and `indent`.
pub fn emit_lines(w: &mut Writer<'_>, prefix: &str, indent: &str, text: &str) {
    let mut inner = prefix.to_string();
    let mut rest = text;
    loop {
        match rest.find('\n') {
            None => {
                emit(w, &inner, rest);
                return;
            }
            Some(end) => {
                emit(w, &inner, rest.get(..end).unwrap_or(""));
                rest = rest.get(end + 1..).unwrap_or("");
                // C2-W-059, C2-B-138: a comment-only line stands at the column of the leaf that holds it.
                inner = if rest.starts_with("%%") {
                    prefix.to_string()
                } else {
                    format!("{prefix}{indent}")
                };
            }
        }
    }
}

/// C2-W-136: the key at `index` of the `attrs` of `node` has no spelling beside the field that writes the same key.
pub fn lose_key(w: &mut Writer<'_>, node: &Value, index: usize, key: &str) {
    let keys = w.sp.lost_keys.entry(id(node)).or_default();
    if keys.iter().any(|k| k == key) {
        return;
    }
    keys.push(key.to_string());
    w.sp.losses.push(Loss {
        code: "field-unspellable",
        message: format!("Carve source cannot spell key {key} in the attrs of a {} node beside the table fields that write it", ty(node)),
        node: Some(id(node)),
        step: Some(format!("/attrs/keyValues/{index}")),
    });
}

/// C2-W-011, C2-W-117: records that `node`, or its field `field`, could not be written. One entry per node.
pub fn lose(w: &mut Writer<'_>, node: &Value, field: Option<&str>) {
    if !w.sp.lost.insert(id(node)) {
        return;
    }
    let (code, message) = match field {
        None => (
            "structure-unspellable",
            format!("Carve source cannot spell a {} node here", ty(node)),
        ),
        Some(f) => (
            "field-unspellable",
            format!(
                "Carve source cannot spell field {f} of a {} node here",
                ty(node)
            ),
        ),
    };
    let step = field
        .filter(|f| node.get(*f).is_some())
        .map(|f| format!("/{f}"));
    w.sp.losses.push(Loss {
        code,
        message,
        node: Some(id(node)),
        step,
    });
}
