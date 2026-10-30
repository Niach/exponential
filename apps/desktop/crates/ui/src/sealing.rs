//! EXP-1459: the desktop's sealed session content (design #EXP-1457,
//! `packages/domain-contract/fixtures/sealed-content.json`).
//!
//! * READ: `sync::sealed` opens every row where it enters memory; this
//!   module hands it the device's seal service and re-opens the closed rows
//!   whenever the service's change counter moves (a wrap arrived).
//! * WRITE: person messages and card answers are sealed before
//!   `sessions.send` / `sessions.answer` ([`send_blocking`],
//!   [`answer_blocking`]); mentions ride clear as `mentionUserIds`. Without
//!   a service (or a key) the message goes as plaintext, which stays legal.

use std::sync::Arc;
use std::time::Duration;

use api::sessions::{AnswerInput, SealWrap, SendInput, SendOutput, SendSeal};
use domain::plan_card::{CardAnswerInput, CardAnswers};
use domain::rows::SessionRow;
use gpui::App;
use serde_json::Value;
use sync::sealed::{OpenedBody, OpenedCard, Opener, Opens};
use sync::Store;

pub use self::service::{service, NewRootSeal, SealService};

/// The device sibling's `coding::sealing` surface (#EXP-1458 slice "device +
/// agent-to-agent"), mirrored until it lands on master: `service()` is
/// `None` until then (plaintext sends, locked rows).
mod service {
    use super::*;

    /// A new tree's key, wrapped for the readers the writer resolved.
    pub struct NewRootSeal {
        pub install_id: String,
        pub public_key: String,
        pub epoch: u32,
        pub wraps: Vec<SealWrap>,
        pub secret: [u8; 32],
    }

    pub trait SealService: Send + Sync {
        fn open_body(&self, root_id: &str, body: &str) -> OpenedBody;
        fn open_card(&self, root_id: &str, card: &Value) -> OpenedCard;
        /// BLOCKING (creates the tree key when the root has none).
        fn seal_body(&self, root_id: &str, text: &str) -> Result<String, String>;
        /// BLOCKING.
        fn seal_card(&self, root_id: &str, card: &Value) -> Result<Value, String>;
        fn open_session_title(&self, row: &SessionRow) -> Option<String>;
        /// Moves whenever new secrets land.
        fn changed(&self) -> u64;
        /// BLOCKING: a fresh tree key for a root `sessions.send` creates.
        fn seal_new_root(&self, team_id: &str, device_install_ids: &[String], shared: bool) -> Result<NewRootSeal, String>;
        /// The new root's id is known: cache its secret.
        fn adopt(&self, root_id: &str, secret: [u8; 32]);
    }

    pub fn service() -> Option<Arc<dyn SealService>> {
        None
    }
}

/// `sync::sealed::Opens` over the seal service.
struct ServiceOpens(Arc<dyn SealService>);

impl Opens for ServiceOpens {
    fn open_body(&self, root_id: &str, body: &str) -> OpenedBody {
        self.0.open_body(root_id, body)
    }
    fn open_card(&self, root_id: &str, card: &Value) -> OpenedCard {
        self.0.open_card(root_id, card)
    }
    fn open_title(&self, row: &SessionRow) -> Option<String> {
        self.0.open_session_title(row)
    }
}

/// How often the change counter is read (a cheap atomic in the service).
const KEYS_POLL: Duration = Duration::from_secs(2);

/// Bootstrap (after `Store::open`): hands the service to the opener once it
/// exists and re-opens the closed rows whenever it reports new keys.
pub fn init(cx: &mut App) {
    cx.spawn(async move |cx| {
        let mut attached = false;
        let mut seen = 0;
        loop {
            cx.background_executor().timer(KEYS_POLL).await;
            let Some(service) = service() else {
                continue;
            };
            if !attached {
                Opener::global().set_keys(Arc::new(ServiceOpens(Arc::clone(&service))));
                attached = true;
                seen = u64::MAX;
            }
            let changed = service.changed();
            if changed != seen {
                seen = changed;
                cx.update(|cx| Store::global(cx).clone().reopen_sealed(cx));
            }
        }
    })
    .detach();
}

/// Where a send's text goes, decided on the foreground ([`plan_send`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub enum SealTarget {
    /// An existing tree: seal for its root (its key is created if missing).
    Root(String),
    /// `sessions.send` creates the root: its key rides inline (`seal`),
    /// wrapped for the executing device's install too (`device_id`).
    NewRoot { team_id: String, device_id: Option<String>, shared: bool },
    /// Nothing to seal (no text, no name).
    #[default]
    None,
}

/// A send's sealing facts, read on the foreground before the blocking call.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SendPlan {
    pub target: SealTarget,
    pub mention_user_ids: Vec<String>,
    /// `refIssueIds`: the `#IDENT` refs the text makes, resolved here.
    pub ref_issue_ids: Vec<String>,
}

/// The tree a send writes into and the users its text mentions.
pub fn plan_send(input: &SendInput, cx: &App) -> SendPlan {
    let collections = Store::global(cx).collections();
    let mention_user_ids = input
        .text
        .as_deref()
        .map(|text| {
            let users = collections.users.read(cx);
            let users: Vec<(&str, &str)> =
                users.iter().filter_map(|user| Some((user.id.as_str(), user.email.as_deref()?))).collect();
            domain::sealed::mention_user_ids(text, users.iter().copied())
        })
        .unwrap_or_default();
    let sessions = collections.sessions.read(cx);
    let team_id = input
        .team_id
        .clone()
        .or_else(|| input.session_id.as_ref().or(input.parent_id.as_ref()).and_then(|id| sessions.get(id)?.team_id.clone()));
    let ref_issue_ids = match (input.text.as_deref(), team_id.as_deref()) {
        (Some(text), Some(team_id)) => {
            let issues = collections.issue_refs_in_team(team_id, cx);
            domain::sealed::ref_issue_ids(text, issues.iter().map(|issue| (issue.id.as_str(), issue.identifier.as_str())))
        }
        _ => Vec::new(),
    };
    let root_of = |id: &str| {
        sessions
            .get(id)
            .map(|row| row.root_id_or_self().to_string())
            .or_else(|| Opener::global().root_of(id))
    };
    let has_content = input.text.is_some() || input.name.is_some();
    let target = if !has_content {
        SealTarget::None
    } else if let Some(session_id) = &input.session_id {
        root_of(session_id).map_or(SealTarget::None, SealTarget::Root)
    } else if let Some(parent_id) = &input.parent_id {
        root_of(parent_id).map_or(SealTarget::None, SealTarget::Root)
    } else if let Some(existing) = existing_conversation(input, &sessions.iter().collect::<Vec<_>>(), cx) {
        SealTarget::Root(existing)
    } else if let Some(team_id) = &input.team_id {
        SealTarget::NewRoot { team_id: team_id.clone(), device_id: input.device_id.clone(), shared: false }
    } else {
        SealTarget::None
    };
    SendPlan { target, mention_user_ids, ref_issue_ids }
}

/// An agent / app send without a session id lands in the member's existing
/// conversation root when there is one.
fn existing_conversation(input: &SendInput, sessions: &[&SessionRow], cx: &App) -> Option<String> {
    let me = crate::queries::active_account(cx)?.user_id;
    let mine = |row: &&&SessionRow| row.is_root() && row.user_id.as_deref() == Some(me.as_str());
    if let Some(agent_id) = &input.agent_id {
        return sessions.iter().filter(mine).find(|row| row.agent_id.as_ref() == Some(agent_id)).map(|row| row.id.clone());
    }
    if let Some(app_id) = &input.app_id {
        return sessions.iter().filter(mine).find(|row| row.app_id.as_ref() == Some(app_id)).map(|row| row.id.clone());
    }
    None
}

/// Seals `input` per `plan` (BLOCKING): the text and name become body
/// envelopes, a new root's key rides inline. Returns the new root's secret
/// to adopt once the server names the root. Any failure leaves the content
/// plaintext.
fn seal_send(
    service: &dyn SealService,
    input: &mut SendInput,
    plan: &SendPlan,
    device_installs: impl FnOnce(&str, &str) -> Vec<String>,
) -> Option<[u8; 32]> {
    match &plan.target {
        SealTarget::None => None,
        SealTarget::Root(root_id) => {
            let sealed_text = match input.text.as_deref() {
                Some(text) => match service.seal_body(root_id, text) {
                    Ok(sealed) => Some(sealed),
                    Err(err) => {
                        log::warn!("[sealing] sending plaintext, no key for {root_id}: {err}");
                        return None;
                    }
                },
                None => None,
            };
            let sealed_name = input.name.as_deref().and_then(|name| service.seal_body(root_id, name).ok());
            if sealed_text.is_some() {
                input.text = sealed_text;
            }
            if sealed_name.is_some() {
                input.sealed_name = sealed_name;
                input.name = None;
            }
            None
        }
        SealTarget::NewRoot { team_id, device_id, shared } => {
            // The executing device's installs read the tree from its first
            // message on; a gap is filled by any holder (`pending`).
            let devices = device_id.as_deref().map(|device| device_installs(team_id, device)).unwrap_or_default();
            let seal = match service.seal_new_root(team_id, &devices, *shared) {
                Ok(seal) => seal,
                Err(err) => {
                    log::warn!("[sealing] sending plaintext, no new tree key: {err}");
                    return None;
                }
            };
            let public = enet_core::sealed::public_key_from_b64(&seal.public_key).ok()?;
            let body = |text: &str| enet_core::sealed::seal_body(&public, seal.epoch, text).ok();
            let sealed_text = match input.text.as_deref() {
                Some(text) => Some(body(text)?),
                None => None,
            };
            if let Some(name) = input.name.as_deref() {
                input.sealed_name = Some(body(name)?);
                input.name = None;
            }
            if sealed_text.is_some() {
                input.text = sealed_text;
            }
            input.seal = Some(SendSeal {
                install_id: seal.install_id,
                public_key: seal.public_key,
                epoch: seal.epoch,
                wraps: seal.wraps,
            });
            Some(seal.secret)
        }
    }
}

/// THE sealed `sessions.send` (BLOCKING, off the UI thread).
pub fn send_blocking(trpc: &api::TrpcClient, input: &SendInput, plan: &SendPlan) -> Result<SendOutput, api::ApiError> {
    let mut input = input.clone();
    if !plan.mention_user_ids.is_empty() {
        input.mention_user_ids = Some(plan.mention_user_ids.clone());
    }
    if !plan.ref_issue_ids.is_empty() {
        input.ref_issue_ids = Some(plan.ref_issue_ids.clone());
    }
    let device_installs = |team_id: &str, device_id: &str| match api::installs::list(trpc, team_id) {
        Ok(rows) => rows
            .into_iter()
            .filter(|row| row.device_id.as_deref() == Some(device_id))
            .map(|row| row.install_id)
            .collect(),
        Err(err) => {
            log::warn!("[sealing] installs.list {team_id} failed: {err}");
            Vec::new()
        }
    };
    let service = service();
    let secret = service.as_deref().and_then(|service| seal_send(service, &mut input, plan, device_installs));
    let out = api::sessions::send(trpc, &input)?;
    if let (Some(service), Some(secret)) = (service, secret) {
        service.adopt(&out.session_id, secret);
    }
    Ok(out)
}

/// An answer's sealing facts: the card's tree and the card as opened.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnswerPlan {
    pub root_id: Option<String>,
    pub card: Option<Value>,
}

pub fn plan_answer(message_id: &str, cx: &App) -> AnswerPlan {
    let messages = Store::global(cx).collections().messages.read(cx);
    let Some(row) = messages.get(message_id) else {
        return AnswerPlan::default();
    };
    AnswerPlan { root_id: row.root_id.clone(), card: row.card.clone().filter(|_| !domain::sealed::is_closed(row.seal)) }
}

/// `input` as a sealed answer: the FULL card re-sealed with the answer, the
/// clear `decision` / `fresh` beside it, no plaintext answer fields.
pub fn seal_answer(service: &dyn SealService, input: &AnswerInput, plan: &AnswerPlan) -> Option<AnswerInput> {
    let (root_id, card) = (plan.root_id.as_deref()?, plan.card.as_ref()?);
    let answers = input.answers.as_ref().and_then(|answers| match answers {
        Value::Array(labels) => Some(CardAnswers::Labels(labels.iter().filter_map(|l| l.as_str().map(str::to_owned)).collect())),
        Value::Object(_) => serde_json::from_value(answers.clone()).ok().map(CardAnswers::ByQuestion),
        _ => None,
    });
    let answered = domain::plan_card::answered_card(
        card,
        &CardAnswerInput {
            answer: input.answer.clone(),
            answers,
            decision: input.decision.clone(),
            fresh: input.fresh,
            feedback: input.feedback.clone(),
            surface_data: input.surface_data.clone(),
        },
    )?;
    let sealed = service.seal_card(root_id, &answered).map_err(|err| log::warn!("[sealing] answer stays plaintext: {err}")).ok()?;
    Some(AnswerInput {
        message_id: input.message_id.clone(),
        decision: input.decision.clone(),
        fresh: input.fresh,
        sealed_card: Some(sealed),
        ..AnswerInput::default()
    })
}

/// THE sealed `sessions.answer` (BLOCKING, off the UI thread).
pub fn answer_blocking(trpc: &api::TrpcClient, input: &AnswerInput, plan: &AnswerPlan) -> Result<api::sessions::TxOutput, api::ApiError> {
    let sealed = service().and_then(|service| seal_answer(service.as_ref(), input, plan));
    api::sessions::answer(trpc, sealed.as_ref().unwrap_or(input))
}

#[cfg(test)]
mod tests {
    use super::*;
    use enet_core::sealed as contract;
    use std::sync::Mutex;

    /// A one-tree service on a fixed key.
    struct OneTree {
        key: contract::TreeKey,
        adopted: Mutex<Vec<String>>,
        devices: Mutex<Vec<String>>,
    }

    impl SealService for OneTree {
        fn open_body(&self, _root_id: &str, body: &str) -> OpenedBody {
            contract::open_body(body, &|epoch| (epoch == 1).then_some(self.key.secret))
        }
        fn open_card(&self, _root_id: &str, card: &Value) -> OpenedCard {
            contract::open_card(card, &|epoch| (epoch == 1).then_some(self.key.secret))
        }
        fn seal_body(&self, _root_id: &str, text: &str) -> Result<String, String> {
            contract::seal_body(&self.key.public, 1, text).map_err(|e| e.to_string())
        }
        fn seal_card(&self, _root_id: &str, card: &Value) -> Result<Value, String> {
            contract::seal_card(&self.key.public, 1, card).map_err(|e| e.to_string())
        }
        fn open_session_title(&self, _row: &SessionRow) -> Option<String> {
            None
        }
        fn changed(&self) -> u64 {
            0
        }
        fn seal_new_root(&self, _team_id: &str, devices: &[String], _shared: bool) -> Result<NewRootSeal, String> {
            *self.devices.lock().unwrap() = devices.to_vec();
            Ok(NewRootSeal {
                install_id: "me".into(),
                public_key: self.key.public_key(),
                epoch: 1,
                wraps: vec![SealWrap { install_id: "me".into(), wrapped: "w".into() }],
                secret: self.key.secret,
            })
        }
        fn adopt(&self, root_id: &str, _secret: [u8; 32]) {
            self.adopted.lock().unwrap().push(root_id.into());
        }
    }

    fn tree() -> OneTree {
        OneTree { key: contract::generate_tree_key(), adopted: Mutex::new(Vec::new()), devices: Mutex::new(Vec::new()) }
    }

    #[test]
    fn a_message_to_an_existing_tree_seals_text_and_name() {
        let service = tree();
        let mut input = SendInput::message("s1", "hello @a@b.co", Vec::new());
        input.name = Some("Title".into());
        let plan = SendPlan { target: SealTarget::Root("r1".into()), ..SendPlan::default() };
        assert_eq!(seal_send(&service, &mut input, &plan, |_, _| unreachable!()), None);
        let text = input.text.as_deref().unwrap();
        assert!(text.starts_with("exp-sealed:v1:1:"));
        assert_eq!(service.open_body("r1", text).text.as_deref(), Some("hello @a@b.co"));
        assert_eq!(input.name, None);
        assert_eq!(service.open_body("r1", input.sealed_name.as_deref().unwrap()).text.as_deref(), Some("Title"));
        assert!(input.seal.is_none());
    }

    #[test]
    fn a_new_root_carries_its_key_inline() {
        let service = tree();
        let mut input = SendInput { team_id: Some("t".into()), device_id: Some("d".into()), text: Some("go".into()), ..SendInput::default() };
        let plan = SendPlan {
            target: SealTarget::NewRoot { team_id: "t".into(), device_id: Some("d".into()), shared: false },
            ..SendPlan::default()
        };
        let mut asked = None;
        let installs = |team: &str, device: &str| {
            asked = Some((team.to_string(), device.to_string()));
            vec!["host-install".to_string()]
        };
        assert_eq!(seal_send(&service, &mut input, &plan, installs), Some(service.key.secret));
        assert_eq!(asked, Some(("t".into(), "d".into())));
        assert_eq!(service.devices.lock().unwrap().clone(), vec!["host-install".to_string()]);
        let seal = input.seal.clone().unwrap();
        assert_eq!((seal.install_id.as_str(), seal.epoch, seal.public_key), ("me", 1, service.key.public_key()));
        assert_eq!(service.open_body("new", input.text.as_deref().unwrap()).text.as_deref(), Some("go"));
        let wire = serde_json::to_value(&input).unwrap();
        assert_eq!(wire["seal"]["wraps"][0], serde_json::json!({"installId": "me", "wrapped": "w"}));
    }

    #[test]
    fn an_answer_reseals_the_full_card_and_drops_plaintext_fields() {
        let service = tree();
        let card = serde_json::json!({"kind": "question", "question": "Which?", "options": ["A", "B"]});
        let input = AnswerInput {
            message_id: "m1".into(),
            answer: Some("A".into()),
            answers: Some(serde_json::json!(["A"])),
            ..AnswerInput::default()
        };
        let plan = AnswerPlan { root_id: Some("r1".into()), card: Some(card) };
        let sealed = seal_answer(&service, &input, &plan).unwrap();
        assert_eq!((sealed.answer.clone(), sealed.answers.clone(), sealed.feedback.clone()), (None, None, None));
        let wire = serde_json::to_value(&sealed).unwrap();
        assert_eq!(wire["sealedCard"]["kind"], "question");
        assert_eq!(wire["sealedCard"]["answered"], true);
        let opened = service.open_card("r1", &wire["sealedCard"]).card;
        assert_eq!(opened["answers"], serde_json::json!(["A"]));
        assert_eq!(opened["question"], "Which?");

        let plan_card = AnswerPlan { root_id: Some("r1".into()), card: Some(serde_json::json!({"kind": "plan", "plan": "p"})) };
        let decide = AnswerInput { message_id: "m2".into(), decision: Some("rejected".into()), feedback: Some("no".into()), ..AnswerInput::default() };
        let sealed = seal_answer(&service, &decide, &plan_card).unwrap();
        assert_eq!((sealed.decision.as_deref(), sealed.feedback.as_deref()), (Some("rejected"), None));
        let wire = serde_json::to_value(&sealed).unwrap();
        assert_eq!(wire["sealedCard"]["decision"], "rejected");
        assert_eq!(service.open_card("r1", &wire["sealedCard"]).card["feedback"], "no");
        // An unknown card (not synced, or closed here) never seals.
        assert!(seal_answer(&service, &decide, &AnswerPlan::default()).is_none());
    }
}
