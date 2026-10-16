//! EXP-1331 — the session row's LIVE caption: one short line of what the
//! agent is doing right now (`Reading CartLineItem.tsx`, `Editing
//! ProductCard.tsx`, `Running bun test`), the line every client's session
//! card and Threads row shows under the title while the session works.
//!
//! - [`tool_caption`] turns a tool edge (the wire [`steer::ActivityEvent::Tool`]
//!   the mapper already built: its name, its `detail`, its contract
//!   `toolKind`) into the caption, capped at [`CAPTION_MAX`] chars;
//! - [`CaptionForwarder`] writes it to `sessions.report{caption}`: only on
//!   CHANGE, at most once per [`CAPTION_MIN_INTERVAL`] (a burst of reads
//!   settles on its last one), a clear (`null`) at once. The engine feeds it
//!   `None` between turns, so a turn end clears the line; the end sequence
//!   [`CaptionForwarder::finish`]es it.

use std::time::{Duration, Instant};

use steer::{ActivityEvent, ToolKind};

/// The `sessions.caption` column's bound (`varchar(80)`), in chars.
pub const CAPTION_MAX: usize = 80;

/// The throttle: at most one caption write per session in this window.
pub const CAPTION_MIN_INTERVAL: Duration = Duration::from_secs(2);

/// The caption a tool edge reads as, `None` for an edge that says nothing
/// worth a line (a mode switch, a blank call). A path detail reads as its
/// FILE name; a command as its head (the mapper already cut it); a detail
/// that is the model's own description of a command (`Run the tests`) is the
/// caption itself.
pub fn tool_caption(event: &ActivityEvent) -> Option<String> {
    let ActivityEvent::Tool { name, detail, tool_kind, .. } = event else {
        return None;
    };
    let detail = detail.as_deref().map(str::trim).filter(|detail| !detail.is_empty());
    let name = name.trim();
    let file = || detail.map(file_name);
    let with = |verb: &str, subject: Option<String>| match subject {
        Some(subject) => format!("{verb} {subject}"),
        None => verb.to_string(),
    };
    let caption = match tool_kind.unwrap_or(ToolKind::Other) {
        ToolKind::Read => with("Reading", file()),
        ToolKind::Edit => with("Editing", file()),
        ToolKind::Delete => with("Deleting", file()),
        ToolKind::Move => with("Moving", file()),
        ToolKind::Search => with("Searching", detail.map(str::to_string)),
        ToolKind::Fetch => with("Fetching", detail.map(str::to_string)),
        ToolKind::Execute => match detail {
            Some(detail) if is_description(detail) => detail.to_string(),
            other => with("Running", other.map(str::to_string)),
        },
        ToolKind::Think => "Thinking".to_string(),
        ToolKind::Wait => with("Waiting on", detail.or(Some(name)).filter(|s| !s.is_empty()).map(str::to_string)),
        ToolKind::Script => "Running a script".to_string(),
        ToolKind::SwitchMode => return None,
        ToolKind::Other => match steer::exp_tool_display(name, false) {
            Some(display) => with(display.caption, detail.map(str::to_string)),
            None if name.is_empty() => return None,
            None => with(name, detail.map(str::to_string)),
        },
    };
    normalize_caption(&caption)
}

/// One line, whitespace collapsed, at most [`CAPTION_MAX`] chars (a cut line
/// ends in `…`); `None` when blank.
pub fn normalize_caption(raw: &str) -> Option<String> {
    let line = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.is_empty() {
        return None;
    }
    if line.chars().count() <= CAPTION_MAX {
        return Some(line);
    }
    let mut cut: String = line.chars().take(CAPTION_MAX - 1).collect();
    cut.truncate(cut.trim_end().len());
    cut.push('…');
    Some(cut)
}

/// The first non-blank line of a reply (a `/btw` aside's answer as its
/// caption), normalised; markdown heading/list/quote marks dropped.
pub fn first_line_caption(text: &str) -> Option<String> {
    let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    let line = line.trim_start_matches(['#', '>', '-', '*', ' ']);
    normalize_caption(line)
}

fn file_name(path: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    trimmed
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(trimmed)
        .to_string()
}

/// A command detail that is the model's description (`Run the test suite`)
/// rather than a command head (`bun test`): it opens with a capital letter
/// and has words after it.
fn is_description(detail: &str) -> bool {
    detail.chars().next().is_some_and(char::is_uppercase) && detail.contains(' ')
}

/// The write hook: `true` = the server took it.
pub type CaptionHook = std::sync::Arc<dyn Fn(Option<&str>) -> bool + Send + Sync>;

/// The throttled, change-only caption writer (see the module docs).
#[derive(Debug, Default)]
pub struct CaptionForwarder {
    /// What the server holds (as far as this run knows): `None` = nothing
    /// written yet or cleared.
    forwarded: Option<String>,
    /// The last write ATTEMPT, landed or not — the throttle's clock.
    last_write: Option<Instant>,
}

impl CaptionForwarder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tick(&mut self, caption: Option<&str>, hook: &CaptionHook) {
        self.tick_at(Instant::now(), caption, hook);
    }

    /// [`Self::tick`] with the clock injected. A set waits out the
    /// throttle (the newest caption wins when it opens); a clear of a
    /// written caption goes at once.
    pub fn tick_at(&mut self, now: Instant, caption: Option<&str>, hook: &CaptionHook) {
        if self.forwarded.as_deref() == caption {
            return;
        }
        if caption.is_some()
            && self
                .last_write
                .is_some_and(|at| now.saturating_duration_since(at) < CAPTION_MIN_INTERVAL)
        {
            return;
        }
        self.write(now, caption, hook);
    }

    /// The end sequence's last write, past the throttle: `pin` (an aside's
    /// answer) or the clear, skipped when the server already holds it.
    pub fn finish(&mut self, pin: Option<&str>, hook: &CaptionHook) {
        if self.forwarded.as_deref() == pin {
            return;
        }
        self.write(Instant::now(), pin, hook);
    }

    fn write(&mut self, now: Instant, caption: Option<&str>, hook: &CaptionHook) {
        self.last_write = Some(now);
        if hook(caption) {
            self.forwarded = caption.map(str::to_string);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn tool(name: &str, detail: Option<&str>, kind: ToolKind) -> ActivityEvent {
        ActivityEvent::Tool {
            name: name.to_string(),
            detail: detail.map(str::to_string),
            id: Some("t1".to_string()),
            tool_kind: Some(kind),
            subagent_id: None,
            at: None,
        }
    }

    #[test]
    fn tool_edges_read_as_a_verb_and_their_subject() {
        let cases = [
            (tool("Read", Some("src/components/CartLineItem.tsx"), ToolKind::Read), Some("Reading CartLineItem.tsx")),
            (tool("Edit", Some("src/ProductCard.tsx"), ToolKind::Edit), Some("Editing ProductCard.tsx")),
            (tool("Write", None, ToolKind::Edit), Some("Editing")),
            (tool("Bash", Some("bun test"), ToolKind::Execute), Some("Running bun test")),
            (tool("Bash", Some("Run the unit tests"), ToolKind::Execute), Some("Run the unit tests")),
            (tool("Grep", Some("fn deliver"), ToolKind::Search), Some("Searching fn deliver")),
            (tool("WebFetch", Some("https://example.com"), ToolKind::Fetch), Some("Fetching https://example.com")),
            (tool("TaskOutput", Some("the build"), ToolKind::Wait), Some("Waiting on the build")),
            (tool("ExitPlanMode", None, ToolKind::SwitchMode), None),
            (tool("mcp.linear.list_issues", None, ToolKind::Other), Some("mcp.linear.list_issues")),
            (tool("", None, ToolKind::Other), None),
        ];
        for (event, expected) in cases {
            assert_eq!(tool_caption(&event).as_deref(), expected, "{event:?}");
        }
        assert_eq!(tool_caption(&ActivityEvent::narration("hi")), None);
    }

    #[test]
    fn a_caption_is_one_line_of_at_most_eighty_chars() {
        let long = format!("Running {}", "x".repeat(200));
        let capped = normalize_caption(&long).unwrap();
        assert_eq!(capped.chars().count(), CAPTION_MAX);
        assert!(capped.ends_with('…'));
        assert_eq!(normalize_caption("  a\n  b "), Some("a b".to_string()));
        assert_eq!(normalize_caption(" \n "), None);
        assert_eq!(
            first_line_caption("\n## The answer is 42\n\nmore"),
            Some("The answer is 42".to_string())
        );
    }

    fn recorder(ok: bool) -> (CaptionHook, Arc<Mutex<Vec<Option<String>>>>) {
        let writes = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&writes);
        let hook: CaptionHook = Arc::new(move |caption: Option<&str>| {
            sink.lock().unwrap().push(caption.map(str::to_string));
            ok
        });
        (hook, writes)
    }

    #[test]
    fn the_forwarder_throttles_skips_repeats_and_clears_at_once() {
        let (hook, writes) = recorder(true);
        let mut forwarder = CaptionForwarder::new();
        let t0 = Instant::now();
        forwarder.tick_at(t0, None, &hook);
        assert!(writes.lock().unwrap().is_empty(), "nothing to clear yet");
        forwarder.tick_at(t0, Some("Reading a.rs"), &hook);
        forwarder.tick_at(t0 + Duration::from_millis(500), Some("Reading a.rs"), &hook);
        forwarder.tick_at(t0 + Duration::from_millis(900), Some("Reading b.rs"), &hook);
        forwarder.tick_at(t0 + Duration::from_millis(1500), Some("Editing c.rs"), &hook);
        assert_eq!(*writes.lock().unwrap(), vec![Some("Reading a.rs".to_string())]);
        // The window opens: the NEWEST caption goes.
        forwarder.tick_at(t0 + Duration::from_millis(2100), Some("Editing c.rs"), &hook);
        forwarder.tick_at(t0 + Duration::from_millis(2200), Some("Editing c.rs"), &hook);
        // The turn ended: the clear does not wait.
        forwarder.tick_at(t0 + Duration::from_millis(2300), None, &hook);
        forwarder.tick_at(t0 + Duration::from_millis(2400), None, &hook);
        assert_eq!(
            *writes.lock().unwrap(),
            vec![Some("Reading a.rs".to_string()), Some("Editing c.rs".to_string()), None]
        );
        // The end sequence: nothing left to clear.
        forwarder.finish(None, &hook);
        assert_eq!(writes.lock().unwrap().len(), 3);
        // An aside's pin goes past the throttle.
        forwarder.finish(Some("42"), &hook);
        assert_eq!(writes.lock().unwrap().last().unwrap().as_deref(), Some("42"));
    }

    #[test]
    fn a_failed_write_is_retried_after_the_window() {
        let (hook, writes) = recorder(false);
        let mut forwarder = CaptionForwarder::new();
        let t0 = Instant::now();
        forwarder.tick_at(t0, Some("Reading a.rs"), &hook);
        forwarder.tick_at(t0 + Duration::from_millis(1000), Some("Reading a.rs"), &hook);
        forwarder.tick_at(t0 + Duration::from_millis(2500), Some("Reading a.rs"), &hook);
        assert_eq!(writes.lock().unwrap().len(), 2);
    }
}
