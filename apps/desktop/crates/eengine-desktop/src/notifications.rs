//! `notifications`: post (now or at `at`), replace by id, cancel, and the
//! tap round trip. Scheduling is IN-PROCESS: one scheduler thread fires a
//! pending notification at its time ONLY WHILE THE HOST RUNS (a host that
//! quits loses its pending ones; nothing is handed to the OS scheduler, so
//! cancel and the tap behave the same on every platform).

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::time::Duration;

use chrono::{DateTime, Utc};
use eengine::providers::Failure;
use eengine::Capability;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::mcp::{self, answer};
use crate::{NativeError, NativeResult, Notifier, OnTap, ShownHandle};

/// What a [`Notifier`] shows. `id` = the bridge's `<app>/<id>`.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub id: String,
    pub title: String,
    pub body: Option<String>,
}

#[derive(Deserialize)]
struct PostArgs {
    id: String,
    title: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    at: Option<String>,
    #[serde(default)]
    tap: Option<Value>,
}

#[derive(Deserialize)]
struct CancelArgs {
    id: String,
}

struct Pending {
    at: DateTime<Utc>,
    shown: Shown,
    tap: Option<Value>,
}

struct ShownEntry {
    generation: u64,
    handle: Option<ShownHandle>,
}

#[derive(Default)]
struct State {
    pending: HashMap<String, Pending>,
    shown: HashMap<String, ShownEntry>,
    generation: u64,
    scheduler: bool,
    shutdown: bool,
}

struct Center {
    notifier: Arc<dyn Notifier>,
    on_tap: OnTap,
    state: Mutex<State>,
    wake: Condvar,
    /// False in tests: [`NotificationsCapability::tick`] drives time.
    spawn_scheduler: bool,
}

pub struct NotificationsCapability {
    center: Arc<Center>,
}

/// The longest the scheduler sleeps (wall-clock changes are noticed within it).
const MAX_SLEEP: Duration = Duration::from_secs(30);

impl NotificationsCapability {
    pub fn new(notifier: Arc<dyn Notifier>, on_tap: OnTap) -> Self {
        Self::build(notifier, on_tap, true)
    }

    pub(crate) fn build(notifier: Arc<dyn Notifier>, on_tap: OnTap, spawn_scheduler: bool) -> Self {
        let center = Center { notifier, on_tap, state: Mutex::new(State::default()), wake: Condvar::new(), spawn_scheduler };
        NotificationsCapability { center: Arc::new(center) }
    }

    /// Fire every pending notification due at `now` (the scheduler thread's step).
    pub fn tick(&self, now: DateTime<Utc>) {
        Center::tick(&self.center, now);
    }

    /// How many are waiting for their time.
    pub fn pending_count(&self) -> usize {
        self.center.state.lock().unwrap().pending.len()
    }

    pub(crate) fn post_at(&self, arguments: &Value, now: DateTime<Utc>) -> NativeResult<Value> {
        let args: PostArgs = mcp::arguments(arguments)?;
        let at = match args.at.as_deref() {
            None => None,
            Some(at) => Some(DateTime::parse_from_rfc3339(at).map_err(|_| NativeError::Failed("bad at".into()))?.with_timezone(&Utc)),
        };
        let tap = args.tap.filter(|t| !t.is_null());
        let shown = Shown { id: args.id, title: args.title, body: args.body };
        // Posting an id again replaces it: the old one goes first.
        Center::withdraw(&self.center, &shown.id);
        match at {
            Some(at) if at > now => {
                {
                    let mut state = self.center.state.lock().unwrap();
                    state.pending.insert(shown.id.clone(), Pending { at, shown, tap });
                }
                Center::ensure_scheduler(&self.center);
                self.center.wake.notify_all();
            }
            _ => Center::show(&self.center, shown, tap)?,
        }
        Ok(json!({}))
    }

    fn cancel(&self, args: CancelArgs) -> NativeResult<Value> {
        Ok(json!({"cancelled": Center::withdraw(&self.center, &args.id)}))
    }
}

impl Drop for NotificationsCapability {
    fn drop(&mut self) {
        self.center.state.lock().unwrap().shutdown = true;
        self.center.wake.notify_all();
    }
}

impl Center {
    /// Remove a pending one and close a shown one; whether one existed.
    fn withdraw(this: &Arc<Center>, id: &str) -> bool {
        let (was_pending, shown) = {
            let mut state = this.state.lock().unwrap();
            (state.pending.remove(id).is_some(), state.shown.remove(id))
        };
        if let Some(ShownEntry { handle: Some(handle), .. }) = &shown {
            this.notifier.close(*handle);
        }
        was_pending || shown.is_some()
    }

    fn show(this: &Arc<Center>, shown: Shown, tap: Option<Value>) -> NativeResult<()> {
        // Reserve the generation BEFORE showing: a click may arrive before
        // `show` returns, and a replaced notification's late click must not fire.
        let generation = {
            let mut state = this.state.lock().unwrap();
            state.generation += 1;
            let generation = state.generation;
            state.shown.insert(shown.id.clone(), ShownEntry { generation, handle: None });
            generation
        };
        let weak: Weak<Center> = Arc::downgrade(this);
        let id = shown.id.clone();
        let on_event = Box::new(move |clicked: bool| {
            if let Some(center) = weak.upgrade() {
                center.event(&id, generation, clicked, tap);
            }
        });
        match this.notifier.show(&shown, on_event) {
            Ok(handle) => {
                let stale = {
                    let mut state = this.state.lock().unwrap();
                    match state.shown.get_mut(&shown.id) {
                        Some(entry) if entry.generation == generation => {
                            entry.handle = Some(handle);
                            false
                        }
                        _ => true,
                    }
                };
                // Cancelled or replaced while it was being shown: withdraw it now.
                if stale {
                    this.notifier.close(handle);
                }
                Ok(())
            }
            Err(error) => {
                let mut state = this.state.lock().unwrap();
                if state.shown.get(&shown.id).is_some_and(|e| e.generation == generation) {
                    state.shown.remove(&shown.id);
                }
                Err(error)
            }
        }
    }

    /// The platform's click (`true`) or dismissal of generation `generation`.
    fn event(&self, id: &str, generation: u64, clicked: bool, tap: Option<Value>) {
        let current = {
            let mut state = self.state.lock().unwrap();
            if state.shown.get(id).is_some_and(|e| e.generation == generation) {
                state.shown.remove(id);
                true
            } else {
                false
            }
        };
        if current && clicked {
            if let Some(tap) = tap {
                (self.on_tap)(tap);
            }
        }
    }

    fn tick(this: &Arc<Center>, now: DateTime<Utc>) {
        let due: Vec<Pending> = {
            let mut state = this.state.lock().unwrap();
            let ids: Vec<String> = state.pending.iter().filter(|(_, p)| p.at <= now).map(|(id, _)| id.clone()).collect();
            let mut due: Vec<Pending> = ids.iter().filter_map(|id| state.pending.remove(id)).collect();
            due.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| a.shown.id.cmp(&b.shown.id)));
            due
        };
        for pending in due {
            if let Err(error) = Center::show(this, pending.shown, pending.tap) {
                log::warn!("eengine-desktop: a scheduled notification failed: {error:?}");
            }
        }
    }

    fn ensure_scheduler(this: &Arc<Center>) {
        if !this.spawn_scheduler {
            return;
        }
        {
            let mut state = this.state.lock().unwrap();
            if state.scheduler {
                return;
            }
            state.scheduler = true;
        }
        let center = this.clone();
        let spawned = std::thread::Builder::new().name("eengine-notifications".into()).spawn(move || loop {
            {
                let state = center.state.lock().unwrap();
                if state.shutdown {
                    return;
                }
                let now = Utc::now();
                let sleep = state
                    .pending
                    .values()
                    .map(|p| p.at)
                    .min()
                    .map(|at| (at - now).to_std().unwrap_or(Duration::ZERO).min(MAX_SLEEP))
                    .unwrap_or(MAX_SLEEP);
                if !sleep.is_zero() {
                    let (state, _) = center.wake.wait_timeout(state, sleep).unwrap();
                    if state.shutdown {
                        return;
                    }
                }
            }
            Center::tick(&center, Utc::now());
        });
        if let Err(error) = spawned {
            log::warn!("eengine-desktop: no notification scheduler: {error}");
            this.state.lock().unwrap().scheduler = false;
        }
    }
}

impl Capability for NotificationsCapability {
    fn name(&self) -> &str {
        "notifications"
    }

    fn tools(&self) -> Vec<Value> {
        mcp::tools("notifications")
    }

    fn call(&self, tool: &str, arguments: &Value) -> Result<Value, Failure> {
        let result = match tool {
            "post" => self.post_at(arguments, Utc::now()),
            "cancel" => mcp::arguments(arguments).and_then(|a| self.cancel(a)),
            _ => return Err(mcp::unknown("notifications", tool)),
        };
        Ok(answer(result))
    }
}
