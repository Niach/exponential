//! #LOGS-11: a large account (8 teams, ~1,700 sessions with children,
//! dozens working, a long chat hosted here and open beside the rail, the
//! pickup pass on), drawn HEADLESS inside the Shell's glyph scope and layer:
//! the cold frame, an idle focused second with working marks, and one
//! delta each — sessions, messages into the open chat, messages elsewhere,
//! events — its observers and its frame. Release timings:
//!
//! ```sh
//! LOGS11_ROOTS=400 LOGS11_MESSAGES=5000 cargo test -p ui --release --lib rail_bench -- --ignored --nocapture
//! ```
//!
//! `LOGS11_PROFILE=<idle|sessions|messages|elsewhere|events>` loops that
//! phase for 8 s, for `sample <pid>`.

use std::time::{Duration, Instant};

use gpui::{div, prelude::*, px, size, AnyWindowHandle, App, Entity, HeadlessAppContext, StyleRefinement, Window};
use serde_json::{json, Value};

use crate::navigation::nav_for_window;
use crate::shell_bench::headless;
use crate::sidebar::{RailView, RAIL_RENDERS};
use crate::transcript_bench::percentile;

const TEAMS: usize = 8;
/// Roots in the active team; every other team holds a tenth.
fn roots() -> usize {
    std::env::var("LOGS11_ROOTS").ok().and_then(|n| n.parse().ok()).unwrap_or(400)
}
fn messages() -> usize {
    std::env::var("LOGS11_MESSAGES").ok().and_then(|n| n.parse().ok()).unwrap_or(5_000)
}

struct Root {
    rail: Entity<RailView>,
    region: Entity<eui_gpui::controls::GlyphRegion>,
    layer: Entity<eui_gpui::controls::GlyphLayer>,
}

impl Render for Root {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .relative()
            .child(eui_gpui::controls::glyph_scope("rail", self.region.clone().cached(StyleRefinement::default().size_full())))
            .child(self.layer.clone())
    }
}

fn at(n: usize) -> String {
    let secs = n as i64 * 7;
    format!("2026-11-{:02}T{:02}:{:02}:{:02}.000Z", 1 + (secs / 86_400) % 9, (secs / 3600) % 24, (secs / 60) % 60, secs % 60)
}

fn seed_rows<T: sync::ShapeRow + serde::de::DeserializeOwned>(collection: &Entity<sync::Collection<T>>, rows: Vec<Value>, cx: &mut App) {
    collection.update(cx, |c, cx| {
        for row in rows {
            let row: T = serde_json::from_value(row).expect("a store row");
            c.seed(row.key(), row);
        }
        cx.notify();
    });
}

fn digest(root: &str, n: usize, waiting: bool) -> Value {
    let message = |id: String, kind: &str, body: String| json!({"id": id, "authorKind": kind, "body": body, "createdAt": at(n)});
    let mut digest = json!({
        "first": message(format!("{root}-first"), "user", format!("Please fix the flaky list under crates/ui, round {n}")),
        "preview": message(format!("{root}-preview"), "agent", format!("Pushed the fix for round {n}; CI is running.")),
        "agentAt": at(n),
        "agentTimes": [at(n)],
    });
    if waiting {
        let wait = message(format!("{root}-wait"), "agent", "Which approach should I take?".into());
        digest["waiting"] = wait.clone();
        digest["waits"] = json!([wait]);
    }
    digest
}

/// The account: `me` starts most chats; the active team is `t0`.
fn seed(me: &str, cx: &mut App) {
    let c = sync::Store::global(cx).collections().clone();
    let teams: Vec<Value> = (0..TEAMS).map(|t| json!({"id": format!("t{t}"), "name": format!("Team {t}"), "slug": format!("team-{t}")})).collect();
    seed_rows(&c.teams, teams, cx);
    let mut boards = Vec::new();
    let mut issues = Vec::new();
    let mut agents = Vec::new();
    let mut sessions = Vec::new();
    let mut links = Vec::new();
    let mut reads = Vec::new();
    let mut messages = Vec::new();
    let mut events = Vec::new();
    let mut pulls = Vec::new();
    let mut n = 0usize;
    for t in 0..TEAMS {
        let team = format!("t{t}");
        for b in 0..6 {
            boards.push(json!({"id": format!("b{t}-{b}"), "team_id": team, "name": format!("Board {b}"), "slug": format!("board-{t}-{b}"), "prefix": format!("B{t}{b}"), "color": "#22c55e", "sort_order": b}));
        }
        for i in 0..120 {
            issues.push(json!({"id": format!("i{t}-{i}"), "board_id": format!("b{t}-{}", i % 6), "number": i, "identifier": format!("B{t}{}-{i}", i % 6), "title": format!("Issue {i} of team {t}"), "status": "in_progress"}));
        }
        for a in 0..6 {
            agents.push(json!({"id": format!("ag{t}-{a}"), "team_id": team, "name": format!("Agent {a}"), "role": if a == 0 { "assistant" } else { "agent" }, "owner_id": me, "sort_order": a}));
            // My conversation with each agent.
            sessions.push(json!({
                "id": format!("conv{t}-{a}"), "team_id": team, "root_id": format!("conv{t}-{a}"), "agent_id": format!("ag{t}-{a}"),
                "user_id": me, "status": if a == 1 { "running" } else { "idle" }, "busy": a == 1, "shared": true, "cli": "claude",
                "created_at": at(n), "updated_at": at(n), "digest": digest(&format!("conv{t}-{a}"), n, false),
            }));
        }
        let roots = if t == 0 { roots() } else { roots() / 10 };
        for r in 0..roots {
            n += 1;
            let root = format!("s{t}-{r}");
            // A tenth working, a twentieth waiting on me, the rest idle/done.
            let working = r % 10 == 0;
            let waiting = r % 20 == 5;
            let ended = r % 3 == 2;
            sessions.push(json!({
                "id": root, "team_id": team, "root_id": root, "user_id": if r % 4 == 3 { "someone-else" } else { me },
                "status": if ended { "ended" } else if working { "running" } else { "idle" }, "busy": working, "needs_input": waiting,
                "shared": true, "cli": if r % 7 == 0 { "codex" } else { "claude" }, "device_id": format!("d{}", r % 3),
                "created_at": at(n), "updated_at": at(n), "title": format!("Fix the flaky list, round {r}"),
                "digest": digest(&root, n, waiting),
            }));
            for k in 0..(r % 4) {
                let child = format!("{root}-c{k}");
                sessions.push(json!({
                    "id": child, "team_id": team, "root_id": root, "parent_id": root, "depth": 1, "user_id": me,
                    "status": if working && k == 0 { "running" } else { "idle" }, "busy": working && k == 0,
                    "shared": true, "cli": "claude", "created_at": at(n), "updated_at": at(n), "title": format!("Child {k} of {r}"),
                }));
            }
            links.push(json!({"id": format!("l{t}-{r}"), "session_id": root, "issue_id": format!("i{t}-{}", r % 120), "role": "primary", "created_at": at(n)}));
            reads.push(json!({"id": format!("rd{t}-{r}"), "user_id": me, "session_id": root, "read_at": at(n.saturating_sub(r % 2))}));
            if r % 5 == 0 {
                pulls.push(json!({"id": format!("pr{t}-{r}"), "team_id": team, "root_session_id": root, "number": r, "state": "open", "title": format!("PR {r}"), "created_at": at(n)}));
            }
        }
        for e in 0..400 {
            events.push(json!({"id": format!("ev{t}-{e}"), "team_id": team, "name": "session.status", "payload": {"n": e}, "created_at": at(e)}));
        }
    }
    // The very long team-agent chat, then a few messages per other root.
    let total = self::messages();
    for m in 0..total {
        // The long chat is hosted here (d0); every other root's few rows.
        let session = if m < total * 3 / 5 { "s0-0".to_string() } else { format!("s0-{}", m % roots()) };
        messages.push(json!({"id": format!("m{m:07}"), "session_id": session, "author_kind": if m % 2 == 0 { "user" } else { "agent" }, "body": format!("Message {m}: the list under crates/ui still jumps when a row arrives."), "created_at": at(m), "consumed_at": at(m)}));
    }
    seed_rows(&c.boards, boards, cx);
    seed_rows(&c.issues, issues, cx);
    seed_rows(&c.agents, agents, cx);
    seed_rows(&c.sessions, sessions, cx);
    seed_rows(&c.session_issues, links, cx);
    seed_rows(&c.session_reads, reads, cx);
    seed_rows(&c.pull_requests, pulls, cx);
    seed_rows(&c.messages, messages, cx);
    seed_rows(&c.events, events, cx);
}

/// A frame as the platform draws one: only what was notified re-renders.
fn frame(window: AnyWindowHandle, cx: &mut HeadlessAppContext) -> f64 {
    let started = Instant::now();
    cx.update_window(window, |_, window, cx| window.draw(cx).clear(cx)).expect("a frame draws");
    started.elapsed().as_secs_f64() * 1000.
}

/// The display link draws every tick; a frame with nothing notified reuses
/// every view (cheap).
fn dirty(_: AnyWindowHandle, _: &mut HeadlessAppContext) -> bool {
    true
}

fn renders() -> usize {
    RAIL_RENDERS.with(|n| n.get())
}

struct Stats(Vec<f64>);

impl std::fmt::Display for Stats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut ms = self.0.clone();
        ms.sort_by(f64::total_cmp);
        write!(f, "n {:3} · p50 {:8.2} ms · p95 {:8.2} ms · max {:8.2} ms", ms.len(), percentile(&ms, 0.5), percentile(&ms, 0.95), percentile(&ms, 1.0))
    }
}

/// One delta: the store write with its observers (what `sync.apply` spans),
/// then the frame it asks for.
fn delta(kind: &str, k: usize, me: &str, window: AnyWindowHandle, cx: &mut HeadlessAppContext) -> (f64, f64) {
    let started = Instant::now();
    cx.update(|cx| {
        let c = sync::Store::global(cx).collections().clone();
        match kind {
            "sessions" => {
                let r = (k * 10) % roots();
                let root = format!("s0-{r}");
                seed_rows(&c.sessions, vec![json!({
                    "id": root, "team_id": "t0", "root_id": root, "user_id": me, "status": "running", "busy": true,
                    "shared": true, "cli": "claude", "device_id": "d0", "created_at": at(r), "updated_at": at(10_000 + k),
                    "title": format!("Fix the flaky list, round {r}"), "digest": digest(&root, 10_000 + k, false),
                })], cx);
            }
            "elsewhere" => {
                // Another working session streams (the long chat stays open).
                seed_rows(&c.messages, vec![json!({"id": format!("e{k:08}"), "session_id": format!("s0-{}", 10 * (1 + k % 30)), "author_kind": "agent", "body": format!("Streaming {k}"), "created_at": at(self::messages() + k)})], cx);
            }
            "messages" => {
                seed_rows(&c.messages, vec![json!({"id": format!("n{k:08}"), "session_id": "s0-0", "author_kind": "agent", "body": format!("Streaming {k}"), "created_at": at(self::messages() + k)})], cx);
            }
            _ => {
                seed_rows(&c.events, vec![json!({"id": format!("ne{k:08}"), "team_id": "t0", "name": "session.status", "payload": {"k": k}, "created_at": at(k)})], cx);
            }
        }
    });
    let apply = started.elapsed().as_secs_f64() * 1000.;
    let drawn = if dirty(window, cx) { frame(window, cx) } else { 0. };
    cx.run_until_parked();
    (apply, drawn)
}

#[test]
#[ignore = "a release bench: run with --ignored --nocapture"]
fn rail_bench() {
    let mut cx = headless();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_path_buf();
    let me = cx.update(|cx| {
        crate::init(cx);
        let store = sync::Store::open(cx, None, None);
        cx.set_global(store);
        crate::local_desktops::install(path, cx);
        assert!(crate::local_desktops::enter_current(cx));
        // The pickup pass runs on every sessions / messages / events delta.
        crate::queries::set_own_device_id_for_test("d0", cx);
        crate::session_pickup::install(cx);
        crate::session_pickup::PASS_ON_LOCAL.with(|on| on.set(true));
        crate::queries::active_account(cx).expect("the desktop's account").user_id
    });
    let seeding = Instant::now();
    cx.update(|cx| seed(&me, cx));
    let sessions = cx.update(|cx| sync::Store::global(cx).collections().sessions.read(cx).len());
    println!("[LOGS-11] seeded {sessions} sessions, {} messages in {:.0} ms", messages(), seeding.elapsed().as_secs_f64() * 1e3);

    let handle = cx
        .open_window(size(px(272.), px(900.)), |window: &mut Window, cx| {
            let nav = nav_for_window(window, cx);
            nav.update(cx, |nav, _| nav.team_id = Some("t0".into()));
            let rail = cx.new(|cx| RailView::new(window, cx));
            let region = cx.new(|_| eui_gpui::controls::GlyphRegion::new(vec![rail.clone().into()]));
            let layer = cx.new(|_| eui_gpui::controls::GlyphLayer);
            cx.new(|_| Root { rail, region, layer })
        })
        .expect("a headless window");
    let window: AnyWindowHandle = handle.into();
    // The long chat open beside the rail (its own window: its views observe
    // the deltas as the dock's do).
    let (chat, _) = cx
        .open_window(size(px(1100.), px(900.)), |window: &mut Window, cx| cx.new(|cx| crate::session_chat::SessionChatScreenView::new("s0-0".to_string(), window, cx)))
        .map(|handle| (AnyWindowHandle::from(handle), ()))
        .expect("a headless window");
    frame(chat, &mut cx);
    cx.update_window(window, |_, window, _| window.activate_window()).unwrap();
    cx.run_until_parked();
    let cold = frame(window, &mut cx);
    cx.run_until_parked();
    for _ in 0..3 {
        frame(window, &mut cx);
        cx.run_until_parked();
    }
    let active = cx.update_window(window, |_, window, _| window.is_window_active()).unwrap();
    println!("[LOGS-11] cold rail frame {cold:.1} ms (window active: {active})");
    cx.update(|cx| {
        let root = handle.entity(cx).unwrap();
        let rail = root.read(cx).rail.clone();
        let region = rail.read(cx).region();
        let input = &region.read(cx).input;
        println!("[LOGS-11] rail rows: {} active · {} agents · {} boards · needs you {}", input.active.len(), input.agents.len(), input.boards.len(), input.needs_you);
    });

    // `LOGS11_PROFILE=<phase>`: loop it for a sampler.
    if let Ok(phase) = std::env::var("LOGS11_PROFILE") {
        eprintln!("[LOGS-11] profiling {phase} in pid {}", std::process::id());
        std::thread::sleep(Duration::from_millis(1500));
        let until = Instant::now() + Duration::from_secs(8);
        let mut k = 0;
        while Instant::now() < until {
            k += 1;
            if phase == "idle" {
                cx.advance_clock(Duration::from_millis(10));
                cx.run_until_parked();
                if dirty(window, &mut cx) {
                    frame(window, &mut cx);
                }
            } else {
                delta(&phase, k, &me, window, &mut cx);
            }
        }
    }

    // Idle, focused, working marks on screen: one simulated second in
    // 10 ms display-link ticks; a frame whenever the window is dirty.
    let before = renders();
    let mut idle = Vec::new();
    for _ in 0..100 {
        cx.advance_clock(Duration::from_millis(10));
        cx.run_until_parked();
        if dirty(window, &mut cx) {
            idle.push(frame(window, &mut cx));
        }
    }
    println!("[LOGS-11] idle 1 s focused: {} rail renders", renders() - before);
    if !idle.is_empty() {
        println!("[LOGS-11] idle frames: {}", Stats(idle));
    }

    // A render of the rail alone (what any notify of it costs).
    let rail = cx.update(|cx| handle.entity(cx).unwrap().read(cx).rail.clone());
    let mut forced = Vec::new();
    for _ in 0..20 {
        cx.update(|cx| rail.update(cx, |_, cx| cx.notify()));
        forced.push(frame(window, &mut cx));
        cx.run_until_parked();
    }
    println!("[LOGS-11] rail notified, frame: {}", Stats(forced));

    for kind in ["sessions", "messages", "elsewhere", "events"] {
        let before = renders();
        let (mut apply, mut drawn) = (Vec::new(), Vec::new());
        for k in 0..20 {
            let (a, d) = delta(kind, k, &me, window, &mut cx);
            apply.push(a);
            drawn.push(d);
        }
        println!("[LOGS-11] {kind:8} delta apply: {}", Stats(apply));
        println!("[LOGS-11] {kind:8} delta frame: {} · {} rail renders", Stats(drawn), renders() - before);
    }
    // `Store::open` holds its handles past gpui's exit-time leak check.
    std::mem::forget(cx);
    std::mem::forget(root);
}
