//! `exponential-ctl`: drive a running `exponential-compositor` over ede-protocol (#EDE-7).
//! `$WAYLAND_DISPLAY` names the socket and the root token file (or
//! `$EDE_TOKEN`) grants every capability, once the person allowed this
//! process (below).
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
//! exponential-ctl session                                the commands above from stdin, one per line, on ONE
//!                                                       connection (#EDE-172 P7): one question to the person for all
//! ```
//!
//! #EDE-172: in a real Exponential Desktop session (the gate, `[control]
//! consent`, on unless the file says `false`), the
//! person is asked once per `exponential-ctl` process whether it may control
//! the desktop: a script that runs many commands runs them through one
//! `exponential-ctl session` and is asked once. A VM or a CI machine names
//! `/usr/bin/exponential-ctl` in the root-owned policy file
//! (`/etc/exponential-desktop/control.toml`, `principal = "root"`) and is
//! never asked; `--bindings` needs that.
//!
//! With a space's token in `$EDE_TOKEN` the other commands act in that
//! space: `windows`, `outputs`, `capture`, `key`, `pointer`.
//!
//! An agent in the VM uses this instead of QMP's screendump and
//! input-send-event: it only ever reaches the nested or headless eDE, never
//! the desktop around it.

use std::{
    fs::File,
    io::{BufRead, BufWriter},
    path::PathBuf,
    process::ExitCode,
};

use ede_protocol::{Capability, CaptureTarget, Client, ErrorCode, InputEvent, SessionId, SpaceId, WindowId};

const USAGE: &str = "usage: exponential-ctl windows | outputs | capture [--output NAME | --window ID | --space ID] --png FILE [--expect-drawn] | space create [WxH] [--session NAME] | space list | space destroy ID | launch [--cwd DIR] [--env K=V] -- ARGV | key CODE [--pressed|--released] [--bindings] | pointer X Y [--click|--press|--release] [--bindings] | minimize ID | restore ID | session (those commands from stdin, one per line)";

/// A command that does not parse.
#[derive(Debug)]
struct Usage;

impl std::fmt::Display for Usage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(USAGE)
    }
}

impl std::error::Error for Usage {}

/// A capture that `--expect-drawn` found empty (exit 1, said on stderr).
#[derive(Debug)]
struct NothingDrawn(u32);

impl std::fmt::Display for NothingDrawn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "nothing drawn: every pixel is {:#010x}", self.0)
    }
}

impl std::error::Error for NothingDrawn {}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// How long the hello waits: the person may be reading the consent prompt.
const HELLO_WITHIN: std::time::Duration = std::time::Duration::from_secs(120);

/// The next argument, or [`Usage`].
fn next(args: &mut impl Iterator<Item = String>) -> Result<String> {
    args.next().ok_or_else(|| Usage.into())
}

/// A session line's words: whitespace splits, `'…'` and `"…"` hold
/// spaces (a path), `\` takes the next character as it is.
fn words(line: &str) -> std::result::Result<Vec<String>, String> {
    let (mut out, mut word, mut quote, mut any) = (Vec::new(), String::new(), None::<char>, false);
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (_, '\\') => {
                word.push(chars.next().ok_or("a line ends in \\")?);
                any = true;
            }
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => word.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                any = true;
            }
            (None, c) if c.is_whitespace() => {
                if any {
                    out.push(std::mem::take(&mut word));
                    any = false;
                }
            }
            (None, c) => {
                word.push(c);
                any = true;
            }
        }
    }
    if quote.is_some() {
        return Err("a quote is not closed".into());
    }
    if any {
        out.push(word);
    }
    Ok(out)
}

/// `argb8888` as the compositor sends it (little-endian words: B, G, R, A
/// bytes) to PNG's RGBA.
fn write_png(path: &PathBuf, width: u32, height: u32, bgra: &[u8]) -> Result<()> {
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

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    // #EDE-172: with the gate on, the compositor answers this hello once
    // the person answered its question: give them the time to read it.
    let mut ede = match Client::connect_env_waiting(HELLO_WITHIN) {
        Ok(ede) => ede,
        Err(e) => {
            eprintln!("exponential-ctl: {e}");
            if refused(&e) {
                eprintln!("exponential-ctl: Exponential Desktop did not let exponential-ctl in (the person said Don't allow, or this is not the installed /usr/bin/exponential-ctl). A test machine names it in /etc/exponential-desktop/control.toml.");
            }
            return ExitCode::FAILURE;
        }
    };
    if cmd == "session" && args.len() == 1 {
        return session(&mut ede);
    }
    match run(&mut ede, args.into_iter()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.is::<Usage>() => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("exponential-ctl: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The compositor refused this process its root `hello` (#EDE-172).
fn refused(e: &ede_protocol::Error) -> bool {
    matches!(e, ede_protocol::Error::Failed(f) if f.code == ErrorCode::Unauthorized)
}

/// `exponential-ctl session` (#EDE-172 P7): every line of stdin is a
/// command, run on this process's one connection, so the person is asked
/// once for all of them. A line that fails says so on stderr (`line N:`)
/// and the next one runs; the exit status is 1 if any failed. Empty lines
/// and `#` comments are skipped.
fn session(ede: &mut Client) -> ExitCode {
    let mut failed = false;
    for (n, line) in std::io::stdin().lock().lines().enumerate() {
        let Ok(line) = line else { break };
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let outcome = words(trimmed).map_err(|e| -> Box<dyn std::error::Error> { e.into() }).and_then(|args| if args.first().map(String::as_str) == Some("session") { Err(Usage.into()) } else { run(ede, args.into_iter()) });
        if let Err(e) = outcome {
            eprintln!("exponential-ctl: line {}: {e}", n + 1);
            failed = true;
        }
        use std::io::Write;
        let _ = std::io::stdout().flush();
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// One command (`args`: its name, then its arguments) on `ede`.
fn run(ede: &mut Client, mut args: impl Iterator<Item = String>) -> Result<()> {
    let cmd = next(&mut args)?;
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
                    "--output" => target = Some(CaptureTarget::Output { name: next(&mut args)? }),
                    "--window" => target = Some(CaptureTarget::Window { window: WindowId::new(next(&mut args)?).map_err(|e| format!("not a window id: {e:?}"))? }),
                    "--space" => target = Some(CaptureTarget::Space { space: SpaceId::new(next(&mut args)?).map_err(|e| format!("not a space id: {e:?}"))? }),
                    "--png" => png = Some(next(&mut args)?.into()),
                    "--expect-drawn" => expect_drawn = true,
                    _ => return Err(Usage.into()),
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
                return Err(NothingDrawn(first).into());
            }
        }
        "key" => {
            let key: u32 = next(&mut args)?.parse()?;
            let rest: Vec<String> = args.collect();
            let bindings = rest.iter().any(|a| a == "--bindings");
            let edge = rest.iter().find(|a| *a == "--pressed" || *a == "--released").cloned();
            let both = edge.is_none();
            let mut send = |pressed: bool| -> std::result::Result<(), ede_protocol::Error> {
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
            let x: f64 = next(&mut args)?.parse()?;
            let y: f64 = next(&mut args)?.parse()?;
            let rest: Vec<String> = args.collect();
            let bindings = rest.iter().any(|a| a == "--bindings");
            let mut send = |event: InputEvent| -> std::result::Result<(), ede_protocol::Error> { if bindings { ede.input_as_person(event) } else { ede.input(event) } };
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
                        "--session" => session = next(&mut args)?,
                        wxh => {
                            let (w, h) = wxh.split_once('x').ok_or(Usage)?;
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
            Some("destroy") => ede.destroy_space(SpaceId::new(next(&mut args)?).map_err(|e| format!("not a space id: {e:?}"))?)?,
            _ => return Err(Usage.into()),
        },
        "launch" => {
            let (mut cwd, mut env) = (None, std::collections::BTreeMap::new());
            loop {
                match args.next().as_deref() {
                    Some("--cwd") => cwd = Some(next(&mut args)?),
                    Some("--env") => {
                        let pair = next(&mut args)?;
                        let (name, value) = pair.split_once('=').ok_or(Usage)?;
                        env.insert(name.to_string(), value.to_string());
                    }
                    Some("--") => break,
                    _ => return Err(Usage.into()),
                }
            }
            let pid = ede.launch(args.collect(), env, cwd)?;
            println!("{{\"pid\":{pid}}}");
        }
        "minimize" | "restore" => {
            let id = WindowId::new(next(&mut args)?).map_err(|e| format!("not a window id: {e:?}"))?;
            if cmd == "minimize" { ede.minimize(id)? } else { ede.restore(id)? }
        }
        _ => return Err(Usage.into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::words;

    /// A session line splits as a shell would split a simple one.
    #[test]
    fn a_session_line_splits_into_its_words() {
        assert_eq!(words("  capture --output  Virtual-1 --png /tmp/a.png ").unwrap(), ["capture", "--output", "Virtual-1", "--png", "/tmp/a.png"]);
        assert_eq!(words("capture --png '/tmp/my shot.png'").unwrap(), ["capture", "--png", "/tmp/my shot.png"]);
        assert_eq!(words(r#"launch --env "A=b c" -- foot"#).unwrap(), ["launch", "--env", "A=b c", "--", "foot"]);
        assert_eq!(words(r"capture --png a\ b.png").unwrap(), ["capture", "--png", "a b.png"]);
        assert_eq!(words("key 30 ''").unwrap(), ["key", "30", ""]);
        assert!(words("capture --png 'open").is_err());
        assert!(words("").unwrap().is_empty());
    }
}
