//! The message a failed session leaves on the screen (#EDE-40). When the
//! compositor could not start or stopped itself, ede-session runs
//! `ede-compositor recovery --reason … --next …`: it takes the seat, paints
//! the reason into a DRM dumb buffer on every connected display with the
//! Tier A canvas (no GPU, no GLES: the one path that still works when
//! rendering is what failed), and holds it until a key, a button or
//! `--timeout` seconds; then ede-session falls back (safe mode once, the
//! login screen after that). `--png PATH --size WxH` draws the same frame
//! into a file (pictures, tests) without touching a seat.

use std::time::Duration;

use crate::text::{self, colour, Canvas};

/// What the screen says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub title: String,
    pub reason: String,
    pub next: String,
}

pub const HINT: &str = "Any key continues · logs: journalctl -b _COMM=ede-compositor _COMM=ede-session";
pub const DEFAULT_TITLE: &str = "Exponential could not start";
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

const WIDTH: i32 = 640;
const PAD: f32 = 24.0;
const MAX_REASON_LINES: usize = 4;

/// `ede-compositor recovery …`, parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub message: Message,
    pub timeout: Duration,
    /// `Some((path, (w, h)))`: draw into a PNG instead of the displays.
    pub png: Option<(String, (u32, u32))>,
}

pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        message: Message { title: DEFAULT_TITLE.into(), reason: String::new(), next: String::new() },
        timeout: DEFAULT_TIMEOUT,
        png: None,
    };
    let mut size = (1280u32, 800u32);
    let mut png = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = || it.next().cloned().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--title" => o.message.title = value()?,
            "--reason" => o.message.reason = value()?,
            "--next" => o.message.next = value()?,
            "--timeout" => o.timeout = Duration::from_secs(value()?.parse().map_err(|_| "--timeout needs seconds".to_string())?),
            "--png" => png = Some(value()?),
            "--size" => {
                let v = value()?;
                let (w, h) = v.split_once('x').ok_or("--size needs WxH")?;
                size = (w.parse().map_err(|_| "--size needs WxH")?, h.parse().map_err(|_| "--size needs WxH")?);
            }
            other => return Err(format!("unknown argument {other:?}; usage: ede-compositor recovery [--title T] --reason R [--next N] [--timeout SECS] [--png PATH --size WxH]")),
        }
    }
    if o.message.reason.is_empty() {
        return Err("--reason is required".into());
    }
    o.png = png.map(|p| (p, size));
    Ok(o)
}

/// Twice the size on a display at least 2560 px wide, like the shell.
pub fn scale_for(width: u32) -> f32 {
    if width >= 2560 {
        2.0
    } else {
        1.0
    }
}

/// `text` in lines no wider than `max_width` at `size` (words kept whole,
/// a word longer than the line is cut with an ellipsis); at most `max`
/// lines, the last one ending in an ellipsis when more was left.
pub fn wrap(text: &str, size: f32, max_width: f32, max: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if text::measure(&candidate, size) <= max_width || line.is_empty() {
            line = candidate;
        } else {
            lines.push(std::mem::replace(&mut line, word.to_string()));
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    let cut = lines.len() > max;
    lines.truncate(max.max(1));
    if cut {
        if let Some(last) = lines.last_mut() {
            *last = text::fit(&format!("{last} …"), size, max_width);
        }
    }
    lines.iter().map(|l| text::fit(l, size, max_width)).collect()
}

/// The card: title, the reason (wrapped), what happens next, the hint.
pub fn card(message: &Message, scale: f32) -> Canvas {
    let s = scale.max(0.5);
    let (title_size, body_size) = (16.0 * s, 14.0 * s);
    let inner = WIDTH as f32 * s - 2.0 * PAD * s;
    let reason = wrap(&message.reason, body_size, inner, MAX_REASON_LINES);
    let (title_h, body_h) = (34.0 * s, 22.0 * s);
    let height = PAD * s + title_h + reason.len() as f32 * body_h + 10.0 * s + (if message.next.is_empty() { 0.0 } else { 26.0 * s }) + 22.0 * s + PAD * s;
    let (w, h) = ((WIDTH as f32 * s).round() as u32, height.round() as u32);
    let mut c = Canvas::new(w, h);
    c.fill_rounded_top(w as i32, h as i32, 12.0 * s, colour::CARD);
    c.fill_rect(0, h as i32 - 1, w as i32, 1, colour::BORDER_ON_CARD);
    let x = PAD * s;
    let mut y = PAD * s + text::ascent(title_size);
    text::draw(&mut c, x, y, &text::fit(&message.title, title_size, inner), title_size, colour::FOREGROUND);
    y += title_h;
    for line in &reason {
        text::draw(&mut c, x, y, line, body_size, colour::DESTRUCTIVE);
        y += body_h;
    }
    y += 10.0 * s;
    if !message.next.is_empty() {
        text::draw(&mut c, x, y, &text::fit(&message.next, body_size, inner), body_size, colour::FOREGROUND);
        y += 26.0 * s;
    }
    text::draw(&mut c, x, y, &text::fit(HINT, 12.0 * s, inner), 12.0 * s, colour::MUTED_FOREGROUND);
    c
}

/// The whole display: the background colour with the card centred.
pub fn frame(message: &Message, width: u32, height: u32) -> Canvas {
    let card = card(message, scale_for(width));
    let mut f = Canvas::new(width, height);
    f.fill(colour::BACKGROUND);
    let (x0, y0) = ((width as i32 - card.width as i32) / 2, (height as i32 - card.height as i32) / 2);
    for y in 0..card.height as i32 {
        for x in 0..card.width as i32 {
            let i = ((y as u32 * card.width + x as u32) * 4) as usize;
            let px = &card.data[i..i + 4];
            f.blend(x0 + x, y0 + y, [px[0], px[1], px[2], 255], px[3]);
        }
    }
    f
}

/// `frame` as `XRGB8888` rows of `pitch` bytes (a dumb buffer's layout).
pub fn xrgb_rows(frame: &Canvas, pitch: usize, out: &mut [u8]) {
    for y in 0..frame.height as usize {
        let row = &frame.data[y * frame.width as usize * 4..(y + 1) * frame.width as usize * 4];
        let dst = &mut out[y * pitch..y * pitch + frame.width as usize * 4];
        for (src, px) in row.chunks_exact(4).zip(dst.chunks_exact_mut(4)) {
            px.copy_from_slice(&[src[2], src[1], src[0], 0]);
        }
    }
}

pub fn write_png(path: &str, frame: &Canvas) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| format!("{path}: {e}"))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), frame.width, frame.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&frame.data).map_err(|e| e.to_string())
}

/// `ede-compositor recovery …`: the file, or the displays until a key or
/// the timeout.
pub fn main(args: &[String]) -> Result<(), String> {
    let options = parse_args(args)?;
    if let Some((path, (w, h))) = &options.png {
        return write_png(path, &frame(&options.message, *w, *h));
    }
    #[cfg(feature = "udev")]
    return show(&options.message, options.timeout);
    #[cfg(not(feature = "udev"))]
    Err("built without the udev backend: only --png works".into())
}

/// Takes the seat, paints the message on every connected display, waits.
#[cfg(feature = "udev")]
pub fn show(message: &Message, timeout: Duration) -> Result<(), String> {
    use smithay::{
        backend::{
            drm::DrmDeviceFd,
            libinput::LibinputSessionInterface,
            session::{libseat::LibSeatSession, Session},
            udev::all_gpus,
        },
        reexports::{
            input::{
                event::{pointer::PointerEvent, Event},
                Libinput,
            },
            rustix::fs::OFlags,
        },
        utils::DeviceFd,
    };
    use std::time::Instant;

    let (mut session, _notifier) = LibSeatSession::new().map_err(|e| format!("no seat session: {e}"))?;
    let seat = session.seat();
    let mut shown = 0;
    // The devices stay open while the message shows: closing one frees its
    // framebuffers and dumb buffers.
    let mut devices = Vec::new();
    for path in all_gpus(&seat).map_err(|e| format!("GPUs on {seat}: {e}"))? {
        let fd = match session.open(&path, OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK) {
            Ok(fd) => fd,
            Err(e) => {
                tracing::warn!("{}: {e}", path.display());
                continue;
            }
        };
        let device = DrmDeviceFd::new(DeviceFd::from(fd));
        match paint_displays(&device, message) {
            Ok(n) => {
                shown += n;
                devices.push(device);
            }
            Err(e) => tracing::warn!("{}: {e}", path.display()),
        }
    }
    if shown == 0 {
        return Err("no display to show the message on".into());
    }
    tracing::info!("recovery message on {shown} display(s) for up to {}s", timeout.as_secs());
    let mut libinput = Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(session.clone().into());
    if libinput.udev_assign_seat(&seat).is_err() {
        tracing::warn!("libinput could not take the seat: the message stays until the timeout");
    }
    let until = Instant::now() + timeout;
    while Instant::now() < until {
        let _ = libinput.dispatch();
        for event in &mut libinput {
            if matches!(event, Event::Keyboard(_) | Event::Pointer(PointerEvent::Button(_))) {
                return Ok(());
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}

/// Every connected connector of `device` gets a dumb buffer with the frame
/// on a free CRTC; how many were set.
#[cfg(feature = "udev")]
fn paint_displays(device: &smithay::backend::drm::DrmDeviceFd, message: &Message) -> Result<usize, String> {
    use smithay::reexports::drm::{
        buffer::{Buffer, DrmFourcc},
        control::{connector, crtc, Device as ControlDevice, ModeTypeFlags},
    };

    let res = device.resource_handles().map_err(|e| format!("resources: {e}"))?;
    let mut used: Vec<crtc::Handle> = Vec::new();
    let mut shown = 0;
    for conn in res.connectors() {
        let Ok(info) = device.get_connector(*conn, false) else { continue };
        if info.state() != connector::State::Connected || info.modes().is_empty() {
            continue;
        }
        let mode = info.modes().iter().find(|m| m.mode_type().contains(ModeTypeFlags::PREFERRED)).copied().unwrap_or(info.modes()[0]);
        let crtc = info
            .encoders()
            .iter()
            .filter_map(|e| device.get_encoder(*e).ok())
            .flat_map(|enc| res.filter_crtcs(enc.possible_crtcs()))
            .find(|c| !used.contains(c));
        let Some(crtc) = crtc else { continue };
        let (w, h) = (mode.size().0 as u32, mode.size().1 as u32);
        let name = format!("{}-{}", info.interface().as_str(), info.interface_id());
        let mut buffer = device.create_dumb_buffer((w, h), DrmFourcc::Xrgb8888, 32).map_err(|e| format!("{name}: dumb buffer: {e}"))?;
        {
            let pitch = buffer.pitch() as usize;
            let mut map = device.map_dumb_buffer(&mut buffer).map_err(|e| format!("{name}: map: {e}"))?;
            xrgb_rows(&frame(message, w, h), pitch, map.as_mut());
        }
        let fb = device.add_framebuffer(&buffer, 24, 32).map_err(|e| format!("{name}: framebuffer: {e}"))?;
        device.set_crtc(crtc, Some(fb), (0, 0), &[*conn], Some(mode)).map_err(|e| format!("{name}: modeset: {e}"))?;
        tracing::info!("{name}: {w}x{h}");
        used.push(crtc);
        shown += 1;
    }
    Ok(shown)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message() -> Message {
        Message {
            title: DEFAULT_TITLE.into(),
            reason: "Virtual-1 never drew a frame: no renderer for card0: The graphics api has found no node matching DrmNode { dev: 57984, ty: Render }".into(),
            next: "Starting again with the stock settings (safe mode)".into(),
        }
    }

    #[test]
    fn the_arguments_name_the_message_and_the_wait() {
        let args: Vec<String> = ["--reason", "boom", "--next", "again", "--timeout", "8"].iter().map(|s| s.to_string()).collect();
        let o = parse_args(&args).unwrap();
        assert_eq!((o.message.title.as_str(), o.message.reason.as_str(), o.message.next.as_str()), (DEFAULT_TITLE, "boom", "again"));
        assert_eq!((o.timeout, o.png), (Duration::from_secs(8), None));
        let png: Vec<String> = ["--reason", "x", "--png", "/tmp/x.png", "--size", "1280x800"].iter().map(|s| s.to_string()).collect();
        assert_eq!(parse_args(&png).unwrap().png, Some(("/tmp/x.png".into(), (1280, 800))));
        assert!(parse_args(&[]).unwrap_err().contains("--reason"));
        assert!(parse_args(&["--bogus".into()]).unwrap_err().contains("usage"));
    }

    #[test]
    fn long_reasons_wrap_and_end_in_an_ellipsis() {
        let lines = wrap(&message().reason, 14.0, 400.0, 4);
        assert_eq!(lines.len(), 3, "{lines:?}");
        assert!(lines.iter().all(|l| text::measure(l, 14.0) <= 400.0));
        assert!(!lines.last().unwrap().ends_with('…'), "it fit: {lines:?}");
        let cut = wrap(&message().reason, 14.0, 400.0, 2);
        assert_eq!(cut.len(), 2);
        assert!(cut.last().unwrap().ends_with('…'), "{cut:?}");
        assert!(text::measure(cut.last().unwrap(), 14.0) <= 400.0);
        assert_eq!(wrap("short", 14.0, 400.0, 3), ["short"]);
        assert_eq!(wrap("", 14.0, 400.0, 3), Vec::<String>::new());
    }

    #[test]
    fn the_frame_is_the_background_with_the_card_in_the_middle() {
        let f = frame(&message(), 1280, 800);
        assert_eq!((f.width, f.height), (1280, 800));
        let px = |x: u32, y: u32| &f.data[((y * f.width + x) * 4) as usize..][..4];
        assert_eq!(px(2, 2), colour::BACKGROUND);
        assert_eq!(px(640, 400), colour::CARD, "the centre is on the card");
        // Something red (the reason) was drawn.
        assert!(f.data.chunks_exact(4).any(|p| p[0] > 200 && p[1] < 150));
        assert_eq!(scale_for(3556), 2.0);
        assert!(card(&message(), 2.0).width == 2 * card(&message(), 1.0).width);
    }

    #[test]
    fn dumb_buffer_rows_are_xrgb_at_the_pitch() {
        let mut c = Canvas::new(2, 2);
        c.fill([1, 2, 3, 255]);
        let mut out = vec![9u8; 2 * 16];
        xrgb_rows(&c, 16, &mut out);
        assert_eq!(&out[..8], &[3, 2, 1, 0, 3, 2, 1, 0]);
        assert_eq!(out[8], 9, "padding untouched");
        assert_eq!(&out[16..20], &[3, 2, 1, 0]);
    }

    /// Needs /dev/dri/card0 (any session's ACL, no DRM master): the dumb
    /// buffer, its mapping and the framebuffer, everything but the modeset.
    #[cfg(feature = "udev")]
    #[test]
    #[ignore]
    fn a_dumb_buffer_takes_the_frame_on_card0() {
        use smithay::reexports::drm::{
            buffer::{Buffer, DrmFourcc},
            control::Device as ControlDevice,
        };
        let file = std::fs::OpenOptions::new().read(true).write(true).open("/dev/dri/card0").unwrap();
        let device = smithay::backend::drm::DrmDeviceFd::new(smithay::utils::DeviceFd::from(std::os::fd::OwnedFd::from(file)));
        let (w, h) = (1280u32, 800u32);
        let mut buffer = device.create_dumb_buffer((w, h), DrmFourcc::Xrgb8888, 32).unwrap();
        let pitch = buffer.pitch() as usize;
        assert!(pitch >= w as usize * 4);
        {
            let mut map = device.map_dumb_buffer(&mut buffer).unwrap();
            xrgb_rows(&frame(&message(), w, h), pitch, map.as_mut());
            let centre = &map.as_mut()[(h as usize / 2) * pitch + (w as usize / 2) * 4..][..4];
            assert_eq!(centre, &[colour::CARD[2], colour::CARD[1], colour::CARD[0], 0]);
        }
        let fb = device.add_framebuffer(&buffer, 24, 32).unwrap();
        device.destroy_framebuffer(fb).unwrap();
        device.destroy_dumb_buffer(buffer).unwrap();
    }

    #[test]
    fn the_png_path_draws_without_a_seat() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("recovery.png");
        let args: Vec<String> = ["--reason", "x", "--png", path.to_str().unwrap(), "--size", "320x200"].iter().map(|s| s.to_string()).collect();
        main(&args).unwrap();
        assert!(std::fs::metadata(&path).unwrap().len() > 100);
    }
}
