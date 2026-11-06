//! `LocalModel` (cargo feature `local-model`, off by default): an in-process
//! model as an [`AgentProvider`], the offline fallback (#EXP-1721). It spawns
//! nothing and links nothing: the model sits behind [`Generate`], which the
//! genUI generator `eui-gen` (#SA-8, llama.cpp + llguidance) implements in
//! the host. `eui-gen` is not in this tree yet, so this is the trait it
//! fills; eagent itself never links llama.cpp, with or without the feature.
//!
//! A run is one turn on its own thread: `Turn { started }`, each generated
//! piece as narration, an `ApiError` row when the model fails, then
//! `Turn { ended }`: the same `wire` events the ACP engine emits.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::{AgentProvider, Brief, ErrorCode, EventSink, Failure, HubGrant, RunControl, RunHandle};
use crate::wire::{ActivityEvent, TurnState};

/// What the model is asked for.
#[derive(Clone, Debug, PartialEq)]
pub struct GenerateRequest {
    /// The system prompt (the brief's append).
    pub system: String,
    pub prompt: String,
    /// The brief's model pick; blank = the generator's default.
    pub model: String,
    /// The run's tools: the generator lists and calls them through the hub.
    pub tools: HubGrant,
}

/// An in-process generator (`eui-gen`'s, later).
pub trait Generate: Send + Sync + 'static {
    /// Is a model loaded and ready?
    fn ready(&self) -> bool {
        true
    }

    /// Generate the reply, handing each piece of text to `emit` as it comes.
    /// Return early once `stop` is set. `Err` = the model failed (its message).
    fn generate(&self, request: &GenerateRequest, emit: &mut dyn FnMut(&str), stop: &AtomicBool) -> Result<(), String>;
}

/// An in-process model.
pub struct LocalModel {
    model: Arc<dyn Generate>,
}

impl LocalModel {
    pub const ID: &'static str = "local-model";

    pub fn new(model: Arc<dyn Generate>) -> LocalModel {
        LocalModel { model }
    }

    pub fn request(brief: &Brief, tools: HubGrant) -> GenerateRequest {
        GenerateRequest {
            system: brief.system_append.clone(),
            prompt: brief.prompt.clone(),
            model: brief.options.model.clone(),
            tools,
        }
    }
}

/// One turn, start to end, into `sink` (the thread body, and what the tests drive).
pub fn run_turn(model: &dyn Generate, request: &GenerateRequest, sink: &dyn EventSink, stop: &AtomicBool) {
    sink.send(ActivityEvent::turn(TurnState::Started));
    let mut emit = |piece: &str| {
        if !piece.is_empty() {
            sink.send(ActivityEvent::narration(piece));
        }
    };
    if let Err(message) = model.generate(request, &mut emit, stop) {
        sink.send(ActivityEvent::api_error(message, Some("local_model".to_string())));
    }
    sink.send(ActivityEvent::turn(TurnState::Ended));
}

impl AgentProvider for LocalModel {
    fn id(&self) -> &str {
        Self::ID
    }

    fn available(&self) -> bool {
        self.model.ready()
    }

    fn start(&self, brief: Brief, tools: HubGrant, sink: Arc<dyn EventSink>) -> Result<RunHandle, Failure> {
        if let Some(refusal) = tools.refusal() {
            return Err(refusal);
        }
        if !self.model.ready() {
            return Err(Failure::new(ErrorCode::Unavailable, "no local model is loaded"));
        }
        let request = Self::request(&brief, tools);
        let stop = Arc::new(AtomicBool::new(false));
        let done = Arc::new(AtomicBool::new(false));
        let (model, thread_stop, thread_done) = (self.model.clone(), stop.clone(), done.clone());
        std::thread::Builder::new()
            .name("eagent-local-model".to_string())
            .spawn(move || {
                run_turn(model.as_ref(), &request, sink.as_ref(), &thread_stop);
                thread_done.store(true, Ordering::SeqCst);
            })
            .map_err(|err| Failure::new(ErrorCode::Internal, &format!("could not start the local model: {err}")))?;
        Ok(RunHandle::new(Self::ID, brief.session_id, LocalTurn { stop, done }))
    }
}

struct LocalTurn {
    stop: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
}

impl RunControl for LocalTurn {
    fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    fn is_done(&self) -> bool {
        self.done.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::engine::RecordingSink;
    use crate::provider::tests::{grant, session};

    struct Scripted {
        pieces: Vec<&'static str>,
        fail: Option<&'static str>,
        ready: bool,
    }

    impl Generate for Scripted {
        fn ready(&self) -> bool {
            self.ready
        }

        fn generate(&self, _request: &GenerateRequest, emit: &mut dyn FnMut(&str), stop: &AtomicBool) -> Result<(), String> {
            for piece in &self.pieces {
                if stop.load(Ordering::SeqCst) {
                    return Ok(());
                }
                emit(piece);
            }
            self.fail.map_or(Ok(()), |message| Err(message.to_string()))
        }
    }

    fn kinds(events: &[ActivityEvent]) -> Vec<String> {
        events
            .iter()
            .map(|event| match event {
                ActivityEvent::Turn { state, .. } => format!("turn:{}", state.id()),
                ActivityEvent::Narration { text, .. } => format!("narration:{text}"),
                ActivityEvent::ApiError { message, .. } => format!("error:{message}"),
                other => format!("{other:?}"),
            })
            .collect()
    }

    fn model(pieces: Vec<&'static str>, fail: Option<&'static str>) -> Scripted {
        Scripted { pieces, fail, ready: true }
    }

    #[test]
    fn a_turn_is_narration_between_turn_edges() {
        let sink = RecordingSink::new();
        let request = LocalModel::request(&Brief::new(session(), "hi").system_append("terse"), grant());
        assert_eq!(request.system, "terse");
        run_turn(&model(vec!["hel", "", "lo"], None), &request, sink.as_ref(), &AtomicBool::new(false));
        assert_eq!(kinds(&sink.drain()), ["turn:started", "narration:hel", "narration:lo", "turn:ended"]);
    }

    #[test]
    fn a_failing_model_is_an_api_error_row() {
        let sink = RecordingSink::new();
        let request = LocalModel::request(&Brief::new(session(), "hi"), grant());
        run_turn(&model(vec!["a"], Some("out of memory")), &request, sink.as_ref(), &AtomicBool::new(false));
        assert_eq!(kinds(&sink.drain()), ["turn:started", "narration:a", "error:out of memory", "turn:ended"]);
    }

    #[test]
    fn a_stopped_turn_still_ends() {
        let sink = RecordingSink::new();
        let request = LocalModel::request(&Brief::new(session(), "hi"), grant());
        run_turn(&model(vec!["a", "b"], None), &request, sink.as_ref(), &AtomicBool::new(true));
        assert_eq!(kinds(&sink.drain()), ["turn:started", "turn:ended"]);
    }

    #[test]
    fn a_start_runs_on_its_own_thread_and_finishes() {
        let sink = RecordingSink::new();
        let provider = LocalModel::new(Arc::new(model(vec!["ok"], None)));
        let handle = provider.start(Brief::new(session(), "hi"), grant(), sink.clone()).unwrap();
        assert_eq!(handle.provider(), "local-model");
        let deadline = Instant::now() + Duration::from_secs(10);
        while !handle.is_done() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(handle.is_done());
        assert_eq!(kinds(&sink.drain()), ["turn:started", "narration:ok", "turn:ended"]);
    }

    #[test]
    fn no_loaded_model_is_unavailable() {
        let provider = LocalModel::new(Arc::new(Scripted { pieces: Vec::new(), fail: None, ready: false }));
        assert!(!provider.available());
        let failure = provider.start(Brief::new(session(), "hi"), grant(), RecordingSink::new()).err().unwrap();
        assert_eq!(failure.code, ErrorCode::Unavailable);
    }
}
