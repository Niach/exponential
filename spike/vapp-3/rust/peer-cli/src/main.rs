//! VAPP-3 spike CLI: `daemon` (relay publisher: answers offers, serves the bench, collects
//! results), `client` (Rust offerer + bench runner), `turn-probe` (coturn smoke test).
use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use futures_util::{SinkExt, StreamExt};
use peer_core::signal::{Body, Envelope};
use peer_core::{IcePolicy, PeerConfig, PeerError, PeerLink};
use serde_json::{json, Value};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio_tungstenite::tungstenite::Message;

#[derive(Parser)]
#[command(name = "peer-cli", about = "VAPP-3 spike: str0m data channels over the steer relay")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Relay publisher: answer offers, serve the bench, append `result` envelopes to --out.
    Daemon(DaemonArgs),
    /// Rust offerer: connect to the daemon N times and bench each link.
    Client(ClientArgs),
    /// Allocate once on the TURN server and print the mapped + relayed address.
    TurnProbe(IceArgs),
}

#[derive(Clone, Copy, ValueEnum, PartialEq, Eq, Debug)]
enum Policy {
    All,
    Relay,
    Host,
}

#[derive(Args, Clone)]
struct IceArgs {
    #[arg(long)]
    stun: Option<String>,
    #[arg(long)]
    turn: Option<String>,
    #[arg(long, default_value = "exp")]
    turn_user: String,
    #[arg(long, default_value = "spike")]
    turn_pass: String,
    #[arg(long, default_value = "exponential.local")]
    turn_realm: String,
    #[arg(long, value_enum, default_value = "all")]
    policy: Policy,
    #[arg(long)]
    tamper_sdp: bool,
    #[arg(long)]
    tamper_sig: bool,
}

#[derive(Args, Clone)]
struct RelayArgs {
    #[arg(long, default_value = "ws://localhost:4002")]
    relay: String,
    #[arg(long)]
    ticket: String,
    #[arg(long, default_value = "vapp3-dev")]
    session: String,
    #[arg(long)]
    peer_id: Option<String>,
    #[arg(long)]
    out: std::path::PathBuf,
}

#[derive(Args)]
struct DaemonArgs {
    #[command(flatten)]
    relay: RelayArgs,
    #[command(flatten)]
    ice: IceArgs,
}

#[derive(Args)]
struct ClientArgs {
    #[command(flatten)]
    relay: RelayArgs,
    #[command(flatten)]
    ice: IceArgs,
    #[arg(long, default_value_t = 1)]
    runs: u32,
    #[arg(long, default_value_t = 2_000_000)]
    body_bytes: u64,
    #[arg(long, default_value_t = 100)]
    pings: u32,
    #[arg(long, default_value_t = 0)]
    reconnect: u32,
    #[arg(long, default_value = "cli")]
    source: String,
    #[arg(long, default_value = "same-machine")]
    scenario: String,
    /// Record a `tamper` line per run instead of a bench line (variant = sdp|sig|control).
    #[arg(long)]
    tamper_variant: Option<String>,
    #[arg(long, default_value = "")]
    notes: String,
}

fn policy(p: Policy) -> IcePolicy {
    match p {
        Policy::All => IcePolicy::All,
        Policy::Relay => IcePolicy::Relay,
        Policy::Host => IcePolicy::Host,
    }
}

fn policy_str(p: Policy) -> &'static str {
    match p {
        Policy::All => "all",
        Policy::Relay => "relay",
        Policy::Host => "host",
    }
}

fn peer_config(peer_id: &str, session: &str, ice: &IceArgs) -> PeerConfig {
    let mut c = PeerConfig::new(peer_id, session);
    c.stun = ice.stun.clone();
    c.turn = ice.turn.clone();
    c.turn_username = Some(ice.turn_user.clone());
    c.turn_password = Some(ice.turn_pass.clone());
    c.turn_realm = Some(ice.turn_realm.clone());
    c.policy = policy(ice.policy);
    c.tamper_sdp = ice.tamper_sdp;
    c.tamper_sig = ice.tamper_sig;
    c
}

fn rand_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos());
    format!("{:08x}", h.finish() as u32)
}

/// RFC 3339 UTC timestamp without a date crate.
fn iso_now() -> String {
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let secs = d.as_secs() as i64;
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        d.subsec_millis()
    )
}

fn append_line(path: &std::path::Path, v: &Value) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).with_context(|| format!("open {}", path.display()))?;
    writeln!(f, "{}", v)?;
    Ok(())
}

type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn dial(relay: &str, ticket: &str) -> Result<Ws> {
    let url = format!("{}/ws?ticket={}", relay.trim_end_matches('/'), ticket);
    let (ws, _) = tokio::time::timeout(Duration::from_secs(10), tokio_tungstenite::connect_async(url))
        .await
        .map_err(|_| anyhow!("relay connect timed out"))?
        .map_err(|e| anyhow!("relay connect: {}", e.to_string().split("ticket=").next().unwrap_or("")))?;
    Ok(ws)
}

fn input_frame(env: &str) -> String {
    json!({"t": "input", "data": env}).to_string()
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_writer(std::io::stderr)
        .init();
    match Cli::parse().cmd {
        Cmd::Daemon(a) => daemon(a).await,
        Cmd::Client(a) => client(a).await,
        Cmd::TurnProbe(a) => turn_probe(a).await,
    }
}

async fn turn_probe(mut ice: IceArgs) -> Result<()> {
    if ice.turn.is_none() {
        bail!("--turn host:port required");
    }
    ice.policy = Policy::Relay;
    let cfg = peer_config("probe", "vapp3-probe", &ice);
    let t0 = Instant::now();
    let link = tokio::task::spawn_blocking(move || PeerLink::new(cfg)).await??;
    let d: Value = serde_json::from_str(&link.diagnostics())?;
    println!(
        "{}",
        json!({"ok": true, "allocateMs": t0.elapsed().as_secs_f64() * 1000.0, "mapped": d["turnMapped"], "relayed": d["turnRelayed"], "local": d["local"]})
    );
    link.close();
    tokio::time::sleep(Duration::from_millis(300)).await;
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// daemon
// ------------------------------------------------------------------------------------------------

struct Daemon {
    id: String,
    session: String,
    ice: IceArgs,
    seed: Vec<u8>,
    pubkey: String,
    out: std::path::PathBuf,
    /// Envelopes to publish (narration).
    publish: UnboundedSender<String>,
}

async fn daemon(a: DaemonArgs) -> Result<()> {
    let id = a.relay.peer_id.clone().unwrap_or_else(|| "daemon".into());
    let ident = peer_core::signing::Identity::generate();
    let (publish, mut publish_rx) = unbounded_channel::<String>();
    let d = Arc::new(Daemon {
        id: id.clone(),
        session: a.relay.session.clone(),
        ice: a.ice.clone(),
        seed: ident.seed().to_vec(),
        pubkey: ident.public_key(),
        out: a.relay.out.clone(),
        publish,
    });
    tracing::info!("daemon {id} pub {} policy {:?} turn {:?}", d.pubkey, a.ice.policy, a.ice.turn);
    let peers: Arc<std::sync::Mutex<HashMap<String, UnboundedSender<Envelope>>>> = Default::default();
    let seq = AtomicU64::new(1);
    let hello_env = Envelope::new(&id, None, &d.session, Body::Hello { pubkey: d.pubkey.clone(), version: peer_core::version().into() }).to_json();
    let mut backoff = Duration::from_millis(500);
    loop {
        let mut ws = match dial(&a.relay.relay, &a.relay.ticket).await {
            Ok(ws) => ws,
            Err(e) => {
                tracing::warn!("{e}; retry in {backoff:?}");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(15));
                continue;
            }
        };
        backoff = Duration::from_millis(500);
        ws.send(Message::Text(json!({"t": "hello", "sessionId": d.session}).to_string())).await?;
        tracing::info!("publisher hello sent for room {}", d.session);
        let narr = |text: &str, seq: &AtomicU64| {
            let n = seq.fetch_add(1, Ordering::Relaxed);
            json!({"t": "activity", "seq": n, "event": {"kind": "narration", "text": text, "messageId": format!("vapp3-{n}")}}).to_string()
        };
        let mut hello_tick = tokio::time::interval(Duration::from_secs(15));
        let mut ping_tick = tokio::time::interval(Duration::from_secs(20));
        let why = loop {
            tokio::select! {
                _ = hello_tick.tick() => {
                    if ws.send(Message::Text(narr(&hello_env, &seq))).await.is_err() { break "send hello failed".to_string(); }
                }
                _ = ping_tick.tick() => {
                    if ws.send(Message::Ping(Vec::new())).await.is_err() { break "ping failed".to_string(); }
                }
                Some(env) = publish_rx.recv() => {
                    if ws.send(Message::Text(narr(&env, &seq))).await.is_err() { break "publish failed".to_string(); }
                }
                msg = ws.next() => {
                    let Some(msg) = msg else { break "ws closed".to_string() };
                    let msg = match msg { Ok(m) => m, Err(e) => break format!("ws error {e}") };
                    let Message::Text(t) = msg else { continue };
                    let Ok(v) = serde_json::from_str::<Value>(&t) else { continue };
                    match v["t"].as_str() {
                        Some("input") => {
                            let Some(data) = v["data"].as_str() else { continue };
                            let Some(env) = Envelope::parse(data) else { tracing::debug!("non-envelope input"); continue };
                            if env.to.as_deref().is_some_and(|to| to != d.id) { continue; }
                            route(&d, &peers, env);
                        }
                        Some("keepalive") => {}
                        Some("error") => tracing::warn!("relay error {}", v),
                        Some("bye") => break format!("relay bye {}", v),
                        _ => tracing::debug!("relay frame {}", v["t"]),
                    }
                }
            }
        };
        tracing::warn!("relay connection ended: {why}; reconnecting");
        tokio::time::sleep(backoff).await;
    }
}

fn route(d: &Arc<Daemon>, peers: &Arc<std::sync::Mutex<HashMap<String, UnboundedSender<Envelope>>>>, env: Envelope) {
    if let Body::Result { payload } = &env.body {
        let mut line = payload.clone();
        if let Value::Object(m) = &mut line {
            m.insert("receivedAt".into(), json!(iso_now()));
            m.insert("from".into(), json!(env.from));
        }
        match append_line(&d.out, &line) {
            Ok(()) => tracing::info!("result from {} appended", env.from),
            Err(e) => tracing::warn!("append result: {e}"),
        }
        return;
    }
    let mut map = peers.lock().unwrap();
    let from = env.from.clone();
    let bye = matches!(env.body, Body::Bye);
    let tx = map.entry(from.clone()).or_insert_with(|| {
        let (tx, rx) = unbounded_channel();
        tokio::spawn(peer_task(d.clone(), from.clone(), rx));
        tx
    });
    let _ = tx.send(env);
    if bye {
        map.remove(&from);
    }
}

/// One remote peer: its envelopes are applied strictly in order (candidates may arrive while the
/// link is still gathering).
async fn peer_task(d: Arc<Daemon>, from: String, mut rx: UnboundedReceiver<Envelope>) {
    let mut link: Option<Arc<PeerLink>> = None;
    while let Some(env) = rx.recv().await {
        match &env.body {
            Body::Offer { .. } => {
                let json = env.to_json();
                let existing = link.clone();
                let d2 = d.clone();
                let from2 = from.clone();
                let r = tokio::task::spawn_blocking(move || -> Result<(Arc<PeerLink>, Result<String, PeerError>, bool), PeerError> {
                    let (l, fresh) = match existing {
                        Some(l) => (l, false),
                        None => {
                            let mut cfg = peer_config(&d2.id, &d2.session, &d2.ice);
                            cfg.identity_seed = Some(d2.seed.clone());
                            (PeerLink::new(cfg)?, true)
                        }
                    };
                    let ans = l.accept_offer(&json);
                    tracing::info!("offer from {from2} ({}): {}", if fresh { "new" } else { "re-offer" }, if ans.is_ok() { "answered" } else { "rejected" });
                    Ok((l, ans, fresh))
                })
                .await;
                match r {
                    Ok(Ok((l, Ok(answer), fresh))) => {
                        let _ = d.publish.send(answer);
                        if fresh {
                            start_link_threads(&d, &from, l.clone());
                            link = Some(l);
                        }
                    }
                    Ok(Ok((l, Err(e), _))) => {
                        tracing::warn!("offer from {from}: {e}");
                        for s in l.poll_signals(Duration::from_millis(0)) {
                            let _ = d.publish.send(s);
                        }
                        if link.is_none() {
                            l.close();
                        }
                    }
                    Ok(Err(e)) => {
                        tracing::warn!("link for {from}: {e}");
                        let rej = Envelope::new(&d.id, Some(&from), &d.session, Body::Reject { reason: format!("link_failed: {e}") });
                        let _ = d.publish.send(rej.to_json());
                    }
                    Err(e) => tracing::warn!("join: {e}"),
                }
            }
            Body::Candidate { .. } | Body::EndOfCandidates => {
                if let Some(l) = &link {
                    if let Err(e) = l.add_remote_candidate(&env.to_json()) {
                        tracing::debug!("candidate from {from}: {e}");
                    }
                }
            }
            Body::Bye => {
                if let Some(l) = link.take() {
                    l.close();
                }
                break;
            }
            _ => {}
        }
    }
    if let Some(l) = link {
        l.close();
    }
}

fn start_link_threads(d: &Arc<Daemon>, from: &str, l: Arc<PeerLink>) {
    let done = Arc::new(AtomicBool::new(false));
    {
        let (l, d, done) = (l.clone(), d.clone(), done.clone());
        std::thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                for s in l.poll_signals(Duration::from_millis(250)) {
                    let _ = d.publish.send(s);
                }
            }
        });
    }
    let (d, from) = (d.clone(), from.to_string());
    std::thread::spawn(move || {
        let r = l.serve_bench();
        let diag: Value = serde_json::from_str(&l.diagnostics()).unwrap_or_default();
        tracing::info!(
            "link {from} served: {:?} path {}↔{} ({} ↔ {})",
            r.as_ref().err(),
            diag["localPath"],
            diag["remotePath"],
            diag["localCandidate"],
            diag["remoteCandidate"]
        );
        // The daemon's own view of the path, for the peer's results line.
        let view = json!({"daemonView": {
            "localPath": diag["localPath"], "remotePath": diag["remotePath"],
            "localCandidate": diag["localCandidate"], "remoteCandidate": diag["remoteCandidate"],
            "startToOpenMs": diag["startToOpenMs"], "gatherMs": diag["gatherMs"], "error": r.as_ref().err().map(|e| e.to_string()),
        }});
        let env = Envelope::new(&d.id, Some(&from), &d.session, Body::Result { payload: view });
        let _ = d.publish.send(env.to_json());
        std::thread::sleep(Duration::from_millis(500));
        done.store(true, Ordering::Relaxed);
        l.close();
    });
}

// ------------------------------------------------------------------------------------------------
// client
// ------------------------------------------------------------------------------------------------

async fn client(a: ClientArgs) -> Result<()> {
    let base = a.relay.peer_id.clone().unwrap_or_else(|| format!("cli-{}", rand_id()));
    let ws = dial(&a.relay.relay, &a.relay.ticket).await?;
    let (mut sink, mut stream) = ws.split();
    sink.send(Message::Text(json!({"t": "join", "channel": "activity"}).to_string())).await?;
    let (frame_tx, mut frame_rx) = unbounded_channel::<String>();
    tokio::spawn(async move {
        while let Some(f) = frame_rx.recv().await {
            if sink.send(Message::Text(f)).await.is_err() {
                break;
            }
        }
    });
    // Reader: hellos → watch, envelopes addressed to one of our run ids → env channel.
    let (hello_tx, mut hello_rx) = tokio::sync::watch::channel::<Option<(String, String)>>(None);
    let (env_tx, mut env_rx) = unbounded_channel::<Envelope>();
    let (synced_tx, synced_rx) = tokio::sync::oneshot::channel::<()>();
    let prefix = format!("{base}-");
    tokio::spawn(async move {
        let mut synced = Some(synced_tx);
        let mut replay_hello = None;
        while let Some(Ok(msg)) = stream.next().await {
            let Message::Text(t) = msg else { continue };
            let Ok(v) = serde_json::from_str::<Value>(&t) else { continue };
            match v["t"].as_str() {
                Some("activity") => {
                    let Some(text) = v["event"]["text"].as_str() else { continue };
                    let Some(env) = Envelope::parse(text) else { continue };
                    match &env.body {
                        Body::Hello { pubkey, .. } => {
                            let h = Some((env.from.clone(), pubkey.clone()));
                            if synced.is_some() {
                                replay_hello = h; // take the LAST one of the replay
                            } else {
                                let _ = hello_tx.send(h);
                            }
                        }
                        _ => {
                            if env.to.as_deref().is_some_and(|to| to.starts_with(&prefix)) {
                                let _ = env_tx.send(env);
                            }
                        }
                    }
                }
                Some("activity_synced") | Some("history_pending") => {
                    if let Some(s) = synced.take() {
                        if replay_hello.is_some() {
                            let _ = hello_tx.send(replay_hello.take());
                        }
                        let _ = s.send(());
                    }
                }
                Some("error") => tracing::warn!("relay error {v}"),
                _ => {}
            }
        }
        tracing::warn!("relay viewer socket closed");
    });
    let _ = tokio::time::timeout(Duration::from_secs(10), synced_rx).await;
    let (daemon_id, daemon_pub) = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Some(h) = hello_rx.borrow_and_update().clone() {
                return h;
            }
            hello_rx.changed().await.ok();
        }
    })
    .await
    .map_err(|_| anyhow!("no daemon hello in the room within 20s"))?;
    tracing::info!("daemon {daemon_id} pub {daemon_pub}");

    for run in 0..a.runs {
        let id = format!("{base}-{run}");
        let line = one_run(&a, &id, &daemon_id, &daemon_pub, &frame_tx, &mut env_rx).await;
        if let Err(e) = append_line(&a.relay.out, &line) {
            tracing::warn!("write result: {e}");
        }
        println!("{line}");
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    Ok(())
}

fn patch_to(env_json: &str, to: &str) -> String {
    match Envelope::parse(env_json) {
        Some(mut e) => {
            e.to = Some(to.to_string());
            e.to_json()
        }
        None => env_json.to_string(),
    }
}

/// Wait for an envelope addressed to `id` of the wanted kinds (candidates are applied on the way).
async fn wait_env(env_rx: &mut UnboundedReceiver<Envelope>, id: &str, link: &Arc<PeerLink>, timeout: Duration) -> Option<Envelope> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let env = tokio::time::timeout_at(deadline, env_rx.recv()).await.ok()??;
        if env.to.as_deref() != Some(id) {
            continue;
        }
        match &env.body {
            Body::Candidate { .. } => {
                let _ = link.add_remote_candidate(&env.to_json());
            }
            Body::Answer { .. } | Body::Reject { .. } | Body::Result { .. } => return Some(env),
            _ => {}
        }
    }
}

async fn one_run(
    a: &ClientArgs,
    id: &str,
    daemon_id: &str,
    daemon_pub: &str,
    frames: &UnboundedSender<String>,
    env_rx: &mut UnboundedReceiver<Envelope>,
) -> Value {
    let mut cfg = peer_config(id, &a.relay.session, &a.ice);
    cfg.expected_remote_pubkey = Some(daemon_pub.to_string());
    let mut line = json!({
        "kind": "bench", "at": iso_now(), "source": a.source, "scenario": a.scenario, "policy": policy_str(a.ice.policy),
        "connectMs": null, "rttP50Ms": null, "rttP95Ms": null, "upMbps": null, "downMbps": null, "bodyBytes": a.body_bytes,
        "localPath": null, "remotePath": null, "ok": false, "error": null, "reconnect": null, "notes": a.notes,
    });
    let link = match tokio::task::spawn_blocking(move || PeerLink::new(cfg)).await {
        Ok(Ok(l)) => l,
        Ok(Err(e)) => {
            line["error"] = json!(format!("new: {e}"));
            return line;
        }
        Err(e) => {
            line["error"] = json!(format!("join: {e}"));
            return line;
        }
    };
    // Outbound signal pump (ICE-restart offers, rejects).
    let pump_done = Arc::new(AtomicBool::new(false));
    {
        let (l, f, done, to) = (link.clone(), frames.clone(), pump_done.clone(), daemon_id.to_string());
        std::thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                for s in l.poll_signals(Duration::from_millis(200)) {
                    let _ = f.send(input_frame(&patch_to(&s, &to)));
                }
            }
        });
    }
    let result = drive(a, id, daemon_id, &link, frames, env_rx).await;
    let diag: Value = serde_json::from_str(&link.diagnostics()).unwrap_or_default();
    match result {
        Ok((r, recon, daemon_view)) => {
            if let Some(v) = a.tamper_variant.as_deref() {
                line = json!({"kind": "tamper", "at": iso_now(), "source": a.source, "variant": v, "rejected": false,
                              "reason": "accepted: connected and benched", "notes": a.notes});
            } else {
                line["connectMs"] = json!(r.connect_ms);
                line["rttP50Ms"] = json!(r.rtt_p50_ms);
                line["rttP95Ms"] = json!(r.rtt_p95_ms);
                line["upMbps"] = json!(r.up_mbps);
                line["downMbps"] = json!(r.down_mbps);
                line["localPath"] = json!(r.local_path);
                line["remotePath"] = json!(r.remote_path);
                line["localCandidate"] = json!(r.local_candidate);
                line["remoteCandidate"] = json!(r.remote_candidate);
                line["ok"] = json!(r.errors.is_empty());
                if !r.errors.is_empty() {
                    line["error"] = json!(r.errors.join("; "));
                }
                if let Some((first, all)) = recon {
                    line["reconnect"] = json!({"trigger": "manual", "ms": first});
                    if all.len() > 1 {
                        line["reconnectAllMs"] = json!(all);
                    }
                }
                line["gatherMs"] = diag["gatherMs"].clone();
                line["daemonView"] = daemon_view.unwrap_or(Value::Null);
            }
        }
        Err(RunErr::Rejected(reason)) => {
            if let Some(v) = a.tamper_variant.as_deref() {
                line = json!({"kind": "tamper", "at": iso_now(), "source": a.source, "variant": v, "rejected": true, "reason": reason, "notes": a.notes});
            } else {
                line["error"] = json!(format!("rejected: {reason}"));
            }
        }
        Err(RunErr::Other(e)) => {
            if let Some(v) = a.tamper_variant.as_deref() {
                line = json!({"kind": "tamper", "at": iso_now(), "source": a.source, "variant": v, "rejected": false, "reason": format!("error: {e}"), "notes": a.notes});
            } else {
                line["error"] = json!(e);
                line["notes"] = json!(format!("{} diag={}", a.notes, diag));
            }
        }
    }
    let _ = frames.send(input_frame(&Envelope::new(id, Some(daemon_id), &a.relay.session, Body::Bye).to_json()));
    pump_done.store(true, Ordering::Relaxed);
    link.close();
    line
}

enum RunErr {
    Rejected(String),
    Other(String),
}

type DriveOk = (peer_core::BenchResult, Option<(f64, Vec<f64>)>, Option<Value>);

async fn drive(
    a: &ClientArgs,
    id: &str,
    daemon_id: &str,
    link: &Arc<PeerLink>,
    frames: &UnboundedSender<String>,
    env_rx: &mut UnboundedReceiver<Envelope>,
) -> Result<DriveOk, RunErr> {
    let offer = {
        let l = link.clone();
        tokio::task::spawn_blocking(move || l.create_offer()).await.map_err(|e| RunErr::Other(e.to_string()))?
    }
    .map_err(|e| RunErr::Other(format!("create_offer: {e}")))?;
    let offer = patch_to(&offer, daemon_id);
    if offer.len() > 8000 {
        tracing::warn!("offer envelope is {} bytes (relay input cap 8 KiB)", offer.len());
    }
    frames.send(input_frame(&offer)).map_err(|_| RunErr::Other("relay gone".into()))?;
    apply_answer(id, link, env_rx).await?;
    let (body, pings) = (a.body_bytes, a.pings);
    let l = link.clone();
    let r = tokio::task::spawn_blocking(move || l.run_bench(body, pings))
        .await
        .map_err(|e| RunErr::Other(e.to_string()))?
        .map_err(|e| RunErr::Other(format!("bench: {e}")))?;
    let mut recon = None;
    if a.reconnect > 0 {
        let mut all = Vec::new();
        for k in 0..a.reconnect {
            let t0 = Instant::now();
            link.restart_ice().map_err(|e| RunErr::Other(format!("restart_ice: {e}")))?;
            apply_answer(id, link, env_rx).await?;
            let l = link.clone();
            tokio::task::spawn_blocking(move || peer_core::bench::ping_once(&l, 10_000 + k, Duration::from_secs(10)))
                .await
                .map_err(|e| RunErr::Other(e.to_string()))?
                .map_err(|e| RunErr::Other(format!("ping after restart: {e}")))?;
            all.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        recon = Some((all[0], all));
    }
    // Tell the daemon we are done, then collect its view of the path.
    let _ = link.send(&[peer_core::bench::CLOSE]);
    let view = match wait_env(env_rx, id, link, Duration::from_secs(3)).await {
        Some(Envelope { body: Body::Result { payload }, .. }) => payload.get("daemonView").cloned(),
        _ => None,
    };
    Ok((r, recon, view))
}

async fn apply_answer(id: &str, link: &Arc<PeerLink>, env_rx: &mut UnboundedReceiver<Envelope>) -> Result<(), RunErr> {
    match wait_env(env_rx, id, link, Duration::from_secs(15)).await {
        Some(env) => match &env.body {
            Body::Answer { .. } => {
                let l = link.clone();
                let json = env.to_json();
                match tokio::task::spawn_blocking(move || l.accept_answer(&json)).await {
                    Ok(Ok(())) => Ok(()),
                    Ok(Err(PeerError::SignatureRejected(r))) => Err(RunErr::Rejected(format!("answer rejected locally: {r}"))),
                    Ok(Err(e)) => Err(RunErr::Other(format!("accept_answer: {e}"))),
                    Err(e) => Err(RunErr::Other(e.to_string())),
                }
            }
            Body::Reject { reason } => Err(RunErr::Rejected(reason.clone())),
            _ => Err(RunErr::Other("unexpected envelope".into())),
        },
        None => Err(RunErr::Other("no answer within 15s".into())),
    }
}
