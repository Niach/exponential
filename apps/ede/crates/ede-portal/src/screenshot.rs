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
    let x0 = outputs.iter().map(|o| o.x).min().unwrap();
    let y0 = outputs.iter().map(|o| o.y).min().unwrap();
    let mut shots = Vec::new();
    for o in &outputs {
        let shot = ctx.ede(|c| c.capture(CaptureTarget::Output { name: o.name.clone() })).map_err(|e| e.to_string())?;
        shots.push((o.x - x0, o.y - y0, shot));
    }
    let width = shots.iter().map(|(x, _, s)| x + s.info.width).max().unwrap() as usize;
    let height = shots.iter().map(|(_, y, s)| y + s.info.height).max().unwrap() as usize;
    let mut rgba = vec![0u8; width * height * 4];
    for (x, y, shot) in &shots {
        for row in 0..shot.info.height as usize {
            for col in 0..shot.info.width as usize {
                let p = shot.at(col as i32, row as i32);
                let i = ((*y as usize + row) * width + *x as usize + col) * 4;
                rgba[i..i + 4].copy_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8, 0xff]);
            }
        }
    }
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
