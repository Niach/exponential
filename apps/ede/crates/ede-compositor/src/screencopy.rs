//! Screen copy for the person's own tools: grim, Flameshot, OBS's
//! wlrobs, the screenshot keys of other desktops' utilities.
//!
//! Two protocols, one path. `ext-image-copy-capture-v1` (with the
//! output source of `ext-image-capture-source-v1`), through smithay's
//! state; and `zwlr_screencopy_manager_v1` v3, written here, because grim
//! 1.4 and most of the tools that exist speak only that one. Both draw a
//! frame through [`crate::backend::capture_output`], the same redacting
//! capture ede-protocol and the portal use: a locked session is black,
//! protected windows and the shell's Agent spaces list are black, so a
//! copy never shows more than an agent's capture would.
//!
//! Only outputs (no toplevel source), only wl_shm (Argb8888, Xrgb8888; no
//! linux-dmabuf), the cursor never painted in (`paint_cursors` and
//! `overlay_cursor` are ignored), every frame fully damaged and copied at
//! once (`copy_with_damage` does not wait for a change).
//!
//! Who sees the globals: [`may_copy_screen`].

use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use smithay::{
    output::{Output, WeakOutput},
    reexports::{
        wayland_protocols_wlr::screencopy::v1::server::{
            zwlr_screencopy_frame_v1::{self, ZwlrScreencopyFrameV1},
            zwlr_screencopy_manager_v1::{self, ZwlrScreencopyManagerV1},
        },
        wayland_server::{
            protocol::{wl_buffer::WlBuffer, wl_output::WlOutput, wl_shm},
            Client, DataInit, DisplayHandle, New, Resource,
        },
    },
    utils::{Clock, Monotonic, Physical, Rectangle, Transform},
    wayland::{
        image_capture_source::{ImageCaptureSource, ImageCaptureSourceHandler, ImageCaptureSourceState, OutputCaptureSourceHandler, OutputCaptureSourceState},
        image_copy_capture::{BufferConstraints, CaptureFailureReason, Frame, ImageCopyCaptureHandler, ImageCopyCaptureState, Session, SessionRef},
        shm::with_buffer_contents_mut,
        Dispatch2, GlobalDispatch2,
    },
};

use crate::{backend::headless, state::{ClientState, Ede}};

/// The wlr manager's version: 3 (`buffer_done`, `linux_dmabuf` never sent).
const WLR_VERSION: u32 = 3;

/// What a copy is drawn in: little-endian `0xAARRGGBB` words, which is
/// what [`headless::Frame`] holds.
const FORMATS: [wl_shm::Format; 2] = [wl_shm::Format::Xrgb8888, wl_shm::Format::Argb8888];

/// The filter of every screen copy global: a client of the person's desk
/// ([`crate::desk::on_person`]) that is not sandboxed. Its peer
/// ([`ClientState::peer`]) runs in the compositor's user namespace, or it
/// has none (the compositor's own clients: Xwayland, a test's pair).
///
/// Why this and no prompt: parity with Cinnamon on X11, where every
/// unsandboxed app the person runs can read the whole screen (`XGetImage`
/// on the root window); grim or Flameshot run from a terminal are those
/// apps. A sandboxed one (a Flatpak, another user namespace) never sees
/// the globals and goes through the portal, which asks the person. An agent
/// space's client is on another desk and never sees them either.
pub fn may_copy_screen(client: &Client) -> bool {
    crate::desk::on_person(client) && unsandboxed(client.get_data::<ClientState>())
}

/// The sandbox half of [`may_copy_screen`]: no peer (the compositor's own
/// client, or Xwayland's, which has no [`ClientState`]) or one in the
/// compositor's user namespace.
pub fn unsandboxed(state: Option<&ClientState>) -> bool {
    state.and_then(|s| s.peer.as_deref()).is_none_or(|peer| peer.same_userns)
}

/// The screen copy globals' state, a field of [`Ede`].
pub struct Screencopy {
    pub image_capture_source: ImageCaptureSourceState,
    pub output_capture_source: OutputCaptureSourceState,
    pub image_copy_capture: ImageCopyCaptureState,
    /// The ext sessions that run: dropping one tells its client `stopped`.
    sessions: Vec<Session>,
}

impl Screencopy {
    /// Announces the ext output source and copy managers and the wlr
    /// manager, each to [`may_copy_screen`]'s clients only ([`Ede::new`]).
    pub fn new(dh: &DisplayHandle) -> Self {
        let output_capture_source = OutputCaptureSourceState::new_with_filter::<Ede, _>(dh, may_copy_screen);
        let image_copy_capture = ImageCopyCaptureState::new_with_filter::<Ede, _>(dh, may_copy_screen);
        dh.create_global::<Ede, ZwlrScreencopyManagerV1, _>(WLR_VERSION, WlrGlobal);
        Self { image_capture_source: ImageCaptureSourceState::new(), output_capture_source, image_copy_capture, sessions: Vec::new() }
    }
}

/// An output's size in buffer pixels: its current mode, what
/// [`crate::backend::capture_output`] draws.
fn output_size(output: &Output) -> Option<(i32, i32)> {
    output.current_mode().map(|m| (m.size.w, m.size.h))
}

/// The presentation time a copy carries: now, on the monotonic clock.
fn now() -> Duration {
    Clock::<Monotonic>::new().now().into()
}

/// Copies `rect` (pixels of `frame`, drawn upright) into a client's shm
/// buffer of exactly that size, laid out in the output's buffer
/// orientation `transform` (wlr-screencopy: the client turns it upright by
/// the wl_output's transform, as grim does; the winit window's output is
/// flipped). Flips and half turns only; a quarter-turned output's copy
/// stays upright. `Err` = not a wl_shm buffer, the wrong size or format,
/// or a stride or offset that does not fit its pool.
fn copy_into(buffer: &WlBuffer, frame: &headless::Frame, rect: Rectangle<i32, Physical>, transform: Transform) -> Result<(), ()> {
    let (flip_x, flip_y) = match transform {
        Transform::_180 => (true, true),
        Transform::Flipped => (true, false),
        Transform::Flipped180 => (false, true),
        _ => (false, false),
    };
    let (w, h) = (rect.size.w, rect.size.h);
    with_buffer_contents_mut(buffer, |ptr, len, data| {
        if data.width != w || data.height != h || !FORMATS.contains(&data.format) || data.stride < w * 4 || data.offset < 0 {
            return Err(());
        }
        let (offset, stride) = (data.offset as usize, data.stride as usize);
        let end = offset + stride * (h as usize - 1) + w as usize * 4;
        if end > len {
            return Err(());
        }
        // Safety: the pool is mapped for `len` bytes, and every row written
        // ends at or before `end` <= `len` (checked above).
        let pool = unsafe { std::slice::from_raw_parts_mut(ptr, len) };
        for y in 0..h as usize {
            let sy = if flip_y { h as usize - 1 - y } else { y };
            let from = (rect.loc.y as usize + sy) * frame.width as usize + rect.loc.x as usize;
            let row = &frame.pixels[from..from + w as usize];
            let at = offset + y * stride;
            for (i, dst) in pool[at..at + w as usize * 4].chunks_exact_mut(4).enumerate() {
                let px = row[if flip_x { w as usize - 1 - i } else { i }];
                dst.copy_from_slice(&px.to_le_bytes());
            }
        }
        Ok(())
    })
    .map_err(|_| ())?
}

// ---------------------------------------------------------------------------
// ext-image-copy-capture-v1
// ---------------------------------------------------------------------------

impl ImageCaptureSourceHandler for Ede {}

impl OutputCaptureSourceHandler for Ede {
    fn output_capture_source_state(&mut self) -> &mut OutputCaptureSourceState {
        &mut self.screencopy.output_capture_source
    }

    fn output_source_created(&mut self, source: ImageCaptureSource, output: &Output) {
        source.user_data().insert_if_missing(|| output.downgrade());
    }
}

/// The output an ext source was made from; `None` once it is gone (or
/// for a source made from a dead wl_output).
fn source_output(source: &ImageCaptureSource) -> Option<Output> {
    source.user_data().get::<WeakOutput>()?.upgrade()
}

fn constraints(output: &Output) -> Option<BufferConstraints> {
    let (w, h) = output_size(output)?;
    Some(BufferConstraints {
        size: (w, h).into(),
        shm: FORMATS.to_vec(),
        #[cfg(any(feature = "gpu", feature = "winit"))]
        dma: None,
    })
}

impl ImageCopyCaptureHandler for Ede {
    fn image_copy_capture_state(&mut self) -> &mut ImageCopyCaptureState {
        &mut self.screencopy.image_copy_capture
    }

    /// An output source's mode; `None` (the session is stopped at once) for
    /// a source with no live output.
    fn capture_constraints(&mut self, source: &ImageCaptureSource) -> Option<BufferConstraints> {
        constraints(&source_output(source)?)
    }

    fn new_session(&mut self, session: Session) {
        self.screencopy.sessions.push(session);
    }

    fn frame(&mut self, session: &SessionRef, frame: Frame) {
        let Some(output) = source_output(&session.source()) else {
            // The output is gone: the session ends (`stopped`, the frame fails).
            self.screencopy.sessions.retain(|s| s != session);
            return frame.fail(CaptureFailureReason::Stopped);
        };
        // A mode change since the constraints were sent: new ones, and this
        // frame's buffer is the old size.
        let Some(now_constraints) = constraints(&output) else { return frame.fail(CaptureFailureReason::Unknown) };
        if session.current_constraints().is_none_or(|c| c.size != now_constraints.size) {
            session.update_constraints(now_constraints);
            return frame.fail(CaptureFailureReason::BufferConstraints);
        }
        let shot = match crate::backend::capture_output(self, &output) {
            Ok(shot) => shot,
            Err(e) => {
                tracing::warn!("screen copy of {}: {e}", output.name());
                return frame.fail(CaptureFailureReason::Unknown);
            }
        };
        let full = Rectangle::from_size((shot.width, shot.height).into());
        if copy_into(&frame.buffer(), &shot, full, Transform::Normal).is_err() {
            return frame.fail(CaptureFailureReason::BufferConstraints);
        }
        let damage: Vec<Rectangle<i32, smithay::utils::Buffer>> = vec![Rectangle::from_size((shot.width, shot.height).into())];
        frame.success(Transform::Normal, damage, now());
    }

    fn session_destroyed(&mut self, session: SessionRef) {
        self.screencopy.sessions.retain(|s| *s != session);
        self.screencopy.image_copy_capture.cleanup();
    }
}

// ---------------------------------------------------------------------------
// zwlr_screencopy_manager_v1 (v3)
// ---------------------------------------------------------------------------

/// The wlr manager global's data.
pub struct WlrGlobal;

/// A bound wlr manager's data.
pub struct WlrManager;

/// A wlr frame: what it copies, decided when it was asked for.
pub struct WlrFrame {
    /// `None` = nothing to copy (a dead output, an empty region): `failed`
    /// was sent.
    what: Option<(WeakOutput, Rectangle<i32, Physical>)>,
    /// The frame took its one `copy` (a second is `already_used`).
    used: AtomicBool,
    /// The output's mode when `buffer` was sent; a copy at another fails.
    mode: (i32, i32),
}

impl GlobalDispatch2<ZwlrScreencopyManagerV1, Ede> for WlrGlobal {
    fn bind(&self, _state: &mut Ede, _handle: &DisplayHandle, _client: &Client, resource: New<ZwlrScreencopyManagerV1>, data_init: &mut DataInit<'_, Ede>) {
        data_init.init(resource, WlrManager);
    }

    fn can_view(&self, client: &Client) -> bool {
        may_copy_screen(client)
    }
}

/// A region in an output's logical coordinates as buffer pixels of its
/// capture (`size`, the mode) at `scale`, clipped to the output; `None`
/// when nothing of it is on the output. Rounded as wlroots scales boxes.
/// Transformed outputs: the region is taken in the capture's own
/// orientation (the capture is drawn untransformed).
pub fn region_pixels((x, y, w, h): (i32, i32, i32, i32), scale: f64, size: (i32, i32)) -> Option<Rectangle<i32, Physical>> {
    if w <= 0 || h <= 0 {
        return None;
    }
    let px = |v: i32, max: i32| ((f64::from(v) * scale).round() as i64).clamp(0, i64::from(max)) as i32;
    let (x0, y0) = (px(x, size.0), px(y, size.1));
    let (x1, y1) = (px(x.saturating_add(w), size.0), px(y.saturating_add(h), size.1));
    (x1 > x0 && y1 > y0).then(|| Rectangle::new((x0, y0).into(), (x1 - x0, y1 - y0).into()))
}

impl Dispatch2<ZwlrScreencopyManagerV1, Ede> for WlrManager {
    fn request(&self, _state: &mut Ede, _client: &Client, _resource: &ZwlrScreencopyManagerV1, request: zwlr_screencopy_manager_v1::Request, _dh: &DisplayHandle, data_init: &mut DataInit<'_, Ede>) {
        let (frame, output, region) = match request {
            zwlr_screencopy_manager_v1::Request::CaptureOutput { frame, overlay_cursor: _, output } => (frame, output, None),
            zwlr_screencopy_manager_v1::Request::CaptureOutputRegion { frame, overlay_cursor: _, output, x, y, width, height } => (frame, output, Some((x, y, width, height))),
            zwlr_screencopy_manager_v1::Request::Destroy => return,
            _ => return,
        };
        let target = wlr_target(&output, region);
        let mode = target.as_ref().map_or((0, 0), |t| t.0);
        let what = target.map(|(_, output, rect)| (output, rect));
        let rect = what.as_ref().map(|(_, rect)| *rect);
        let frame = data_init.init(frame, WlrFrame { what, used: AtomicBool::new(false), mode });
        let Some(rect) = rect else { return frame.failed() };
        // Opaque, as wlroots announces an output's (the alpha is the clear
        // colour's 0xff); an Argb8888 buffer is taken as well.
        frame.buffer(wl_shm::Format::Xrgb8888, rect.size.w as u32, rect.size.h as u32, rect.size.w as u32 * 4);
        if frame.version() >= 3 {
            frame.buffer_done();
        }
    }
}

/// What a wlr frame copies: the mode it was taken at, the output and the
/// pixels of it (the whole mode, or the region scaled by its fractional
/// scale).
fn wlr_target(output: &WlOutput, region: Option<(i32, i32, i32, i32)>) -> Option<((i32, i32), WeakOutput, Rectangle<i32, Physical>)> {
    let output = Output::from_resource(output)?;
    let size = output_size(&output)?;
    let rect = match region {
        None => Rectangle::from_size(size.into()),
        Some(region) => region_pixels(region, output.current_scale().fractional_scale(), size)?,
    };
    Some((size, output.downgrade(), rect))
}

impl Dispatch2<ZwlrScreencopyFrameV1, Ede> for WlrFrame {
    fn request(&self, state: &mut Ede, _client: &Client, resource: &ZwlrScreencopyFrameV1, request: zwlr_screencopy_frame_v1::Request, _dh: &DisplayHandle, _data_init: &mut DataInit<'_, Ede>) {
        let (buffer, damaged) = match request {
            zwlr_screencopy_frame_v1::Request::Copy { buffer } => (buffer, false),
            zwlr_screencopy_frame_v1::Request::CopyWithDamage { buffer } => (buffer, true),
            zwlr_screencopy_frame_v1::Request::Destroy => return,
            _ => return,
        };
        if self.used.swap(true, Ordering::SeqCst) {
            return resource.post_error(zwlr_screencopy_frame_v1::Error::AlreadyUsed, "the frame was already copied");
        }
        let Some((output, rect)) = self.what.as_ref() else { return resource.failed() };
        let Some(output) = output.upgrade() else { return resource.failed() };
        let shot = match crate::backend::capture_output(state, &output) {
            Ok(shot) => shot,
            Err(e) => {
                tracing::warn!("screen copy of {}: {e}", output.name());
                return resource.failed();
            }
        };
        // The mode changed since `buffer` was sent: the region was scaled
        // and clipped for the old one.
        if (shot.width, shot.height) != self.mode {
            return resource.failed();
        }
        // A wrong size, format or stride: `failed`, not the protocol's
        // `invalid_buffer` error (a tool that guessed wrong keeps its
        // connection).
        if copy_into(&buffer, &shot, *rect, output.current_transform()).is_err() {
            return resource.failed();
        }
        resource.flags(zwlr_screencopy_frame_v1::Flags::empty());
        if damaged && resource.version() >= 2 {
            resource.damage(0, 0, rect.size.w as u32, rect.size.h as u32);
        }
        let time = now();
        resource.ready((time.as_secs() >> 32) as u32, (time.as_secs() & 0xffff_ffff) as u32, time.subsec_nanos());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_region_is_scaled_rounded_and_clipped() {
        assert_eq!(region_pixels((10, 20, 100, 50), 1.0, (800, 600)), Some(Rectangle::new((10, 20).into(), (100, 50).into())));
        assert_eq!(region_pixels((10, 20, 100, 50), 1.25, (1000, 750)), Some(Rectangle::new((13, 25).into(), (125, 63).into())));
        assert_eq!(region_pixels((-10, -10, 20, 20), 2.0, (800, 600)), Some(Rectangle::new((0, 0).into(), (20, 20).into())));
        assert_eq!(region_pixels((700, 500, 500, 500), 1.0, (800, 600)), Some(Rectangle::new((700, 500).into(), (100, 100).into())));
        assert_eq!(region_pixels((900, 0, 10, 10), 1.0, (800, 600)), None);
        assert_eq!(region_pixels((0, 0, 0, 10), 1.0, (800, 600)), None);
    }

    #[test]
    fn a_sandboxed_peer_may_not_copy_the_screen() {
        let peer = |same_userns| ClientState {
            peer: Some(std::sync::Arc::new(crate::peer::Peer { pid: 1, uid: 0, pidfd: None, start: 0, exe: None, same_userns, parent: None })),
            ..Default::default()
        };
        assert!(unsandboxed(None), "Xwayland: no ClientState");
        assert!(unsandboxed(Some(&ClientState::default())), "the compositor's own client: no peer");
        assert!(unsandboxed(Some(&peer(true))), "grim from a terminal");
        assert!(!unsandboxed(Some(&peer(false))), "a Flatpak: another user namespace");
    }
}
