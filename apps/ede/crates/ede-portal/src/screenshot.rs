//! `org.freedesktop.impl.portal.Screenshot`: every output, composed at
//! its layout position into one PNG; `interactive` lets the person drag the
//! area first (the compositor's pick, the Shift+Print one: a click takes
//! the window under the pointer, Escape cancels); PickColor takes the
//! colour of the pixel the person clicks.

use std::{path::PathBuf, sync::Arc};

use ede_protocol::{shot, CaptureTarget, PickMode};
use zbus::{interface, zvariant::OwnedObjectPath};

use crate::{opt, screencast::blocking, value, Ctx, PortalError, Vardict, CANCELLED, ENDED, SUCCESS};

pub struct Screenshot {
    pub ctx: Arc<Ctx>,
}

#[interface(name = "org.freedesktop.impl.portal.Screenshot")]
impl Screenshot {
    async fn screenshot(&self, _handle: OwnedObjectPath, _app_id: String, _parent_window: String, options: Vardict) -> Result<(u32, Vardict), PortalError> {
        let interactive = opt::<bool>(&options, "interactive").unwrap_or(false);
        let ctx = self.ctx.clone();
        let taken = blocking(move || {
            let region = if interactive {
                match pick(&ctx, PickMode::Area)? {
                    Some(r) => Some(r),
                    None => return Ok(None),
                }
            } else {
                None
            };
            take(&ctx, region).map(Some)
        })
        .await;
        match taken {
            Ok(Some(path)) => {
                let uri = format!("file://{}", path.display());
                Ok((SUCCESS, [("uri".to_string(), value(uri))].into()))
            }
            Ok(None) => Ok((CANCELLED, Vardict::new())),
            Err(e) => {
                tracing::warn!("screenshot: {e}");
                Ok((ENDED, Vardict::new()))
            }
        }
    }

    /// The colour under the person's click, as `(ddd)` from 0 to 1.
    async fn pick_color(&self, _handle: OwnedObjectPath, _app_id: String, _parent_window: String, _options: Vardict) -> Result<(u32, Vardict), PortalError> {
        let ctx = self.ctx.clone();
        match blocking(move || pick_color(&ctx)).await {
            Ok(Some((r, g, b))) => Ok((SUCCESS, [("color".to_string(), value(zbus::zvariant::Structure::from((r, g, b))))].into())),
            Ok(None) => Ok((CANCELLED, Vardict::new())),
            Err(e) => {
                tracing::warn!("pick color: {e}");
                Ok((ENDED, Vardict::new()))
            }
        }
    }

    #[zbus(property(emits_changed_signal = "const"), name = "version")]
    fn version(&self) -> u32 {
        2
    }
}

/// The person's pick, on a connection of its own (the shared one stays
/// free for the ScreenCast while the person drags). `None` = cancelled.
fn pick(ctx: &Ctx, mode: PickMode) -> Result<Option<shot::Rect>, String> {
    let mut ede = ctx.connect_new().map_err(|e| e.to_string())?;
    let picked = ede.pick_region(mode).map_err(|e| e.to_string())?;
    Ok(picked.map(|r| shot::Rect { x: r.x, y: r.y, width: r.width, height: r.height }))
}

/// The clicked pixel's colour (its output's capture, at that output's scale).
pub fn pick_color(ctx: &Ctx) -> Result<Option<(f64, f64, f64)>, String> {
    let Some(at) = pick(ctx, PickMode::Point)? else { return Ok(None) };
    let outputs = ctx.ede(|c| c.outputs()).map_err(|e| e.to_string())?;
    let Some(o) = outputs.iter().find(|o| o.is_persons() && at.x >= o.x && at.x < o.x + o.width && at.y >= o.y && at.y < o.y + o.height) else { return Ok(None) };
    let capture = ctx.ede(|c| c.capture(CaptureTarget::Output { name: o.name.clone() })).map_err(|e| e.to_string())?;
    Ok(Some(color_at(o, &capture, at.x, at.y)))
}

/// The capture's pixel under logical `(x, y)` as `(r, g, b)` from 0 to 1.
fn color_at(o: &ede_protocol::OutputInfo, capture: &ede_protocol::Capture, x: i32, y: i32) -> (f64, f64, f64) {
    let ratio = capture.info.width as f64 / o.width.max(1) as f64;
    let px = (((x - o.x) as f64 + 0.5) * ratio) as i32;
    let py = (((y - o.y) as f64 + 0.5) * ratio) as i32;
    let p = capture.at(px.clamp(0, capture.info.width - 1), py.clamp(0, capture.info.height - 1));
    let c = |v: u32| (v & 0xff) as f64 / 255.0;
    (c(p >> 16), c(p >> 8), c(p))
}

/// xdg-desktop-portal asks the permission store before a non-interactive
/// screenshot and, with no entry, shows its Access dialog. For unsandboxed
/// apps (app id `""`) that dialog stands between Flameshot and every
/// capture, and it opens behind the focused window (#EDE-82): the call
/// times out. Cinnamon lets every X11 app take screenshots unasked, so eDE
/// grants unsandboxed apps once, at start, unless the person said no.
/// Sandboxed apps (their own app id) still ask.
pub const PERMISSION_TABLE: &str = "screenshot";

/// Whether the store's `screenshot` entry still needs the host apps' yes.
pub fn host_grant_needed(entry: Option<&std::collections::HashMap<String, Vec<String>>>) -> bool {
    entry.is_none_or(|apps| !apps.contains_key(""))
}

/// [`PERMISSION_TABLE`]: grants unsandboxed apps unless an answer is stored.
pub async fn grant_host_screenshots(conn: &zbus::Connection) -> zbus::Result<bool> {
    const STORE: &str = "org.freedesktop.impl.portal.PermissionStore";
    const STORE_PATH: &str = "/org/freedesktop/impl/portal/PermissionStore";
    let entry = match conn.call_method(Some(STORE), STORE_PATH, Some(STORE), "Lookup", &(PERMISSION_TABLE, PERMISSION_TABLE)).await {
        Ok(reply) => Some(reply.body().deserialize::<(std::collections::HashMap<String, Vec<String>>, zbus::zvariant::OwnedValue)>()?.0),
        Err(zbus::Error::MethodError(name, _, _)) if name.as_str() == "org.freedesktop.portal.Error.NotFound" => None,
        Err(e) => return Err(e),
    };
    if !host_grant_needed(entry.as_ref()) {
        return Ok(false);
    }
    conn.call_method(Some(STORE), STORE_PATH, Some(STORE), "SetPermission", &(PERMISSION_TABLE, true, PERMISSION_TABLE, "", vec!["yes"])).await?;
    Ok(true)
}

/// Captures every output (or the outputs under `region`) and writes the PNG.
pub fn take(ctx: &Ctx, region: Option<shot::Rect>) -> Result<PathBuf, String> {
    let outputs: Vec<_> = ctx.ede(|c| c.outputs()).map_err(|e| e.to_string())?.into_iter().filter(|o| o.is_persons()).collect();
    if outputs.is_empty() {
        return Err("no outputs".into());
    }
    let mut shots = Vec::new();
    for o in &outputs {
        shots.push(ctx.ede(|c| c.capture(CaptureTarget::Output { name: o.name.clone() })).map_err(|e| e.to_string())?);
    }
    let (width, height, rgba) = shot::compose(&outputs, &shots, region).ok_or("nothing to take")?;
    let dir = ctx.config.lock().unwrap().screenshot_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = unique(&dir);
    let file = std::fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&rgba).map_err(|e| e.to_string())?;
    Ok(path)
}

/// `Screenshot 2026-11-01 14-03-07.png` (or `… (2).png`) in local time
/// as the system's `date` sees it.
fn unique(dir: &std::path::Path) -> PathBuf {
    let stamp = std::process::Command::new("date")
        .arg("+%Y-%m-%d %H-%M-%S")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()));
    let mut path = dir.join(format!("Screenshot {stamp}.png"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("Screenshot {stamp} ({n}).png"));
        n += 1;
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use ede_compositor::backend::headless::Harness;

    /// PickColor on a 1.25 screen to the right of another: the click's
    /// logical point lands on that output's own pixel.
    #[test]
    fn a_picked_colour_is_the_pixel_under_the_click() {
        let o = ede_protocol::OutputInfo { name: "DP-2".into(), x: 3840, y: 0, width: 2048, height: 1152, scale: 1.25, modes: vec![], make: String::new(), model: String::new(), physical_mm: [0, 0], space: None };
        let (w, h) = (2560, 1440);
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        let at = |x: i32, y: i32| ((y * w + x) * 4) as usize;
        // B, G, R, A in memory: 0xff336699 at the pixel under logical (3940, 40).
        pixels[at(125, 50)..at(125, 50) + 4].copy_from_slice(&[0x99, 0x66, 0x33, 0xff]);
        let capture = ede_protocol::Capture { info: ede_protocol::CaptureInfo { width: w, height: h, stride: w * 4, format: "argb8888".into(), serial: 1, unchanged: false, notice: None }, pixels };
        let (r, g, b) = color_at(&o, &capture, 3940, 40);
        assert_eq!(((r * 255.0).round(), (g * 255.0).round(), (b * 255.0).round()), (51.0, 102.0, 153.0));
    }

    #[test]
    fn unsandboxed_apps_are_granted_once_and_a_stored_answer_stays() {
        let entry = |pairs: &[(&str, &str)]| pairs.iter().map(|(k, v)| (k.to_string(), vec![v.to_string()])).collect::<std::collections::HashMap<_, _>>();
        assert!(host_grant_needed(None), "no table yet");
        assert!(host_grant_needed(Some(&entry(&[("org.example.App", "no")]))), "a sandboxed app's answer is its own");
        assert!(!host_grant_needed(Some(&entry(&[("", "no")]))), "the person said no");
        assert!(!host_grant_needed(Some(&entry(&[("", "yes")]))));
    }

    /// #EDE-101: a scale-2 output beside a scale-1 one. Captures are mode
    /// pixels, the layout is logical: the PNG is drawn at the highest scale
    /// so neither screen covers the other.
    #[test]
    fn a_scaled_output_beside_another_does_not_overlap_it() {
        let h = Harness::start(ede_compositor::Config::default(), &[(800, 600), (800, 600)]);
        h.with(|ede| {
            ede.set_output("HEADLESS-1", &ede_compositor::outputs::OutputChange { scale: Some(2.0), ..Default::default() }).unwrap();
            ede.set_output("HEADLESS-2", &ede_compositor::outputs::OutputChange { position: Some((400, 0)), ..Default::default() }).unwrap();
        });
        let dir = tempfile::tempdir().unwrap();
        let runtime = dir.path().to_path_buf();
        let socket = h.with(move |ede| ede.ipc_listen(&runtime, "wayland-test")).unwrap();
        let token = ede_protocol::Token::new(std::fs::read_to_string(dir.path().join("exponential-desktop/wayland-test.token")).unwrap()).unwrap();
        let config = dir.path().join("portal.toml");
        std::fs::write(&config, format!("[screenshot]\ndir = \"{}\"\n", dir.path().join("shots").display())).unwrap();
        let ctx = Ctx::new(Arc::new(move || ede_protocol::Client::connect(&socket, token.clone())), Some(config));
        let outputs = ctx.ede(|c| c.outputs()).unwrap();
        assert_eq!(outputs.iter().map(|o| (o.x, o.width)).collect::<Vec<_>>(), [(0, 400), (400, 800)]);
        let left = ctx.ede(|c| c.capture(CaptureTarget::Output { name: "HEADLESS-1".into() })).unwrap();
        let right = ctx.ede(|c| c.capture(CaptureTarget::Output { name: "HEADLESS-2".into() })).unwrap();

        let file = take(&ctx, None).unwrap();
        let mut reader = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(file).unwrap())).read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height), (2400, 1200), "the right screen at 2x beside the left one's 800 px");
        let px = |x: u32, y: u32| -> [u8; 3] {
            let i = ((y * info.width + x) * 4) as usize;
            [buf[i], buf[i + 1], buf[i + 2]]
        };
        let rgb = |p: u32| [(p >> 16) as u8, (p >> 8) as u8, p as u8];
        for (x, y) in [(0, 0), (799, 599), (420, 300)] {
            assert_eq!(px(x, y), rgb(left.at(x as i32, y as i32)), "the left screen at ({x}, {y})");
        }
        for (x, y) in [(0, 0), (799, 599), (20, 300)] {
            assert_eq!(px(800 + 2 * x, 2 * y), rgb(right.at(x as i32, y as i32)), "the right screen at ({x}, {y})");
        }
    }
}
