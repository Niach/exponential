//! The state, events and effects the fixtures speak (`packages/eagent/src/model.ts`):
//! camelCase JSON, the same field names. Times are unix ms, ids follow eCore's
//! id rule, refusals are eCore `Failure`s and every event is an eCore `Envelope`.

use std::collections::BTreeMap;

use ecore::{Envelope, Failure};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

/// A JSON field that can be absent, `null` or set: TypeScript's `undefined`,
/// `null` and a value, kept apart because the fixtures compare them apart.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Field<T> {
    #[default]
    Missing,
    Null,
    Value(T),
}

impl<T> Field<T> {
    pub fn is_missing(&self) -> bool {
        matches!(self, Field::Missing)
    }

    /// `== null` in TypeScript: absent or null.
    pub fn is_nullish(&self) -> bool {
        !matches!(self, Field::Value(_))
    }

    pub fn value(&self) -> Option<&T> {
        match self {
            Field::Value(v) => Some(v),
            _ => None,
        }
    }

    /// `null` for `None`.
    pub fn from_option(value: Option<T>) -> Self {
        value.map_or(Field::Null, Field::Value)
    }
}

impl<T: Serialize> Serialize for Field<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Field::Value(v) => v.serialize(serializer),
            _ => serializer.serialize_none(),
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Field<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Option::<T>::deserialize(deserializer)?.map_or(Field::Null, Field::Value))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Queued,
    Running,
    Idle,
    Ended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockedKind {
    RateLimit,
    DeviceOffline,
    Unclaimed,
    Stuck,
    Restarted,
}

impl BlockedKind {
    /// `device_offline` and `restarted` are INTERRUPTIONS.
    pub fn is_interruption(self) -> bool {
        matches!(self, BlockedKind::DeviceOffline | BlockedKind::Restarted)
    }
}

/// Why a live session is held. `resume` = it was mid-turn (its device
/// continues it), `resumeSince` = since when the device could. A device's
/// wall keeps whatever else it said (`extra`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Blocked {
    pub kind: BlockedKind,
    pub since: i64,
    #[serde(default, skip_serializing_if = "Field::is_missing")]
    pub window: Field<String>,
    /// The device's own stamp (RFC 3339), kept verbatim; unparseable = never expires.
    #[serde(default, skip_serializing_if = "Field::is_missing")]
    pub resets_at: Field<String>,
    #[serde(default, skip_serializing_if = "Field::is_missing")]
    pub resume: Field<bool>,
    #[serde(default, skip_serializing_if = "Field::is_missing")]
    pub resume_since: Field<i64>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HubSession {
    pub id: String,
    pub parent_id: Option<String>,
    pub root_id: String,
    pub depth: i64,
    pub team_id: String,
    pub user_id: String,
    pub shared: bool,
    /// A team agent's conversation (persistent: it never ends).
    pub agent_id: Option<String>,
    /// The agent whose session started this root.
    pub started_by_agent_id: Option<String>,
    pub device_id: String,
    pub status: Status,
    pub busy: bool,
    pub background: bool,
    pub needs_input: bool,
    pub blocked: Option<Blocked>,
    /// Sessions this one watches (the bus wakes it with their transitions).
    pub wake_on: Vec<String>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub last_turn_at: Option<i64>,
    pub last_activity_at: Option<i64>,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanOption {
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanQuestion {
    pub id: String,
    pub prompt: String,
    pub options: Vec<PlanOption>,
}

/// A `kind: "event"` card: a session transition (`sessionId`, maybe
/// `pullRequestId`) or a `trigger_fired` (`agentId`, `triggerId`, `eventId`, `name`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventCard {
    pub event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Field::is_missing")]
    pub trigger_id: Field<String>,
    #[serde(default, skip_serializing_if = "Field::is_missing")]
    pub event_id: Field<String>,
    #[serde(default, skip_serializing_if = "Field::is_missing")]
    pub name: Field<String>,
}

impl EventCard {
    pub fn session(event: &str, session_id: &str) -> Self {
        Self {
            event: event.to_string(),
            session_id: Some(session_id.to_string()),
            pull_request_id: None,
            agent_id: None,
            trigger_id: Field::Missing,
            event_id: Field::Missing,
            name: Field::Missing,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum Card {
    Question {
        question: String,
        /// Non-string options are kept and skipped when rendered (like the TS).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        options: Option<Vec<Value>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        multi: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        custom: Option<bool>,
    },
    Plan {
        plan: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        questions: Option<Vec<PlanQuestion>>,
    },
    Ask {
        session_id: String,
        message_id: String,
    },
    Report {
        session_id: String,
        summary: String,
        #[serde(rename = "final")]
        is_final: bool,
    },
    Event(EventCard),
    Session {
        session_id: String,
    },
    ThreadStarted {
        session_id: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorKind {
    User,
    Agent,
    System,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HubMessage {
    pub id: String,
    /// The conversation (session) it is in.
    pub session_id: String,
    pub author_kind: AuthorKind,
    /// The session that wrote it into another's conversation (the bus, a send).
    pub author_session_id: Option<String>,
    pub body: String,
    pub card: Option<Card>,
    /// Sealed to its tree (EXP-1458): never read by the hub.
    pub sealed: bool,
    /// Unconsumed input its reader's device still has to deliver.
    pub pending: bool,
    /// A question/plan card's answer (or an `ask` mirror closed with it).
    pub answered: bool,
    pub at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HubDevice {
    pub id: String,
    pub last_seen_at: i64,
    /// The sessions its process runs (the heartbeat's claim); null = an older device that never says.
    pub live_session_ids: Option<Vec<String>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
    Schedule,
    Event,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Trigger {
    pub id: String,
    pub kind: TriggerKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Agent,
    /// Watches all its member's chats (its quiet-tick rule).
    Assistant,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HubAgent {
    pub id: String,
    pub owner_id: String,
    pub team_id: String,
    pub role: AgentRole,
    /// The owner's conversation session with the agent (where firings land).
    pub conversation_id: String,
    pub triggers: Vec<Trigger>,
    /// triggerId → the due minute it last fired for.
    pub fired_at: BTreeMap<String, i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HubState {
    /// The last message id handed out (`m<seq>`).
    pub seq: u64,
    pub sessions: BTreeMap<String, HubSession>,
    pub messages: Vec<HubMessage>,
    pub devices: BTreeMap<String, HubDevice>,
    pub agents: BTreeMap<String, HubAgent>,
}

/// Everything the hub does besides its own state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum Effect {
    Post {
        message_id: String,
        to: String,
        author_kind: AuthorKind,
        author_session_id: Option<String>,
        body: String,
        card: Option<Card>,
        sealed: bool,
        /// Pending input for `to` (its device delivers it: a wake-up).
        waking: bool,
    },
    Edit {
        message_id: String,
        body: String,
        waking: bool,
    },
    /// The first input a claim delivers as the Request (null = none).
    Brief {
        session_id: String,
        message_id: Option<String>,
        sealed: bool,
    },
    Nudge {
        device_id: String,
    },
    Kill {
        session_id: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Field::is_missing")]
    pub message_id: Field<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_ids: Option<Vec<String>>,
}

impl StepResult {
    pub fn session(id: &str) -> Self {
        Self { session_id: Some(id.to_string()), ..Self::default() }
    }
}

/// One event's outcome: the next state, the effects and the result, or an
/// eCore refusal with the state unchanged and no effects.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
    pub state: HubState,
    pub effects: Vec<Effect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<StepResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<Failure>,
}

pub type HubEvent = Envelope<Value>;
