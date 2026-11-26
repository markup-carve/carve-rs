//! The rule every hold list of a spec-case runner follows (carve-js `scripts/holds.ts`):
//! a case the runner fails passes only when it is held, and a held case that passes,
//! or that the runner never sees, is a stale hold that fails the run.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

/// `(case id, the ticket or clause that fixes it)`.
pub type Holds = &'static [(&'static str, &'static str)];

/// The results of one runner, judged against its hold list once every case is seen.
pub struct Gate {
    name: String,
    holds: Vec<(String, String)>,
    /// Failure details per checked key; an empty list is a pass.
    results: BTreeMap<String, Vec<String>>,
}

/// What a runner's results come to against its hold list.
pub struct Judged {
    pub pass: usize,
    pub held: usize,
    /// The failures the list does not hold.
    pub failures: Vec<String>,
    /// The held keys that pass or that the runner never saw.
    pub stale: Vec<String>,
}

impl Gate {
    /// Holds keyed by case id, each with its cause.
    pub fn new(name: &str, holds: &[(&str, &str)]) -> Self {
        Self::from_pairs(
            name,
            holds.iter().map(|&(id, cause)| (id.to_string(), cause)),
        )
    }

    /// Holds keyed by case id, the causes in the list's comments.
    pub fn by_id(name: &str, holds: &[&str]) -> Self {
        Self::from_pairs(name, holds.iter().map(|&id| (id.to_string(), "")))
    }

    /// Holds keyed by `(case id, checked field)`, checked with [`Gate::check_field`].
    pub fn by_field(name: &str, holds: &[(&str, &str)]) -> Self {
        Self::from_pairs(name, holds.iter().map(|&(id, f)| (format!("{id} {f}"), "")))
    }

    fn from_pairs<'a>(name: &str, holds: impl Iterator<Item = (String, &'a str)>) -> Self {
        let mut list: Vec<(String, String)> = Vec::new();
        for (key, cause) in holds {
            assert!(
                !list.iter().any(|(k, _)| *k == key),
                "{name}: {key} is held twice"
            );
            list.push((key, cause.to_string()));
        }
        Self {
            name: name.to_string(),
            holds: list,
            results: BTreeMap::new(),
        }
    }

    /// Records one check of a case; a key checked more than once fails if any check fails.
    pub fn check(&mut self, key: &str, ok: bool, detail: impl FnOnce() -> String) {
        let entry = self.results.entry(key.to_string()).or_default();
        if !ok {
            entry.push(detail());
        }
    }

    pub fn check_field(
        &mut self,
        id: &str,
        field: &str,
        ok: bool,
        detail: impl FnOnce() -> String,
    ) {
        self.check(&format!("{id} {field}"), ok, detail);
    }

    /// Records the problems of one case; none is a pass.
    pub fn problems(&mut self, key: &str, problems: &[String]) {
        let entry = self.results.entry(key.to_string()).or_default();
        entry.extend(problems.iter().cloned());
    }

    /// Runs the assertions of one case; a panic is its failure.
    pub fn case(&mut self, key: &str, body: impl FnOnce()) {
        let problem = catch_unwind(AssertUnwindSafe(body))
            .err()
            .map(|e| panic_text(&*e));
        self.problems(key, problem.as_slice());
    }

    pub fn judge(self) -> Judged {
        let mut judged = Judged {
            pass: 0,
            held: 0,
            failures: Vec::new(),
            stale: Vec::new(),
        };
        for (key, problems) in &self.results {
            let hold = self.holds.iter().find(|(k, _)| k == key);
            match (problems.is_empty(), hold) {
                (true, None) => judged.pass += 1,
                (false, Some(_)) => judged.held += 1,
                (true, Some((_, cause))) => judged.stale.push(format!(
                    "=== {key}: held{} but passes; drop the hold",
                    why(cause)
                )),
                (false, None) => judged
                    .failures
                    .push(format!("=== {key}\n{}", problems.join("\n"))),
            }
        }
        for (key, cause) in &self.holds {
            if !self.results.contains_key(key) {
                judged.stale.push(format!(
                    "=== {key}: held{} but not a case of {}; drop the hold",
                    why(cause),
                    self.name
                ));
            }
        }
        println!(
            "{}: {} pass, {} held, {} fail, {} stale holds",
            self.name,
            judged.pass,
            judged.held,
            judged.failures.len(),
            judged.stale.len()
        );
        judged
    }

    /// Judges the run and fails it on an unheld failure or a stale hold.
    pub fn finish(self) {
        self.judge().assert();
    }
}

impl Judged {
    /// Unheld failures, then stale holds.
    pub fn lines(&self) -> Vec<String> {
        self.failures.iter().chain(&self.stale).cloned().collect()
    }

    pub fn assert(self) {
        let lines = self.lines();
        assert!(lines.is_empty(), "{}", lines.join("\n"));
    }
}

fn why(cause: &str) -> String {
    if cause.is_empty() {
        String::new()
    } else {
        format!(" ({cause})")
    }
}

/// Runs the test body of the one case `id`, plain assertions: as it is when `id` is
/// not held, and as a held case that must still fail when it is. `holds` is the
/// list of this test alone, so a hold of another id is stale.
pub fn expect_held(id: &str, holds: &[(&str, &str)], body: impl FnOnce()) {
    if let Some((other, cause)) = holds.iter().find(|(h, _)| *h != id) {
        panic!(
            "{other}: held{} but not the case of this test; drop the hold",
            why(cause)
        );
    }
    let Some((_, cause)) = holds.first() else {
        body();
        return;
    };
    assert!(
        catch_unwind(AssertUnwindSafe(body)).is_err(),
        "{id}: held{} but passes; drop the hold",
        why(cause)
    );
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
        .unwrap_or_else(|| "panicked".into())
}
