//! A host without a server (spezl's shape): one local agent session, a local
//! transcript, settings in a local file. Compiles the `TalkHost` contract; no
//! window opens. A real app calls `etalk_gpui::init(cx, Box::new(host))` in
//! its gpui `Application::run` and then feeds `set_settings` /
//! `messages_changed` as its own state changes.
//!
//! `cargo run -p etalk-gpui --example host`

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use etalk::meeting::{AudioApp, DesktopAudioCapability};
use etalk::voice::{UtteranceTarget, VoiceMessage, VoiceSettings};
use etalk_gpui::{
    Brand, CallConnection, Notice, Outgoing, SendJob, Sent, SignedIn, Surface, TalkAgent, TalkDevice, TalkHost, TalkIcon, TalkMessage,
    VoiceWrite,
};
use gpui::{App, Task};
use gpui_component::{Icon, IconName};

/// The one local agent session.
const SESSION: &str = "local";
const AGENT: &str = "studio";

/// One line of the local transcript.
#[derive(Clone, Debug)]
struct Line {
    id: String,
    /// `user` or `agent`.
    author: &'static str,
    text: String,
    created_at: String,
}

struct LocalHost {
    data_dir: PathBuf,
    transcript: Rc<RefCell<Vec<Line>>>,
    settings: Rc<RefCell<VoiceSettings>>,
}

impl LocalHost {
    fn new(data_dir: PathBuf) -> Self {
        // This machine is the voice device, the one agent is picked.
        let settings = VoiceSettings {
            voice_device_id: Some("this-machine".into()),
            agent_id: Some(AGENT.into()),
            ptt_shortcut: "ctrl+space".into(),
            ..Default::default()
        };
        LocalHost { data_dir, transcript: Rc::default(), settings: Rc::new(RefCell::new(settings)) }
    }
}

impl TalkHost for LocalHost {
    fn brand(&self) -> Brand {
        Brand {
            name: "spezl",
            tray_icon_svg: br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><circle cx="50" cy="50" r="48" fill="#ffffff"></circle>
</svg>"##,
            app_id: "app.spezl",
        }
    }

    fn data_dir(&self) -> PathBuf {
        self.data_dir.clone()
    }

    fn device_id(&self) -> String {
        "this-machine".into()
    }

    fn account(&self, _cx: &App) -> Option<String> {
        Some("local".into())
    }

    fn accounts(&self, _cx: &App) -> SignedIn {
        SignedIn { active: Some("local".into()), all: vec!["local".into()] }
    }

    fn agent(&self, agent_id: &str, _cx: &App) -> Option<TalkAgent> {
        (agent_id == AGENT).then(|| TalkAgent { name: "Studio".into(), title: "Studio".into(), ..Default::default() })
    }

    fn conversation(&self, agent_id: &str, _cx: &App) -> Option<String> {
        (agent_id == AGENT).then(|| SESSION.into())
    }

    fn messages(&self, _cx: &App, visit: &mut dyn FnMut(&TalkMessage<'_>)) -> bool {
        for line in self.transcript.borrow().iter() {
            visit(&TalkMessage {
                id: &line.id,
                message: VoiceMessage {
                    author_kind: Some(line.author),
                    body: Some(&line.text),
                    has_card: false,
                    author_session_id: None,
                    session_id: Some(SESSION),
                    created_at: Some(&line.created_at),
                },
            });
        }
        true
    }

    fn send(&self, outgoing: Outgoing, _cx: &App) -> Option<SendJob> {
        let text = match outgoing.target {
            UtteranceTarget::Session { text, .. } | UtteranceTarget::Agent { text, .. } => text,
        };
        // A real host hands `text` to its agent session here (and appends
        // the user's line and, later, the reply to the transcript, calling
        // `etalk_gpui::messages_changed`).
        Some(Box::new(move || {
            println!("to the agent: {text}");
            Ok(Sent { session_id: SESSION.into(), message_id: None })
        }))
    }

    fn write(&self, write: VoiceWrite, done: Box<dyn FnOnce(&mut App)>, cx: &mut App) -> bool {
        let mut stored = self.settings.borrow().clone();
        match write {
            VoiceWrite::WakeWord(on) => stored.wake_word = on,
            VoiceWrite::StartMeeting => stored.meeting_started_at = Some(chrono::Utc::now().to_rfc3339()),
            VoiceWrite::StopMeeting(_) => stored.meeting_started_at = None,
        }
        *self.settings.borrow_mut() = stored.clone();
        cx.defer(move |cx| {
            etalk_gpui::set_settings(stored, cx);
            done(cx);
        });
        true
    }

    fn connect_call(&self, _call_id: &str, _cx: &mut App) -> Option<Task<Result<CallConnection, String>>> {
        // No relay: nobody calls in from elsewhere.
        None
    }

    fn device(&self, device_id: &str, _cx: &App) -> Option<TalkDevice> {
        (device_id == "this-machine").then(|| TalkDevice { label: Some("This Mac".into()), online: true })
    }

    fn desktop_audio(&self, _device_id: &str, _cx: &App) -> (DesktopAudioCapability, Vec<AudioApp>) {
        let capability = match etalk::apps::capability() {
            etalk::apps::Capability::PerApp => DesktopAudioCapability::PerApp,
            etalk::apps::Capability::System => DesktopAudioCapability::System,
            etalk::apps::Capability::None => DesktopAudioCapability::None,
        };
        (capability, Vec::new())
    }

    fn icon(&self, icon: TalkIcon<'_>) -> Icon {
        match icon {
            TalkIcon::Brand => Icon::new(IconName::Bot),
            TalkIcon::Concept(_) | TalkIcon::Agent { .. } => Icon::new(IconName::CircleUser),
        }
    }

    fn open(&self, surface: Surface, _cx: &mut App) {
        println!("open {surface:?}");
    }

    fn notify(&self, notice: Notice, _cx: &mut App) {
        println!("{notice:?}");
    }

    fn readiness_changed(&self, _cx: &mut App) {}
}

fn main() {
    let host = LocalHost::new(std::env::temp_dir().join("spezl-example"));
    host.transcript.borrow_mut().push(Line {
        id: "m1".into(),
        author: "agent",
        text: "Hello.".into(),
        created_at: "2026-10-29T09:00:00Z".into(),
    });
    let host: Box<dyn TalkHost> = Box::new(host);
    let brand = host.brand();
    println!("{} hosts eTalk on {} (data in {})", brand.name, host.device_id(), host.data_dir().display());
}
