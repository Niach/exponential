//! X11's CLIPBOARD selection for `paste_text`, `copy_selection` and
//! `read_clipboard` (`tools.rs`).
//!
//! An X selection lives as long as its owner serves it, so one KEEPER
//! thread per process owns it for us: its own connection and an input-only
//! window, serving `SelectionRequest`s for the text a paste offers and,
//! afterwards, for the person's restored clipboard (until another program
//! takes the selection). It also counts XFixes owner changes: the
//! clipboard's change count. A call's own connection reads the clipboard
//! (`ConvertSelection` into its own scratch window) and asks the keeper to
//! own or drop it (a command plus a ClientMessage that wakes its loop).
//!
//! SENSITIVE: the text is served only to the focused window's X client
//! (its resource-id base, the client that will take the paste) and the
//! offer carries `x-kde-passwordManagerHint: secret` (Klipper, GPaste,
//! KeePassXC's convention). Every other requestor (Diodon, any clipboard
//! manager or history) sees the targets but gets a refused conversion, so
//! nothing records it; `SAVE_TARGETS` (the CLIPBOARD_MANAGER handover) is
//! never sent. Nothing here logs clipboard bytes.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use x11rb::connection::{Connection, RequestConnection as _};
use x11rb::protocol::xfixes::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{self, Atom, ConnectionExt as _, Window};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::NONE;

use super::{reply, x, X11};
use crate::backend::{Clipboard, Pasted, SavedClipboard};

/// One saved format past this is skipped (an image of a whole screen).
const FORMAT_MAX: usize = 8 << 20;
/// The whole saved clipboard stops here.
const SAVED_MAX: usize = 32 << 20;
const TARGETS_MAX: usize = 64;
/// One `ConvertSelection` round trip (a hung owner answers nothing).
const CONVERT_TIMEOUT: Duration = Duration::from_millis(600);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);
/// The KDE convention's mime (KeePassXC, Klipper, GPaste).
const HINT: &str = "x-kde-passwordManagerHint";

/// Targets that are about the selection, never its content.
const META: [&str; 7] = ["TARGETS", "MULTIPLE", "TIMESTAMP", "SAVE_TARGETS", "DELETE", "INSERT_SELECTION", "INSERT_PROPERTY"];

struct Format {
    target: Atom,
    kind: Atom,
    format: u8,
    data: Vec<u8>,
    /// The sensitive hint itself (served to anyone, never counts as taken).
    hint: bool,
}

/// What the keeper serves.
struct Offer {
    formats: Vec<Format>,
    sensitive: bool,
    /// The focused window's client (resource-id base): the paste target.
    client: Option<u32>,
    /// The target client converted a content format.
    taken: bool,
    /// Another program took the selection.
    lost: bool,
}

enum Command {
    Own(Offer, mpsc::Sender<Result<(), String>>),
    Disown(mpsc::Sender<Result<(), String>>),
}

#[derive(Default)]
struct Shared {
    changes: AtomicU64,
    offer: Mutex<Option<Offer>>,
    commands: Mutex<Vec<Command>>,
}

struct Keeper {
    window: Window,
    wake: Atom,
    shared: Arc<Shared>,
    alive: Arc<AtomicBool>,
}

static KEEPER: Mutex<Option<Arc<Keeper>>> = Mutex::new(None);

#[derive(Clone, Copy)]
struct Names {
    clipboard: Atom,
    targets: Atom,
    incr: Atom,
    utf8: Atom,
    text: Atom,
    plain_utf8: Atom,
    plain: Atom,
    hint: Atom,
    scratch: Atom,
}

fn intern(conn: &RustConnection, name: &str) -> Result<Atom, String> {
    Ok(reply("intern atom", conn.intern_atom(false, name.as_bytes()))?.atom)
}

impl Names {
    fn get(conn: &RustConnection) -> Result<Names, String> {
        Ok(Names {
            clipboard: intern(conn, "CLIPBOARD")?,
            targets: intern(conn, "TARGETS")?,
            incr: intern(conn, "INCR")?,
            utf8: intern(conn, "UTF8_STRING")?,
            text: intern(conn, "TEXT")?,
            plain_utf8: intern(conn, "text/plain;charset=utf-8")?,
            plain: intern(conn, "text/plain")?,
            hint: intern(conn, HINT)?,
            scratch: intern(conn, "_EXP_EPILOT_CLIPBOARD")?,
        })
    }
}

/// The process's keeper, started on first use (and again when its X
/// connection died).
fn keeper() -> Result<Arc<Keeper>, String> {
    let mut slot = KEEPER.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(keeper) = slot.as_ref().filter(|keeper| keeper.alive.load(Ordering::SeqCst)) {
        return Ok(keeper.clone());
    }
    let (ready, started) = mpsc::channel();
    let shared = Arc::new(Shared::default());
    let alive = Arc::new(AtomicBool::new(true));
    {
        let (shared, alive) = (shared.clone(), alive.clone());
        std::thread::Builder::new()
            .name("computer-use-clipboard".into())
            .spawn(move || {
                let setup = (|| -> Result<_, String> {
                    let (conn, screen) = x("X connection", x11rb::connect(None))?;
                    let root = conn.setup().roots[screen].root;
                    let names = Names::get(&conn)?;
                    let wake = intern(&conn, "_EXP_EPILOT_WAKE")?;
                    reply("xfixes", conn.xfixes_query_version(5, 0))?;
                    let window = x("window", conn.generate_id())?;
                    x(
                        "window",
                        conn.create_window(0, window, root, 0, 0, 1, 1, 0, xproto::WindowClass::INPUT_ONLY, 0, &xproto::CreateWindowAux::new()),
                    )?;
                    let mask = xfixes::SelectionEventMask::SET_SELECTION_OWNER
                        | xfixes::SelectionEventMask::SELECTION_WINDOW_DESTROY
                        | xfixes::SelectionEventMask::SELECTION_CLIENT_CLOSE;
                    x("xfixes", conn.xfixes_select_selection_input(window, names.clipboard, mask))?;
                    x("sync", conn.sync())?;
                    Ok((conn, window, names, wake))
                })();
                match setup {
                    Err(err) => {
                        alive.store(false, Ordering::SeqCst);
                        let _ = ready.send(Err(err));
                    }
                    Ok((conn, window, names, wake)) => {
                        let _ = ready.send(Ok((window, wake)));
                        serve(&conn, window, names, wake, &shared);
                        alive.store(false, Ordering::SeqCst);
                        log::debug!("[computer] the clipboard keeper's X connection closed");
                    }
                }
            })
            .map_err(|err| format!("clipboard thread: {err}"))?;
    }
    let (window, wake) = started.recv_timeout(COMMAND_TIMEOUT).map_err(|_| "The clipboard thread did not start.".to_string())??;
    let keeper = Arc::new(Keeper { window, wake, shared, alive });
    *slot = Some(keeper.clone());
    Ok(keeper)
}

/// The keeper's loop: until its connection dies.
fn serve(conn: &RustConnection, window: Window, names: Names, wake: Atom, shared: &Shared) {
    let mask = conn.setup().resource_id_mask;
    let max_bytes = conn.maximum_request_bytes().saturating_sub(64);
    loop {
        let event = match conn.wait_for_event() {
            Ok(event) => event,
            Err(_) => return,
        };
        match event {
            Event::XfixesSelectionNotify(_) => {
                shared.changes.fetch_add(1, Ordering::SeqCst);
            }
            Event::ClientMessage(message) if message.type_ == wake => {
                let commands = std::mem::take(&mut *shared.commands.lock().unwrap_or_else(|p| p.into_inner()));
                for command in commands {
                    match command {
                        Command::Own(offer, done) => {
                            let owned = (|| {
                                x("own", conn.set_selection_owner(window, names.clipboard, x11rb::CURRENT_TIME))?;
                                let owner = reply("owner", conn.get_selection_owner(names.clipboard))?.owner;
                                if owner == window {
                                    Ok(())
                                } else {
                                    Err("Another program kept the clipboard.".to_string())
                                }
                            })();
                            if owned.is_ok() {
                                *shared.offer.lock().unwrap_or_else(|p| p.into_inner()) = Some(offer);
                            }
                            let _ = done.send(owned);
                        }
                        Command::Disown(done) => {
                            let result = (|| {
                                let owner = reply("owner", conn.get_selection_owner(names.clipboard))?.owner;
                                if owner == window {
                                    x("disown", conn.set_selection_owner(NONE, names.clipboard, x11rb::CURRENT_TIME))?;
                                    x("flush", conn.flush())?;
                                }
                                Ok(())
                            })();
                            *shared.offer.lock().unwrap_or_else(|p| p.into_inner()) = None;
                            let _ = done.send(result);
                        }
                    }
                }
            }
            Event::SelectionClear(clear) if clear.selection == names.clipboard => {
                if let Some(offer) = shared.offer.lock().unwrap_or_else(|p| p.into_inner()).as_mut() {
                    offer.lost = true;
                    offer.formats.clear();
                }
            }
            Event::SelectionRequest(request) => {
                let property = if request.property == NONE { request.target } else { request.property };
                let mut answered = false;
                if request.selection == names.clipboard {
                    let mut offer = shared.offer.lock().unwrap_or_else(|p| p.into_inner());
                    if let Some(offer) = offer.as_mut().filter(|offer| !offer.lost) {
                        if request.target == names.targets {
                            let mut list = vec![names.targets];
                            list.extend(offer.formats.iter().map(|format| format.target));
                            answered = conn
                                .change_property32(xproto::PropMode::REPLACE, request.requestor, property, xproto::AtomEnum::ATOM, &list)
                                .is_ok();
                        } else if let Some(format) = offer.formats.iter().find(|format| format.target == request.target) {
                            let target_client = offer.client == Some(request.requestor & !mask);
                            if (!offer.sensitive || target_client || format.hint) && format.data.len() <= max_bytes {
                                let unit = usize::from(format.format.max(8) / 8);
                                let length = u32::try_from(format.data.len() / unit).unwrap_or(u32::MAX);
                                answered = conn
                                    .change_property(
                                        xproto::PropMode::REPLACE,
                                        request.requestor,
                                        property,
                                        format.kind,
                                        format.format,
                                        length,
                                        &format.data,
                                    )
                                    .is_ok();
                                if answered && target_client && !format.hint {
                                    offer.taken = true;
                                }
                            }
                        }
                    }
                }
                let notify = xproto::SelectionNotifyEvent {
                    response_type: xproto::SELECTION_NOTIFY_EVENT,
                    sequence: 0,
                    time: request.time,
                    requestor: request.requestor,
                    selection: request.selection,
                    target: request.target,
                    property: if answered { property } else { NONE },
                };
                let _ = conn.send_event(false, request.requestor, xproto::EventMask::NO_EVENT, notify);
                let _ = conn.flush();
            }
            // A requestor that vanished mid-answer: its BadWindow is moot.
            _ => {}
        }
    }
}

/// The X11 clipboard, on one call's connection.
pub(super) struct X11Clipboard<'a> {
    x11: &'a X11,
    keeper: Arc<Keeper>,
    names: Names,
    /// The scratch window conversions land on.
    window: Window,
}

impl<'a> X11Clipboard<'a> {
    pub(super) fn open(x11: &'a X11) -> Result<X11Clipboard<'a>, String> {
        let keeper = keeper()?;
        let names = Names::get(&x11.conn)?;
        let window = x("window", x11.conn.generate_id())?;
        x(
            "window",
            x11.conn.create_window(0, window, x11.root, 0, 0, 1, 1, 0, xproto::WindowClass::INPUT_ONLY, 0, &xproto::CreateWindowAux::new()),
        )?;
        Ok(X11Clipboard { x11, keeper, names, window })
    }

    fn command(&self, make: impl FnOnce(mpsc::Sender<Result<(), String>>) -> Command) -> Result<(), String> {
        let (done, answer) = mpsc::channel();
        self.keeper.shared.commands.lock().unwrap_or_else(|p| p.into_inner()).push(make(done));
        let wake = xproto::ClientMessageEvent::new(32, self.keeper.window, self.keeper.wake, [0u32; 5]);
        x("wake", self.x11.conn.send_event(false, self.keeper.window, xproto::EventMask::NO_EVENT, wake))?;
        self.x11.flush()?;
        answer.recv_timeout(COMMAND_TIMEOUT).map_err(|_| "The clipboard thread did not answer.".to_string())?
    }

    /// The clipboard's `target` (type, format, bytes); `None` = refused,
    /// empty or too large (INCR).
    fn convert(&self, target: Atom) -> Result<Option<(Atom, u8, Vec<u8>)>, String> {
        let conn = &self.x11.conn;
        x("convert", conn.convert_selection(self.window, self.names.clipboard, target, self.names.scratch, x11rb::CURRENT_TIME))?;
        self.x11.flush()?;
        let until = Instant::now() + CONVERT_TIMEOUT;
        let property = loop {
            match x("event", conn.poll_for_event())? {
                Some(Event::SelectionNotify(notify)) if notify.requestor == self.window && notify.target == target => break notify.property,
                Some(_) => continue,
                None if Instant::now() < until => std::thread::sleep(Duration::from_millis(2)),
                None => return Ok(None),
            }
        };
        if property == NONE {
            return Ok(None);
        }
        let value = reply("clipboard property", conn.get_property(true, self.window, property, xproto::AtomEnum::ANY, 0, (FORMAT_MAX / 4 + 1) as u32))?;
        if value.type_ == self.names.incr || value.bytes_after > 0 {
            // Too large for one property: skipped, and the owner's INCR
            // transfer ends with our scratch window.
            let _ = conn.delete_property(self.window, property);
            return Ok(None);
        }
        Ok(Some((value.type_, value.format, value.value)))
    }

    fn owner(&self) -> Result<Window, String> {
        Ok(reply("owner", self.x11.conn.get_selection_owner(self.names.clipboard))?.owner)
    }

    fn atom_name(&self, atom: Atom) -> Result<String, String> {
        Ok(String::from_utf8_lossy(&reply("atom name", self.x11.conn.get_atom_name(atom))?.name).into_owned())
    }
}

impl Drop for X11Clipboard<'_> {
    fn drop(&mut self) {
        let _ = self.x11.conn.destroy_window(self.window);
        let _ = self.x11.conn.flush();
    }
}

/// `STRING` is Latin-1: the text when it fits, else not offered.
fn latin1(text: &str) -> Option<Vec<u8>> {
    text.chars().map(|ch| u8::try_from(u32::from(ch)).ok()).collect()
}

impl Clipboard for X11Clipboard<'_> {
    fn change_count(&self) -> Result<u64, String> {
        // A round trip to the keeper's server first: an owner change the
        // server already made has reached the keeper's queue.
        x("sync", self.x11.conn.sync())?;
        std::thread::sleep(Duration::from_millis(5));
        Ok(self.keeper.shared.changes.load(Ordering::SeqCst))
    }

    fn save(&self) -> Result<SavedClipboard, String> {
        let mut saved = SavedClipboard::default();
        if self.owner()? == NONE {
            return Ok(saved);
        }
        let Some((_, 32, targets)) = self.convert(self.names.targets)? else { return Ok(saved) };
        let mut total = 0;
        for chunk in targets.chunks_exact(4).take(TARGETS_MAX) {
            let target = u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            let name = self.atom_name(target)?;
            if META.contains(&name.as_str()) || saved.formats.iter().any(|(saved, _)| saved.split('\t').next() == Some(name.as_str())) {
                continue;
            }
            let Some((kind, format, data)) = self.convert(target)? else { continue };
            if data.len() > FORMAT_MAX || total + data.len() > SAVED_MAX {
                continue;
            }
            total += data.len();
            saved.formats.push((format!("{name}\t{}\t{format}", self.atom_name(kind)?), data));
        }
        Ok(saved)
    }

    fn set_text(&self, text: &str, sensitive: bool) -> Result<(), String> {
        let names = self.names;
        let utf8 = text.as_bytes().to_vec();
        let mut formats: Vec<Format> = [names.utf8, names.plain_utf8, names.text]
            .into_iter()
            .map(|target| Format { target, kind: names.utf8, format: 8, data: utf8.clone(), hint: false })
            .collect();
        formats.push(Format { target: names.plain, kind: names.plain, format: 8, data: utf8, hint: false });
        if let Some(latin1) = latin1(text) {
            formats.push(Format { target: xproto::AtomEnum::STRING.into(), kind: xproto::AtomEnum::STRING.into(), format: 8, data: latin1, hint: false });
        }
        if sensitive {
            formats.push(Format { target: names.hint, kind: names.hint, format: 8, data: b"secret".to_vec(), hint: true });
        }
        let mask = self.x11.conn.setup().resource_id_mask;
        let client = self.x11.active_window().map(|window| window & !mask);
        if sensitive && client.is_none() {
            return Err("No focused window to paste into.".to_string());
        }
        let offer = Offer { formats, sensitive, client, taken: false, lost: false };
        self.command(|done| Command::Own(offer, done))
    }

    fn wait_pasted(&self, timeout: Duration) -> Pasted {
        let until = Instant::now() + timeout;
        loop {
            {
                let offer = self.keeper.shared.offer.lock().unwrap_or_else(|p| p.into_inner());
                match offer.as_ref() {
                    Some(offer) if offer.taken => return Pasted::Taken,
                    Some(offer) if !offer.lost => {}
                    _ => return Pasted::Missed,
                }
            }
            if Instant::now() >= until {
                return Pasted::Missed;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn restore(&self, saved: SavedClipboard) -> Result<(), String> {
        if saved.formats.is_empty() {
            return self.command(Command::Disown);
        }
        let mut formats = Vec::with_capacity(saved.formats.len());
        for (name, data) in saved.formats {
            let mut parts = name.split('\t');
            let (Some(target), Some(kind), Some(format)) = (parts.next(), parts.next(), parts.next()) else { continue };
            let format = format.parse::<u8>().unwrap_or(8);
            let target = intern(&self.x11.conn, target)?;
            let kind = intern(&self.x11.conn, kind)?;
            formats.push(Format { target, kind, format, data, hint: target == self.names.hint });
        }
        // The person's own clipboard: served to anyone, as before.
        let offer = Offer { formats, sensitive: false, client: None, taken: false, lost: false };
        self.command(|done| Command::Own(offer, done))
    }

    fn text(&self) -> Result<Option<String>, String> {
        if self.owner()? == NONE {
            return Ok(None);
        }
        if let Some((_, _, data)) = self.convert(self.names.utf8)? {
            return Ok(Some(String::from_utf8_lossy(&data).into_owned()));
        }
        Ok(self.convert(xproto::AtomEnum::STRING.into())?.map(|(_, _, data)| data.iter().map(|&byte| char::from(byte)).collect()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_is_latin1_or_absent() {
        assert_eq!(latin1("abc é"), Some(b"abc \xe9".to_vec()));
        assert_eq!(latin1("snow ☃"), None);
    }
}
