/*
 * Copyright 2022 - 2025 Zed Industries, Inc.
 * License: Apache-2.0
 * See LICENSE-APACHE for complete license terms
 *
 * Adapted from the x11 submodule of the arboard project https://github.com/1Password/arboard
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 *
 * Copyright 2022 The Arboard contributors
 *
 * The project to which this file belongs is licensed under either of
 * the Apache 2.0 or the MIT license at the licensee's choice. The terms
 * and conditions of the chosen license apply to this file.
*/

// More info about using the clipboard on X11:
// https://tronche.com/gui/x/icccm/sec-2.html#s-2.6
// https://freedesktop.org/wiki/ClipboardManager/

use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{HashMap, hash_map::Entry},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    thread_local,
    time::{Duration, Instant},
};

use parking_lot::{Condvar, Mutex, MutexGuard, RwLock};
use x11rb::{
    COPY_DEPTH_FROM_PARENT, COPY_FROM_PARENT, NONE,
    connection::{Connection, RequestConnection as _},
    protocol::{
        Event,
        xproto::{
            Atom, AtomEnum, ChangeWindowAttributesAux, ConnectionExt as _, CreateWindowAux,
            EventMask, PropMode, Property, PropertyNotifyEvent, SELECTION_NOTIFY_EVENT,
            SelectionNotifyEvent, SelectionRequestEvent, Time, Window, WindowClass,
        },
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

use gpui::{ClipboardItem, Image, ImageFormat, hash};
use strum::IntoEnumIterator;

type Result<T, E = Error> = std::result::Result<T, E>;

static CLIPBOARD: Mutex<Option<GlobalClipboard>> = parking_lot::const_mutex(None);

x11rb::atom_manager! {
    pub Atoms: AtomCookies {
        CLIPBOARD,
        PRIMARY,
        SECONDARY,

        CLIPBOARD_MANAGER,
        SAVE_TARGETS,
        TARGETS,
        ATOM,
        INCR,

        UTF8_STRING,
        UTF8_MIME_0: b"text/plain;charset=utf-8",
        UTF8_MIME_1: b"text/plain;charset=UTF-8",
        // Text in ISO Latin-1 encoding
        // See: https://tronche.com/gui/x/icccm/sec-2.html#s-2.6.2
        STRING,
        // Text in unknown encoding
        // See: https://tronche.com/gui/x/icccm/sec-2.html#s-2.6.2
        TEXT,
        TEXT_MIME_UNKNOWN: b"text/plain",

        // HTML: b"text/html",
        // URI_LIST: b"text/uri-list",

        PNG__MIME: ImageFormat::mime_type(ImageFormat::Png ).as_bytes(),
        JPEG_MIME: ImageFormat::mime_type(ImageFormat::Jpeg).as_bytes(),
        WEBP_MIME: ImageFormat::mime_type(ImageFormat::Webp).as_bytes(),
        GIF__MIME: ImageFormat::mime_type(ImageFormat::Gif ).as_bytes(),
        SVG__MIME: ImageFormat::mime_type(ImageFormat::Svg ).as_bytes(),
        BMP__MIME: ImageFormat::mime_type(ImageFormat::Bmp ).as_bytes(),
        TIFF_MIME: ImageFormat::mime_type(ImageFormat::Tiff).as_bytes(),
        ICO__MIME: ImageFormat::mime_type(ImageFormat::Ico ).as_bytes(),
        PNM__MIME: ImageFormat::mime_type(ImageFormat::Pnm ).as_bytes(),
        // This is just some random name for the property on our window, into which
        // the clipboard owner writes the data we requested.
        ARBOARD_CLIPBOARD,
    }
}

thread_local! {
    static ATOM_NAME_CACHE: RefCell<HashMap<Atom, &'static str>> = Default::default();
}

// Some clipboard items, like images, may take a very long time to produce a
// `SelectionNotify`. Multiple seconds long.
const LONG_TIMEOUT_DUR: Duration = Duration::from_millis(4000);
// Exponential patch (#EXP-1946): the wait for an INCR transfer's next chunk. Upstream's 10 ms
// dropped a large image whenever the owner (or this process, under load) took longer per chunk.
const SHORT_TIMEOUT_DUR: Duration = Duration::from_millis(1000);

#[derive(Debug, PartialEq, Eq)]
enum ManagerHandoverState {
    Idle,
    InProgress,
    Finished,
}

struct GlobalClipboard {
    inner: Arc<Inner>,

    /// Join handle to the thread which serves selection requests.
    server_handle: JoinHandle<()>,
}

struct XContext {
    conn: RustConnection,
    win_id: u32,
}

struct Inner {
    /// The X display every clipboard connection uses.
    display: String,
    /// The context for the thread which serves clipboard read
    /// requests coming to us.
    server: XContext,
    atoms: Atoms,

    clipboard: Selection,
    primary: Selection,
    secondary: Selection,

    handover_state: Mutex<ManagerHandoverState>,
    handover_cv: Condvar,

    serve_stopped: AtomicBool,

    /// Exponential patch (#EXP-1946): conversions too large for one request, sent in INCR
    /// chunks (ICCCM 2.7.2). Only the server thread touches it; the handover reads its length.
    incr_sends: Mutex<Vec<IncrSend>>,
}

/// Exponential patch (#EXP-1946): one INCR transfer in progress. A copied image (a PNG copy of a
/// photo) easily exceeds the server's maximum request size, which a single `ChangeProperty`
/// cannot carry.
struct IncrSend {
    requestor: Window,
    property: Atom,
    target: Atom,
    bytes: Vec<u8>,
    /// Bytes sent so far; the zero-length chunk that ends the transfer follows `bytes.len()`.
    offset: usize,
    /// The last time the requestor took a chunk: a requestor that stops reading is dropped.
    active: Instant,
}

/// Exponential patch (#EXP-1946): an INCR requestor silent this long is given up on.
const INCR_STALE: Duration = Duration::from_secs(5);
/// Exponential patch (#EXP-1946): the largest INCR chunk.
const INCR_CHUNK_MAX: usize = 1 << 20;
/// Exponential patch (#EXP-1946): how long the exit handover waits for the clipboard manager while
/// an INCR transfer to it still makes progress.
const HANDOVER_INCR_MAX: Duration = Duration::from_secs(3);

impl XContext {
    fn new(display: &str) -> Result<Self> {
        // create a new connection to an X11 server
        let (conn, screen_num): (RustConnection, _) = RustConnection::connect(Some(display))
            .map_err(|_| {
                Error::unknown("X11 server connection timed out because it was unreachable")
            })?;
        let screen = conn
            .setup()
            .roots
            .get(screen_num)
            .ok_or(Error::unknown("no screen found"))?;
        let win_id = conn.generate_id().map_err(into_unknown)?;

        let event_mask =
            // Just in case that some program reports SelectionNotify events
            // with XCB_EVENT_MASK_PROPERTY_CHANGE mask.
            EventMask::PROPERTY_CHANGE |
            // To receive DestroyNotify event and stop the message loop.
            EventMask::STRUCTURE_NOTIFY;
        // create the window
        conn.create_window(
            // copy as much as possible from the parent, because no other specific input is needed
            COPY_DEPTH_FROM_PARENT,
            win_id,
            screen.root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::COPY_FROM_PARENT,
            COPY_FROM_PARENT,
            // don't subscribe to any special events because we are requesting everything we need ourselves
            &CreateWindowAux::new().event_mask(event_mask),
        )
        .map_err(into_unknown)?;
        conn.flush().map_err(into_unknown)?;

        Ok(Self { conn, win_id })
    }
}

#[derive(Default)]
struct Selection {
    data: RwLock<Option<Vec<ClipboardData>>>,
    /// Mutex around nothing to use with the below condvar.
    mutex: Mutex<()>,
    /// A condvar that is notified when the contents of this clipboard are changed.
    ///
    /// This is associated with `Self::mutex`.
    data_changed: Condvar,
}

#[derive(Debug, Clone)]
struct ClipboardData {
    bytes: Vec<u8>,

    /// The atom representing the format in which the data is encoded.
    format: Atom,
}

enum ReadSelNotifyResult {
    GotData(ClipboardData),
    IncrStarted,
    EventNotRecognized,
}

impl Inner {
    fn new(display: &str) -> Result<Self> {
        let server = XContext::new(display)?;
        let atoms = Atoms::new(&server.conn)
            .map_err(into_unknown)?
            .reply()
            .map_err(into_unknown)?;

        Ok(Self {
            display: display.to_owned(),
            server,
            atoms,
            clipboard: Selection::default(),
            primary: Selection::default(),
            secondary: Selection::default(),
            handover_state: Mutex::new(ManagerHandoverState::Idle),
            handover_cv: Condvar::new(),
            serve_stopped: AtomicBool::new(false),
            incr_sends: Mutex::new(Vec::new()),
        })
    }

    fn write(
        &self,
        data: Vec<ClipboardData>,
        selection: ClipboardKind,
        wait: WaitConfig,
    ) -> Result<()> {
        if self.serve_stopped.load(Ordering::Relaxed) {
            return Err(Error::unknown(
                "The clipboard handler thread seems to have stopped. Logging messages may reveal the cause. (See the `log` crate.)",
            ));
        }

        let server_win = self.server.win_id;

        // ICCCM version 2, section 2.6.1.3 states that we should re-assert ownership whenever data
        // changes.
        self.server
            .conn
            .set_selection_owner(server_win, self.atom_of(selection), Time::CURRENT_TIME)
            .map_err(|_| Error::ClipboardOccupied)?;

        self.server.conn.flush().map_err(into_unknown)?;

        // Just setting the data, and the `serve_requests` will take care of the rest.
        let selection = self.selection_of(selection);
        let mut data_guard = selection.data.write();
        *data_guard = Some(data);

        // Lock the mutex to both ensure that no wakers of `data_changed` can wake us between
        // dropping the `data_guard` and calling `wait[_for]` and that we don't we wake other
        // threads in that position.
        let mut guard = selection.mutex.lock();

        // Notify any existing waiting threads that we have changed the data in the selection.
        // It is important that the mutex is locked to prevent this notification getting lost.
        selection.data_changed.notify_all();

        match wait {
            WaitConfig::None => {}
            WaitConfig::Forever => {
                drop(data_guard);
                selection.data_changed.wait(&mut guard);
            }
            WaitConfig::Until(deadline) => {
                drop(data_guard);
                selection.data_changed.wait_until(&mut guard, deadline);
            }
        }

        Ok(())
    }

    /// `formats` must be a slice of atoms, where each atom represents a target format.
    /// The first format from `formats`, which the clipboard owner supports will be the
    /// format of the return value.
    fn read(&self, formats: &[Atom], selection: ClipboardKind) -> Result<ClipboardData> {
        // if we are the current owner, we can get the current clipboard ourselves
        if self.is_owner(selection)? {
            let data = self.selection_of(selection).data.read();
            if let Some(data_list) = &*data {
                for data in data_list {
                    for format in formats {
                        if *format == data.format {
                            return Ok(data.clone());
                        }
                    }
                }
            }
            return Err(Error::ContentNotAvailable);
        }
        let reader = XContext::new(&self.display)?;

        let highest_precedence_format =
            match self.read_single(&reader, selection, self.atoms.TARGETS) {
                Err(err) => {
                    log::trace!("Clipboard TARGETS query failed with {err:?}");
                    None
                }
                Ok(ClipboardData { bytes, format }) => {
                    if format == self.atoms.ATOM {
                        let available_formats = Self::parse_formats(&bytes);
                        formats
                            .iter()
                            .find(|format| available_formats.contains(format))
                    } else {
                        log::trace!(
                            "Unexpected clipboard TARGETS format {}",
                            self.atom_name(format)
                        );
                        None
                    }
                }
            };

        if let Some(&format) = highest_precedence_format {
            let data = self.read_single(&reader, selection, format)?;
            if !formats.contains(&data.format) {
                // This shouldn't happen since the format is from the TARGETS list.
                log::trace!(
                    "Conversion to {} responded with {} which is not supported",
                    self.atom_name(format),
                    self.atom_name(data.format),
                );
                return Err(Error::ConversionFailure);
            }
            return Ok(data);
        }

        log::trace!("Falling back on attempting to convert clipboard to each format.");
        for format in formats {
            match self.read_single(&reader, selection, *format) {
                Ok(data) => {
                    if formats.contains(&data.format) {
                        return Ok(data);
                    } else {
                        log::trace!(
                            "Conversion to {} responded with {} which is not supported",
                            self.atom_name(*format),
                            self.atom_name(data.format),
                        );
                        continue;
                    }
                }
                Err(Error::ContentNotAvailable) => {
                    continue;
                }
                Err(e) => {
                    log::trace!("Conversion to {} failed: {}", self.atom_name(*format), e);
                    return Err(e);
                }
            }
        }
        log::trace!("All conversions to supported formats failed.");
        Err(Error::ContentNotAvailable)
    }

    fn parse_formats(bytes: &[u8]) -> Vec<Atom> {
        bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect()
    }

    fn read_single(
        &self,
        reader: &XContext,
        selection: ClipboardKind,
        target_format: Atom,
    ) -> Result<ClipboardData> {
        // Delete the property so that we can detect (using property notify)
        // when the selection owner receives our request.
        reader
            .conn
            .delete_property(reader.win_id, self.atoms.ARBOARD_CLIPBOARD)
            .map_err(into_unknown)?;

        // request to convert the clipboard selection to our data type(s)
        reader
            .conn
            .convert_selection(
                reader.win_id,
                self.atom_of(selection),
                target_format,
                self.atoms.ARBOARD_CLIPBOARD,
                Time::CURRENT_TIME,
            )
            .map_err(into_unknown)?;
        reader.conn.sync().map_err(into_unknown)?;

        log::trace!("Finished `convert_selection`");

        let mut incr_data: Vec<u8> = Vec::new();
        let mut using_incr = false;

        let mut timeout_end = Instant::now() + LONG_TIMEOUT_DUR;

        while Instant::now() < timeout_end {
            let event = reader.conn.poll_for_event().map_err(into_unknown)?;
            let event = match event {
                Some(e) => e,
                None => {
                    std::thread::sleep(Duration::from_millis(1));
                    continue;
                }
            };
            match event {
                // The first response after requesting a selection.
                Event::SelectionNotify(event) => {
                    log::trace!("Read SelectionNotify");
                    let result = self.handle_read_selection_notify(
                        reader,
                        target_format,
                        &mut using_incr,
                        &mut incr_data,
                        event,
                    )?;
                    match result {
                        ReadSelNotifyResult::GotData(data) => return Ok(data),
                        ReadSelNotifyResult::IncrStarted => {
                            // This means we received an indication that an the
                            // data is going to be sent INCRementally. Let's
                            // reset our timeout.
                            timeout_end += SHORT_TIMEOUT_DUR;
                        }
                        ReadSelNotifyResult::EventNotRecognized => (),
                    }
                }
                // If the previous SelectionNotify event specified that the data
                // will be sent in INCR segments, each segment is transferred in
                // a PropertyNotify event.
                Event::PropertyNotify(event) => {
                    let result = self.handle_read_property_notify(
                        reader,
                        target_format,
                        using_incr,
                        &mut incr_data,
                        &mut timeout_end,
                        event,
                    )?;
                    if result {
                        return Ok(ClipboardData {
                            bytes: incr_data,
                            format: target_format,
                        });
                    }
                }
                _ => log::trace!(
                    "An unexpected event arrived while reading the clipboard: {:?}",
                    event
                ),
            }
        }
        log::info!("Time-out hit while reading the clipboard.");
        Err(Error::ContentNotAvailable)
    }

    fn atom_of(&self, selection: ClipboardKind) -> Atom {
        match selection {
            ClipboardKind::Clipboard => self.atoms.CLIPBOARD,
            ClipboardKind::Primary => self.atoms.PRIMARY,
            ClipboardKind::Secondary => self.atoms.SECONDARY,
        }
    }

    fn selection_of(&self, selection: ClipboardKind) -> &Selection {
        match selection {
            ClipboardKind::Clipboard => &self.clipboard,
            ClipboardKind::Primary => &self.primary,
            ClipboardKind::Secondary => &self.secondary,
        }
    }

    fn kind_of(&self, atom: Atom) -> Option<ClipboardKind> {
        match atom {
            a if a == self.atoms.CLIPBOARD => Some(ClipboardKind::Clipboard),
            a if a == self.atoms.PRIMARY => Some(ClipboardKind::Primary),
            a if a == self.atoms.SECONDARY => Some(ClipboardKind::Secondary),
            _ => None,
        }
    }

    fn is_owner(&self, selection: ClipboardKind) -> Result<bool> {
        let current = self
            .server
            .conn
            .get_selection_owner(self.atom_of(selection))
            .map_err(into_unknown)?
            .reply()
            .map_err(into_unknown)?
            .owner;

        Ok(current == self.server.win_id)
    }

    fn query_atom_name(&self, atom: x11rb::protocol::xproto::Atom) -> Result<String> {
        String::from_utf8(
            self.server
                .conn
                .get_atom_name(atom)
                .map_err(into_unknown)?
                .reply()
                .map_err(into_unknown)?
                .name,
        )
        .map_err(into_unknown)
    }

    fn atom_name(&self, atom: x11rb::protocol::xproto::Atom) -> &'static str {
        ATOM_NAME_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            match cache.entry(atom) {
                Entry::Occupied(entry) => *entry.get(),
                Entry::Vacant(entry) => {
                    let s = self
                        .query_atom_name(atom)
                        .map(|s| Box::leak(s.into_boxed_str()) as &str)
                        .unwrap_or("FAILED-TO-GET-THE-ATOM-NAME");
                    entry.insert(s);
                    s
                }
            }
        })
    }

    fn handle_read_selection_notify(
        &self,
        reader: &XContext,
        target_format: u32,
        using_incr: &mut bool,
        incr_data: &mut Vec<u8>,
        event: SelectionNotifyEvent,
    ) -> Result<ReadSelNotifyResult> {
        // The property being set to NONE means that the `convert_selection`
        // failed.

        // According to: https://tronche.com/gui/x/icccm/sec-2.html#s-2.4
        // the target must be set to the same as what we requested.
        if event.property == NONE || event.target != target_format {
            return Err(Error::ContentNotAvailable);
        }
        if self.kind_of(event.selection).is_none() {
            log::info!(
                "Received a SelectionNotify for a selection other than CLIPBOARD, PRIMARY or SECONDARY. This is unexpected."
            );
            return Ok(ReadSelNotifyResult::EventNotRecognized);
        }
        if *using_incr {
            log::warn!("Received a SelectionNotify while already expecting INCR segments.");
            return Ok(ReadSelNotifyResult::EventNotRecognized);
        }
        // Accept any property type. The property type will typically match the format type except
        // when it is `TARGETS` in which case it is `ATOM`. `ANY` is provided to handle the case
        // where the clipboard is not convertible to the requested format. In this case
        // `reply.type_` will have format information, but `bytes` will only be non-empty if `ANY`
        // is provided.
        let property_type = AtomEnum::ANY;
        // request the selection
        let mut reply = reader
            .conn
            .get_property(
                true,
                event.requestor,
                event.property,
                property_type,
                0,
                u32::MAX / 4,
            )
            .map_err(into_unknown)?
            .reply()
            .map_err(into_unknown)?;

        // we found something
        if reply.type_ == self.atoms.INCR {
            // Note that we call the get_property again because we are
            // indicating that we are ready to receive the data by deleting the
            // property, however deleting only works if the type matches the
            // property type. But the type didn't match in the previous call.
            reply = reader
                .conn
                .get_property(
                    true,
                    event.requestor,
                    event.property,
                    self.atoms.INCR,
                    0,
                    u32::MAX / 4,
                )
                .map_err(into_unknown)?
                .reply()
                .map_err(into_unknown)?;
            log::trace!("Receiving INCR segments");
            *using_incr = true;
            if reply.value_len == 4 {
                let min_data_len = reply
                    .value32()
                    .and_then(|mut vals| vals.next())
                    .unwrap_or(0);
                incr_data.reserve(min_data_len as usize);
            }
            Ok(ReadSelNotifyResult::IncrStarted)
        } else {
            Ok(ReadSelNotifyResult::GotData(ClipboardData {
                bytes: reply.value,
                format: reply.type_,
            }))
        }
    }

    /// Returns Ok(true) when the incr_data is ready
    fn handle_read_property_notify(
        &self,
        reader: &XContext,
        target_format: u32,
        using_incr: bool,
        incr_data: &mut Vec<u8>,
        timeout_end: &mut Instant,
        event: PropertyNotifyEvent,
    ) -> Result<bool> {
        if event.atom != self.atoms.ARBOARD_CLIPBOARD || event.state != Property::NEW_VALUE {
            return Ok(false);
        }
        if !using_incr {
            // This must mean the selection owner received our request, and is
            // now preparing the data
            return Ok(false);
        }
        let reply = reader
            .conn
            .get_property(
                true,
                event.window,
                event.atom,
                if target_format == self.atoms.TARGETS {
                    self.atoms.ATOM
                } else {
                    target_format
                },
                0,
                u32::MAX / 4,
            )
            .map_err(into_unknown)?
            .reply()
            .map_err(into_unknown)?;

        // log::trace!("Received segment. value_len {}", reply.value_len,);
        if reply.value_len == 0 {
            // This indicates that all the data has been sent.
            return Ok(true);
        }
        incr_data.extend(reply.value);

        // Let's reset our timeout, since we received a valid chunk.
        *timeout_end = Instant::now() + SHORT_TIMEOUT_DUR;

        // Not yet complete
        Ok(false)
    }

    fn handle_selection_request(&self, event: SelectionRequestEvent) -> Result<()> {
        let selection = match self.kind_of(event.selection) {
            Some(kind) => kind,
            None => {
                log::warn!(
                    "Received a selection request to a selection other than the CLIPBOARD, PRIMARY or SECONDARY. This is unexpected."
                );
                return Ok(());
            }
        };

        let success;
        // we are asked for a list of supported conversion targets
        if event.target == self.atoms.TARGETS {
            log::trace!(
                "Handling TARGETS, dst property is {}",
                self.atom_name(event.property)
            );
            let mut targets = Vec::with_capacity(10);
            targets.push(self.atoms.TARGETS);
            targets.push(self.atoms.SAVE_TARGETS);
            let data = self.selection_of(selection).data.read();
            if let Some(data_list) = &*data {
                for data in data_list {
                    targets.push(data.format);
                    if data.format == self.atoms.UTF8_STRING {
                        // When we are storing a UTF8 string,
                        // add all equivalent formats to the supported targets
                        targets.push(self.atoms.UTF8_MIME_0);
                        targets.push(self.atoms.UTF8_MIME_1);
                    }
                }
            }
            self.server
                .conn
                .change_property32(
                    PropMode::REPLACE,
                    event.requestor,
                    event.property,
                    // TODO: change to `AtomEnum::ATOM`
                    self.atoms.ATOM,
                    &targets,
                )
                .map_err(into_unknown)?;
            self.server.conn.flush().map_err(into_unknown)?;
            success = true;
        } else {
            log::trace!("Handling request for (probably) the clipboard contents.");
            let data = self.selection_of(selection).data.read();
            if let Some(data_list) = &*data {
                success = match data_list.iter().find(|d| d.format == event.target) {
                    // Exponential patch (#EXP-1946): too large for one request, sent in chunks.
                    Some(data) if data.bytes.len() > self.max_property_bytes() => {
                        self.start_incr_send(&event, data.bytes.clone())?;
                        true
                    }
                    Some(data) => {
                        self.server
                            .conn
                            .change_property8(
                                PropMode::REPLACE,
                                event.requestor,
                                event.property,
                                event.target,
                                &data.bytes,
                            )
                            .map_err(into_unknown)?;
                        self.server.conn.flush().map_err(into_unknown)?;
                        true
                    }
                    None => false,
                };
            } else {
                // This must mean that we lost ownership of the data
                // since the other side requested the selection.
                // Let's respond with the property set to none.
                success = false;
            }
        }
        // on failure we notify the requester of it
        let property = if success {
            event.property
        } else {
            AtomEnum::NONE.into()
        };
        // tell the requestor that we finished sending data
        self.server
            .conn
            .send_event(
                false,
                event.requestor,
                EventMask::NO_EVENT,
                SelectionNotifyEvent {
                    response_type: SELECTION_NOTIFY_EVENT,
                    sequence: event.sequence,
                    time: event.time,
                    requestor: event.requestor,
                    selection: event.selection,
                    target: event.target,
                    property,
                },
            )
            .map_err(into_unknown)?;

        self.server.conn.flush().map_err(into_unknown)
    }

    /// Exponential patch (#EXP-1946): the most property bytes one `ChangeProperty` request carries.
    fn max_property_bytes(&self) -> usize {
        // The request's own header is 24 bytes; keep a margin.
        self.server
            .conn
            .maximum_request_bytes()
            .saturating_sub(64)
            .max(4096)
    }

    /// Exponential patch (#EXP-1946): answers a conversion with an `INCR` property (ICCCM 2.7.2).
    /// The chunks follow as the requestor deletes the property ([`Self::handle_incr_property_notify`]).
    fn start_incr_send(&self, event: &SelectionRequestEvent, bytes: Vec<u8>) -> Result<()> {
        log::trace!(
            "Sending {} bytes of {} in INCR chunks",
            bytes.len(),
            self.atom_name(event.target)
        );
        // Before the INCR property, so the requestor's first delete is not missed.
        self.server
            .conn
            .change_window_attributes(
                event.requestor,
                &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
            )
            .map_err(into_unknown)?;
        self.server
            .conn
            .change_property32(
                PropMode::REPLACE,
                event.requestor,
                event.property,
                self.atoms.INCR,
                &[u32::try_from(bytes.len()).unwrap_or(u32::MAX)],
            )
            .map_err(into_unknown)?;
        let mut sends = self.incr_sends.lock();
        sends.retain(|send| !(send.requestor == event.requestor && send.property == event.property));
        sends.push(IncrSend {
            requestor: event.requestor,
            property: event.property,
            target: event.target,
            bytes,
            offset: 0,
            active: Instant::now(),
        });
        Ok(())
    }

    /// Exponential patch (#EXP-1946): sends an INCR transfer's next chunk once the requestor has
    /// deleted (read) the previous one. Returns whether a transfer finished.
    fn handle_incr_property_notify(&self, event: PropertyNotifyEvent) -> Result<bool> {
        let mut sends = self.incr_sends.lock();
        sends.retain(|send| send.active.elapsed() < INCR_STALE);
        if event.state != Property::DELETE {
            return Ok(false);
        }
        let Some(index) = sends
            .iter()
            .position(|send| send.requestor == event.window && send.property == event.atom)
        else {
            return Ok(false);
        };
        let send = &mut sends[index];
        let chunk_len = (send.bytes.len() - send.offset).min(incr_chunk_len(self.max_property_bytes()));
        self.server
            .conn
            .change_property8(
                PropMode::REPLACE,
                send.requestor,
                send.property,
                send.target,
                &send.bytes[send.offset..send.offset + chunk_len],
            )
            .map_err(into_unknown)?;
        send.active = Instant::now();
        let finished = chunk_len == 0;
        send.offset += chunk_len;
        if finished {
            // The zero-length chunk ends the transfer; stop watching the requestor's properties.
            let send = sends.remove(index);
            self.server
                .conn
                .change_window_attributes(
                    send.requestor,
                    &ChangeWindowAttributesAux::new().event_mask(EventMask::NO_EVENT),
                )
                .map_err(into_unknown)?;
        }
        self.server.conn.flush().map_err(into_unknown)?;
        Ok(finished)
    }

    fn incr_sends_pending(&self) -> bool {
        !self.incr_sends.lock().is_empty()
    }

    fn ask_clipboard_manager_to_request_our_data(&self) -> Result<()> {
        if self.server.win_id == 0 {
            // This shouldn't really ever happen but let's just check.
            log::error!("The server's window id was 0. This is unexpected");
            return Ok(());
        }

        if !self.is_owner(ClipboardKind::Clipboard)? {
            // We are not owning the clipboard, nothing to do.
            return Ok(());
        }
        if self
            .selection_of(ClipboardKind::Clipboard)
            .data
            .read()
            .is_none()
        {
            // If we don't have any data, there's nothing to do.
            return Ok(());
        }

        // It's important that we lock the state before sending the request
        // because we don't want the request server thread to lock the state
        // after the request but before we can lock it here.
        let mut handover_state = self.handover_state.lock();

        log::trace!("Sending the data to the clipboard manager");
        self.server
            .conn
            .convert_selection(
                self.server.win_id,
                self.atoms.CLIPBOARD_MANAGER,
                self.atoms.SAVE_TARGETS,
                self.atoms.ARBOARD_CLIPBOARD,
                Time::CURRENT_TIME,
            )
            .map_err(into_unknown)?;
        self.server.conn.flush().map_err(into_unknown)?;

        *handover_state = ManagerHandoverState::InProgress;
        let max_handover_duration = Duration::from_millis(100);

        // Note that we are using a parking_lot condvar here, which doesn't wake up
        // spuriously
        let mut result = self
            .handover_cv
            .wait_for(&mut handover_state, max_handover_duration);
        // Exponential patch (#EXP-1946): an image sent in INCR chunks takes longer than 100 ms;
        // keep waiting while that transfer still runs (stale ones are dropped), up to a cap.
        let handover_start = Instant::now();
        while result.timed_out()
            && *handover_state == ManagerHandoverState::InProgress
            && self.incr_sends_pending()
            && handover_start.elapsed() < HANDOVER_INCR_MAX
        {
            result = self
                .handover_cv
                .wait_for(&mut handover_state, max_handover_duration);
        }

        if *handover_state == ManagerHandoverState::Finished {
            return Ok(());
        }
        if result.timed_out() {
            log::warn!(
                "Could not hand the clipboard contents over to the clipboard manager. The request timed out."
            );
            return Ok(());
        }

        Err(Error::unknown(
            "The handover was not finished and the condvar didn't time out, yet the condvar wait ended. This should be unreachable.",
        ))
    }
}

fn serve_requests(context: Arc<Inner>) -> Result<(), Box<dyn std::error::Error>> {
    fn handover_finished(clip: &Arc<Inner>, mut handover_state: MutexGuard<ManagerHandoverState>) {
        log::trace!("Finishing clipboard manager handover.");
        *handover_state = ManagerHandoverState::Finished;

        // Not sure if unlocking the mutex is necessary here but better safe than sorry.
        drop(handover_state);

        clip.handover_cv.notify_all();
    }

    log::trace!("Started serve requests thread.");

    let _guard = gpui_util::defer(|| {
        context.serve_stopped.store(true, Ordering::Relaxed);
    });

    let mut written = false;
    let mut notified = false;

    loop {
        match context.server.conn.wait_for_event().map_err(into_unknown)? {
            Event::DestroyNotify(_) => {
                // This window is being destroyed.
                log::trace!("Clipboard server window is being destroyed x_x");
                return Ok(());
            }
            Event::SelectionClear(event) => {
                // TODO: check if this works
                // Someone else has new content in the clipboard, so it is
                // notifying us that we should delete our data now.
                log::trace!("Somebody else owns the clipboard now");

                if let Some(selection) = context.kind_of(event.selection) {
                    let selection = context.selection_of(selection);
                    let mut data_guard = selection.data.write();
                    *data_guard = None;

                    // It is important that this mutex is locked at the time of calling
                    // `notify_all` to prevent notifications getting lost in case the sleeping
                    // thread has unlocked its `data_guard` and is just about to sleep.
                    // It is also important that the RwLock is kept write-locked for the same
                    // reason.
                    let _guard = selection.mutex.lock();
                    selection.data_changed.notify_all();
                }
            }
            Event::SelectionRequest(event) => {
                log::trace!(
                    "SelectionRequest - selection is: {}, target is {}",
                    context.atom_name(event.selection),
                    context.atom_name(event.target),
                );
                // Someone is requesting the clipboard content from us.
                context
                    .handle_selection_request(event)
                    .map_err(into_unknown)?;

                // if we are in the progress of saving to the clipboard manager
                // make sure we save that we have finished writing
                let handover_state = context.handover_state.lock();
                if *handover_state == ManagerHandoverState::InProgress {
                    // Only set written, when the actual contents were written,
                    // not just a response to what TARGETS we have.
                    if event.target != context.atoms.TARGETS {
                        log::trace!("The contents were written to the clipboard manager.");
                        written = true;
                        // if we have written and notified, make sure to notify that we are done
                        // Exponential patch (#EXP-1946): and every INCR chunk has gone out.
                        if notified && !context.incr_sends_pending() {
                            handover_finished(&context, handover_state);
                        }
                    }
                }
            }
            Event::SelectionNotify(event) => {
                // We've requested the clipboard content and this is the answer.
                // Considering that this thread is not responsible for reading
                // clipboard contents, this must come from the clipboard manager
                // signaling that the data was handed over successfully.
                if event.selection != context.atoms.CLIPBOARD_MANAGER {
                    log::error!(
                        "Received a `SelectionNotify` from a selection other than the CLIPBOARD_MANAGER. This is unexpected in this thread."
                    );
                    continue;
                }
                let handover_state = context.handover_state.lock();
                if *handover_state == ManagerHandoverState::InProgress {
                    // Note that some clipboard managers send a selection notify
                    // before even sending a request for the actual contents.
                    // (That's why we use the "notified" & "written" flags)
                    log::trace!(
                        "The clipboard manager indicated that it's done requesting the contents from us."
                    );
                    notified = true;

                    // One would think that we could also finish if the property
                    // here is set 0, because that indicates failure. However
                    // this is not the case; for example on KDE plasma 5.18, we
                    // immediately get a SelectionNotify with property set to 0,
                    // but following that, we also get a valid SelectionRequest
                    // from the clipboard manager.
                    if written && !context.incr_sends_pending() {
                        handover_finished(&context, handover_state);
                    }
                }
            }
            // Exponential patch (#EXP-1946): the requestor took an INCR chunk.
            Event::PropertyNotify(event) => {
                let finished = context
                    .handle_incr_property_notify(event)
                    .map_err(into_unknown)?;
                let handover_state = context.handover_state.lock();
                if finished
                    && *handover_state == ManagerHandoverState::InProgress
                    && written
                    && notified
                    && !context.incr_sends_pending()
                {
                    handover_finished(&context, handover_state);
                }
            }
            _event => {
                // May be useful for debugging but nothing else really.
                //log::trace!("Received unwanted event: {:?}", event);
            }
        }
    }
}

pub(crate) struct Clipboard {
    inner: Arc<Inner>,
}

impl Clipboard {
    /// Returns the process's clipboard, connecting it to `display` if it doesn't exist yet.
    pub(crate) fn new(display: &str) -> Result<Self> {
        let mut global_cb = CLIPBOARD.lock();
        if let Some(global_cb) = &*global_cb {
            return Ok(Self {
                inner: Arc::clone(&global_cb.inner),
            });
        }
        // At this point we know that the clipboard does not exist.
        let ctx = Arc::new(Inner::new(display)?);
        let join_handle = std::thread::Builder::new()
            .name("Clipboard".to_owned())
            .spawn({
                let ctx = Arc::clone(&ctx);
                move || {
                    if let Err(error) = serve_requests(ctx) {
                        log::error!("Worker thread errored with: {}", error);
                    }
                }
            })
            .unwrap();
        *global_cb = Some(GlobalClipboard {
            inner: Arc::clone(&ctx),
            server_handle: join_handle,
        });
        Ok(Self { inner: ctx })
    }

    // Exponential patch (#EXP-1946): every write goes through `set_item` now.
    #[allow(unused)]
    pub(crate) fn set_text(
        &self,
        message: Cow<'_, str>,
        selection: ClipboardKind,
        wait: WaitConfig,
    ) -> Result<()> {
        let data = vec![ClipboardData {
            bytes: message.into_owned().into_bytes(),
            format: self.inner.atoms.UTF8_STRING,
        }];
        self.inner.write(data, selection, wait)
    }

    fn image_format_atom(&self, format: ImageFormat) -> Atom {
        match format {
            ImageFormat::Png => self.inner.atoms.PNG__MIME,
            ImageFormat::Jpeg => self.inner.atoms.JPEG_MIME,
            ImageFormat::Webp => self.inner.atoms.WEBP_MIME,
            ImageFormat::Gif => self.inner.atoms.GIF__MIME,
            ImageFormat::Svg => self.inner.atoms.SVG__MIME,
            ImageFormat::Bmp => self.inner.atoms.BMP__MIME,
            ImageFormat::Tiff => self.inner.atoms.TIFF_MIME,
            ImageFormat::Ico => self.inner.atoms.ICO__MIME,
            ImageFormat::Pnm => self.inner.atoms.PNM__MIME,
        }
    }

    /// Exponential patch (#EXP-1946): offers the item's text and image (in its own format plus
    /// a PNG copy). Upstream wrote every item through `set_text`, so a copied image offered
    /// nothing; its unused `set_image` offered any format under the PNG target.
    pub(crate) fn set_item(
        &self,
        item: &ClipboardItem,
        selection: ClipboardKind,
        wait: WaitConfig,
    ) -> Result<()> {
        let data = item_data(item, self.inner.atoms.UTF8_STRING, |format| {
            self.image_format_atom(format)
        });
        self.inner.write(data, selection, wait)
    }

    pub(crate) fn get_any(&self, selection: ClipboardKind) -> Result<ClipboardItem> {
        let image_entries = ImageFormat::iter()
            .map(|format| (self.image_format_atom(format), format))
            .collect::<Vec<_>>();

        let text_format_atoms: &[Atom] = &[
            self.inner.atoms.UTF8_STRING,
            self.inner.atoms.UTF8_MIME_0,
            self.inner.atoms.UTF8_MIME_1,
            self.inner.atoms.STRING,
            self.inner.atoms.TEXT,
            self.inner.atoms.TEXT_MIME_UNKNOWN,
        ];

        // image formats first, as they are more specific, and read will return the first
        // format that the contents can be converted to
        let mut format_atoms = Vec::with_capacity(image_entries.len() + text_format_atoms.len());
        format_atoms.extend(image_entries.iter().map(|(atom, _)| *atom));
        format_atoms.extend_from_slice(text_format_atoms);

        let result = self.inner.read(&format_atoms, selection)?;

        log::trace!(
            "read clipboard as format {:?}",
            self.inner.atom_name(result.format)
        );

        for (format_atom, image_format) in image_entries {
            if result.format == format_atom {
                let bytes = result.bytes;
                let id = hash(&bytes);
                return Ok(ClipboardItem::new_image(&Image {
                    id,
                    format: image_format,
                    bytes,
                }));
            }
        }

        let text = if result.format == self.inner.atoms.STRING {
            // ISO Latin-1
            // See: https://stackoverflow.com/questions/28169745/what-are-the-options-to-convert-iso-8859-1-latin-1-to-a-string-utf-8
            result.bytes.into_iter().map(|c| c as char).collect()
        } else {
            String::from_utf8(result.bytes).map_err(|_| Error::ConversionFailure)?
        };
        Ok(ClipboardItem::new_string(text))
    }

    pub fn is_owner(&self, selection: ClipboardKind) -> bool {
        self.inner.is_owner(selection).unwrap_or(false)
    }
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        // There are always at least 3 owners:
        // the global, the server thread, and one `Clipboard::inner`
        const MIN_OWNERS: usize = 3;

        // We start with locking the global guard to prevent race
        // conditions below.
        let mut global_cb = CLIPBOARD.lock();
        if Arc::strong_count(&self.inner) == MIN_OWNERS {
            // If the are the only owners of the clipboard are ourselves and
            // the global object, then we should destroy the global object,
            // and send the data to the clipboard manager.
            //
            // The handover waits for the clipboard manager, so it runs on its own thread rather
            // than blocking this one, which can be the UI thread switching display modes. A
            // clipboard created meanwhile is independent of this one.
            let Some(global_cb) = global_cb.take() else {
                return;
            };
            let inner = Arc::clone(&self.inner);
            match std::thread::Builder::new()
                .name("Clipboard handover".to_owned())
                .spawn(move || shut_down(inner, global_cb))
            {
                Ok(handover) => HANDOVERS.lock().push(handover),
                Err(error) => {
                    log::error!(
                        "Failed to hand the clipboard over to the clipboard manager: {error}"
                    )
                }
            }
        }
    }
}

/// Threads handing a dropped clipboard's data to the clipboard manager.
static HANDOVERS: Mutex<Vec<JoinHandle<()>>> = parking_lot::const_mutex(Vec::new());

/// Waits for dropped clipboards to finish handing their data to the clipboard manager.
///
/// Called before the process exits, which would otherwise lose data still being handed over.
pub(crate) fn wait_for_clipboard_handovers() {
    let handovers = std::mem::take(&mut *HANDOVERS.lock());
    for handover in handovers {
        if handover.join().is_err() {
            log::error!("The clipboard handover thread panicked.");
        }
    }
}

/// Hands the clipboard's data to the clipboard manager, then stops serving it.
fn shut_down(inner: Arc<Inner>, global_cb: GlobalClipboard) {
    if let Err(e) = inner.ask_clipboard_manager_to_request_our_data() {
        log::error!(
            "Could not hand the clipboard data over to the clipboard manager: {}",
            e
        );
    }
    if let Err(e) = inner.server.conn.destroy_window(inner.server.win_id) {
        log::error!("Failed to destroy the clipboard window. Error: {}", e);
        return;
    }
    if let Err(e) = inner.server.conn.flush() {
        log::error!("Failed to flush the clipboard window. Error: {}", e);
        return;
    }
    if let Err(e) = global_cb.server_handle.join() {
        // Let's try extracting the error message
        let message;
        if let Some(msg) = e.downcast_ref::<&'static str>() {
            message = Some((*msg).to_string());
        } else if let Some(msg) = e.downcast_ref::<String>() {
            message = Some(msg.clone());
        } else {
            message = None;
        }
        if let Some(message) = message {
            log::error!(
                "The clipboard server thread panicked. Panic message: '{}'",
                message,
            );
        } else {
            log::error!("The clipboard server thread panicked.");
        }
    }
}

/// Exponential patch (#EXP-1946): what an item offers, as X11 targets: its images first (the more
/// specific targets), then its text as `UTF8_STRING` (TARGETS adds the text/plain aliases). An
/// item with neither (an empty one) offers empty text, as upstream did.
fn item_data(
    item: &ClipboardItem,
    text_atom: Atom,
    image_atom: impl Fn(ImageFormat) -> Atom,
) -> Vec<ClipboardData> {
    let mut data: Vec<ClipboardData> = crate::linux::clipboard_formats::image_offers(item)
        .into_iter()
        .map(|offer| ClipboardData {
            bytes: offer.bytes,
            format: image_atom(offer.format),
        })
        .collect();
    if data.is_empty() || crate::linux::clipboard_formats::has_text(item) {
        data.push(ClipboardData {
            bytes: item.text().unwrap_or_default().into_bytes(),
            format: text_atom,
        });
    }
    data
}

/// Exponential patch (#EXP-1946): the INCR chunk size for a server's maximum property bytes.
fn incr_chunk_len(max_property_bytes: usize) -> usize {
    max_property_bytes.min(INCR_CHUNK_MAX)
}

fn into_unknown<E: std::fmt::Display>(error: E) -> Error {
    Error::Unknown {
        description: error.to_string(),
    }
}

/// Clipboard selection
///
/// Linux has a concept of clipboard "selections" which tend to be used in different contexts. This
/// enum provides a way to get/set to a specific clipboard
///
/// See <https://specifications.freedesktop.org/clipboards-spec/clipboards-0.1.txt> for a better
/// description of the different clipboards.
#[derive(Copy, Clone, Debug)]
pub enum ClipboardKind {
    /// Typically used selection for explicit cut/copy/paste actions (ie. windows/macos like
    /// clipboard behavior)
    Clipboard,

    /// Typically used for mouse selections and/or currently selected text. Accessible via middle
    /// mouse click.
    Primary,

    /// The secondary clipboard is rarely used but theoretically available on X11.
    Secondary,
}

/// Configuration on how long to wait for a new X11 copy event is emitted.
#[derive(Default)]
pub(crate) enum WaitConfig {
    /// Waits until the given [`Instant`] has reached.
    #[allow(
        unused,
        reason = "Right now we don't wait for clipboard contents to sync on app close, but we may in the future"
    )]
    Until(Instant),

    /// Waits forever until a new event is reached.
    #[allow(unused)]
    #[allow(
        unused,
        reason = "Right now we don't wait for clipboard contents to sync on app close, but we may in the future"
    )]
    Forever,

    /// It shouldn't wait.
    #[default]
    None,
}

#[non_exhaustive]
pub enum Error {
    /// The clipboard contents were not available in the requested format.
    /// This could either be due to the clipboard being empty or the clipboard contents having
    /// an incompatible format to the requested one (eg when calling `get_image` on text)
    ContentNotAvailable,

    /// The native clipboard is not accessible due to being held by an other party.
    ///
    /// This "other party" could be a different process or it could be within
    /// the same program. So for example you may get this error when trying
    /// to interact with the clipboard from multiple threads at once.
    ///
    /// Note that it's OK to have multiple `Clipboard` instances. The underlying
    /// implementation will make sure that the native clipboard is only
    /// opened for transferring data and then closed as soon as possible.
    ClipboardOccupied,

    /// The image or the text that was about the be transferred to/from the clipboard could not be
    /// converted to the appropriate format.
    ConversionFailure,

    /// Any error that doesn't fit the other error types.
    ///
    /// The `description` field is only meant to help the developer and should not be relied on as a
    /// means to identify an error case during runtime.
    Unknown { description: String },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
			Error::ContentNotAvailable => f.write_str("The clipboard contents were not available in the requested format or the clipboard is empty."),
			Error::ClipboardOccupied => f.write_str("The native clipboard is not accessible due to being held by an other party."),
			Error::ConversionFailure => f.write_str("The image or the text that was about the be transferred to/from the clipboard could not be converted to the appropriate format."),
			Error::Unknown { description } => f.write_fmt(format_args!("Unknown error while interacting with the clipboard: {description}")),
		}
    }
}

impl std::error::Error for Error {}

impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use Error::*;
        macro_rules! kind_to_str {
			($( $e: pat ),*) => {
				match self {
					$(
						$e => stringify!($e),
					)*
				}
			}
		}
        let name = kind_to_str!(
            ContentNotAvailable,
            ClipboardOccupied,
            ConversionFailure,
            Unknown { .. }
        );
        f.write_fmt(format_args!("{name} - \"{self}\""))
    }
}

impl Error {
    pub(crate) fn unknown<M: Into<String>>(message: M) -> Self {
        Error::Unknown {
            description: message.into(),
        }
    }
}

// Exponential patch (#EXP-1946)
#[cfg(test)]
mod tests {
    use super::{INCR_CHUNK_MAX, incr_chunk_len, item_data};
    use gpui::{ClipboardEntry, ClipboardItem, ClipboardString, Image, ImageFormat};

    const TEXT: u32 = 1;
    const PNG: u32 = 2;
    const OTHER: u32 = 3;

    fn atom(format: ImageFormat) -> u32 {
        if format == ImageFormat::Png { PNG } else { OTHER }
    }

    fn png() -> Vec<u8> {
        let mut bytes = Vec::new();
        image::DynamicImage::new_rgb8(2, 2)
            .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        bytes
    }

    fn targets(item: &ClipboardItem) -> Vec<(u32, Vec<u8>)> {
        item_data(item, TEXT, atom)
            .into_iter()
            .map(|data| (data.format, data.bytes))
            .collect()
    }

    #[test]
    fn clipboard_text_item_is_utf8_string_as_before() {
        let item = ClipboardItem::new_string("héllo".into());
        assert_eq!(targets(&item), vec![(TEXT, "héllo".as_bytes().to_vec())]);
    }

    #[test]
    fn clipboard_empty_item_offers_empty_text_as_before() {
        let item = ClipboardItem { entries: Vec::new() };
        assert_eq!(targets(&item), vec![(TEXT, Vec::new())]);
    }

    #[test]
    fn clipboard_image_item_offers_the_image_and_no_text() {
        let png = png();
        let item = ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Png, png.clone()));
        assert_eq!(targets(&item), vec![(PNG, png)]);
    }

    #[test]
    fn clipboard_gif_item_offers_gif_then_png() {
        let mut gif = Vec::new();
        image::DynamicImage::new_rgb8(2, 2)
            .write_to(&mut std::io::Cursor::new(&mut gif), image::ImageFormat::Gif)
            .unwrap();
        let item = ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Gif, gif.clone()));
        let targets = targets(&item);
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0], (OTHER, gif));
        assert_eq!(targets[1].0, PNG);
    }

    #[test]
    fn clipboard_mixed_item_offers_image_then_text() {
        let png = png();
        let item = ClipboardItem {
            entries: vec![
                ClipboardEntry::String(ClipboardString::new("alt".into())),
                ClipboardEntry::Image(Image::from_bytes(ImageFormat::Png, png.clone())),
            ],
        };
        assert_eq!(targets(&item), vec![(PNG, png), (TEXT, b"alt".to_vec())]);
    }

    /// A private X server (never the person's display); `None` = no Xvfb here.
    struct Server {
        child: std::process::Child,
        display: String,
    }

    impl Server {
        /// Exponential patch (#EXP-1970): Xvfb picks a free display number itself
        /// (`-displayfd`) and writes it once it accepts connections, so the server
        /// reached is always this child (a guessed number could be another's).
        fn start() -> Option<Self> {
            use std::io::BufRead as _;
            use std::process::{Command, Stdio};
            let Ok(mut child) = Command::new("Xvfb")
                .args(["-displayfd", "1", "-screen", "0", "640x480x24", "-nolisten", "tcp"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            else {
                return None;
            };
            let stdout = child.stdout.take().unwrap();
            // The guard from here on: a failing test still stops its server.
            let mut server = Server { child, display: String::new() };
            let mut number = String::new();
            std::io::BufReader::new(stdout).read_line(&mut number).ok()?;
            let number = number.trim();
            if number.is_empty() {
                // Xvfb exited without a display (no `-displayfd` support).
                return None;
            }
            server.display = format!(":{number}");
            Some(server)
        }
    }

    impl Drop for Server {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    /// Another X client's read (its own window, a real `ConvertSelection`), not the owner's
    /// shortcut: a small PNG in one property, one larger than a request in INCR chunks, and the
    /// text of a mixed item next to its image.
    #[test]
    fn clipboard_x11_round_trip_serves_images_and_large_ones_in_incr_chunks() {
        let Some(server) = Server::start() else {
            eprintln!("skipped: no Xvfb binary");
            return;
        };
        let owner = super::Clipboard::new(&server.display).unwrap();
        let reader = super::Inner::new(&server.display).unwrap();
        let png_atom = reader.atoms.PNG__MIME;
        let text_atom = reader.atoms.UTF8_STRING;
        let read = |format: u32| {
            reader
                .read(&[format], super::ClipboardKind::Clipboard)
                .map(|data| (data.format, data.bytes))
        };

        let small = png();
        owner
            .set_item(
                &ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Png, small.clone())),
                super::ClipboardKind::Clipboard,
                super::WaitConfig::None,
            )
            .unwrap();
        assert_eq!(read(png_atom).unwrap(), (png_atom, small.clone()));
        assert!(read(text_atom).is_err(), "an image-only item offers no text");

        // Larger than the server's maximum request (BIG-REQUESTS: 16 MiB on Xvfb).
        let max = owner.inner.max_property_bytes();
        let mut state = 0x2545_f491_u32;
        let large: Vec<u8> = (0..max + 3 * super::INCR_CHUNK_MAX + 17)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as u8
            })
            .collect();
        owner
            .set_item(
                &ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Png, large.clone())),
                super::ClipboardKind::Clipboard,
                super::WaitConfig::None,
            )
            .unwrap();
        let (format, bytes) = read(png_atom).unwrap();
        assert_eq!(format, png_atom);
        assert!(bytes == large, "INCR bytes differ: {} of {}", bytes.len(), large.len());
        assert!(owner.inner.incr_sends.lock().is_empty(), "the transfer ended");

        let mixed = ClipboardItem {
            entries: vec![
                ClipboardEntry::String(ClipboardString::new("caption".into())),
                ClipboardEntry::Image(Image::from_bytes(ImageFormat::Png, small.clone())),
            ],
        };
        owner
            .set_item(&mixed, super::ClipboardKind::Clipboard, super::WaitConfig::None)
            .unwrap();
        assert_eq!(read(text_atom).unwrap(), (text_atom, b"caption".to_vec()));
        assert_eq!(read(png_atom).unwrap(), (png_atom, small));

        // The exit handover: a clipboard manager takes the large image (INCR) before we go.
        let saved = fake_manager(&server.display);
        owner
            .set_item(
                &ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Png, large.clone())),
                super::ClipboardKind::Clipboard,
                super::WaitConfig::None,
            )
            .unwrap();
        drop(owner);
        super::wait_for_clipboard_handovers();
        let saved = saved.lock().take().expect("the manager saved nothing");
        assert!(saved == large, "the manager saved {} of {} bytes", saved.len(), large.len());
    }

    /// A tiny clipboard manager on `display`: it owns `CLIPBOARD_MANAGER` and, asked to
    /// `SAVE_TARGETS`, reads the clipboard's `image/png` as any other client would (INCR
    /// included), keeps it, then answers the request.
    fn fake_manager(display: &str) -> std::sync::Arc<parking_lot::Mutex<Option<Vec<u8>>>> {
        use x11rb::{
            connection::Connection as _, protocol::xproto::ConnectionExt as _,
            wrapper::ConnectionExt as _,
        };
        let saved = std::sync::Arc::new(parking_lot::Mutex::new(None));
        let fetcher = super::Inner::new(display).unwrap();
        let (conn, screen) = x11rb::connect(Some(display)).unwrap();
        let root = conn.setup().roots[screen].root;
        let window = conn.generate_id().unwrap();
        conn.create_window(
            0,
            window,
            root,
            0,
            0,
            1,
            1,
            0,
            x11rb::protocol::xproto::WindowClass::INPUT_ONLY,
            0,
            &Default::default(),
        )
        .unwrap();
        conn.set_selection_owner(window, fetcher.atoms.CLIPBOARD_MANAGER, x11rb::CURRENT_TIME)
            .unwrap();
        conn.sync().unwrap();
        std::thread::spawn({
            let saved = saved.clone();
            move || {
                while let Ok(event) = conn.wait_for_event() {
                    let x11rb::protocol::Event::SelectionRequest(request) = event else {
                        continue;
                    };
                    if request.target != fetcher.atoms.SAVE_TARGETS {
                        continue;
                    }
                    let fetched = fetcher
                        .read(&[fetcher.atoms.PNG__MIME], super::ClipboardKind::Clipboard)
                        .ok();
                    let answered = fetched.is_some();
                    *saved.lock() = fetched.map(|data| data.bytes);
                    let notify = x11rb::protocol::xproto::SelectionNotifyEvent {
                        response_type: x11rb::protocol::xproto::SELECTION_NOTIFY_EVENT,
                        sequence: 0,
                        time: request.time,
                        requestor: request.requestor,
                        selection: request.selection,
                        target: request.target,
                        property: if answered { request.property } else { x11rb::NONE },
                    };
                    let _ = conn.send_event(
                        false,
                        request.requestor,
                        x11rb::protocol::xproto::EventMask::NO_EVENT,
                        notify,
                    );
                    let _ = conn.flush();
                }
            }
        });
        saved
    }

    #[test]
    fn clipboard_incr_chunks_fit_one_request_and_stay_bounded() {
        assert_eq!(incr_chunk_len(256 * 1024), 256 * 1024);
        assert_eq!(incr_chunk_len(16 << 20), INCR_CHUNK_MAX);
    }
}
