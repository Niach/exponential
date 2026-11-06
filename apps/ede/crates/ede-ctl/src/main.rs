//! `exponential-ctl`: drive a running `exponential-compositor` over ede-protocol (#EDE-7).
//! It connects like the shell does: `$WAYLAND_DISPLAY` names the socket and
//! the root token file (or `$EDE_TOKEN`) grants every capability.
//!
//! ```text
//! exponential-ctl windows                          the open windows as JSON lines
//! exponential-ctl outputs                          the outputs
//! exponential-ctl capture [--output NAME | --window ID] --png FILE   a capture as a PNG
//! exponential-ctl capture --png FILE --expect-drawn                  …and fail unless something is drawn
//! exponential-ctl key <keycode> [--pressed|--released] [--bindings]  a key by evdev code (both edges by default; --bindings = as the person's, through Alt+Tab, Super, the media keys)
//! exponential-ctl pointer <x> <y> [--click] [--bindings]              pointer motion, optionally a left click (--bindings = as the person's)
//! exponential-ctl minimize <id> | restore <id>                         into the taskbar and back (#EDE-48)
//! ```
//!
//! An agent in the VM uses this instead of QMP's screendump and
//! input-send-event: it only ever reaches the nested or headless eDE, never
//! the desktop around it.

use std::{fs::File, io::BufWriter, path::PathBuf};

use ede_protocol::{CaptureTarget, Client, InputEvent, WindowId};

fn usage() -> ! {
    eprintln!("usage: exponential-ctl windows | outputs | capture [--output NAME | --window ID] --png FILE [--expect-drawn] | key CODE [--pressed|--released] [--bindings] | pointer X Y [--click] [--bindings] | minimize ID | restore ID");
    std::process::exit(2)
}

/// `argb8888` as the compositor sends it (little-endian words: B, G, R, A
/// bytes) to PNG's RGBA.
fn write_png(path: &PathBuf, width: u32, height: u32, bgra: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut rgba = Vec::with_capacity(bgra.len());
    for px in bgra.chunks_exact(4) {
        rgba.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
    }
    let file = File::create(path)?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&rgba)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let Some(cmd) = args.next() else { usage() };
    let mut ede = Client::connect_env()?;
    match cmd.as_str() {
        "windows" => {
            for w in ede.windows()? {
                println!("{}", serde_json::to_string(&w)?);
            }
        }
        "outputs" => {
            for o in ede.outputs()? {
                println!("{}", serde_json::to_string(&o)?);
            }
        }
        "capture" => {
            let mut target: Option<CaptureTarget> = None;
            let mut png: Option<PathBuf> = None;
            let mut expect_drawn = false;
            while let Some(a) = args.next() {
                match a.as_str() {
                    "--output" => target = Some(CaptureTarget::Output { name: args.next().unwrap_or_else(|| usage()) }),
                    "--window" => target = Some(CaptureTarget::Window { window: WindowId::new(args.next().unwrap_or_else(|| usage())).map_err(|e| format!("not a window id: {e:?}"))? }),
                    "--png" => png = Some(args.next().unwrap_or_else(|| usage()).into()),
                    "--expect-drawn" => expect_drawn = true,
                    _ => usage(),
                }
            }
            let target = match target {
                Some(t) => t,
                None => CaptureTarget::Output { name: ede.outputs()?.first().ok_or("no output")?.name.clone() },
            };
            let capture = ede.capture(target)?;
            let (w, h) = (capture.info.width, capture.info.height);
            let first = capture.at(0, 0);
            let uniform = (0..h).all(|y| (0..w).all(|x| capture.at(x, y) == first));
            println!("{{\"width\":{w},\"height\":{h},\"uniform\":{uniform}}}");
            if let Some(path) = png {
                write_png(&path, w as u32, h as u32, &capture.pixels)?;
            }
            if expect_drawn && uniform {
                eprintln!("nothing drawn: every pixel is {first:#010x}");
                std::process::exit(1);
            }
        }
        "key" => {
            let key: u32 = args.next().unwrap_or_else(|| usage()).parse()?;
            let rest: Vec<String> = args.collect();
            let bindings = rest.iter().any(|a| a == "--bindings");
            let edge = rest.iter().find(|a| *a == "--pressed" || *a == "--released").cloned();
            let both = edge.is_none();
            let mut send = |pressed: bool| -> Result<(), ede_protocol::Error> {
                let event = InputEvent::Key { key, pressed };
                if bindings { ede.input_as_person(event) } else { ede.input(event) }
            };
            if both || edge.as_deref() == Some("--pressed") {
                send(true)?;
            }
            if both || edge.as_deref() == Some("--released") {
                send(false)?;
            }
        }
        "pointer" => {
            let x: f64 = args.next().unwrap_or_else(|| usage()).parse()?;
            let y: f64 = args.next().unwrap_or_else(|| usage()).parse()?;
            let rest: Vec<String> = args.collect();
            let bindings = rest.iter().any(|a| a == "--bindings");
            let mut send = |event: InputEvent| -> Result<(), ede_protocol::Error> { if bindings { ede.input_as_person(event) } else { ede.input(event) } };
            send(InputEvent::PointerMotion { x, y })?;
            if rest.iter().any(|a| a == "--click") {
                send(InputEvent::PointerButton { button: 0x110, pressed: true })?;
                send(InputEvent::PointerButton { button: 0x110, pressed: false })?;
            }
        }
        "minimize" | "restore" => {
            let id = WindowId::new(args.next().unwrap_or_else(|| usage())).map_err(|e| format!("not a window id: {e:?}"))?;
            if cmd == "minimize" { ede.minimize(id)? } else { ede.restore(id)? }
        }
        _ => usage(),
    }
    Ok(())
}
