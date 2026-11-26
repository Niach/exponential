//! #EXP-2143 follow-up: "Confirm it's you" — the step-up sheet's reducer,
//! locked ×4 by `packages/domain-contract/fixtures/step-up.json` (web
//! `@exp/domain-contract/step-up`, iOS ExpCore `StepUp`, Android
//! `domain/StepUp`). A SESSION_NOT_FRESH refusal opens the sheet in place; a
//! proof marks the same session fresh and the refused action resumes.
//!
//! The desktop registers no passkeys itself (Add passkey opens the web
//! Account page, where the web sheet runs), so nothing in the app calls this
//! yet; it stays the desktop's replay of the shared rule so a future
//! fresh-gated native action starts from the same machine.

use serde::{Deserialize, Serialize};

pub const NOT_FRESH: &str = "SESSION_NOT_FRESH";
/// The person dismissed the system passkey prompt: no error.
pub const CANCELLED: &str = "CANCELLED";
pub const NOT_FRESH_COPY: &str = "Sign in again to do this (your current session is older than a day).";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Method {
    Passkey,
    Email,
    Password,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sheet {
    Closed,
    Open,
    Busy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Idle,
    Waiting,
    Resume,
    Dropped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub sheet: Sheet,
    pub method: Option<Method>,
    pub methods: Vec<Method>,
    pub error: Option<String>,
    pub action: Action,
    pub retried: bool,
}

impl Default for State {
    fn default() -> Self {
        Self { sheet: Sheet::Closed, method: None, methods: vec![], error: None, action: Action::Idle, retried: false }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "on", rename_all = "lowercase")]
pub enum Event {
    Refused { code: String, methods: Vec<Method> },
    Choose { method: Method },
    Submit,
    Proved,
    Rejected { code: String },
    Cancel,
    Done,
}

/// The sentence for a refused proof (`CANCELLED` = none).
pub fn error_copy(code: &str) -> Option<String> {
    let text = match code {
        CANCELLED => return None,
        "INVALID_OTP" => "That code is not right. Check the email and try again.",
        "OTP_EXPIRED" => "That code expired. Request a new one.",
        "TOO_MANY_ATTEMPTS" => "Too many attempts. Request a new code.",
        "INVALID_PASSWORD" => "That password is not right.",
        "PASSKEY_FAILED" => "Couldn't confirm it with that passkey.",
        _ => "Couldn't confirm it's you. Try again.",
    };
    Some(text.to_string())
}

pub fn reduce(state: &State, event: &Event) -> State {
    let mut next = state.clone();
    match event {
        Event::Refused { code, methods } => {
            next.sheet = Sheet::Closed;
            next.action = Action::Failed;
            if code != NOT_FRESH {
                next.error = None;
            } else if state.retried || methods.is_empty() {
                next.error = Some(NOT_FRESH_COPY.to_string());
                if !state.retried {
                    next.methods = vec![];
                }
            } else {
                next.sheet = Sheet::Open;
                next.method = Some(methods[0]);
                next.methods = methods.clone();
                next.error = None;
                next.action = Action::Waiting;
            }
        }
        Event::Choose { method } => {
            if state.sheet == Sheet::Open && state.methods.contains(method) {
                next.method = Some(*method);
                next.error = None;
            }
        }
        Event::Submit => {
            if state.sheet == Sheet::Open {
                next.sheet = Sheet::Busy;
                next.error = None;
            }
        }
        Event::Proved => {
            if state.sheet == Sheet::Busy {
                next.sheet = Sheet::Closed;
                next.error = None;
                next.action = Action::Resume;
                next.retried = true;
            }
        }
        Event::Rejected { code } => {
            if state.sheet == Sheet::Busy {
                next.sheet = Sheet::Open;
                next.error = error_copy(code);
            }
        }
        Event::Cancel => {
            if state.sheet != Sheet::Closed {
                next.sheet = Sheet::Closed;
                next.error = None;
                next.action = Action::Dropped;
            }
        }
        Event::Done => {
            next.action = Action::Idle;
            next.retried = false;
        }
    }
    next
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/step-up.json");

    #[derive(Deserialize)]
    struct Flow {
        name: String,
        events: Vec<Event>,
        expect: State,
    }

    #[derive(Deserialize)]
    struct Fixture {
        initial: State,
        errors: BTreeMap<String, String>,
        copy: serde_json::Value,
        flows: Vec<Flow>,
    }

    fn fixture() -> Fixture {
        serde_json::from_str(FIXTURE).expect("the step-up fixture parses")
    }

    #[test]
    fn initial_state_and_copy_match_the_fixture() {
        let fixture = fixture();
        assert_eq!(fixture.initial, State::default());
        assert_eq!(fixture.copy["notFresh"], NOT_FRESH_COPY);
        for (code, text) in &fixture.errors {
            let code = if code == "default" { "SOMETHING_ELSE" } else { code.as_str() };
            assert_eq!(error_copy(code).as_deref(), Some(text.as_str()), "{code}");
        }
        assert_eq!(error_copy(CANCELLED), None);
    }

    #[test]
    fn flows() {
        for flow in fixture().flows {
            let end = flow.events.iter().fold(State::default(), |state, event| reduce(&state, event));
            assert_eq!(end, flow.expect, "{}", flow.name);
        }
    }
}
