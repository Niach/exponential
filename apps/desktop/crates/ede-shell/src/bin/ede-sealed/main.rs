//! `exponential-sealed` (#EDE-17, Tier B): the surfaces a document on disk must
//! never be, compiled into this binary (`apps/ede/sealed/*.json`, never
//! agent-editable; only the theme's accent is anyone's), each a
//! full-screen overlay layer surface with the keyboard to itself:
//!
//! - `exponential-sealed lock`: the lock screen, one surface per output
//!   (it follows outputs coming, going and changing size). The compositor
//!   starts it (no `[lock] command`), shows nothing but this process's
//!   surfaces while
//!   locked, and takes `unlock` only with the grant it minted for this
//!   process, handed over the descriptor `EDE_TOKEN_FD` names (never the
//!   environment, which `/proc` shows). The password goes to PAM in this
//!   process.
//! - `exponential-sealed polkit`: the polkit authentication agent for the session
//!   (`exponential-polkit-agent.service`): polkitd's `BeginAuthentication` opens
//!   the prompt, polkit's helper checks the password; it stays up between prompts,
//!   its renderer up from its start (#EDE-184).
//!   A request from an agent space is refused and opens nothing (#EDE-127);
//!   one from outside the person's login session opens the prompt without
//!   the keyboard (on demand: the person clicks into it) and names the
//!   asking process (#EDE-143).
//! - `exponential-sealed consent --title T --body B [--allow A] [--deny D] [--size WxH]`: a
//!   question to the person; prints `allow` or `deny`, exits 0 or 1.
//!   `--size` (#EDE-172 P7): the logical size of the output it covers, from
//!   its caller; with it, consent (and the share chooser, whose ask says
//!   its `size`) asks the compositor nothing.
//! - `exponential-sealed share`: the screen-share chooser the portal runs
//!   (#EDE-15): the ask as JSON on stdin (`app_id`, `types`, `multiple`,
//!   `remote_control`, `outputs`, `windows`, `selected` = the rows a
//!   restored share starts with), the picks on stdout
//!   (`output:NAME`, `window:ID`, and `allow` for remote control).
//!
//! Drawn with the same gpui + eui-gpui the shell uses, the same tokens;
//! the compositor marks every layer surface of this executable as
//! protected (black in captures, never driven, no input method).

#[path = "../../assets.rs"]
mod assets;
mod control;
mod pam;
mod polkit;

use std::{
    cell::RefCell,
    rc::Rc,
    sync::mpsc,
    time::Duration,
};

use eui_core::{surface::{SurfaceCommand, SurfaceSettings}, theme::{Mode, ModeSetting}, NestedNode};
use eui_gpui::host::{FunctionCallEvent, HostPlugin, InputEvent};
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{div, layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions}, point, prelude::*, px, size, App, Bounds, DisplayId, Entity, Pixels, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions};
use serde_json::{json, Value};

const LOCK: &str = include_str!("../../../../../../ede/sealed/lock.json");
const POLKIT: &str = include_str!("../../../../../../ede/sealed/polkit.json");
const CONSENT: &str = include_str!("../../../../../../ede/sealed/consent.json");
const SHARE: &str = include_str!("../../../../../../ede/sealed/share.json");

struct Surface {
    view: Entity<SurfaceView>,
}

impl Render for Surface {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div().size_full().child(self.view.clone())
    }
}

/// What the person did in a surface.
#[derive(Debug)]
enum Act {
    Call { name: String, args: Value },
    Input { path: String, value: Value },
}

struct Host {
    acts: mpsc::Sender<Act>,
}

impl HostPlugin for Host {
    fn icon(&self, name: &str) -> Option<gpui::SharedString> {
        assets::icon_path(name)
    }
    fn on_function_call(&self, event: &FunctionCallEvent, _cx: &mut App) {
        let _ = self.acts.send(Act::Call { name: event.name.clone(), args: event.args.clone() });
    }
    fn on_input(&self, event: &InputEvent, _cx: &mut App) {
        if let Some(path) = &event.path {
            let _ = self.acts.send(Act::Input { path: path.clone(), value: event.value.clone() });
        }
    }
}

struct Open {
    handle: WindowHandle<Surface>,
    view: Entity<SurfaceView>,
}

/// A sealed document as a full-screen overlay with the keyboard, on the
/// first output, whose logical size is `size` when the caller said it.
fn open(doc: &str, data: Value, acts: mpsc::Sender<Act>, size: Option<(f32, f32)>, cx: &mut App) -> Option<Open> {
    open_taking(doc, data, acts, true, size, cx)
}

/// [`open`]; `keyboard` = the prompt takes the keyboard as it shows
/// (Exclusive), else on the person's click (OnDemand: #EDE-143, a polkit
/// request from outside their session). #EDE-172 (P7): a prompt whose
/// caller said its size (consent, the share chooser) asks the compositor
/// nothing; one that did not (the polkit agent) asks on its channel.
fn open_taking(doc: &str, data: Value, acts: mpsc::Sender<Act>, keyboard: bool, size: Option<(f32, f32)>, cx: &mut App) -> Option<Open> {
    let (w, h) = size
        .or_else(|| control::with(|c| c.outputs(), ede_protocol::Client::connect_env).ok().and_then(|o| o.into_iter().find(|o| o.is_persons()).map(|o| (o.width as f32, o.height as f32))))
        .unwrap_or((1280.0, 720.0));
    open_on(doc, data, acts, None, (w, h), Layer::Overlay, if keyboard { KeyboardInteractivity::Exclusive } else { KeyboardInteractivity::OnDemand }, cx)
}

/// [`open`] on `display` (`None` = the compositor's choice), whose logical
/// size is `w`×`h`: the root fills it in px (a layer window takes its
/// content's size otherwise) and the surface asks for it, on `layer`.
fn open_on(doc: &str, data: Value, acts: mpsc::Sender<Act>, display: Option<DisplayId>, (w, h): (f32, f32), layer: Layer, keyboard: KeyboardInteractivity, cx: &mut App) -> Option<Open> {
    let mut parsed: Value = serde_json::from_str(doc).expect("a sealed document");
    if let Some(style) = parsed["root"].get_mut("style").and_then(Value::as_object_mut) {
        style.insert("width".into(), json!(w));
        style.insert("height".into(), json!(h));
    }
    let tree: NestedNode = serde_json::from_value(parsed["root"].clone()).expect("the sealed document's tree");
    let mut initial = parsed.get("data").cloned().unwrap_or(json!({}));
    if let (Some(base), Some(over)) = (initial.as_object_mut(), data.as_object()) {
        for (k, v) in over {
            base.insert(k.clone(), v.clone());
        }
    }
    let theme = eui_core::themes::builtin_theme("exponential").expect("the exponential theme");
    let host: Rc<dyn HostPlugin> = Rc::new(Host { acts });
    let kind = WindowKind::LayerShell(LayerShellOptions {
        namespace: "exponential-sealed".into(),
        layer,
        anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
        exclusive_zone: Some(px(-1.0)),
        exclusive_edge: None,
        margin: None,
        keyboard_interactivity: keyboard,
    });
    let view_out: Rc<RefCell<Option<Entity<SurfaceView>>>> = Rc::new(RefCell::new(None));
    let out = view_out.clone();
    let opened = cx.open_window(
        WindowOptions {
            kind,
            window_bounds: Some(WindowBounds::Windowed(Bounds { origin: point(px(0.0), px(0.0)), size: size(px(w), px(h)) })),
            titlebar: None,
            display_id: display,
            app_id: Some("at.exponential.ede.sealed".into()),
            focus: true,
            is_resizable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        },
        move |window, cx| {
            let view = cx.new(|cx| {
                let mut v = SurfaceView::new(
                    SurfaceViewOptions { surface_id: "sealed".into(), theme: Some(theme), mode: Mode::Dark, host, settings: Some(SurfaceSettings { locale: "en-US".into(), mode: ModeSetting::Dark, ..SurfaceSettings::default() }), ..Default::default() },
                    window,
                    cx,
                );
                if let Some(obj) = initial.as_object() {
                    for (k, val) in obj {
                        let _ = v.set_data(&format!("/{k}"), Some(val.clone()), cx);
                    }
                }
                v.set_nested(tree.clone(), cx);
                v.command(&SurfaceCommand::Focus { id: "password".into() }, cx);
                v
            });
            *out.borrow_mut() = Some(view.clone());
            cx.new(|_| Surface { view })
        },
    );
    let open = match opened {
        Ok(handle) => view_out.borrow().clone().map(|view| Open { handle, view }),
        Err(e) => {
            tracing::error!(error = %e, "a sealed window could not open");
            None
        }
    };
    if let Some(o) = open.as_ref() {
        focus(o, "password", cx);
    }
    open
}

fn set(open: &Open, path: &str, value: Value, cx: &mut App) {
    let p = path.to_string();
    let _ = open.handle.update(cx, |_, _, cx| {
        open.view.update(cx, |v, cx| {
            let _ = v.set_data(&p, Some(value), cx);
        })
    });
}

/// A write the host makes over what the person typed: a focused field
/// bound to `path` takes it (`set` leaves a focused field its own text).
fn assign(open: &Open, path: &str, value: Value, cx: &mut App) {
    let p = path.to_string();
    let _ = open.handle.update(cx, |_, _, cx| {
        open.view.update(cx, |v, cx| {
            let _ = v.assign_data(&p, Some(value), cx);
        })
    });
}

fn focus(open: &Open, id: &str, cx: &mut App) {
    let id = id.to_string();
    let _ = open.handle.update(cx, |_, _, cx| {
        open.view.update(cx, |v, cx| {
            v.command(&SurfaceCommand::Focus { id }, cx);
            cx.notify();
        })
    });
}

fn close(open: Open, cx: &mut App) {
    let _ = open.handle.update(cx, |_, window, _| window.remove_window());
}

/// #EDE-184: what a process's first window pays for (wgpu's instance,
/// adapter and device, which gpui keeps for the process, the shaders and
/// the prompt's fonts) the polkit agent pays at its start: a prompt laid
/// out and painted once, on the background layer without the keyboard,
/// and closed before the compositor ever configures it, so it never shows
/// and never takes input. Without it the first prompt after login came up
/// seconds late on llvmpipe, and what the person typed meanwhile went to
/// the window under it.
fn warm(cx: &mut App) {
    let started = std::time::Instant::now();
    let data = json!({"message": "Authentication is needed to run `/usr/bin/true' as the super user", "identity": "org.freedesktop.policykit.exec · as root", "asker": "Asked by /usr/bin/bash (process 1)"});
    let (acts, _) = mpsc::channel();
    match open_on(POLKIT, data, acts, None, (1280.0, 720.0), Layer::Background, KeyboardInteractivity::None, cx) {
        Some(open) => {
            close(open, cx);
            tracing::info!(ms = started.elapsed().as_millis() as u64, "the renderer is up");
        }
        None => tracing::warn!("the renderer is not up ahead of the first prompt"),
    }
}

/// `HH:MM` local time.
fn clock() -> String {
    if let Some(fixed) = std::env::var_os("EDE_SHELL_CLOCK").filter(|v| !v.is_empty()) {
        return fixed.to_string_lossy().into_owned();
    }
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: time() with a null pointer only returns; localtime_r writes the tm we own.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        libc::localtime_r(&now, &mut tm);
    }
    format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)
}

fn user_name() -> String {
    std::env::var("USER").ok().filter(|u| !u.is_empty()).unwrap_or_else(|| {
        // SAFETY: getpwuid returns a static buffer or null.
        let pw = unsafe { libc::getpwuid(libc::getuid()) };
        if pw.is_null() { "user".into() } else { unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }.to_string_lossy().into_owned() }
    })
}

fn usage() -> ! {
    eprintln!("usage: exponential-sealed lock | polkit | consent --title TITLE --body BODY [--allow LABEL] [--deny LABEL] [--size WxH] | share (the ask as JSON on stdin)");
    std::process::exit(2)
}

/// #ERR-29: a dialog's failure exits: what is queued for GlitchTip gets out
/// first (the exit skips the diagnostics guard's drop). An answer's exit
/// does not wait on the network, nor does the lock's (the compositor
/// starts it again, and reports its end itself).
fn exit(code: i32) -> ! {
    ede_observability::flush();
    std::process::exit(code)
}

fn run_app(f: impl FnOnce(&mut App) + 'static) {
    gpui_platform::application().with_assets(assets::Assets).run(move |cx: &mut App| {
        // #EDE-172 (P4): gpui's Wayland connection did its first roundtrip.
        control::refuse_ptrace();
        gpui_component::init(cx);
        let _ = cx.text_system().add_fonts(assets::fonts());
        f(cx);
    });
}

/// The grant the compositor wrote to the inherited descriptor `fd`.
fn grant_from(fd: std::os::fd::RawFd) -> Option<ede_protocol::Token> {
    use std::os::fd::FromRawFd;
    // SAFETY: the compositor handed this descriptor to this process for this one read; nothing else here owns it.
    let mut pipe = unsafe { std::fs::File::from_raw_fd(fd) };
    let mut text = String::new();
    std::io::Read::read_to_string(&mut pipe, &mut text).ok()?;
    ede_protocol::Token::new(text.trim()).ok()
}

/// A lock surface and the output it covers.
struct Shown {
    display: DisplayId,
    size: (i32, i32),
    open: Open,
}

/// The outputs to cover: gpui's displays, each with the logical size the
/// compositor reports for the output of that name (gpui's own bounds
/// round a fractional scale up).
fn lock_outputs(cx: &App) -> Vec<(DisplayId, (i32, i32))> {
    let outputs = ede_protocol::persons(control::with(|c| c.outputs(), ede_protocol::Client::connect_env).unwrap_or_default());
    cx.displays()
        .into_iter()
        .map(|d| {
            let named = d.uuid().ok().and_then(|id| outputs.iter().find(|o| uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_DNS, o.name.as_bytes()).as_bytes() == id.as_bytes()));
            let bounds = d.bounds().size;
            (d.id(), named.map_or((f32::from(bounds.width) as i32, f32::from(bounds.height) as i32), |o| (o.width, o.height)))
        })
        .collect()
}

/// One lock surface per output: opens the missing ones and closes those
/// whose output went or changed size. `seen` is what the outputs were at
/// the last look. A surface the compositor closed under us while its
/// output is still there ends the process: the compositor starts the lock
/// client again.
fn cover_outputs(shown: &mut Vec<Shown>, seen: &mut Vec<(DisplayId, Bounds<Pixels>)>, data: Value, acts: &mpsc::Sender<Act>, cx: &mut App) {
    let now: Vec<(DisplayId, Bounds<Pixels>)> = cx.displays().iter().map(|d| (d.id(), d.bounds())).collect();
    if shown.iter().any(|s| now.iter().any(|(id, _)| *id == s.display) && s.open.handle.update(cx, |_, _, _| ()).is_err()) {
        tracing::error!("a lock surface was closed");
        std::process::exit(1);
    }
    if now == *seen {
        return;
    }
    *seen = now;
    let want = lock_outputs(cx);
    let (keep, gone): (Vec<Shown>, Vec<Shown>) = std::mem::take(shown).into_iter().partition(|s| want.contains(&(s.display, s.size)));
    *shown = keep;
    for s in gone {
        close(s.open, cx);
    }
    for (display, size) in want.iter().copied() {
        if shown.iter().any(|s| s.display == display) {
            continue;
        }
        match open_on(LOCK, data.clone(), acts.clone(), Some(display), (size.0 as f32, size.1 as f32), Layer::Overlay, KeyboardInteractivity::Exclusive, cx) {
            Some(open) => shown.push(Shown { display, size, open }),
            None => std::process::exit(1),
        }
    }
}

/// The lock screen: PAM, then the compositor's `unlock`.
fn lock() {
    let user = user_name();
    let grant = std::env::var("EDE_TOKEN_FD").ok().and_then(|fd| fd.parse().ok()).and_then(grant_from);
    run_app(move |cx| {
        let (acts_tx, acts_rx) = mpsc::channel::<Act>();
        let (auth_tx, auth_rx) = mpsc::channel::<Result<(), String>>();
        cx.spawn(async move |cx| {
            let mut shown: Vec<Shown> = Vec::new();
            let mut seen = Vec::new();
            let mut minute = clock();
            let mut password = String::new();
            loop {
                cx.update(|cx| cover_outputs(&mut shown, &mut seen, json!({"time": clock(), "user": user.clone()}), &acts_tx, cx));
                while let Ok(act) = acts_rx.try_recv() {
                    match act {
                        Act::Input { path, value } if path == "/password" => password = value.as_str().unwrap_or_default().to_string(),
                        Act::Call { name, args } if name == "sealed_unlock" => {
                            let typed = args.get("password").and_then(Value::as_str).map(String::from).unwrap_or_else(|| password.clone());
                            cx.update(|cx| {
                                for s in &shown {
                                    set(&s.open, "/busy", json!(true), cx);
                                    set(&s.open, "/error", json!(""), cx);
                                }
                            });
                            let (tx, user) = (auth_tx.clone(), user.clone());
                            std::thread::spawn(move || {
                                let _ = tx.send(pam::authenticate(&user, &typed));
                            });
                        }
                        _ => {}
                    }
                }
                while let Ok(result) = auth_rx.try_recv() {
                    match result {
                        // #EDE-172 (P4): on the channel when there is one: only it unlocks.
                        Ok(()) => match control::with(|c| c.unlock(), || grant.clone().map_or_else(ede_protocol::Client::connect_env, ede_protocol::Client::connect_with_token)) {
                            Ok(()) => {
                                tracing::info!("unlocked");
                                cx.update(|cx| cx.quit());
                                return;
                            }
                            Err(e) => {
                                tracing::error!(error = %e, "the compositor refused the unlock");
                                cx.update(|cx| {
                                    for s in &shown {
                                        set(&s.open, "/busy", json!(false), cx);
                                        set(&s.open, "/error", json!(format!("The session could not be unlocked: {e}")), cx);
                                    }
                                });
                            }
                        },
                        Err(e) => {
                            tracing::info!("authentication failed: {e}");
                            password.clear();
                            cx.update(|cx| {
                                for s in &shown {
                                    set(&s.open, "/busy", json!(false), cx);
                                    assign(&s.open, "/password", json!(""), cx);
                                    set(&s.open, "/error", json!("Wrong password"), cx);
                                    focus(&s.open, "password", cx);
                                }
                            });
                        }
                    }
                }
                let now = clock();
                if now != minute {
                    minute = now.clone();
                    cx.update(|cx| {
                        for s in &shown {
                            set(&s.open, "/time", json!(now), cx);
                        }
                    });
                }
                cx.background_executor().timer(Duration::from_millis(100)).await;
            }
        })
        .detach();
    });
}

/// The polkit agent: a prompt per request.
fn polkit_agent() {
    let (ui_tx, ui_rx) = mpsc::channel::<polkit::UiEvent>();
    let conn = match zbus::blocking::connection::Builder::system().and_then(|b| b.serve_at(polkit::AGENT_PATH, polkit::Agent { to_ui: ui_tx, space_of: std::sync::Arc::new(polkit::space_of) })).and_then(|b| b.build()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("exponential-sealed polkit: no system bus: {e}");
            std::process::exit(1);
        }
    };
    match polkit::register(&conn) {
        Ok(session) => tracing::info!("polkit agent registered for session {session}"),
        Err(e) => {
            eprintln!("exponential-sealed polkit: not registered: {e}");
            std::process::exit(1);
        }
    }
    run_app(move |cx| {
        // #EDE-87: gpui quits when its last window closes; the agent's
        // last window is the prompt, and the agent outlives every prompt.
        cx.set_quit_mode(gpui::QuitMode::Explicit);
        warm(cx);
        let (acts_tx, acts_rx) = mpsc::channel::<Act>();
        // The open prompt and its answer channel.
        let mut current: Option<(String, Open, mpsc::Sender<polkit::Answer>)> = None;
        let mut password = String::new();
        cx.spawn(async move |cx| {
            loop {
                while let Ok(event) = ui_rx.try_recv() {
                    match event {
                        polkit::UiEvent::Begin(request, answer) => {
                            if let Some((_, open, _)) = current.take() {
                                cx.update(|cx| close(open, cx));
                            }
                            let identity = format!("{} · as {}", request.action_id, request.users.join(" or "));
                            // #EDE-143: a request from outside the person's
                            // session names who asks and waits for a click.
                            let asker = request.asker.as_ref().map(|asker| format!("Asked by {asker}")).unwrap_or_default();
                            let keyboard = request.asker.is_none();
                            let data = json!({"message": request.message, "identity": identity, "asker": asker, "password": "", "busy": false, "error": ""});
                            let tx = acts_tx.clone();
                            if let Some(open) = cx.update(|cx| open_taking(POLKIT, data, tx, keyboard, None, cx)) {
                                current = Some((request.cookie, open, answer));
                            }
                            password.clear();
                        }
                        polkit::UiEvent::Failed(cookie, error) => {
                            if let Some((c, open, _)) = current.as_ref() {
                                if *c == cookie {
                                    cx.update(|cx| {
                                        set(open, "/busy", json!(false), cx);
                                        assign(open, "/password", json!(""), cx);
                                        set(open, "/error", json!(if error.contains("wrong") { "Wrong password".to_string() } else { error }), cx);
                                        focus(open, "password", cx);
                                    });
                                }
                            }
                        }
                        polkit::UiEvent::Done(cookie) => {
                            if current.as_ref().is_some_and(|(c, _, _)| *c == cookie) {
                                if let Some((_, open, _)) = current.take() {
                                    cx.update(|cx| close(open, cx));
                                }
                            }
                        }
                    }
                }
                while let Ok(act) = acts_rx.try_recv() {
                    match act {
                        Act::Input { path, value } if path == "/password" => password = value.as_str().unwrap_or_default().to_string(),
                        Act::Call { name, args } => {
                            if let Some((_, open, answer)) = current.as_ref() {
                                match name.as_str() {
                                    "sealed_authenticate" => {
                                        let typed = args.get("password").and_then(Value::as_str).map(String::from).unwrap_or_else(|| password.clone());
                                        cx.update(|cx| set(open, "/busy", json!(true), cx));
                                        let _ = answer.send(polkit::Answer::Password(typed));
                                    }
                                    "sealed_cancel" => {
                                        let _ = answer.send(polkit::Answer::Cancel);
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }
                cx.background_executor().timer(Duration::from_millis(100)).await;
            }
        })
        .detach();
    });
}

/// `WxH` (logical px, both above 0).
fn parse_size(text: &str) -> Option<(f32, f32)> {
    let (w, h) = text.split_once('x')?;
    let (w, h): (f32, f32) = (w.trim().parse().ok()?, h.trim().parse().ok()?);
    (w > 0.0 && h > 0.0 && w.is_finite() && h.is_finite()).then_some((w, h))
}

/// #EDE-172 (P7): the share chooser's size, from its ask: `size`, else
/// the first output it offers.
fn ask_size(ask: &Value) -> Option<(f32, f32)> {
    let pair = |w: &Value, h: &Value| Some((w.as_f64()? as f32, h.as_f64()? as f32)).filter(|(w, h)| *w > 0.0 && *h > 0.0);
    pair(&ask["size"][0], &ask["size"][1]).or_else(|| pair(&ask["outputs"][0]["width"], &ask["outputs"][0]["height"]))
}

/// A question; the answer on stdout and in the exit code. `--size WxH`
/// (#EDE-172 P7): the logical size of the output it covers, from its
/// caller, so it asks the compositor nothing.
fn consent(args: &[String]) {
    let mut title = String::new();
    let mut body = String::new();
    let mut allow = "Allow".to_string();
    let mut deny = "Deny".to_string();
    let mut size = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--title" => title = it.next().cloned().unwrap_or_default(),
            "--body" => body = it.next().cloned().unwrap_or_default(),
            "--allow" => allow = it.next().cloned().unwrap_or_default(),
            "--deny" => deny = it.next().cloned().unwrap_or_default(),
            "--size" => size = Some(it.next().and_then(|s| parse_size(s)).unwrap_or_else(|| usage())),
            _ => usage(),
        }
    }
    if title.is_empty() {
        usage();
    }
    run_app(move |cx| {
        let (acts_tx, acts_rx) = mpsc::channel::<Act>();
        let open = open(CONSENT, json!({"title": title, "body": body, "allow": allow, "deny": deny}), acts_tx, size, cx);
        if open.is_none() {
            exit(1);
        }
        cx.spawn(async move |cx| {
            loop {
                while let Ok(act) = acts_rx.try_recv() {
                    if let Act::Call { name, args } = act {
                        if name == "sealed_answer" {
                            let answer = args.get("answer").and_then(Value::as_str).unwrap_or("deny").to_string();
                            println!("{answer}");
                            let code = if answer == "allow" { 0 } else { 1 };
                            cx.update(|cx| cx.quit());
                            std::process::exit(code);
                        }
                    }
                }
                cx.background_executor().timer(Duration::from_millis(100)).await;
            }
        })
        .detach();
    });
}

/// The screen-share chooser: a pick (several when `multiple`), the
/// remote-control switch, Share or Cancel; the answer on stdout.
fn share() {
    let mut input = String::new();
    let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut input);
    let ask: Value = serde_json::from_str(&input).unwrap_or(json!({}));
    let app = ask["app_id"].as_str().filter(|a| !a.is_empty()).unwrap_or("An app").to_string();
    let types: Vec<String> = ask["types"].as_array().map(|l| l.iter().filter_map(Value::as_str).map(String::from).collect()).unwrap_or_default();
    let multiple = ask["multiple"].as_bool().unwrap_or(false);
    let remote = ask["remote_control"].as_bool().unwrap_or(false);
    let mut outputs: Vec<Value> = ask["outputs"].as_array().map(|l| l.iter().map(|o| json!({"id": format!("output:{}", o["name"].as_str().unwrap_or("")), "label": o["name"], "detail": format!("{}×{}", o["width"], o["height"]), "selected": false})).collect()).unwrap_or_default();
    let mut windows: Vec<Value> = ask["windows"].as_array().map(|l| l.iter().map(|w| json!({"id": format!("window:{}", w["id"].as_str().unwrap_or("")), "label": w["title"].as_str().or(w["app_id"].as_str()).unwrap_or("Window"), "detail": w["app_id"].as_str().unwrap_or(""), "selected": false})).collect()).unwrap_or_default();
    let preselected = preselect(&ask, multiple, &mut outputs, &mut windows);
    let what = match (types.iter().any(|t| t == "monitor"), types.iter().any(|t| t == "window")) {
        (true, true) => "share a screen or a window",
        (true, false) => "share a screen",
        (false, true) => "share a window",
        (false, false) => "control the keyboard and pointer",
    };
    let title = format!("{app} wants to {what}");
    let body = if remote { "It also asks to control this desktop. Nothing is shared until you choose.".to_string() } else { "Nothing is shared until you choose.".to_string() };
    let size = ask_size(&ask);
    run_app(move |cx| {
        let (acts_tx, acts_rx) = mpsc::channel::<Act>();
        let data = json!({"title": title, "body": body, "outputs": outputs.clone(), "windows": windows.clone(), "remoteControl": remote, "allow": false});
        let open = Rc::new(open(SHARE, data, acts_tx, size, cx));
        if open.is_none() {
            exit(1);
        }
        let mut picks = preselected;
        let (mut outputs, mut windows) = (outputs, windows);
        cx.spawn(async move |cx| {
            loop {
                while let Ok(act) = acts_rx.try_recv() {
                    match act {
                        Act::Call { name, args } => match name.as_str() {
                            "sealed_pick" => {
                                let id = args["id"].as_str().unwrap_or("").to_string();
                                if picks.contains(&id) {
                                    picks.retain(|p| p != &id);
                                } else {
                                    if !multiple {
                                        picks.clear();
                                    }
                                    picks.push(id);
                                }
                                for list in [&mut outputs, &mut windows] {
                                    for item in list.iter_mut() {
                                        let chosen = picks.iter().any(|p| Some(p.as_str()) == item["id"].as_str());
                                        item["selected"] = json!(chosen);
                                    }
                                }
                                if let Some(o) = open.as_ref() {
                                    let (os, ws) = (outputs.clone(), windows.clone());
                                    cx.update(|cx| {
                                        set(o, "/outputs", Value::Array(os), cx);
                                        set(o, "/windows", Value::Array(ws), cx);
                                    });
                                }
                            }
                            "sealed_answer" => {
                                let lines = share_answer(&args, &picks);
                                for line in &lines {
                                    println!("{line}");
                                }
                                let shared = args["answer"].as_str() == Some("share");
                                cx.update(|cx| cx.quit());
                                std::process::exit(if shared { 0 } else { 1 });
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                }
                cx.background_executor().timer(Duration::from_millis(100)).await;
            }
        })
        .detach();
    });
}

/// The chooser's answer to the portal for `sealed_answer` `args`: on
/// Share, the picks, then `allow` when the remote-control switch is on.
/// #EDE-203: the switch's state comes with Share's args (`/allow`): a
/// `Switch` sends its host no input event, so it is never read from one.
fn share_answer(args: &Value, picks: &[String]) -> Vec<String> {
    if args["answer"].as_str() != Some("share") {
        return Vec::new();
    }
    let mut lines = picks.to_vec();
    if args["allow"].as_bool() == Some(true) {
        lines.push("allow".into());
    }
    lines
}

/// #EDE-172 (D6 a): the ask's `selected` (a restored share's last picks)
/// start selected, so one click on Share shares them again; an id not
/// offered is dropped, one pick unless `multiple`.
fn preselect(ask: &Value, multiple: bool, outputs: &mut [Value], windows: &mut [Value]) -> Vec<String> {
    let offered = |id: &str| outputs.iter().chain(windows.iter()).any(|row| row["id"] == id);
    let picks: Vec<String> = ask["selected"].as_array().into_iter().flatten().filter_map(Value::as_str).filter(|&id| offered(id)).take(if multiple { usize::MAX } else { 1 }).map(String::from).collect();
    for row in outputs.iter_mut().chain(windows.iter_mut()) {
        row["selected"] = json!(picks.iter().any(|p| row["id"] == p.as_str()));
    }
    picks
}

fn main() {
    // A crash-monitor process (ede-observability) runs and exits here.
    ede_observability::run_crash_monitor_if_requested();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map(String::as_str).unwrap_or_default();
    let component = match mode {
        "lock" => "sealed-lock",
        "polkit" => "sealed-polkit",
        "consent" => "sealed-consent",
        "share" => "sealed-share",
        _ => usage(),
    };
    // #EDE-172 (P4): the channel the compositor started the lock or the
    // polkit agent with, before the crash monitor or any thread starts.
    control::take();
    // #ERR-29: the compositor's diagnostics (panics, errors, native
    // crashes), tagged per mode; stdout is consent's and share's answer. A
    // report says where a line was logged, never what it said.
    let _diagnostics = ede_observability::init_with(component, "info", ede_observability::Output::Stderr, ede_observability::Trail::Places);
    ede_observability::watch_native_crashes(component);
    match mode {
        "lock" => lock(),
        "polkit" => polkit_agent(),
        "consent" => consent(&args[1..]),
        _ => share(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The host functions this binary answers (A2UI 1.0's flat names).
    const FUNCTIONS: &[&str] = &["sealed_unlock", "sealed_authenticate", "sealed_cancel", "sealed_answer", "sealed_pick"];

    fn calls(node: &Value, out: &mut Vec<String>) {
        match node {
            Value::Object(o) => {
                if let Some(call) = o.get("functionCall").and_then(|f| f.get("@call")).and_then(Value::as_str) {
                    out.push(call.to_string());
                }
                o.values().for_each(|v| calls(v, out));
            }
            Value::Array(a) => a.iter().for_each(|v| calls(v, out)),
            _ => {}
        }
    }

    /// #EDE-172 (D6 a): a restored share opens with its last picks
    /// selected; a new one with none.
    #[test]
    fn a_restored_share_opens_with_its_last_picks_selected() {
        let row = |id: &str| json!({"id": id, "selected": false});
        let ask = json!({"selected": ["window:w2", "output:gone", "output:A"]});
        let (mut outputs, mut windows) = (vec![row("output:A"), row("output:B")], vec![row("window:w2")]);
        let selected = |rows: &[Value]| rows.iter().map(|r| r["selected"] == true).collect::<Vec<_>>();
        assert_eq!(preselect(&ask, true, &mut outputs, &mut windows), ["window:w2", "output:A"]);
        assert_eq!((selected(&outputs), selected(&windows)), (vec![true, false], vec![true]));
        assert_eq!(preselect(&ask, false, &mut outputs, &mut windows), ["window:w2"]);
        assert_eq!((selected(&outputs), selected(&windows)), (vec![false, false], vec![true]));
        assert!(preselect(&json!({}), true, &mut outputs, &mut windows).is_empty());
        assert_eq!((selected(&outputs), selected(&windows)), (vec![false, false], vec![false]));
    }

    /// #EDE-203: the chooser as eUI runs it: the remote-control switch on,
    /// then Share, answers the portal `allow`; with it off, no `allow`.
    /// Cancel answers nothing.
    #[test]
    fn share_with_the_switch_on_answers_allow() {
        use eui_core::{
            measure::FixedMeasure,
            surface::{OutEvent, Surface, SurfaceOptions},
        };
        let press = |toggle: bool, button: &str| {
            let parsed: Value = serde_json::from_str(SHARE).expect("the chooser");
            let tree: NestedNode = serde_json::from_value(parsed["root"].clone()).expect("its tree");
            let mut s = Surface::new("share", SurfaceOptions::default());
            assert!(s.set_nested(tree).issues.is_empty());
            for (path, value) in [("/title", json!("probe wants to control the keyboard and pointer")), ("/body", json!("")), ("/outputs", json!([])), ("/windows", json!([])), ("/remoteControl", json!(true)), ("/allow", json!(false))] {
                s.set_data(path, Some(value)).unwrap();
            }
            s.set_viewport(854.0, 534.0, None);
            let mut measure = FixedMeasure { sizes: Default::default(), wrap: true };
            s.layout(&mut measure);
            if toggle {
                let switch = s.index_of("control").expect("the remote-control switch shows");
                s.event(switch, "press", None);
                s.layout(&mut measure);
            }
            let out = s.event(s.index_of(button).expect("the button"), "press", None);
            let args = out.into_iter().find_map(|e| match e {
                OutEvent::FunctionCall { name, args, .. } if name == "sealed_answer" => Some(args),
                _ => None,
            });
            share_answer(&args.expect("the button answers"), &["output:Virtual-1".to_string()])
        };
        assert_eq!(press(true, "ok"), ["output:Virtual-1", "allow"]);
        assert_eq!(press(false, "ok"), ["output:Virtual-1"], "no allow with the switch off");
        assert!(press(true, "cancel").is_empty(), "Cancel answers nothing");
    }

    /// #EDE-172 (P7): consent's `--size` and the chooser's ask say the size
    /// the prompt covers.
    #[test]
    fn the_caller_says_the_size() {
        assert_eq!(parse_size("1280x800"), Some((1280.0, 800.0)));
        assert_eq!(parse_size("853.5x533"), Some((853.5, 533.0)), "a fractional scale's logical size");
        for bad in ["", "x", "0x800", "1280", "-1x2", "axb"] {
            assert_eq!(parse_size(bad), None, "{bad}");
        }
        assert_eq!(ask_size(&json!({"size": [853, 533], "outputs": [{"width": 1, "height": 1}]})), Some((853.0, 533.0)));
        assert_eq!(ask_size(&json!({"outputs": [{"name": "A", "width": 1920, "height": 1080}]})), Some((1920.0, 1080.0)), "an older portal's ask");
        assert_eq!(ask_size(&json!({"outputs": []})), None);
    }

    /// The unlock grant is read from the descriptor the compositor passed.
    #[test]
    fn the_grant_is_read_from_the_inherited_descriptor() {
        use std::{io::Write, os::fd::IntoRawFd};
        let (read, mut write) = std::io::pipe().unwrap();
        let grant = "5e".repeat(32);
        writeln!(write, "{grant}").unwrap();
        drop(write);
        assert_eq!(grant_from(read.into_raw_fd()).map(|t| t.as_str().to_string()), Some(grant));
    }

    /// The nodes from `node` down to the one with `id`, in that order.
    fn path_to<'a>(node: &'a Value, id: &str) -> Option<Vec<&'a Value>> {
        if node["id"] == id {
            return Some(vec![node]);
        }
        node["children"].as_array()?.iter().find_map(|child| path_to(child, id)).map(|mut path| {
            path.insert(0, node);
            path
        })
    }

    /// #EDE-181: every sealed prompt's card is opaque in both modes: an
    /// opaque colour lies under it in its document (the lock's screen, the
    /// others' `backing`). The dark theme's card is translucent, so a card
    /// over nothing let the window under the prompt show through its words.
    #[test]
    fn every_sealed_card_lies_on_an_opaque_colour() {
        let theme = eui_core::themes::builtin_theme("exponential").expect("the exponential theme");
        for (name, doc) in [("lock", LOCK), ("polkit", POLKIT), ("consent", CONSENT), ("share", SHARE)] {
            let parsed: Value = serde_json::from_str(doc).expect(name);
            let path = path_to(&parsed["root"], "card").unwrap_or_else(|| panic!("{name} has a card"));
            for mode in [Mode::Light, Mode::Dark] {
                let colors = &theme.modes.get(mode).color;
                let opaque = path.iter().filter_map(|n| n["style"]["backgroundColor"].as_str()).any(|c| {
                    let hex = c.strip_prefix("$color.").map_or(Some(c), |token| colors.get(token).map(String::as_str)).unwrap_or_else(|| panic!("{name}: {c} is no colour"));
                    hex.get(7..9).map_or(hex.len() == 7, |alpha| alpha.eq_ignore_ascii_case("ff"))
                });
                assert!(opaque, "{name}'s card lies on nothing opaque in {mode:?}");
            }
        }
    }

    /// Every call a sealed document makes is one this binary handles, and
    /// the source matches each by its flat name (#VAPP-289 renamed the
    /// documents' calls; the lock screen must keep unlocking).
    #[test]
    fn the_sealed_documents_call_what_the_binary_answers() {
        let source = include_str!("main.rs");
        for (name, doc) in [("lock", LOCK), ("polkit", POLKIT), ("consent", CONSENT), ("share", SHARE)] {
            let parsed: Value = serde_json::from_str(doc).expect(name);
            let mut found = Vec::new();
            calls(&parsed, &mut found);
            let found: Vec<String> = found.into_iter().filter(|c| c.starts_with("sealed")).collect();
            assert!(!found.is_empty(), "{name} calls the binary");
            for call in found {
                assert!(FUNCTIONS.contains(&call.as_str()), "{name}: {call} is not a sealed function");
                assert!(source.contains(&format!("\"{call}\"")), "{name}: the binary never matches {call}");
            }
        }
    }
}
