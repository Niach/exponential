//! `ede-sealed` (#EDE-17, Tier B): the surfaces a document on disk must
//! never be, compiled into this binary (`apps/ede/sealed/*.json`, never
//! agent-editable; only the theme's accent is anyone's), each a
//! full-screen overlay layer surface with the keyboard to itself:
//!
//! - `ede-sealed lock`: the lock screen. The compositor starts it (no
//!   `[lock] command`), shows nothing but this process's surfaces while
//!   locked, and takes `unlock` only with the grant it minted for this
//!   process (`EDE_TOKEN`). The password goes to PAM in this process.
//! - `ede-sealed polkit`: the polkit authentication agent for the session
//!   (`ede-polkit-agent.service`): polkitd's `BeginAuthentication` opens
//!   the prompt, `polkit-agent-helper-1` checks the password.
//! - `ede-sealed consent --title T --body B [--allow A] [--deny D]`: a
//!   question to the person; prints `allow` or `deny`, exits 0 or 1.
//!
//! Drawn with the same gpui + eui-gpui the shell uses, the same tokens;
//! the compositor marks every layer surface of this executable as
//! protected (black in captures, never driven, no input method).

#[path = "../../assets.rs"]
mod assets;
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
use gpui::{div, layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions}, point, prelude::*, px, size, App, Bounds, Entity, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions};
use serde_json::{json, Value};

const LOCK: &str = include_str!("../../../../../../sealed/lock.json");
const POLKIT: &str = include_str!("../../../../../../sealed/polkit.json");
const CONSENT: &str = include_str!("../../../../../../sealed/consent.json");

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

/// A sealed document as a full-screen overlay with the keyboard.
fn open(doc: &str, data: Value, acts: mpsc::Sender<Act>, cx: &mut App) -> Option<Open> {
    let mut parsed: Value = serde_json::from_str(doc).expect("a sealed document");
    // The first output's logical size: the root fills it in px (a layer
    // window takes its content's size otherwise) and the surface asks for it.
    let (w, h) = ede_protocol::Client::connect_env()
        .ok()
        .and_then(|mut c| c.outputs().ok())
        .and_then(|o| o.first().map(|o| (o.width as f32, o.height as f32)))
        .unwrap_or((1280.0, 720.0));
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
        namespace: "ede-sealed".into(),
        layer: Layer::Overlay,
        anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
        exclusive_zone: Some(px(-1.0)),
        exclusive_edge: None,
        margin: None,
        keyboard_interactivity: KeyboardInteractivity::Exclusive,
    });
    let view_out: Rc<RefCell<Option<Entity<SurfaceView>>>> = Rc::new(RefCell::new(None));
    let out = view_out.clone();
    let opened = cx.open_window(
        WindowOptions {
            kind,
            window_bounds: Some(WindowBounds::Windowed(Bounds { origin: point(px(0.0), px(0.0)), size: size(px(w), px(h)) })),
            titlebar: None,
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
            tracing::error!("window: {e}");
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
    eprintln!("usage: ede-sealed lock | polkit | consent --title TITLE --body BODY [--allow LABEL] [--deny LABEL]");
    std::process::exit(2)
}

fn run_app(f: impl FnOnce(&mut App) + 'static) {
    gpui_platform::application().with_assets(assets::Assets).run(move |cx: &mut App| {
        gpui_component::init(cx);
        let _ = cx.text_system().add_fonts(assets::fonts());
        f(cx);
    });
}

/// The lock screen: PAM, then the compositor's `unlock`.
fn lock() {
    let user = user_name();
    run_app(move |cx| {
        let (acts_tx, acts_rx) = mpsc::channel::<Act>();
        let (auth_tx, auth_rx) = mpsc::channel::<Result<(), String>>();
        let open = Rc::new(RefCell::new(open(LOCK, json!({"time": clock(), "user": user.clone()}), acts_tx, cx)));
        if open.borrow().is_none() {
            std::process::exit(1);
        }
        let o = open.clone();
        cx.spawn(async move |cx| {
            let mut minute = clock();
            let mut password = String::new();
            loop {
                while let Ok(act) = acts_rx.try_recv() {
                    match act {
                        Act::Input { path, value } if path == "/password" => password = value.as_str().unwrap_or_default().to_string(),
                        Act::Call { name, args } if name == "sealed.unlock" => {
                            let typed = args.get("password").and_then(Value::as_str).map(String::from).unwrap_or_else(|| password.clone());
                            if let Some(open) = o.borrow().as_ref() {
                                cx.update(|cx| {
                                    set(open, "/busy", json!(true), cx);
                                    set(open, "/error", json!(""), cx);
                                });
                            }
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
                        Ok(()) => match ede_protocol::Client::connect_env().and_then(|mut c| c.unlock()) {
                            Ok(()) => {
                                tracing::info!("unlocked");
                                cx.update(|cx| cx.quit());
                                return;
                            }
                            Err(e) => {
                                tracing::error!("unlock refused: {e}");
                                if let Some(open) = o.borrow().as_ref() {
                                    cx.update(|cx| {
                                        set(open, "/busy", json!(false), cx);
                                        set(open, "/error", json!(format!("The session could not be unlocked: {e}")), cx);
                                    });
                                }
                            }
                        },
                        Err(e) => {
                            tracing::info!("authentication failed: {e}");
                            password.clear();
                            if let Some(open) = o.borrow().as_ref() {
                                cx.update(|cx| {
                                    set(open, "/busy", json!(false), cx);
                                    set(open, "/password", json!(""), cx);
                                    set(open, "/error", json!("Wrong password"), cx);
                                    focus(open, "password", cx);
                                });
                            }
                        }
                    }
                }
                let now = clock();
                if now != minute {
                    minute = now.clone();
                    if let Some(open) = o.borrow().as_ref() {
                        cx.update(|cx| set(open, "/time", json!(now), cx));
                    }
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
    let conn = match zbus::blocking::connection::Builder::system().and_then(|b| b.serve_at(polkit::AGENT_PATH, polkit::Agent { to_ui: ui_tx })).and_then(|b| b.build()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("ede-sealed polkit: no system bus: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = polkit::register(&conn) {
        eprintln!("ede-sealed polkit: not registered: {e}");
        std::process::exit(1);
    }
    tracing::info!("polkit agent registered");
    run_app(move |cx| {
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
                            let data = json!({"message": request.message, "identity": identity, "password": "", "busy": false, "error": ""});
                            let tx = acts_tx.clone();
                            if let Some(open) = cx.update(|cx| open(POLKIT, data, tx, cx)) {
                                current = Some((request.cookie, open, answer));
                            }
                            password.clear();
                        }
                        polkit::UiEvent::Failed(cookie, error) => {
                            if let Some((c, open, _)) = current.as_ref() {
                                if *c == cookie {
                                    cx.update(|cx| {
                                        set(open, "/busy", json!(false), cx);
                                        set(open, "/password", json!(""), cx);
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
                                    "sealed.authenticate" => {
                                        let typed = args.get("password").and_then(Value::as_str).map(String::from).unwrap_or_else(|| password.clone());
                                        cx.update(|cx| set(open, "/busy", json!(true), cx));
                                        let _ = answer.send(polkit::Answer::Password(typed));
                                    }
                                    "sealed.cancel" => {
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

/// A question; the answer on stdout and in the exit code.
fn consent(args: &[String]) {
    let mut title = String::new();
    let mut body = String::new();
    let mut allow = "Allow".to_string();
    let mut deny = "Deny".to_string();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--title" => title = it.next().cloned().unwrap_or_default(),
            "--body" => body = it.next().cloned().unwrap_or_default(),
            "--allow" => allow = it.next().cloned().unwrap_or_default(),
            "--deny" => deny = it.next().cloned().unwrap_or_default(),
            _ => usage(),
        }
    }
    if title.is_empty() {
        usage();
    }
    run_app(move |cx| {
        let (acts_tx, acts_rx) = mpsc::channel::<Act>();
        let open = open(CONSENT, json!({"title": title, "body": body, "allow": allow, "deny": deny}), acts_tx, cx);
        if open.is_none() {
            std::process::exit(1);
        }
        cx.spawn(async move |cx| {
            loop {
                while let Ok(act) = acts_rx.try_recv() {
                    if let Act::Call { name, args } = act {
                        if name == "sealed.answer" {
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

fn main() {
    tracing_subscriber::fmt().with_writer(std::io::stderr).with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())).init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("lock") => lock(),
        Some("polkit") => polkit_agent(),
        Some("consent") => consent(&args[1..]),
        _ => usage(),
    }
}
