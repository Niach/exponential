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
//! exponential-ctl pointer <x> <y> [--click|--press|--release] [--bindings]  pointer motion, optionally a left click, or one edge of it for a drag (--bindings = as the person's)
//! exponential-ctl minimize <id> | restore <id>                         into the taskbar and back (#EDE-48)
//! exponential-ctl space create [WxH] [--session NAME]    an agent space (#EDE-126): mints a space grant, makes its space, prints it with the grant's token
//! exponential-ctl space list | space destroy <id>        the spaces (all of them with the root token); close one
//! exponential-ctl launch [--cwd DIR] [--env K=V]… -- ARGV…   start a program in the space of the token in $EDE_TOKEN
//! exponential-ctl capture --space ID --png FILE          a space's screen (the root token, or the space's own; with
//!                                                       the space's own, a notice for its agent comes along)
//! ```
//!
//! With a space's token in `$EDE_TOKEN` the other commands act in that
//! space: `windows`, `outputs`, `capture`, `key`, `pointer`.
//!
//! An agent in the VM uses this instead of QMP's screendump and
//! input-send-event: it only ever reaches the nested or headless eDE, never
//! the desktop around it.

use std::{fs::File, io::BufWriter, path::PathBuf};

use ede_protocol::{Capability, CaptureTarget, Client, InputEvent, SessionId, SpaceId, WindowId};

fn usage() -> ! {
    eprintln!("usage: exponential-ctl windows | outputs | capture [--output NAME | --window ID | --space ID] --png FILE [--expect-drawn] | space create [WxH] [--session NAME] | space list | space destroy ID | launch [--cwd DIR] [--env K=V] -- ARGV | key CODE [--pressed|--released] [--bindings] | pointer X Y [--click|--press|--release] [--bindings] | minimize ID | restore ID");
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
                    "--space" => target = Some(CaptureTarget::Space { space: SpaceId::new(args.next().unwrap_or_else(|| usage())).map_err(|e| format!("not a space id: {e:?}"))? }),
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
            // #EDE-162: what the space's agent is told with this capture (a
            // space's own token only), when there is something.
            match &capture.info.notice {
                Some(notice) => println!("{{\"width\":{w},\"height\":{h},\"uniform\":{uniform},\"notice\":{}}}", serde_json::to_string(notice)?),
                None => println!("{{\"width\":{w},\"height\":{h},\"uniform\":{uniform}}}"),
            }
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
            let click = rest.iter().any(|a| a == "--click");
            if click || rest.iter().any(|a| a == "--press") {
                send(InputEvent::PointerButton { button: 0x110, pressed: true })?;
            }
            if click || rest.iter().any(|a| a == "--release") {
                send(InputEvent::PointerButton { button: 0x110, pressed: false })?;
            }
        }
        "space" => match args.next().as_deref() {
            Some("create") => {
                let (mut size, mut session) = ((1280, 800), format!("exponential-ctl-{}", std::process::id()));
                while let Some(a) = args.next() {
                    match a.as_str() {
                        "--session" => session = args.next().unwrap_or_else(|| usage()),
                        wxh => {
                            let (w, h) = wxh.split_once('x').unwrap_or_else(|| usage());
                            size = (w.parse()?, h.parse()?);
                        }
                    }
                }
                if !ede.hello.offers_spaces() {
                    return Err("this compositor makes no agent spaces".into());
                }
                // The space is its grant's: it ends when the grant is revoked
                // (`space destroy`), or with the compositor.
                let grant = ede.mint(SessionId::new(session).map_err(|e| format!("not a session name: {e:?}"))?, vec![Capability::Space], None)?;
                let space = Client::connect_with_token(grant.token.clone())?.create_space(size.0, size.1)?;
                let mut out = serde_json::to_value(&space)?;
                out["token"] = serde_json::json!(grant.token.as_str());
                println!("{out}");
            }
            Some("list") => {
                for space in ede.spaces()? {
                    println!("{}", serde_json::to_string(&space)?);
                }
            }
            Some("destroy") => ede.destroy_space(SpaceId::new(args.next().unwrap_or_else(|| usage())).map_err(|e| format!("not a space id: {e:?}"))?)?,
            _ => usage(),
        },
        "launch" => {
            let (mut cwd, mut env) = (None, std::collections::BTreeMap::new());
            loop {
                match args.next().as_deref() {
                    Some("--cwd") => cwd = Some(args.next().unwrap_or_else(|| usage())),
                    Some("--env") => {
                        let pair = args.next().unwrap_or_else(|| usage());
                        let (name, value) = pair.split_once('=').unwrap_or_else(|| usage());
                        env.insert(name.to_string(), value.to_string());
                    }
                    Some("--") => break,
                    _ => usage(),
                }
            }
            let pid = ede.launch(args.collect(), env, cwd)?;
            println!("{{\"pid\":{pid}}}");
        }
        "minimize" | "restore" => {
            let id = WindowId::new(args.next().unwrap_or_else(|| usage())).map_err(|e| format!("not a window id: {e:?}"))?;
            if cmd == "minimize" { ede.minimize(id)? } else { ede.restore(id)? }
        }
        _ => usage(),
    }
    Ok(())
}
