//! The seat's CLIPBOARD for ePilot's paste (`ede-protocol`'s `clipboard_*`
//! ops): a serial that moves on every selection change, a snapshot of
//! every type the selection offers, a selection the compositor owns itself
//! (served from memory to the focused client and to Xwayland), and whether
//! a client read it.
//!
//! Nothing here blocks the event loop: a client's (or Xwayland's) types
//! are read through pipes the loop polls, one type at a time, each with a
//! [`ede_protocol::CLIPBOARD_READ_MS`] timeout, and the call's reply is
//! deferred until the read is done ([`Ede::ipc_reply_later`]). Our own
//! bytes go out the same way. Contents are never logged.
//!
//! eDE has no wlr/ext data-control global, so no clipboard manager sees a
//! selection: only the keyboard-focused client (Wayland's rule, and the
//! XWM's `allow_selection_access`) can read a sensitive one.

use std::{
    cell::Cell,
    collections::VecDeque,
    io::{Read, Write},
    os::fd::OwnedFd,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use ede_protocol::{ClipboardItem, ClipboardSnapshot, ClipboardTaken, ClipboardText, ErrorCode, Failure, CLIPBOARD_ITEM_MAX, CLIPBOARD_READ_MS, CLIPBOARD_SECRET_HINT, CLIPBOARD_TEXT_MIMES, CLIPBOARD_TOTAL_MAX};
use smithay::{
    reexports::{
        calloop::{
            generic::Generic,
            timer::{TimeoutAction, Timer},
            Interest, Mode, PostAction, RegistrationToken,
        },
        rustix::{
            fs::{fcntl_setfl, OFlags},
            pipe::{pipe_with, PipeFlags},
        },
    },
    wayland::selection::data_device::{clear_data_device_selection, request_data_device_client_selection, set_data_device_selection},
};

use crate::state::Ede;

/// A whole snapshot gives up after this long, whatever is left unread.
const SNAPSHOT_MS: u64 = 3000;
/// Our bytes to a reader that stopped reading are dropped after this long.
const WRITE_MS: u64 = 10_000;
/// X11's selection plumbing, never data.
const META_TARGETS: [&str; 7] = ["TARGETS", "MULTIPLE", "TIMESTAMP", "SAVE_TARGETS", "DELETE", "INSERT_SELECTION", "INSERT_PROPERTY"];

/// The selection's user data: who serves a compositor-side selection.
#[derive(Clone, Debug)]
pub enum Selection {
    /// An X11 client owns the clipboard; the XWM reads it.
    X11,
    /// Ours ([`Ede::clipboard_set`]).
    Ours(Arc<Owned>),
}

/// A selection the compositor owns: its serial and its bytes.
pub struct Owned {
    pub serial: u64,
    pub items: Vec<ClipboardItem>,
    pub sensitive: bool,
}

impl std::fmt::Debug for Owned {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Owned(serial {}, {} types, sensitive {})", self.serial, self.items.len(), self.sensitive)
    }
}

impl Owned {
    fn item(&self, mime: &str) -> Option<&ClipboardItem> {
        self.items.iter().find(|item| item.mime == mime)
    }
}

/// Who holds the selection now.
#[derive(Clone, Debug, Default)]
pub enum Owner {
    #[default]
    Empty,
    /// A Wayland client's source, offering these types.
    Client(Vec<String>),
    /// An X11 client's, through the XWM.
    X11(Vec<String>),
    Ours(Arc<Owned>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Want {
    Snapshot,
    Text,
}

#[derive(Debug, Default)]
pub struct Clipboard {
    /// Moves on every selection change.
    pub serial: u64,
    pub owner: Owner,
    /// A client read a type (not the hint) of our selection at `serial`.
    taken: bool,
    jobs: Vec<Job>,
    next_job: u64,
}

impl Clipboard {
    /// The selection changed hands.
    pub fn changed(&mut self, owner: Owner) -> u64 {
        self.serial += 1;
        self.taken = false;
        self.owner = owner;
        self.serial
    }

    /// A client asked for `mime` of our selection `owned`.
    pub fn served(&mut self, owned: &Owned, mime: &str) {
        if owned.serial == self.serial && mime != CLIPBOARD_SECRET_HINT {
            self.taken = true;
        }
    }

    pub fn taken(&self, serial: u64) -> ClipboardTaken {
        ClipboardTaken { serial: self.serial, taken: serial == self.serial && self.taken }
    }
}

/// The types a snapshot reads (or the one text type a text read takes),
/// in the source's order.
pub fn wanted(offered: &[String], want: Want) -> Vec<String> {
    match want {
        Want::Text => CLIPBOARD_TEXT_MIMES.iter().find(|mime| offered.iter().any(|o| o == *mime)).map(|mime| vec![mime.to_string()]).unwrap_or_default(),
        Want::Snapshot => {
            let mut out: Vec<String> = Vec::new();
            for mime in offered {
                if !META_TARGETS.contains(&mime.as_str()) && !out.contains(mime) {
                    out.push(mime.clone());
                }
            }
            out
        }
    }
}

/// The items a `clipboard_set` asked for, checked: unique types, the hint
/// added when sensitive, the total in bounds.
pub fn owned_items(mut items: Vec<ClipboardItem>, sensitive: bool) -> Result<Vec<ClipboardItem>, Failure> {
    let mut seen: Vec<&str> = Vec::new();
    for item in &items {
        if item.mime.is_empty() || seen.contains(&item.mime.as_str()) {
            return Err(Failure::new(ErrorCode::Invalid, "every clipboard type needs a mime, once"));
        }
        seen.push(&item.mime);
    }
    if items.iter().map(|item| item.data.len()).sum::<usize>() > CLIPBOARD_TOTAL_MAX {
        return Err(Failure::new(ErrorCode::Invalid, "the clipboard holds 32 MiB at most"));
    }
    if sensitive && !items.is_empty() && !items.iter().any(|item| item.mime == CLIPBOARD_SECRET_HINT) {
        items.push(ClipboardItem { mime: CLIPBOARD_SECRET_HINT.into(), data: b"secret".to_vec() });
    }
    Ok(items)
}

/// The text of a read, `None` without a text type.
fn text_of(items: &[ClipboardItem]) -> Option<String> {
    CLIPBOARD_TEXT_MIMES.iter().find_map(|mime| items.iter().find(|item| item.mime == *mime)).map(|item| String::from_utf8_lossy(&item.data).into_owned())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    Client,
    X11,
}

#[derive(Debug)]
struct Job {
    id: u64,
    conn: u64,
    call_id: u64,
    want: Want,
    from: Source,
    serial: u64,
    mimes: VecDeque<String>,
    items: Vec<ClipboardItem>,
    total: usize,
    deadline: Instant,
    /// Bumped per type: a stale pipe or timer callback sees another one.
    attempt: u64,
    reading: Option<Reading>,
}

#[derive(Debug)]
struct Reading {
    mime: String,
    buf: Vec<u8>,
    pipe: Option<RegistrationToken>,
    timer: Option<RegistrationToken>,
}

enum Step {
    More,
    Done,
    Skip,
}

impl Ede {
    /// Owns the selection with `items` (empty = clears it): its serial.
    pub fn clipboard_set(&mut self, items: Vec<ClipboardItem>, sensitive: bool) -> Result<u64, Failure> {
        let items = owned_items(items, sensitive)?;
        if items.is_empty() {
            let serial = self.clipboard.changed(Owner::Empty);
            clear_data_device_selection(&self.display_handle, &self.seat);
            self.xwm_selection(None);
            return Ok(serial);
        }
        let mimes: Vec<String> = items.iter().map(|item| item.mime.clone()).collect();
        let owned = Arc::new(Owned { serial: self.clipboard.serial + 1, items, sensitive });
        let serial = self.clipboard.changed(Owner::Ours(owned.clone()));
        set_data_device_selection(&self.display_handle, &self.seat, mimes.clone(), Selection::Ours(owned));
        self.xwm_selection(Some(mimes));
        Ok(serial)
    }

    pub fn clipboard_taken(&self, serial: u64) -> ClipboardTaken {
        self.clipboard.taken(serial)
    }

    /// Xwayland's CLIPBOARD follows ours (`None` = we cleared it).
    #[allow(unused_variables)]
    pub(crate) fn xwm_selection(&mut self, mimes: Option<Vec<String>>) {
        #[cfg(feature = "xwayland")]
        if let Some(xwm) = self.xwm.as_mut() {
            if let Err(e) = xwm.new_selection(smithay::wayland::selection::SelectionTarget::Clipboard, mimes) {
                tracing::warn!("clipboard to X11: {e:?}");
            }
            // smithay's `new_selection` leaves the SetSelectionOwner in the
            // connection's buffer until the next X event: an X client
            // pasting right away would find no owner. An empty restack is
            // the public call that flushes.
            let _ = xwm.update_stacking_order_upwards(std::iter::empty::<&smithay::xwayland::X11Surface>());
        }
    }

    /// A client (Wayland or X11) reads `mime` of our selection into `fd`.
    pub(crate) fn serve_owned(&mut self, owned: &Arc<Owned>, mime: &str, fd: OwnedFd) {
        let Some(index) = owned.items.iter().position(|item| item.mime == mime) else { return };
        self.clipboard.served(owned, mime);
        let _ = fcntl_setfl(&fd, OFlags::WRONLY | OFlags::NONBLOCK);
        let file = std::fs::File::from(fd);
        let owned = owned.clone();
        let mut at = 0usize;
        if !write_some(&file, &owned.items[index].data, &mut at) {
            return;
        }
        let done = Rc::new(Cell::new(false));
        let flag = done.clone();
        let token = self.loop_handle.insert_source(Generic::new(file, Interest::WRITE, Mode::Level), move |_, file, _| {
            let file: &std::fs::File = file;
            if write_some(file, &owned.items[index].data, &mut at) {
                Ok(PostAction::Continue)
            } else {
                flag.set(true);
                Ok(PostAction::Remove)
            }
        });
        let Ok(token) = token else { return };
        let _ = self.loop_handle.insert_source(Timer::from_duration(Duration::from_millis(WRITE_MS)), move |_, _, state| {
            if !done.get() {
                state.loop_handle.remove(token);
            }
            TimeoutAction::Drop
        });
    }

    /// `clipboard_snapshot` / `clipboard_text`: the reply now (`Some`),
    /// or later once the source's pipes are read (`None`).
    pub(crate) fn clipboard_read(&mut self, conn: u64, call_id: u64, want: Want) -> Option<serde_json::Value> {
        let serial = self.clipboard.serial;
        let (from, offered) = match self.clipboard.owner.clone() {
            Owner::Empty => return Some(reply(want, serial, Vec::new())),
            Owner::Ours(owned) => {
                let items = wanted(&owned.items.iter().map(|item| item.mime.clone()).collect::<Vec<_>>(), want)
                    .iter()
                    .filter_map(|mime| owned.item(mime).cloned())
                    .collect();
                return Some(reply(want, serial, items));
            }
            Owner::Client(mimes) => (Source::Client, mimes),
            Owner::X11(mimes) => (Source::X11, mimes),
        };
        let mimes: VecDeque<String> = wanted(&offered, want).into();
        if mimes.is_empty() {
            return Some(reply(want, serial, Vec::new()));
        }
        self.clipboard.next_job += 1;
        let id = self.clipboard.next_job;
        self.clipboard.jobs.push(Job {
            id,
            conn,
            call_id,
            want,
            from,
            serial,
            mimes,
            items: Vec::new(),
            total: 0,
            deadline: Instant::now() + Duration::from_millis(SNAPSHOT_MS),
            attempt: 0,
            reading: None,
        });
        self.read_next(id);
        None
    }

    /// Starts the job's next type, or finishes it.
    fn read_next(&mut self, id: u64) {
        loop {
            let Some(job) = self.clipboard.jobs.iter_mut().find(|j| j.id == id) else { return };
            job.reading = None;
            job.attempt += 1;
            let attempt = job.attempt;
            let from = job.from;
            let next = if Instant::now() < job.deadline && job.total < CLIPBOARD_TOTAL_MAX { job.mimes.pop_front() } else { None };
            let Some(mime) = next else { return self.finish_job(id) };
            let Ok((read, write)) = pipe_with(PipeFlags::CLOEXEC) else { return self.finish_job(id) };
            let _ = fcntl_setfl(&read, OFlags::NONBLOCK);
            if !self.ask_source(from, &mime, write) {
                continue;
            }
            let file = std::fs::File::from(read);
            let pipe = self.loop_handle.insert_source(Generic::new(file, Interest::READ, Mode::Level), move |_, file, state| {
                let file: &std::fs::File = file;
                Ok(state.read_some(id, attempt, file))
            });
            let Ok(pipe) = pipe else { continue };
            let timer = self.loop_handle.insert_source(Timer::from_duration(Duration::from_millis(CLIPBOARD_READ_MS)), move |_, _, state| {
                state.read_timed_out(id, attempt);
                TimeoutAction::Drop
            });
            let Some(job) = self.clipboard.jobs.iter_mut().find(|j| j.id == id) else { return };
            job.reading = Some(Reading { mime, buf: Vec::new(), pipe: Some(pipe), timer: timer.ok() });
            return;
        }
    }

    /// Hands `fd` to whoever owns the selection; `false` = it cannot serve.
    fn ask_source(&mut self, from: Source, mime: &str, fd: OwnedFd) -> bool {
        match from {
            Source::Client => match request_data_device_client_selection(&self.seat, mime.to_string(), fd) {
                Ok(()) => true,
                Err(smithay::wayland::selection::data_device::SelectionRequestError::NoSelection) => {
                    // The source died without a new selection.
                    if matches!(self.clipboard.owner, Owner::Client(_)) {
                        self.clipboard.changed(Owner::Empty);
                    }
                    false
                }
                Err(_) => false,
            },
            #[cfg(feature = "xwayland")]
            Source::X11 => self.xwm.as_mut().is_some_and(|xwm| xwm.send_selection(smithay::wayland::selection::SelectionTarget::Clipboard, mime.to_string(), fd).is_ok()),
            #[cfg(not(feature = "xwayland"))]
            Source::X11 => false,
        }
    }

    fn read_some(&mut self, id: u64, attempt: u64, mut file: &std::fs::File) -> PostAction {
        let step = {
            let Some(job) = self.clipboard.jobs.iter_mut().find(|j| j.id == id && j.attempt == attempt) else { return PostAction::Remove };
            let Some(reading) = job.reading.as_mut() else { return PostAction::Remove };
            let mut chunk = [0u8; 65536];
            loop {
                match file.read(&mut chunk) {
                    Ok(0) => break Step::Done,
                    Ok(n) => {
                        reading.buf.extend_from_slice(&chunk[..n]);
                        if reading.buf.len() > CLIPBOARD_ITEM_MAX || job.total + reading.buf.len() > CLIPBOARD_TOTAL_MAX {
                            break Step::Skip;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break Step::More,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => break Step::Done,
                }
            }
        };
        match step {
            Step::More => PostAction::Continue,
            Step::Done | Step::Skip => {
                let Some(job) = self.clipboard.jobs.iter_mut().find(|j| j.id == id) else { return PostAction::Remove };
                let reading = job.reading.take().expect("reading");
                if let Some(timer) = reading.timer {
                    self.loop_handle.remove(timer);
                }
                let Some(job) = self.clipboard.jobs.iter_mut().find(|j| j.id == id) else { return PostAction::Remove };
                if matches!(step, Step::Done) {
                    job.total += reading.buf.len();
                    job.items.push(ClipboardItem { mime: reading.mime, data: reading.buf });
                } else if reading.buf.len() <= CLIPBOARD_ITEM_MAX {
                    // Past the total: nothing more fits.
                    job.mimes.clear();
                }
                self.read_next(id);
                PostAction::Remove
            }
        }
    }

    fn read_timed_out(&mut self, id: u64, attempt: u64) {
        let Some(job) = self.clipboard.jobs.iter_mut().find(|j| j.id == id && j.attempt == attempt) else { return };
        let Some(reading) = job.reading.take() else { return };
        if let Some(pipe) = reading.pipe {
            self.loop_handle.remove(pipe);
        }
        self.read_next(id);
    }

    fn finish_job(&mut self, id: u64) {
        let Some(index) = self.clipboard.jobs.iter().position(|j| j.id == id) else { return };
        let job = self.clipboard.jobs.remove(index);
        let value = reply(job.want, job.serial, job.items);
        self.ipc_reply_later(job.conn, job.call_id, Ok(value));
    }
}

fn reply(want: Want, serial: u64, items: Vec<ClipboardItem>) -> serde_json::Value {
    match want {
        Want::Snapshot => serde_json::to_value(ClipboardSnapshot { serial, items }).unwrap(),
        Want::Text => serde_json::to_value(ClipboardText { serial, text: text_of(&items) }).unwrap(),
    }
}

/// Writes what the pipe takes; `true` = more to write later.
fn write_some(mut file: &std::fs::File, data: &[u8], at: &mut usize) -> bool {
    while *at < data.len() {
        match file.write(&data[*at..]) {
            Ok(0) => return false,
            Ok(n) => *at += n,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return true,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return false,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(mime: &str, data: &[u8]) -> ClipboardItem {
        ClipboardItem { mime: mime.into(), data: data.to_vec() }
    }

    #[test]
    fn the_serial_moves_and_taken_counts_only_the_current_selection() {
        let mut clipboard = Clipboard::default();
        let owned = Owned { serial: 1, items: vec![item("text/plain", b"a")], sensitive: false };
        assert_eq!(clipboard.changed(Owner::Empty), 1);
        assert!(!clipboard.taken(1).taken);
        clipboard.served(&owned, CLIPBOARD_SECRET_HINT);
        assert!(!clipboard.taken(1).taken, "the hint is not the text");
        clipboard.served(&owned, "text/plain");
        assert_eq!(clipboard.taken(1), ClipboardTaken { serial: 1, taken: true });
        assert_eq!(clipboard.taken(0), ClipboardTaken { serial: 1, taken: false });
        clipboard.changed(Owner::Client(vec!["text/plain".into()]));
        assert_eq!(clipboard.taken(1), ClipboardTaken { serial: 2, taken: false });
        // A read of an older selection of ours does not count.
        clipboard.served(&owned, "text/plain");
        assert!(!clipboard.taken(2).taken);
    }

    #[test]
    fn a_snapshot_skips_x11_plumbing_and_text_takes_the_best_type() {
        let offered: Vec<String> = ["TARGETS", "UTF8_STRING", "text/plain", "TIMESTAMP", "text/html", "text/plain"].iter().map(|s| s.to_string()).collect();
        assert_eq!(wanted(&offered, Want::Snapshot), ["UTF8_STRING", "text/plain", "text/html"]);
        assert_eq!(wanted(&offered, Want::Text), ["text/plain"]);
        assert_eq!(wanted(&["image/png".to_string()], Want::Text), Vec::<String>::new());
        assert_eq!(text_of(&[item("STRING", b"x"), item("text/plain;charset=utf-8", "ü".as_bytes())]).as_deref(), Some("ü"));
    }

    #[test]
    fn a_sensitive_set_carries_the_hint_and_bad_sets_are_refused() {
        let items = owned_items(vec![item("text/plain", b"pw")], true).unwrap();
        assert_eq!(items[1].mime, CLIPBOARD_SECRET_HINT);
        assert_eq!(items[1].data, b"secret");
        assert_eq!(owned_items(vec![item("text/plain", b"pw")], false).unwrap().len(), 1);
        assert!(owned_items(vec![], true).unwrap().is_empty(), "clearing adds no hint");
        assert!(owned_items(vec![item("a", b""), item("a", b"")], false).is_err());
        assert!(owned_items(vec![item("", b"")], false).is_err());
    }
}
