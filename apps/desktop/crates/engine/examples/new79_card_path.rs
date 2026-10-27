//! #NEW-79 — the live card path: the REAL claude CLI under the voice posture
//! (`AdapterSpec::voice_policy`), one warm-up turn, then a direct command
//! whose tool call the policy intercepts. The client side runs the engine's
//! own mapper, so "on screen" = the Dock question event THE card renders
//! from. Prints the time from the command to that event, then denies the
//! card so nothing runs.
//!
//! `cargo run -p engine --example new79_card_path -- [model] [runs] [command]`
//! (defaults: `haiku`, 3, a `touch` in the temp dir). Uses the account the
//! environment's `CLAUDE_CONFIG_DIR` (or `~/.claude`) is signed into.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent_client_protocol::schema::v1::{
    ContentBlock, CreateElicitationRequest, CreateElicitationResponse, ElicitationAcceptAction,
    ElicitationAction, ElicitationContentValue, InitializeRequest, NewSessionRequest,
    PromptRequest, SessionNotification, TextContent,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{on_receive_notification, on_receive_request, Client, ConnectionTo, Error};
use engine::adapters::claude::ClaudeAgent;
use engine::adapters::{AdapterKind, AdapterSpec};
use engine::mapper::{MapOut, Mapper, MapperConfig};

fn spec(model: &str, work: &PathBuf) -> AdapterSpec {
    let program = std::env::var("EXP_CLAUDE_BIN").unwrap_or_else(|_| "claude".to_string());
    AdapterSpec {
        kind: AdapterKind::Claude,
        agent: coding::CodingAgent::Claude,
        spawn: terminal::pty::SpawnSpec::new(program).cwd(work),
        options: coding::LaunchOptions {
            agent: coding::CodingAgent::Claude,
            model: model.to_string(),
            effort: String::new(),
            ultracode: false,
            plan_mode: false,
            subagent_model: String::new(),
            mcp_server_ids: Vec::new(),
            account: None,
            computer_use: None,
        },
        // No Exponential server here (a closed port): the probe's call is a
        // built-in, so nothing reaches a real instance.
        mcp: coding::AgentMcp::ClaudeInline { url: "http://127.0.0.1:9/api/mcp".to_string(), session_id: None },
        servers: Vec::new(),
        cwd: work.clone(),
        session_id: "new79-probe".to_string(),
        prompt: None,
        resume: None,
        replay: false,
        personal_key: None,
        reaper_settings_path: None,
        system_append: "You are a voice assistant. Act on direct commands at once, without asking first.".to_string(),
        context_layers: coding::ContextLayers::default(),
        name_conversation: false,
        aside: false,
        confine_to: None,
        voice_policy: true,
        exit: engine::ChildExitLink::new(),
    }
}

#[derive(Default)]
struct Seen {
    command_at: Option<Instant>,
    card_after: Option<Duration>,
    question: Option<String>,
}

async fn one_run(model: &str, command: &str) -> Result<Seen, Error> {
    let work = std::env::temp_dir().join(format!("new79-probe-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&work).expect("a work dir");
    let seen = Arc::new(Mutex::new(Seen::default()));
    let mapper = Arc::new(Mutex::new(Mapper::new(MapperConfig {
        redactor: Arc::new(steer::Redactor::new(Vec::new())),
        cwd: work.clone(),
        agent: steer::SessionAgent::Claude,
        session_seed: "new79-probe".to_string(),
    })));
    let notify_mapper = mapper.clone();
    let elicit_seen = seen.clone();
    let command = command.to_string();
    let prompt_seen = seen.clone();
    let adapter = ClaudeAgent::new(spec(model, &work)).expect("the adapter builds");
    let result = Client
        .builder()
        .name("new79-probe")
        .on_receive_notification(
            async move |notification: SessionNotification, _cx| {
                notify_mapper.lock().unwrap().on_update(&notification, &mut MapOut::default());
                Ok(())
            },
            on_receive_notification!(),
        )
        .on_receive_request(
            async move |request: CreateElicitationRequest, responder, _cx| {
                // What the host does: the mapper turns it into THE Dock card.
                let mut out = MapOut::default();
                mapper.lock().unwrap().on_elicitation("probe", &request, &mut out);
                let shown = out.wire.iter().any(|event| matches!(event, steer::ActivityEvent::Question { .. }));
                {
                    let mut seen = elicit_seen.lock().unwrap();
                    if shown && seen.card_after.is_none() {
                        seen.card_after = seen.command_at.map(|at| at.elapsed());
                        seen.question = Some(request.message.clone());
                    }
                }
                let mut content = BTreeMap::new();
                content.insert("decision".to_string(), ElicitationContentValue::String("Deny".to_string()));
                responder.respond(CreateElicitationResponse::new(ElicitationAction::Accept(
                    ElicitationAcceptAction::new().content(content),
                )))
            },
            on_receive_request!(),
        )
        .connect_with(adapter, async move |cx: ConnectionTo<agent_client_protocol::Agent>| {
            cx.send_request(InitializeRequest::new(ProtocolVersion::V1).client_capabilities(engine::client_capabilities()))
                .block_task()
                .await?;
            let session = cx.send_request(NewSessionRequest::new(work.clone())).block_task().await?;
            // Warm-up: the conversation is live before the command, as it is
            // in a voice conversation.
            cx.send_request(PromptRequest::new(
                session.session_id.clone(),
                vec![ContentBlock::Text(TextContent::new("Reply with the single word ok."))],
            ))
            .block_task()
            .await?;
            prompt_seen.lock().unwrap().command_at = Some(Instant::now());
            cx.send_request(PromptRequest::new(
                session.session_id.clone(),
                vec![ContentBlock::Text(TextContent::new(command))],
            ))
            .block_task()
            .await?;
            Ok::<_, Error>(())
        });
    tokio::time::timeout(Duration::from_secs(180), result).await.expect("the run finishes inside 3 min")?;
    let seen = std::mem::take(&mut *seen.lock().unwrap());
    Ok(seen)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let model = args.first().cloned().unwrap_or_else(|| "haiku".to_string());
    let runs: usize = args.get(1).and_then(|n| n.parse().ok()).unwrap_or(3);
    let target = std::env::temp_dir().join("new79-probe-touched");
    let command = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| format!("Run the shell command `touch {}` now.", target.display()));
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("a runtime");
    let mut times = Vec::new();
    for run in 1..=runs {
        match runtime.block_on(one_run(&model, &command)) {
            Ok(seen) => {
                match seen.card_after {
                    Some(after) => {
                        println!("run {run}: card after {} ms", after.as_millis());
                        times.push(after);
                    }
                    None => println!("run {run}: NO card"),
                }
                if let Some(question) = seen.question {
                    println!("  card: {}", question.replace('\n', " ⏎ "));
                }
            }
            Err(error) => println!("run {run}: failed: {error:?}"),
        }
    }
    println!("touched (must be false, the card was denied): {}", target.exists());
    if !times.is_empty() {
        times.sort();
        println!(
            "model {model}: {} runs, median {} ms, max {} ms",
            times.len(),
            times[times.len() / 2].as_millis(),
            times.last().unwrap().as_millis()
        );
    }
}
