//! [`RemoteSession`]: a run inside Exponential, started with its
//! `sessions.send` (phones and web, where nothing can spawn). eagent stays
//! server-free: the host implements [`SessionsSend`] (tRPC, MCP, a test
//! double) and forwards the remote session's `wire` events into the sink it
//! is handed; this adapter maps the brief onto the call and marks the turn
//! edges, so a viewer reads the same events as from a local run.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::{AgentProvider, Brief, EventSink, Failure, HubGrant, RunControl, RunHandle};
use crate::wire::{ActivityEvent, TurnState};

/// One `sessions.send` call, as this adapter asks for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionsSendRequest {
    /// The Exponential session to message (the brief's `resume`); `None` =
    /// start a new one where the host decides (its device, its board).
    pub session_id: Option<String>,
    /// The message: the system append (if any) first, then the prompt.
    pub text: String,
    /// The hub grant's `teamId`: the team the run acts in.
    pub team_id: Option<String>,
}

/// Exponential's `sessions.send`, implemented by the host.
pub trait SessionsSend: Send + Sync + 'static {
    /// Send `request`; answer the session id it reached. Until that session
    /// ends, the host forwards its `wire` events into `sink`.
    fn send(&self, request: SessionsSendRequest, sink: Arc<dyn EventSink>) -> Result<String, Failure>;

    /// `sessions.send { sessionId, stop: true }`: interrupt its turn.
    fn stop(&self, session_id: &str);
}

/// A run in an Exponential session.
pub struct RemoteSession {
    host: Arc<dyn SessionsSend>,
}

impl RemoteSession {
    pub const ID: &'static str = "remote";

    pub fn new(host: Arc<dyn SessionsSend>) -> RemoteSession {
        RemoteSession { host }
    }

    /// The call a brief becomes.
    pub fn request(brief: &Brief, tools: &HubGrant) -> SessionsSendRequest {
        let append = brief.system_append.trim();
        let text = if append.is_empty() { brief.prompt.clone() } else { format!("{append}\n\n{}", brief.prompt) };
        SessionsSendRequest { session_id: brief.resume.clone(), text, team_id: tools.team_id.clone() }
    }
}

impl AgentProvider for RemoteSession {
    fn id(&self) -> &str {
        Self::ID
    }

    /// The grant's endpoint and token are the device's own hub: a remote
    /// session gets its tools from Exponential's MCP gate instead, so only
    /// the team travels.
    fn start(&self, brief: Brief, tools: HubGrant, sink: Arc<dyn EventSink>) -> Result<RunHandle, Failure> {
        let request = Self::request(&brief, &tools);
        sink.send(ActivityEvent::turn(TurnState::Started));
        let remote = match self.host.send(request, sink.clone()) {
            Ok(remote) => remote,
            Err(failure) => {
                sink.send(ActivityEvent::turn(TurnState::Ended));
                return Err(failure);
            }
        };
        let control = RemoteRun { host: self.host.clone(), remote, sink, done: AtomicBool::new(false) };
        Ok(RunHandle::new(Self::ID, brief.session_id, control))
    }
}

struct RemoteRun {
    host: Arc<dyn SessionsSend>,
    remote: String,
    sink: Arc<dyn EventSink>,
    done: AtomicBool,
}

impl RunControl for RemoteRun {
    fn stop(&self) {
        if !self.done.swap(true, Ordering::SeqCst) {
            self.host.stop(&self.remote);
            self.sink.send(ActivityEvent::turn(TurnState::Ended));
        }
    }

    fn is_done(&self) -> bool {
        self.done.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::engine::RecordingSink;
    use crate::provider::tests::{grant, session};
    use crate::provider::ErrorCode;

    #[derive(Default)]
    struct FakeHost {
        sent: Mutex<Vec<SessionsSendRequest>>,
        stopped: Mutex<Vec<String>>,
        refuse: bool,
    }

    impl SessionsSend for FakeHost {
        fn send(&self, request: SessionsSendRequest, sink: Arc<dyn EventSink>) -> Result<String, Failure> {
            if self.refuse {
                return Err(Failure::new(ErrorCode::Forbidden, "not a member"));
            }
            self.sent.lock().unwrap().push(request);
            // The host forwards what the remote session says.
            sink.send(ActivityEvent::narration("on it"));
            Ok("remote-7".to_string())
        }

        fn stop(&self, session_id: &str) {
            self.stopped.lock().unwrap().push(session_id.to_string());
        }
    }

    fn kinds(events: &[ActivityEvent]) -> Vec<String> {
        events
            .iter()
            .map(|event| match event {
                ActivityEvent::Turn { state, .. } => format!("turn:{}", state.id()),
                ActivityEvent::Narration { text, .. } => format!("narration:{text}"),
                other => format!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn a_brief_becomes_one_sessions_send() {
        let brief = Brief::new(session(), "ship it").system_append("you are the release shepherd").resume("sess-9");
        assert_eq!(
            RemoteSession::request(&brief, &grant()),
            SessionsSendRequest {
                session_id: Some("sess-9".to_string()),
                text: "you are the release shepherd\n\nship it".to_string(),
                team_id: Some("t1".to_string()),
            }
        );
        let fresh = RemoteSession::request(&Brief::new(session(), "hi"), &grant());
        assert_eq!(fresh.session_id, None);
        assert_eq!(fresh.text, "hi");
    }

    #[test]
    fn the_remote_events_reach_the_sink_between_turn_edges() {
        let host = Arc::new(FakeHost::default());
        let sink = RecordingSink::new();
        let handle = RemoteSession::new(host.clone()).start(Brief::new(session(), "hi"), grant(), sink.clone()).unwrap();
        assert_eq!(handle.provider(), "remote");
        assert_eq!(host.sent.lock().unwrap().len(), 1);
        assert!(!handle.is_done());
        handle.stop();
        handle.stop();
        assert!(handle.is_done());
        assert_eq!(*host.stopped.lock().unwrap(), ["remote-7"], "one stop reaches the host");
        assert_eq!(kinds(&sink.drain()), ["turn:started", "narration:on it", "turn:ended"]);
    }

    #[test]
    fn a_refused_send_is_the_failure_and_an_ended_turn() {
        let host = Arc::new(FakeHost { refuse: true, ..FakeHost::default() });
        let sink = RecordingSink::new();
        let failure = RemoteSession::new(host).start(Brief::new(session(), "hi"), grant(), sink.clone()).err().unwrap();
        assert_eq!(failure.code, ErrorCode::Forbidden);
        assert!(failure.is_refusal());
        assert_eq!(kinds(&sink.drain()), ["turn:started", "turn:ended"]);
    }
}
