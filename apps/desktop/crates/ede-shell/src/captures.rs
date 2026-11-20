//! #EXP-2024 (6b): THE shell's capture loop. One thread with its own
//! ede-protocol connection (the root grant: `system` lists and captures the
//! agent spaces) photographs what the person can see: the windows of the
//! switcher, the taskbar previews and Expo (#EDE-12), the agent spaces of the
//! taskbar's open Agent spaces list and the pinned view of one. The shell
//! derives the [`Plan`] from what is open each time anything opens or
//! closes (nothing while the screen is locked, under a fullscreen window,
//! under the chat or Settings); without a target nothing is captured.
//!
//! The rates (`f5-f6-design.md` 6.3): a capture is a software pass on the
//! thread that draws the person's frames, so after a space capture that took
//! t the next one of that target waits max(250 ms, 12 × t) for the view and
//! max(5 s, 12 × t) for a thumbnail of the list (taken at once when the list
//! opens); `since` makes an undrawn space cost nothing and send nothing. t is
//! the capture call's round trip (never the shell's own shrink and PNG),
//! the shortest of the target's last frames: a wait in the compositor's
//! queue (another caller's pass inside its floor, a GPU fence) is not what
//! the pass cost, a pass that stays slow is. A failed capture is retried
//! after the view's floor; the fallback needs 5 s of failures.
//! The list of spaces (the taskbar's button) is polled here, never on the
//! shell's thread: every 2 s while a space exists, every 10 s while none
//! does. An IO error (a call slower than ede-protocol's call timeout ends
//! the connection) reconnects with a back-off; a target whose captures
//! failed for 5 s shows Image's fallback; a space that is gone is said.
//! Frames are pictures in memory: never written, logged or sent anywhere
//! but the shell's own documents.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Condvar, Mutex};
use std::time::{Duration, Instant};

use ede_protocol::{CaptureTarget, Client, ErrorCode, SpaceId, SpaceInfo, WindowId};

use crate::thumbs;

/// Every capture call the loop sent (the log lines carry it: it stands
/// still while nothing is captured).
pub static CAPTURES: AtomicU64 = AtomicU64::new(0);

/// The windows' round (about 5 fps, #EDE-12).
pub const WINDOW_ROUND: Duration = Duration::from_millis(200);
/// The pinned view's floor: at most 4 frames a second.
pub const VIEW_FLOOR: Duration = Duration::from_millis(250);
/// A list thumbnail's floor: one every 5 s.
pub const THUMB_FLOOR: Duration = Duration::from_secs(5);
/// The next capture of a target waits this many times the last one's cost.
pub const COST_FACTOR: u32 = 12;
/// `ListSpaces` while a space exists, and while none does.
pub const LIST_WITH_SPACES: Duration = Duration::from_secs(2);
pub const LIST_WITHOUT_SPACES: Duration = Duration::from_secs(10);
/// Failed captures for this long: the target's fallback picture.
pub const FALLBACK_AFTER: Duration = Duration::from_secs(5);
/// The reconnect back-off: doubled from the first to the last.
pub const BACKOFF_FIRST: Duration = Duration::from_millis(500);
pub const BACKOFF_LAST: Duration = Duration::from_secs(10);

/// A space to photograph: its id and the pixels its picture is shrunk to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shot {
    pub space: String,
    pub pixels: (u32, u32),
}

/// What the loop photographs now.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub windows: Vec<String>,
    /// The pinned view's space.
    pub view: Option<Shot>,
    /// The open list's thumbnails.
    pub thumbs: Vec<Shot>,
}

impl Plan {
    /// The plan has a space target.
    pub fn has_spaces(&self) -> bool {
        self.view.is_some() || !self.thumbs.is_empty()
    }
}

/// What the loop tells the shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Out {
    /// A window's thumbnail (a `data:` url).
    Window { id: String, url: String },
    /// The compositor's spaces, when they changed.
    Spaces(Vec<SpaceInfo>),
    /// A space's new picture: the view's (`view`) or the list's thumbnail.
    Picture { space: String, view: bool, url: String },
    /// The target's captures failed for [`FALLBACK_AFTER`]: its fallback.
    Failing { space: String, view: bool },
    /// The compositor knows no such space any more.
    Gone { space: String },
}

/// What the shell asks the loop to do on its connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ask {
    /// Close (`DestroySpace`) a space: the list's Close.
    Close(String),
    /// List the spaces at once (a capture found one gone).
    List,
}

/// What the shell's thread and the loop share.
#[derive(Default)]
pub struct Shared {
    state: Mutex<SharedState>,
    wake: Condvar,
}

#[derive(Default)]
struct SharedState {
    plan: Plan,
    generation: u64,
    asks: Vec<Ask>,
    stop: bool,
}

impl Shared {
    /// A new plan; `true` when it differs from the last (the loop wakes).
    pub fn set_plan(&self, plan: Plan) -> bool {
        let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if s.plan == plan {
            return false;
        }
        s.plan = plan;
        s.generation += 1;
        self.wake.notify_all();
        true
    }

    pub fn ask(&self, ask: Ask) {
        self.state.lock().unwrap_or_else(|p| p.into_inner()).asks.push(ask);
        self.wake.notify_all();
    }

    /// The loop ends at its next turn.
    pub fn stop(&self) {
        self.state.lock().unwrap_or_else(|p| p.into_inner()).stop = true;
        self.wake.notify_all();
    }

    /// The plan if it moved since `generation`, the asks, the stop.
    fn take(&self, generation: u64) -> (Option<(Plan, u64)>, Vec<Ask>, bool) {
        let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
        let plan = (s.generation != generation).then(|| (s.plan.clone(), s.generation));
        (plan, std::mem::take(&mut s.asks), s.stop)
    }

    /// Sleeps until `until` (or for good without one), or until the plan
    /// moves, an ask comes or the loop is stopped.
    fn wait(&self, generation: u64, until: Option<Instant>) {
        let s = self.state.lock().unwrap_or_else(|p| p.into_inner());
        let idle = |s: &mut SharedState| s.generation == generation && s.asks.is_empty() && !s.stop;
        match until {
            Some(until) => {
                let left = until.saturating_duration_since(Instant::now());
                drop(self.wake.wait_timeout_while(s, left, idle));
            }
            None => {
                drop(self.wake.wait_while(s, idle));
            }
        }
    }
}

/// The wait before a target's next capture after one that took `took`.
pub fn next_after(floor: Duration, took: Duration) -> Duration {
    floor.max(took.saturating_mul(COST_FACTOR))
}

/// One space target's schedule.
#[derive(Debug, Clone)]
struct Target {
    pixels: (u32, u32),
    due: Instant,
    since: Option<u64>,
    failing_since: Option<Instant>,
    fell_back: bool,
    /// The round trips of its last frames (a pass each), the newest last.
    costs: Vec<Duration>,
}

/// How many recent frames a target's cost is the shortest of.
pub const COST_WINDOW: usize = 4;

impl Target {
    fn new(pixels: (u32, u32), now: Instant) -> Self {
        Target { pixels, due: now, since: None, failing_since: None, fell_back: false, costs: Vec::new() }
    }
}

/// What the loop does next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    List,
    Windows,
    Space { space: String, view: bool, since: Option<u64>, pixels: (u32, u32) },
    /// Nothing is due before then (`None` = nothing at all).
    Wait(Option<Instant>),
}

/// How a space capture ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Captured {
    /// A picture of `serial`, or nothing drawn since `serial`.
    Frame(u64),
    Unchanged,
    Gone,
    Failed,
}

/// The loop's schedule, pure: what is due when, from the plan and how each
/// capture went.
#[derive(Debug)]
pub struct Schedule {
    windows: Vec<String>,
    windows_due: Option<Instant>,
    view: Option<(String, Target)>,
    thumbs: HashMap<String, Target>,
    list_due: Instant,
}

impl Schedule {
    pub fn new(now: Instant) -> Self {
        Schedule { windows: Vec::new(), windows_due: None, view: None, thumbs: HashMap::new(), list_due: now }
    }

    /// The plan moved: a new target is due at once; a target that stays
    /// keeps its schedule (a new size takes a fresh picture at once).
    pub fn set_plan(&mut self, plan: &Plan, now: Instant) {
        self.windows = plan.windows.clone();
        self.windows_due = match (self.windows.is_empty(), self.windows_due) {
            (true, _) => None,
            (false, Some(due)) => Some(due),
            (false, None) => Some(now),
        };
        self.view = plan.view.as_ref().map(|shot| match self.view.take() {
            Some((space, target)) if space == shot.space && target.pixels == shot.pixels => (space, target),
            Some((space, target)) if space == shot.space => (space, Target { pixels: shot.pixels, due: now, since: None, ..target }),
            _ => (shot.space.clone(), Target::new(shot.pixels, now)),
        });
        let mut thumbs = HashMap::new();
        for shot in &plan.thumbs {
            let target = match self.thumbs.remove(&shot.space) {
                Some(t) if t.pixels == shot.pixels => t,
                Some(t) => Target { pixels: shot.pixels, due: now, since: None, ..t },
                None => Target::new(shot.pixels, now),
            };
            thumbs.insert(shot.space.clone(), target);
        }
        self.thumbs = thumbs;
    }

    /// Lists the spaces at the next step.
    pub fn list_now(&mut self, now: Instant) {
        self.list_due = now;
    }

    /// The spaces were listed: the next list in 2 s while one exists, else 10 s.
    pub fn listed(&mut self, now: Instant, any: bool) {
        self.list_due = now + if any { LIST_WITH_SPACES } else { LIST_WITHOUT_SPACES };
    }

    /// A windows' round that started at `started` is done.
    pub fn windows_done(&mut self, started: Instant) {
        if self.windows_due.is_some() {
            self.windows_due = Some(started + WINDOW_ROUND);
        }
    }

    /// What is due at `now`, the earliest first (on a tie: the windows, the
    /// view, the list, the thumbnails: Alt+Tab is open for a moment).
    pub fn next(&self, now: Instant) -> Step {
        let mut best: Option<(Instant, u8, Step)> = None;
        let mut consider = |due: Instant, rank: u8, step: Step| {
            if best.as_ref().is_none_or(|(d, r, _)| (due, rank) < (*d, *r)) {
                best = Some((due, rank, step));
            }
        };
        if let Some((space, t)) = &self.view {
            consider(t.due, 1, Step::Space { space: space.clone(), view: true, since: t.since, pixels: t.pixels });
        }
        consider(self.list_due, 2, Step::List);
        if let Some(due) = self.windows_due {
            consider(due, 0, Step::Windows);
        }
        let mut thumbs: Vec<(&String, &Target)> = self.thumbs.iter().collect();
        thumbs.sort_by(|a, b| a.1.due.cmp(&b.1.due).then_with(|| a.0.cmp(b.0)));
        if let Some((space, t)) = thumbs.first() {
            consider(t.due, 3, Step::Space { space: (*space).clone(), view: false, since: t.since, pixels: t.pixels });
        }
        match best {
            Some((due, _, step)) if due <= now => step,
            Some((due, _, _)) => Step::Wait(Some(due)),
            None => Step::Wait(None),
        }
    }

    fn target(&mut self, space: &str, view: bool) -> Option<&mut Target> {
        if view {
            self.view.as_mut().filter(|(s, _)| s == space).map(|(_, t)| t)
        } else {
            self.thumbs.get_mut(space)
        }
    }

    /// A space capture of `space` that took `took` ended at `now` as `how`;
    /// `Some` = what the shell hears of it beyond the picture (the space is
    /// gone, the target fell back).
    pub fn captured(&mut self, space: &str, view: bool, now: Instant, took: Duration, how: Captured) -> Option<Out> {
        let floor = if view { VIEW_FLOOR } else { THUMB_FLOOR };
        let target = self.target(space, view)?;
        match how {
            Captured::Frame(serial) => {
                target.costs.push(took);
                if target.costs.len() > COST_WINDOW {
                    target.costs.remove(0);
                }
                let cost = target.costs.iter().copied().min().unwrap_or(took);
                target.due = now + next_after(floor, cost);
                target.since = Some(serial);
                target.failing_since = None;
                target.fell_back = false;
                None
            }
            Captured::Unchanged => {
                // No pass: nothing new about the cost.
                let cost = target.costs.iter().copied().min().unwrap_or_default();
                target.due = now + next_after(floor, cost);
                target.failing_since = None;
                None
            }
            // Its time is no pass (a timeout, a refusal): soon again.
            Captured::Gone => {
                target.due = now + VIEW_FLOOR;
                Some(Out::Gone { space: space.to_string() })
            }
            Captured::Failed => {
                target.due = now + VIEW_FLOOR;
                target.failing_since.get_or_insert(now);
                None
            }
        }
    }

    /// The connection is gone: every space target counts as failing from
    /// now and is due again as soon as a connection is back.
    pub fn disconnected(&mut self, now: Instant) {
        for t in self.view.iter_mut().map(|(_, t)| t).chain(self.thumbs.values_mut()) {
            t.failing_since.get_or_insert(now);
            t.since = None;
            t.due = now;
        }
    }

    /// The targets whose captures failed for [`FALLBACK_AFTER`] by `now`
    /// and did not say so yet.
    pub fn fallbacks(&mut self, now: Instant) -> Vec<Out> {
        let mut out = Vec::new();
        let due = |t: &mut Target| {
            let over = t.failing_since.is_some_and(|at| now.saturating_duration_since(at) >= FALLBACK_AFTER) && !t.fell_back;
            if over {
                t.fell_back = true;
                // The fallback shows: the next good capture must be a whole
                // picture, even of a space that drew nothing since.
                t.since = None;
            }
            over
        };
        if let Some((space, t)) = self.view.as_mut() {
            if due(t) {
                out.push(Out::Failing { space: space.clone(), view: true });
            }
        }
        let mut spaces: Vec<&String> = self.thumbs.keys().collect();
        spaces.sort();
        let spaces: Vec<String> = spaces.into_iter().cloned().collect();
        for space in spaces {
            if let Some(t) = self.thumbs.get_mut(&space) {
                if due(t) {
                    out.push(Out::Failing { space, view: false });
                }
            }
        }
        out
    }

    /// When the next fallback is due while every capture fails (`None` = none failing).
    fn next_fallback(&self) -> Option<Instant> {
        let failing = |t: &Target| t.failing_since.filter(|_| !t.fell_back).map(|at| at + FALLBACK_AFTER);
        self.view.iter().map(|(_, t)| t).chain(self.thumbs.values()).filter_map(failing).min()
    }
}

/// The back-off before the `attempt`-th reconnect (0 = the first).
pub fn backoff(attempt: u32) -> Duration {
    BACKOFF_FIRST.saturating_mul(1 << attempt.min(8)).min(BACKOFF_LAST)
}

/// The loop: runs until [`Shared::stop`] or until the shell stops listening.
/// `connect` makes a connection (the root grant's, `Client::connect_env`).
pub fn run(connect: impl Fn() -> Result<Client, ede_protocol::Error>, shared: &Shared, tx: &mpsc::Sender<Out>) {
    let mut schedule = Schedule::new(Instant::now());
    let mut generation = u64::MAX;
    let mut ede: Option<Client> = None;
    let mut attempts = 0u32;
    let mut retry_at = Instant::now();
    let mut spaces: Option<Vec<SpaceInfo>> = None;
    let mut space_targets = false;
    // Asks wait for a connection.
    let mut asks: Vec<Ask> = Vec::new();
    loop {
        let (plan, more, stop) = shared.take(generation);
        if stop {
            return;
        }
        asks.extend(more);
        let now = Instant::now();
        if let Some((plan, g)) = plan {
            generation = g;
            schedule.set_plan(&plan, now);
            if plan.has_spaces() != space_targets {
                space_targets = plan.has_spaces();
                let captures = CAPTURES.load(Ordering::Relaxed);
                if space_targets {
                    tracing::info!(view = plan.view.is_some(), thumbnails = plan.thumbs.len(), captures, "space targets started");
                } else {
                    tracing::info!(captures, "space targets stopped");
                }
            }
        }
        if ede.is_none() && now >= retry_at {
            match connect() {
                Ok(client) => {
                    if attempts > 0 {
                        tracing::info!(attempts, "capture connection back");
                    }
                    ede = Some(client);
                    attempts = 0;
                    schedule.list_now(now);
                }
                Err(e) => {
                    retry_at = now + backoff(attempts);
                    if attempts == 0 {
                        tracing::info!("capture connection: {e}");
                    }
                    attempts = attempts.saturating_add(1);
                    schedule.disconnected(now);
                }
            }
        }
        for out in schedule.fallbacks(now) {
            if tx.send(out).is_err() {
                return;
            }
        }
        let Some(client) = ede.as_mut() else {
            let until = [Some(retry_at), schedule.next_fallback()].into_iter().flatten().min();
            shared.wait(generation, until);
            continue;
        };
        let mut lost = false;
        let mut pending = std::mem::take(&mut asks).into_iter();
        while let Some(ask) = pending.next() {
            match ask {
                Ask::Close(space) => {
                    let Ok(id) = SpaceId::new(&space) else { continue };
                    match client.destroy_space(id) {
                        Ok(()) => tracing::info!("space closed"),
                        Err(ede_protocol::Error::Io(_)) => {
                            // Again on the next connection, with the rest.
                            lost = true;
                            asks.push(Ask::Close(space));
                            asks.extend(pending.by_ref());
                            break;
                        }
                        Err(e) => tracing::warn!("space close: {e}"),
                    }
                    schedule.list_now(now);
                }
                Ask::List => schedule.list_now(now),
            }
        }
        if !lost {
            match schedule.next(Instant::now()) {
                Step::Wait(until) => {
                    let until = [until, schedule.next_fallback()].into_iter().flatten().min();
                    shared.wait(generation, until);
                    continue;
                }
                Step::List => match client.spaces() {
                    Ok(list) => {
                        schedule.listed(Instant::now(), !list.is_empty());
                        if spaces.as_ref() != Some(&list) {
                            spaces = Some(list.clone());
                            if tx.send(Out::Spaces(list)).is_err() {
                                return;
                            }
                        }
                    }
                    Err(ede_protocol::Error::Io(_)) => lost = true,
                    Err(e) => {
                        tracing::warn!("spaces: {e}");
                        schedule.listed(Instant::now(), false);
                    }
                },
                Step::Windows => {
                    let started = Instant::now();
                    for id in schedule.windows.clone() {
                        let Ok(window) = WindowId::new(&id) else { continue };
                        CAPTURES.fetch_add(1, Ordering::Relaxed);
                        // A protected window refuses the capture: no picture, as intended.
                        match client.capture(CaptureTarget::Window { window }) {
                            Ok(capture) => {
                                if let Some(url) = thumbs::data_url(&capture) {
                                    if tx.send(Out::Window { id, url }).is_err() {
                                        return;
                                    }
                                }
                            }
                            Err(ede_protocol::Error::Io(_)) => {
                                lost = true;
                                break;
                            }
                            Err(_) => {}
                        }
                    }
                    schedule.windows_done(started);
                }
                Step::Space { space, view, since, pixels } => {
                    let started = Instant::now();
                    let mut took = Duration::ZERO;
                    CAPTURES.fetch_add(1, Ordering::Relaxed);
                    let how = match SpaceId::new(&space) {
                        Err(_) => Captured::Gone,
                        Ok(id) => match client.capture_since(CaptureTarget::Space { space: id }, since).inspect(|_| took = started.elapsed()) {
                            Ok(Some(capture)) => {
                                let serial = capture.info.serial;
                                if let Some(url) = thumbs::data_url_within(&capture, pixels) {
                                    if tx.send(Out::Picture { space: space.clone(), view, url }).is_err() {
                                        return;
                                    }
                                }
                                Captured::Frame(serial)
                            }
                            Ok(None) => Captured::Unchanged,
                            Err(e) if e.has_code(ErrorCode::NotFound) => Captured::Gone,
                            Err(ede_protocol::Error::Io(_)) => {
                                lost = true;
                                Captured::Failed
                            }
                            Err(_) => Captured::Failed,
                        },
                    };
                    if let Some(out) = schedule.captured(&space, view, Instant::now(), took, how) {
                        if tx.send(out).is_err() {
                            return;
                        }
                    }
                }
            }
        }
        if lost {
            tracing::info!("capture connection lost");
            ede = None;
            retry_at = Instant::now() + backoff(0);
            attempts = 1;
            schedule.disconnected(Instant::now());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;
    use std::sync::Arc;

    use ede_protocol::{read_frame, write_frame, Call, CaptureInfo, Failure, Hello, Message, Request, SessionId, Token};
    use serde_json::Value;

    const SPACE: &str = "space-1";

    fn shot(space: &str) -> Shot {
        Shot { space: space.into(), pixels: (64, 40) }
    }

    /// Section 8, 6b: after a capture that took t the next one of that
    /// target waits 12 × t, never under its floor.
    #[test]
    fn after_a_capture_of_t_the_next_waits_twelve_t() {
        assert_eq!(next_after(VIEW_FLOOR, Duration::from_millis(10)), VIEW_FLOOR, "a cheap capture: 4 frames a second");
        assert_eq!(next_after(VIEW_FLOOR, Duration::from_millis(50)), Duration::from_millis(600));
        assert_eq!(next_after(THUMB_FLOOR, Duration::from_millis(100)), THUMB_FLOOR);
        assert_eq!(next_after(THUMB_FLOOR, Duration::from_secs(1)), Duration::from_secs(12));
        let t0 = Instant::now();
        let mut s = Schedule::new(t0);
        s.listed(t0, false);
        s.set_plan(&Plan { view: Some(shot(SPACE)), ..Default::default() }, t0);
        assert_eq!(s.next(t0), Step::Space { space: SPACE.into(), view: true, since: None, pixels: (64, 40) }, "a new view is due at once");
        let done = t0 + Duration::from_millis(184);
        assert_eq!(s.captured(SPACE, true, done, Duration::from_millis(184), Captured::Frame(7)), None);
        assert_eq!(s.next(done), Step::Wait(Some(done + Duration::from_millis(184 * 12))), "the VM's slow pass: 2.2 s");
        let later = done + Duration::from_millis(184 * 12);
        assert_eq!(s.next(later), Step::Space { space: SPACE.into(), view: true, since: Some(7), pixels: (64, 40) }, "then with since");
    }

    /// A wait in the compositor's queue is not the pass's cost: the shortest
    /// of the target's last frames sets the pace; a pass that stays slow does.
    #[test]
    fn the_pace_follows_the_shortest_recent_frame() {
        let t0 = Instant::now();
        let mut s = Schedule::new(t0);
        s.listed(t0 + Duration::from_secs(60), false);
        s.set_plan(&Plan { view: Some(shot(SPACE)), ..Default::default() }, t0);
        s.captured(SPACE, true, t0, Duration::from_millis(10), Captured::Frame(1));
        // One call that waited 800 ms behind the budget: still 4 frames a second.
        s.captured(SPACE, true, t0, Duration::from_millis(800), Captured::Frame(2));
        assert_eq!(s.next(t0), Step::Wait(Some(t0 + VIEW_FLOOR)));
        // Slow for the whole window: slow.
        for serial in 3..=(2 + COST_WINDOW as u64) {
            s.captured(SPACE, true, t0, Duration::from_millis(100), Captured::Frame(serial));
        }
        assert_eq!(s.next(t0), Step::Wait(Some(t0 + Duration::from_millis(1200))));
        // An unchanged answer is no pass: the pace stays.
        s.captured(SPACE, true, t0, Duration::from_micros(200), Captured::Unchanged);
        assert_eq!(s.next(t0), Step::Wait(Some(t0 + Duration::from_millis(1200))));
    }

    /// The list's thumbnails: at once when it opens, then every 5 s each;
    /// `ListSpaces` every 2 s with a space, every 10 s without.
    #[test]
    fn thumbnails_at_open_then_every_five_seconds_and_the_list_polls() {
        let t0 = Instant::now();
        let mut s = Schedule::new(t0);
        assert_eq!(s.next(t0), Step::List, "the first thing: the spaces");
        s.listed(t0, false);
        assert_eq!(s.next(t0), Step::Wait(Some(t0 + LIST_WITHOUT_SPACES)), "no space: every 10 s");
        s.listed(t0, true);
        assert_eq!(s.next(t0), Step::Wait(Some(t0 + LIST_WITH_SPACES)));
        s.set_plan(&Plan { thumbs: vec![shot("space-1"), shot("space-2")], ..Default::default() }, t0);
        let Step::Space { space: first, view: false, .. } = s.next(t0) else { panic!("a thumbnail first") };
        s.captured(&first, false, t0, Duration::from_millis(5), Captured::Frame(1));
        let Step::Space { space: second, view: false, .. } = s.next(t0) else { panic!("the other at once too") };
        assert_ne!(first, second);
        s.captured(&second, false, t0, Duration::from_millis(5), Captured::Frame(1));
        assert_eq!(s.next(t0), Step::Wait(Some(t0 + LIST_WITH_SPACES)), "the list before the next thumbnail");
        s.listed(t0 + THUMB_FLOOR, true);
        assert!(matches!(s.next(t0 + THUMB_FLOOR), Step::Space { view: false, since: Some(1), .. }), "5 s later, with since");
        s.set_plan(&Plan::default(), t0 + THUMB_FLOOR);
        assert_eq!(s.next(t0 + THUMB_FLOOR), Step::Wait(Some(t0 + THUMB_FLOOR + LIST_WITH_SPACES)), "the list closed: no capture");
    }

    /// A target whose captures failed for 5 s shows the fallback once; a
    /// picture again clears it; a lost connection counts as failing.
    #[test]
    fn five_seconds_of_failed_captures_fall_back_once() {
        let t0 = Instant::now();
        let mut s = Schedule::new(t0);
        s.listed(t0 + Duration::from_secs(60), false);
        s.set_plan(&Plan { view: Some(shot(SPACE)), ..Default::default() }, t0);
        s.captured(SPACE, true, t0, Duration::ZERO, Captured::Frame(2));
        assert_eq!(s.captured(SPACE, true, t0, Duration::ZERO, Captured::Failed), None);
        assert_eq!(s.next(t0), Step::Wait(Some(t0 + VIEW_FLOOR)), "a failed capture is retried soon");
        assert!(s.fallbacks(t0 + Duration::from_secs(4)).is_empty());
        assert_eq!(s.fallbacks(t0 + FALLBACK_AFTER), vec![Out::Failing { space: SPACE.into(), view: true }]);
        assert!(s.fallbacks(t0 + Duration::from_secs(6)).is_empty(), "said once");
        assert!(matches!(s.next(t0 + Duration::from_secs(6)), Step::Space { since: None, .. }), "after the fallback, a whole picture even of a still space");
        s.captured(SPACE, true, t0 + Duration::from_secs(7), Duration::ZERO, Captured::Frame(3));
        let t1 = t0 + Duration::from_secs(8);
        s.disconnected(t1);
        assert!(s.fallbacks(t1 + Duration::from_secs(4)).is_empty());
        assert_eq!(s.next_fallback(), Some(t1 + FALLBACK_AFTER));
        assert_eq!(s.fallbacks(t1 + FALLBACK_AFTER), vec![Out::Failing { space: SPACE.into(), view: true }]);
        assert_eq!(s.captured(SPACE, true, t1, Duration::ZERO, Captured::Gone), Some(Out::Gone { space: SPACE.into() }));
        // A call that failed after a 10 s timeout is no pass of 10 s: the floor.
        let t2 = t1 + Duration::from_secs(20);
        s.listed(t2 + Duration::from_secs(60), false);
        s.captured(SPACE, true, t2, Duration::from_secs(10), Captured::Failed);
        assert_eq!(s.next(t2), Step::Wait(Some(t2 + VIEW_FLOOR)));
        s.disconnected(t2);
        assert!(matches!(s.next(t2), Step::Space { view: true, since: None, .. }), "due again with a new connection");
    }

    /// A view that moves to another space (its session's space made anew)
    /// starts over at once; another size takes a fresh picture at once.
    #[test]
    fn a_moved_or_resized_view_is_due_at_once() {
        let t0 = Instant::now();
        let mut s = Schedule::new(t0);
        s.listed(t0, true);
        s.set_plan(&Plan { view: Some(shot("space-1")), ..Default::default() }, t0);
        s.captured("space-1", true, t0, Duration::from_millis(5), Captured::Frame(9));
        s.set_plan(&Plan { view: Some(shot("space-2")), ..Default::default() }, t0);
        assert_eq!(s.next(t0), Step::Space { space: "space-2".into(), view: true, since: None, pixels: (64, 40) });
        s.captured("space-2", true, t0, Duration::from_millis(5), Captured::Frame(2));
        s.set_plan(&Plan { view: Some(Shot { space: "space-2".into(), pixels: (128, 80) }), ..Default::default() }, t0);
        assert_eq!(s.next(t0), Step::Space { space: "space-2".into(), view: true, since: None, pixels: (128, 80) });
    }

    /// A compositor over a socket pair per connection: hello, ListSpaces,
    /// space captures (a new serial while `drawing`, else unchanged),
    /// DestroySpace; it can stay silent for one call and lose a space.
    #[derive(Default)]
    struct Fake {
        spaces: Mutex<Vec<String>>,
        drawing: std::sync::atomic::AtomicBool,
        silent_once: std::sync::atomic::AtomicBool,
        serial: AtomicU64,
        ops: Mutex<Vec<String>>,
        hellos: AtomicU64,
    }

    impl Fake {
        fn info(&self, id: &str) -> SpaceInfo {
            SpaceInfo { id: SpaceId::new(id).unwrap(), session_id: SessionId::new("s1").unwrap(), output: "space".into(), width: 64, height: 40, wayland_display: "/run/x".into() }
        }

        fn connect(self: &Arc<Self>) -> Result<Client, ede_protocol::Error> {
            let (mine, theirs) = UnixStream::pair()?;
            let fake = self.clone();
            std::thread::spawn(move || fake.serve(theirs));
            let mut client = Client::over(mine, Token::new("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef").unwrap())?;
            client.set_call_timeout(Duration::from_millis(300));
            Ok(client)
        }

        fn captures(&self) -> usize {
            self.ops.lock().unwrap().iter().filter(|o| o.starts_with("capture")).count()
        }

        fn serve(self: Arc<Self>, mut theirs: UnixStream) {
            let reply = |s: &mut UnixStream, m: Message| write_frame(s, &serde_json::to_vec(&m).unwrap());
            while let Ok(frame) = read_frame(&mut theirs) {
                let call: Call = serde_json::from_slice(&frame).unwrap();
                let id = call.id;
                let result = match call.request {
                    Request::Hello { .. } => {
                        self.hellos.fetch_add(1, Ordering::SeqCst);
                        serde_json::to_value(Hello { version: 1, capabilities: vec![ede_protocol::Capability::System, ede_protocol::Capability::Capture], session_id: SessionId::new("exponential-shell").unwrap(), features: vec!["spaces".into()], unknown_capabilities: vec![] }).unwrap()
                    }
                    Request::ListSpaces => {
                        self.ops.lock().unwrap().push("list".into());
                        serde_json::to_value(self.spaces.lock().unwrap().iter().map(|s| self.info(s)).collect::<Vec<_>>()).unwrap()
                    }
                    Request::DestroySpace { space } => {
                        self.ops.lock().unwrap().push(format!("destroy {space}"));
                        self.spaces.lock().unwrap().retain(|s| *s != space.as_str());
                        Value::Null
                    }
                    Request::Capture { target: CaptureTarget::Space { space }, since } => {
                        self.ops.lock().unwrap().push(format!("capture {space}"));
                        if self.silent_once.swap(false, Ordering::SeqCst) {
                            // Never answers: the client's call times out and its connection ends.
                            std::thread::sleep(Duration::from_secs(2));
                            return;
                        }
                        if !self.spaces.lock().unwrap().iter().any(|s| *s == space.as_str()) {
                            let _ = reply(&mut theirs, Message::Error { id, error: Failure::new(ErrorCode::NotFound, "no such space") });
                            continue;
                        }
                        if self.drawing.load(Ordering::SeqCst) || since.is_none() {
                            self.serial.fetch_add(1, Ordering::SeqCst);
                        }
                        let serial = self.serial.load(Ordering::SeqCst);
                        if since == Some(serial) {
                            serde_json::to_value(CaptureInfo { width: 64, height: 40, stride: 256, format: "argb8888".into(), serial, unchanged: true, notice: None }).unwrap()
                        } else {
                            let info = CaptureInfo { width: 64, height: 40, stride: 256, format: "argb8888".into(), serial, unchanged: false, notice: None };
                            if reply(&mut theirs, Message::Reply { id, result: serde_json::to_value(info).unwrap() }).is_err() {
                                return;
                            }
                            let _ = write_frame(&mut theirs, &vec![0x80u8; 256 * 40]);
                            continue;
                        }
                    }
                    _ => Value::Null,
                };
                if reply(&mut theirs, Message::Reply { id, result }).is_err() {
                    return;
                }
            }
        }
    }

    /// The loop on its own thread over the fake; the shared state to drive it.
    fn start(fake: &Arc<Fake>) -> (Arc<Shared>, mpsc::Receiver<Out>, std::thread::JoinHandle<()>) {
        let shared = Arc::new(Shared::default());
        let (tx, rx) = mpsc::channel();
        let (fake, loop_shared) = (fake.clone(), shared.clone());
        let handle = std::thread::spawn(move || run(|| fake.connect(), &loop_shared, &tx));
        (shared, rx, handle)
    }

    fn until(rx: &mpsc::Receiver<Out>, within: Duration, what: impl Fn(&Out) -> bool) -> Option<Out> {
        let end = Instant::now() + within;
        while let Some(left) = end.checked_duration_since(Instant::now()) {
            match rx.recv_timeout(left) {
                Ok(out) if what(&out) => return Some(out),
                Ok(_) => {}
                Err(_) => return None,
            }
        }
        None
    }

    fn picture(out: &Out) -> bool {
        matches!(out, Out::Picture { view: true, .. })
    }

    /// Unchanged frames send nothing: one picture for a space nobody draws
    /// in, however often it is captured.
    #[test]
    fn unchanged_frames_send_nothing() {
        let fake = Arc::new(Fake::default());
        fake.spaces.lock().unwrap().push(SPACE.into());
        let (shared, rx, handle) = start(&fake);
        assert!(until(&rx, Duration::from_secs(2), |o| matches!(o, Out::Spaces(l) if l.len() == 1)).is_some(), "the spaces");
        shared.set_plan(Plan { view: Some(shot(SPACE)), ..Default::default() });
        let Some(Out::Picture { url, .. }) = until(&rx, Duration::from_secs(2), picture) else { panic!("the first picture") };
        assert!(url.starts_with("data:image/png;base64,"));
        assert!(until(&rx, Duration::from_millis(900), picture).is_none(), "nothing drawn: no picture");
        assert!(fake.captures() >= 3, "captured again and again ({}), at most 4 a second", fake.captures());
        assert!(fake.captures() <= 6, "{} captures in about a second", fake.captures());
        fake.drawing.store(true, Ordering::SeqCst);
        assert!(until(&rx, Duration::from_secs(1), picture).is_some(), "drawn: a picture");
        shared.stop();
        handle.join().unwrap();
    }

    /// A compositor silent for one call: the call times out, the loop
    /// reconnects and the pictures come again.
    #[test]
    fn a_silent_compositor_for_one_call_then_frames_again() {
        let fake = Arc::new(Fake::default());
        fake.spaces.lock().unwrap().push(SPACE.into());
        fake.drawing.store(true, Ordering::SeqCst);
        let (shared, rx, handle) = start(&fake);
        shared.set_plan(Plan { view: Some(shot(SPACE)), ..Default::default() });
        assert!(until(&rx, Duration::from_secs(2), picture).is_some(), "a first picture");
        fake.silent_once.store(true, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(400));
        assert!(until(&rx, Duration::from_secs(3), picture).is_some(), "pictures again after the reconnect");
        assert!(fake.hellos.load(Ordering::SeqCst) >= 2, "a second connection");
        shared.stop();
        handle.join().unwrap();
    }

    /// A space that is gone answers `not_found`: the loop says so (the shell
    /// closes the view); the next list no longer has it.
    #[test]
    fn a_gone_space_is_said() {
        let fake = Arc::new(Fake::default());
        fake.spaces.lock().unwrap().push(SPACE.into());
        let (shared, rx, handle) = start(&fake);
        assert!(until(&rx, Duration::from_secs(2), |o| matches!(o, Out::Spaces(l) if l.len() == 1)).is_some(), "the spaces");
        shared.set_plan(Plan { view: Some(shot(SPACE)), ..Default::default() });
        assert!(until(&rx, Duration::from_secs(2), picture).is_some());
        fake.spaces.lock().unwrap().clear();
        fake.drawing.store(true, Ordering::SeqCst);
        assert_eq!(until(&rx, Duration::from_secs(2), |o| matches!(o, Out::Gone { .. })), Some(Out::Gone { space: SPACE.into() }));
        assert!(until(&rx, Duration::from_secs(3), |o| matches!(o, Out::Spaces(l) if l.is_empty())).is_some(), "the next list");
        shared.stop();
        handle.join().unwrap();
    }

    /// No target, no capture (the list and the view closed, the screen
    /// locked): only the list of spaces is asked for; Close destroys the
    /// space on the loop's connection and lists again at once.
    #[test]
    fn no_target_no_capture_and_close_destroys() {
        let fake = Arc::new(Fake::default());
        fake.spaces.lock().unwrap().push(SPACE.into());
        fake.drawing.store(true, Ordering::SeqCst);
        let (shared, rx, handle) = start(&fake);
        shared.set_plan(Plan { thumbs: vec![shot(SPACE)], ..Default::default() });
        assert!(until(&rx, Duration::from_secs(2), |o| matches!(o, Out::Picture { view: false, .. })).is_some(), "the thumbnail at open");
        shared.set_plan(Plan::default());
        let before = fake.captures();
        assert!(until(&rx, Duration::from_millis(1500), |o| matches!(o, Out::Picture { .. })).is_none());
        assert_eq!(fake.captures(), before, "no capture without a target");
        shared.ask(Ask::Close(SPACE.into()));
        assert!(until(&rx, Duration::from_secs(1), |o| matches!(o, Out::Spaces(l) if l.is_empty())).is_some(), "listed again at once");
        assert!(fake.ops.lock().unwrap().contains(&format!("destroy {SPACE}")));
        shared.stop();
        handle.join().unwrap();
    }

    #[test]
    fn the_back_off_doubles_to_ten_seconds() {
        assert_eq!((backoff(0), backoff(1), backoff(2)), (Duration::from_millis(500), Duration::from_secs(1), Duration::from_secs(2)));
        assert_eq!(backoff(30), BACKOFF_LAST);
    }
}
