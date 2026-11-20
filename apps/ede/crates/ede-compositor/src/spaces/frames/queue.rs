//! #EDE-162: a space's captures wait their turn. A capture is a pass over
//! the space's whole output on the thread that serves the person's frames,
//! and an agent can ask for them back to back. So a capture of a space that
//! changed is answered after the person's frames of that turn of the loop
//! (each backend calls [`Ede::spaces_after_frames`] once it drew), at most
//! one pass per space per [`FLOOR`] and all spaces together at most
//! [`BUDGET`] of that thread's time per [`WINDOW`]. Callers that ask for
//! the same space in one turn share one pass. A capture that finds nothing
//! new since the caller's last one answers at once.

use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

use ede_protocol::{CaptureInfo, ErrorCode, Failure};
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};

use super::stats::thread_cpu;
use crate::{
    desk::DeskId,
    spaces::scope::{Caller, Reply},
    state::Ede,
};

/// At most one pass per space this often.
pub const FLOOR: Duration = Duration::from_millis(50);
/// All spaces' passes together, per [`WINDOW`], in thread CPU.
pub const BUDGET: Duration = Duration::from_millis(100);
pub const WINDOW: Duration = Duration::from_secs(1);

#[derive(Default)]
pub(crate) struct Queue {
    waiting: Vec<Waiting>,
    last_pass: HashMap<DeskId, Instant>,
    /// The passes of the last [`WINDOW`]: when, and their thread CPU.
    passes: VecDeque<(Instant, Duration)>,
    /// A timer is armed for then.
    wake_at: Option<Instant>,
    /// Passes drawn per space (tests read it).
    drawn: HashMap<DeskId, u64>,
    /// [`FLOOR`] unless a test sets another.
    floor: Option<Duration>,
}

struct Waiting {
    caller: Caller,
    desk: DeskId,
    since: Option<u64>,
    /// It had to wait at least once (counted once in the stats).
    waited: bool,
}

/// When the next pass of a space may be drawn: `None` = now. `last` is
/// that space's last pass, `passes` every space's of the last window.
pub fn next_pass(now: Instant, last: Option<Instant>, passes: &VecDeque<(Instant, Duration)>, floor: Duration) -> Option<Instant> {
    let floor = last.map(|at| at + floor).filter(|at| *at > now);
    let recent = || passes.iter().filter(|(at, _)| now.saturating_duration_since(*at) < WINDOW);
    let spent: Duration = recent().map(|(_, cpu)| *cpu).sum();
    let budget = (spent >= BUDGET)
        .then(|| {
            // When enough of the oldest passes have left the window.
            let excess = spent - BUDGET;
            let mut gone = Duration::ZERO;
            recent().find_map(|(at, cpu)| {
                gone += *cpu;
                (gone > excess).then(|| *at + WINDOW)
            })
        })
        .flatten();
    floor.max(budget)
}

fn unchanged(serial: u64) -> serde_json::Value {
    serde_json::to_value(CaptureInfo { width: 0, height: 0, stride: 0, format: "argb8888".into(), serial, unchanged: true }).unwrap()
}

impl Ede {
    /// A capture of space `desk` for `caller`; `since` = the serial of the
    /// caller's last one. Nothing changed since: answered now, no pixels.
    /// Else the reply comes after the person's frames, when the space's turn
    /// has come ([`Self::spaces_after_frames`]).
    pub(crate) fn space_capture(&mut self, desk: DeskId, since: Option<u64>, caller: Caller) -> Reply {
        let serial = self.space_serial(desk).ok_or_else(|| Failure::new(ErrorCode::NotFound, "no such space"))?;
        if since == Some(serial) {
            return Ok(Some((unchanged(serial), None)));
        }
        self.spaces.frames.queue.waiting.push(Waiting { caller, desk, since, waited: false });
        Ok(None)
    }

    /// The waiting captures whose turn it is. Every backend calls this once
    /// per turn of the loop, after the person's outputs were drawn.
    pub fn spaces_after_frames(&mut self) {
        if self.spaces.frames.queue.waiting.is_empty() {
            return;
        }
        let mut desks: Vec<DeskId> = Vec::new();
        for w in &self.spaces.frames.queue.waiting {
            if !desks.contains(&w.desk) {
                desks.push(w.desk);
            }
        }
        let mut wake: Option<Instant> = None;
        for desk in desks {
            let (jobs, rest): (Vec<Waiting>, Vec<Waiting>) = std::mem::take(&mut self.spaces.frames.queue.waiting).into_iter().partition(|w| w.desk == desk);
            self.spaces.frames.queue.waiting = rest;
            let Some(serial) = self.space_serial(desk) else {
                for job in jobs {
                    self.ipc_reply_later(job.caller.conn, job.caller.call, Err(Failure::new(ErrorCode::NotFound, "no such space")));
                }
                continue;
            };
            let mut draw = Vec::new();
            for job in jobs {
                if job.since == Some(serial) {
                    self.ipc_reply_later_with(job.caller.conn, job.caller.call, Ok((unchanged(serial), None)));
                } else {
                    draw.push(job);
                }
            }
            if draw.is_empty() {
                continue;
            }
            let now = Instant::now();
            let queue = &mut self.spaces.frames.queue;
            queue.passes.retain(|(at, _)| now.saturating_duration_since(*at) < WINDOW);
            if let Some(at) = next_pass(now, queue.last_pass.get(&desk).copied(), &queue.passes, queue.floor.unwrap_or(FLOOR)) {
                wake = Some(wake.map_or(at, |w| w.min(at)));
                for job in draw.iter_mut().filter(|j| !j.waited) {
                    job.waited = true;
                    self.spaces.frames.stats.waited();
                }
                // Back in front: they asked first.
                draw.append(&mut self.spaces.frames.queue.waiting);
                self.spaces.frames.queue.waiting = draw;
                continue;
            }
            let cpu = thread_cpu();
            let frame = self.capture_space(desk);
            let (cpu, wall) = (thread_cpu().saturating_sub(cpu), now.elapsed());
            self.spaces.frames.stats.pass(cpu, wall);
            let queue = &mut self.spaces.frames.queue;
            queue.passes.push_back((now, cpu));
            queue.last_pass.insert(desk, Instant::now());
            *queue.drawn.entry(desk).or_default() += 1;
            let serial = self.spaces.get(desk).map_or(serial, |s| s.serial);
            match frame {
                Ok(frame) => {
                    let info = CaptureInfo { width: frame.width, height: frame.height, stride: frame.width * 4, format: "argb8888".into(), serial, unchanged: false };
                    let info = serde_json::to_value(info).unwrap();
                    let bytes: Vec<u8> = frame.pixels.iter().flat_map(|p| p.to_le_bytes()).collect();
                    for job in draw {
                        self.ipc_reply_later_with(job.caller.conn, job.caller.call, Ok((info.clone(), Some(bytes.clone()))));
                    }
                }
                Err(e) => {
                    for job in draw {
                        self.ipc_reply_later(job.caller.conn, job.caller.call, Err(Failure::new(ErrorCode::Internal, &e)));
                    }
                }
            }
        }
        // Forget spaces that are gone.
        let live: Vec<DeskId> = self.spaces.iter().map(|s| s.desk.id).collect();
        self.spaces.frames.queue.last_pass.retain(|desk, _| live.contains(desk));
        if let Some(at) = wake {
            self.spaces_wake_at(at);
        }
    }

    /// The loop turns again at `at` (the run callback then calls
    /// [`Self::spaces_after_frames`]); one timer at a time, the earliest.
    fn spaces_wake_at(&mut self, at: Instant) {
        if self.spaces.frames.queue.wake_at.is_some_and(|armed| armed <= at) {
            return;
        }
        let armed = self.loop_handle.insert_source(Timer::from_deadline(at), move |_, _, state| {
            if state.spaces.frames.queue.wake_at == Some(at) {
                state.spaces.frames.queue.wake_at = None;
            }
            TimeoutAction::Drop
        });
        if armed.is_ok() {
            self.spaces.frames.queue.wake_at = Some(at);
        }
    }

    /// A floor other than [`FLOOR`] between two passes of a space (tests:
    /// a capture that must wait).
    pub fn set_space_capture_floor(&mut self, floor: Duration) {
        self.spaces.frames.queue.floor = Some(floor);
    }

    /// How many capture passes space `desk` has drawn.
    pub fn space_passes(&self, desk: DeskId) -> u64 {
        self.spaces.frames.queue.drawn.get(&desk).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_space_is_drawn_at_most_once_per_floor() {
        let now = Instant::now();
        let none = VecDeque::new();
        assert_eq!(next_pass(now, None, &none, FLOOR), None, "never drawn: now");
        assert_eq!(next_pass(now, Some(now - ms(60)), &none, FLOOR), None, "the floor has passed");
        assert_eq!(next_pass(now, Some(now - ms(20)), &none, FLOOR), Some(now + ms(30)), "30 ms of the floor are left");
    }

    #[test]
    fn all_spaces_together_stay_within_the_budget_of_a_window() {
        let now = Instant::now();
        // 90 ms spent in the last second: one more may go.
        let passes: VecDeque<_> = [(now - ms(900), ms(40)), (now - ms(500), ms(50))].into();
        assert_eq!(next_pass(now, None, &passes, FLOOR), None);
        // 120 ms spent: the next waits until the oldest passes that make up
        // the 20 ms over have left the window (the 40 ms one, at +100 ms).
        let passes: VecDeque<_> = [(now - ms(900), ms(40)), (now - ms(500), ms(50)), (now - ms(100), ms(30))].into();
        assert_eq!(next_pass(now, None, &passes, FLOOR), Some(now + ms(100)));
        // Exactly at the budget counts as spent.
        let passes: VecDeque<_> = [(now - ms(300), ms(100))].into();
        assert_eq!(next_pass(now, None, &passes, FLOOR), Some(now + ms(700)));
        // Passes older than the window do not count.
        let passes: VecDeque<_> = [(now - ms(1500), ms(500))].into();
        assert_eq!(next_pass(now, None, &passes, FLOOR), None);
        // The later of the floor and the budget (here the budget frees up at +20 ms).
        let passes: VecDeque<_> = [(now - ms(980), ms(120))].into();
        assert_eq!(next_pass(now, Some(now - ms(10)), &passes, FLOOR), Some(now + ms(40)), "the floor is later");
        assert_eq!(next_pass(now, Some(now - ms(45)), &passes, FLOOR), Some(now + ms(20)), "the budget is later");
    }
}
