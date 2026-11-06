//! VAPP-261 (contract E0 VAPP-260): an app instance's lifecycle as one pure
//! reducer; the host's `WindowHost` reports the platform's events, the
//! effects say what the instance does. Locked by
//! `packages/eengine/fixtures/lifecycle.json`. Screens resolve through
//! genUI's host: `resolve_screen` for the app's own taps, `cross_app_target`
//! for links from outside.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    #[default]
    Stopped,
    Foreground,
    Background,
}

/// The instance's state: its phase and the open screen (none while stopped).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleState {
    pub phase: Phase,
    pub screen: Option<String>,
    pub params: Option<Map<String, Value>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LifecycleEvent {
    Launch,
    Foreground,
    Background,
    Terminate,
    Online,
    DeepLink {
        #[serde(default)]
        screen: Option<String>,
        #[serde(default)]
        params: Option<Value>,
    },
    NotificationTap {
        app: String,
        #[serde(default)]
        screen: Option<String>,
        #[serde(default)]
        params: Option<Value>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "effect", rename_all = "camelCase")]
pub enum Effect {
    Open { screen: String, params: Map<String, Value> },
    Flush,
    Sources { live: bool },
    Automations { on: bool },
    Persist { screen: String, params: Map<String, Value> },
    Refused { reason: String },
}

/// What the reducer reads besides the state: the package and the screen it persisted last.
#[derive(Clone, Copy, Debug)]
pub struct LifecycleContext<'a> {
    pub pkg: &'a Value,
    pub last_screen: Option<(&'a str, &'a Value)>,
}

type Target = (String, Map<String, Value>);

fn plain_target(cx: &LifecycleContext) -> Option<Target> {
    let (screen, params) = match cx.last_screen {
        Some((s, p)) => (Some(s), Some(p)),
        None => (None, None),
    };
    eui_core::host::resolve_screen(cx.pkg, screen, params).map(|r| (r.screen, r.params))
}

fn link_target(event: &LifecycleEvent, cx: &LifecycleContext) -> Option<Result<Target, String>> {
    match event {
        LifecycleEvent::DeepLink { screen, params } => {
            Some(eui_core::host::cross_app_target(cx.pkg, screen.as_deref(), params.as_ref()).map(|t| (t.screen, t.params)))
        }
        LifecycleEvent::NotificationTap { app, screen, params } => {
            if cx.pkg.get("id").and_then(Value::as_str) != Some(app.as_str()) {
                return Some(Err("otherApp".into()));
            }
            Some(eui_core::host::resolve_screen(cx.pkg, screen.as_deref(), params.as_ref()).map(|r| (r.screen, r.params)).ok_or_else(|| "no such screen".to_string()))
        }
        _ => None,
    }
}

fn cold_launch(target: Option<Target>, mut effects: Vec<Effect>) -> (LifecycleState, Vec<Effect>) {
    effects.push(Effect::Automations { on: true });
    if let Some((screen, params)) = &target {
        effects.push(Effect::Open { screen: screen.clone(), params: params.clone() });
    }
    effects.push(Effect::Sources { live: true });
    effects.push(Effect::Flush);
    let (screen, params) = match target {
        Some((s, p)) => (Some(s), Some(p)),
        None => (None, None),
    };
    (LifecycleState { phase: Phase::Foreground, screen, params }, effects)
}

/// One lifecycle event → the next state and its effects, in order.
pub fn lifecycle_step(state: &LifecycleState, event: &LifecycleEvent, cx: &LifecycleContext) -> (LifecycleState, Vec<Effect>) {
    let running = state.phase != Phase::Stopped;
    let ignored = || (state.clone(), Vec::new());
    let persist = || -> Vec<Effect> {
        match &state.screen {
            Some(screen) => vec![Effect::Persist { screen: screen.clone(), params: state.params.clone().unwrap_or_default() }],
            None => Vec::new(),
        }
    };
    match event {
        LifecycleEvent::Launch => {
            if running {
                ignored()
            } else {
                cold_launch(plain_target(cx), Vec::new())
            }
        }
        LifecycleEvent::Foreground => {
            if state.phase != Phase::Background {
                return ignored();
            }
            (LifecycleState { phase: Phase::Foreground, ..state.clone() }, vec![Effect::Sources { live: true }, Effect::Flush])
        }
        LifecycleEvent::Background => {
            if state.phase != Phase::Foreground {
                return ignored();
            }
            let mut effects = persist();
            effects.push(Effect::Sources { live: false });
            (LifecycleState { phase: Phase::Background, ..state.clone() }, effects)
        }
        LifecycleEvent::Terminate => {
            if !running {
                return ignored();
            }
            let mut effects = persist();
            if state.phase == Phase::Foreground {
                effects.push(Effect::Sources { live: false });
            }
            effects.push(Effect::Automations { on: false });
            (LifecycleState::default(), effects)
        }
        LifecycleEvent::Online => {
            if state.phase == Phase::Foreground {
                (state.clone(), vec![Effect::Flush])
            } else {
                ignored()
            }
        }
        LifecycleEvent::DeepLink { .. } | LifecycleEvent::NotificationTap { .. } => match link_target(event, cx) {
            None => ignored(),
            Some(Err(reason)) => {
                let refused = Effect::Refused { reason };
                if running {
                    (state.clone(), vec![refused])
                } else {
                    cold_launch(plain_target(cx), vec![refused])
                }
            }
            Some(Ok(target)) => {
                if !running {
                    return cold_launch(Some(target), Vec::new());
                }
                let (screen, params) = target;
                let mut effects = vec![Effect::Open { screen: screen.clone(), params: params.clone() }];
                if state.phase == Phase::Background {
                    effects.push(Effect::Sources { live: true });
                    effects.push(Effect::Flush);
                }
                (LifecycleState { phase: Phase::Foreground, screen: Some(screen), params: Some(params) }, effects)
            }
        },
    }
}
