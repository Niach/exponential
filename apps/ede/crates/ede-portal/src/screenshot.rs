//! `org.freedesktop.impl.portal.Screenshot`: every output, composed at
//! its layout position into one PNG.

use std::{path::PathBuf, sync::Arc};

use ede_protocol::CaptureTarget;
use zbus::{interface, zvariant::OwnedObjectPath};

use crate::{value, Ctx, PortalError, Vardict, ENDED, SUCCESS};

pub struct Screenshot {
    pub ctx: Arc<Ctx>,
}

#[interface(name = "org.freedesktop.impl.portal.Screenshot")]
impl Screenshot {
    async fn screenshot(&self, _handle: OwnedObjectPath, _app_id: String, _parent_window: String, _options: Vardict) -> Result<(u32, Vardict), PortalError> {
        // `interactive` (pick an area) waits for the shell (#NEW-108); the
        // whole screen is what every caller can use meanwhile.
        match take(&self.ctx) {
            Ok(path) => {
                let uri = format!("file://{}", path.display());
                Ok((SUCCESS, [("uri".to_string(), value(uri))].into()))
            }
            Err(e) => {
                tracing::warn!("screenshot: {e}");
                Ok((ENDED, Vardict::new()))
            }
        }
    }

    /// Picking needs a crosshair from the shell (#NEW-108).
    async fn pick_color(&self, _handle: OwnedObjectPath, _app_id: String, _parent_window: String, _options: Vardict) -> Result<(u32, Vardict), PortalError> {
        Ok((ENDED, Vardict::new()))
    }

    #[zbus(property(emits_changed_signal = "const"), name = "version")]
    fn version(&self) -> u32 {
        2
    }
}

/// Captures every output and writes the PNG.
pub fn take(ctx: &Ctx) -> Result<PathBuf, String> {
    let outputs = ctx.ede(|c| c.outputs()).map_err(|e| e.to_string())?;
    if outputs.is_empty() {
        return Err("no outputs".into());
    }
    let mut shots = Vec::new();
    for o in &outputs {
        shots.push(ctx.ede(|c| c.capture(CaptureTarget::Output { name: o.name.clone() })).map_err(|e| e.to_string())?);
    }
    let (width, height, rgba) = compose(&outputs, &shots);
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

/// The outputs' captures as one RGBA image. A capture is the output's mode
/// in pixels while the layout is logical (#EDE-101): the image is drawn at
/// the highest scale, each output at its logical position times that scale,
/// a lower-scaled one stretched to its place.
fn compose(outputs: &[ede_protocol::OutputInfo], shots: &[ede_protocol::Capture]) -> (usize, usize, Vec<u8>) {
    let ratio = |o: &ede_protocol::OutputInfo, s: &ede_protocol::Capture| s.info.width as f64 / o.width.max(1) as f64;
    let k = outputs.iter().zip(shots).map(|(o, s)| ratio(o, s)).fold(f64::MIN_POSITIVE, f64::max);
    let x0 = outputs.iter().map(|o| o.x).min().unwrap_or(0);
    let y0 = outputs.iter().map(|o| o.y).min().unwrap_or(0);
    let places: Vec<(usize, usize, usize, usize)> = outputs
        .iter()
        .zip(shots)
        .map(|(o, s)| {
            let at = |v: i32| (v as f64 * k).round() as usize;
            let size = if ratio(o, s) == k { (s.info.width as usize, s.info.height as usize) } else { (at(o.width), at(o.height)) };
            (at(o.x - x0), at(o.y - y0), size.0, size.1)
        })
        .collect();
    let width = places.iter().map(|(x, _, w, _)| x + w).max().unwrap_or(0);
    let height = places.iter().map(|(_, y, _, h)| y + h).max().unwrap_or(0);
    let mut rgba = vec![0u8; width * height * 4];
    for ((x, y, w, h), shot) in places.iter().zip(shots) {
        let (sw, sh) = (shot.info.width as usize, shot.info.height as usize);
        for row in 0..*h {
            let sy = (row * sh / h).min(sh.saturating_sub(1));
            for col in 0..*w {
                let p = shot.at(((col * sw / w).min(sw.saturating_sub(1))) as i32, sy as i32);
                let i = ((y + row) * width + x + col) * 4;
                rgba[i..i + 4].copy_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8, 0xff]);
            }
        }
    }
    (width, height, rgba)
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

        let file = take(&ctx).unwrap();
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
