use super::{parse_blocks_with_options_at_level, Options, ProbeGuard, SourceEndingGuard};

/// Verse ownership comes from the block parser, including lazy and attached lines.
/// Only documents containing a candidate opener need this structural pass.
#[derive(Clone, Copy, Default)]
pub(super) enum VerseLine {
    #[default]
    Outside,
    Opener,
    Body,
}

impl VerseLine {
    pub(super) fn is_owned(self) -> bool {
        !matches!(self, Self::Outside)
    }
    pub(super) fn is_body(self) -> bool {
        matches!(self, Self::Body)
    }
}

pub(super) fn verse_owned_lines(source: &str, options: &Options<'_>) -> Vec<VerseLine> {
    let owned = vec![VerseLine::Outside; source.lines().count()];
    if !source.contains("]:") {
        return owned;
    }
    if !source
        .lines()
        .any(|line| line.contains("::: ") && line.trim_end().ends_with('|'))
    {
        return owned;
    }
    let _probe = ProbeGuard::enter();
    let _ending = SourceEndingGuard::fragment();
    let recorder = VerseRecorder::enter(owned);
    let probe_options = Options {
        extensions: options.extensions.clone(),
        source_lines: true,
        ..Options::default()
    };
    parse_blocks_with_options_at_level(source, &probe_options, true);
    let owned = RECORDED.with(|stack| std::mem::take(stack.borrow_mut().last_mut().unwrap()));
    drop(recorder);
    owned
}

thread_local! {
    static PAUSED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static RECORDED: std::cell::RefCell<Vec<Vec<VerseLine>>> = const { std::cell::RefCell::new(Vec::new()) };
}

struct VerseRecorder(bool);
impl VerseRecorder {
    fn enter(owned: Vec<VerseLine>) -> Self {
        RECORDED.with(|stack| stack.borrow_mut().push(owned));
        Self(PAUSED.with(|paused| paused.replace(false)))
    }
}
impl Drop for VerseRecorder {
    fn drop(&mut self) {
        PAUSED.with(|paused| paused.set(self.0));
        RECORDED.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

pub(super) fn recording() -> bool {
    !PAUSED.with(std::cell::Cell::get) && RECORDED.with(|stack| !stack.borrow().is_empty())
}

pub(super) fn record(line: Option<usize>, kind: VerseLine) {
    RECORDED.with(|stack| {
        if let Some(owned) = stack.borrow_mut().last_mut() {
            if let Some(slot) = line
                .and_then(|line| line.checked_sub(1))
                .and_then(|line| owned.get_mut(line))
            {
                *slot = kind;
            }
        }
    });
}

pub(super) struct RecordingPause(bool);
impl RecordingPause {
    pub(super) fn enter() -> Self {
        Self(PAUSED.with(|paused| paused.replace(true)))
    }
}
impl Drop for RecordingPause {
    fn drop(&mut self) {
        PAUSED.with(|paused| paused.set(self.0));
    }
}
