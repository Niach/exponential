//! Settings → Coding CLIs (EXP-288 — split out of the old Coding pane, the
//! remaining this-device knobs live in [`super::tools`]; EXP-1294 renamed
//! the entry from "Agents", which are the team's agents now).
//!
//! The JetBrains-SDK-settings-style pane for the Start-coding launcher,
//! grouped HARD by agent (EXP-206):
//!
//! | Card   | Contents                                                     |
//! |--------|--------------------------------------------------------------|
//! | Agents | One TAB per agent: CLI path + model +                        |
//! |        | effort, the agent's own toggles — Claude: ultracode, plan    |
//! |        | mode; Codex: none (EXP-690: every run bypasses permissions,  |
//! |        | no toggle). EXP-862: accounts and usage live on the Devices  |
//! |        | page, never here                                             |
//!
//! Model/effort are [`crate::coding_selects`] choice selects (never free
//! text — the closed alias sets the CLI accepts). The per-agent toggles are
//! what the shared Start-coding dialog prefills from — ONE set of defaults
//! for single- and multi-issue sessions alike (EXP-206).
//!
//! EXP-694: the pane wears the SHARED grouped agent picker
//! ([`crate::launch_options::AgentDefaultsGroup`]) — the exact component the
//! Device settings dialog and the Start-coding dialog render — and AUTOSAVES
//! like both of them: no Save button, pickers and switches write on change, a
//! typed CLI path after [`PATH_SAVE_DEBOUNCE`] (or on blur). EXP-1158: there
//! is no default agent or account to pick — every start runs on the login
//! last used on this machine.
//!
//! Settings persist through [`crate::coding_flow::CodingHub`] to the local
//! per-install `settings.json` — never synced. Saving re-runs the doctor
//! against the new agent paths, and the doctor's report is exactly what
//! gates the Start-coding button (§7.1 step 1 ANDs `agent.ok && git.ok`).
//! The "Tooling doctor" report itself renders in Settings → Tools
//! ([`super::doctor_section`], EXP-367 — one panel shared with the
//! onboarding tools step).
//!
//! This pane and the Tools pane share ONE settings struct but own DISJOINT
//! fields — see the [`super::tools`] module doc for the drafted/save/resync
//! contract that keeps their saves from clobbering each other.
//!
//! #EXP-1629: the pane is a document on eUI's native host binding: the
//! agent tabs (the embedded glass tab strip), the CLI path as a `row`
//! Input, Model and Effort as `row` Selects, Ultracode a Switch row; the
//! hub logic (baseline, autosave, debounce, failure toast) is unchanged.
//!
//! The personal API key is provisioned and rotated **fully automatically**
//! (`api::users::ensure_personal_key` on the first coding session; the
//! `.exp-mcp.json` writer picks it up), so there is no key UI here at all.

use std::rc::Rc;
use std::time::Duration;

use eui_gpui::{binding::Binding, host::HostPlugin, view::SurfaceView};
use gpui::{div, App, AppContext as _, Entity, IntoElement, ParentElement as _, Render, SharedString, Styled as _, Subscription, Task, Window};
use serde_json::{json, Value};

use coding::{CodingAgent, Settings};

use crate::coding_flow::CodingHub;
use crate::coding_selects::{effort_choices_for, model_choices_for};

use super::doc::{hex, not, path, IdeToasts, Ink, SM};

/// EXP-694: the pane AUTOSAVES like the Device settings dialog — no Save
/// button. A typed CLI path settles for this long before it is written; a
/// blur (or Enter) commits it immediately, and every picker/switch writes
/// straight through on change.
const PATH_SAVE_DEBOUNCE: Duration = Duration::from_millis(800);

/// The data-model path the pane's projection is written at.
const PATH: &str = "/pane";

/// The "ultracode sets effort" hint under a locked Effort row.
const ULTRACODE_HINT: &str = "ultracode sets effort";

/// A state the pictures show (`settings_bench`).
#[cfg(test)]
pub(crate) struct AgentsShot {
    /// The tab showing.
    pub tab: CodingAgent,
    pub claude_path: &'static str,
    pub claude_model: &'static str,
    pub claude_effort: &'static str,
    pub codex_model: &'static str,
    pub codex_effort: &'static str,
    pub ultracode: bool,
    /// Typed into the CLI path field after the sync.
    pub typed: Option<&'static str>,
}

/// This pane's owned fields as the controls hold them.
#[derive(Clone, Default, PartialEq)]
struct Controls {
    claude_path: String,
    claude_model: String,
    claude_effort: String,
    codex_model: String,
    codex_effort: String,
    claude_ultracode: bool,
}

impl Controls {
    fn of(settings: &Settings) -> Self {
        Self {
            claude_path: settings.claude_path.clone(),
            claude_model: settings.claude_model.clone(),
            claude_effort: settings.claude_effort.clone(),
            codex_model: settings.codex_model.clone(),
            codex_effort: settings.codex_effort.clone(),
            claude_ultracode: settings.claude_ultracode,
        }
    }
}

pub struct AgentsPane {
    /// The controls as edited (the document's changes).
    controls: Controls,
    /// The CLI path the field was last given (its bound value): a resync
    /// rewrites it, the person's typing does not.
    shown_path: String,
    /// Which agent tab is showing — pure UI state, not persisted (EXP-206).
    agent_tab: CodingAgent,
    /// The hub settings the controls were last synced from (the autosave
    /// baseline: a control rewrite the pane itself performed drafts back to
    /// it and writes nothing).
    synced: Option<Settings>,
    /// The pending debounced CLI-path write (dropping it cancels).
    path_save: Option<Task<()>>,
    save_error: Option<SharedString>,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    _subscriptions: Vec<Subscription>,
}

impl AgentsPane {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        // Creating the hub also kicks the FIRST doctor run (§7.7 onboarding).
        let hub = CodingHub::global(cx);
        let subscriptions = vec![
            // Doctor results / external settings changes re-render + resync.
            cx.observe(&hub, |this, _, cx| {
                this.resync(cx);
                cx.notify();
            }),
        ];
        // Leaving Settings drops the pane — pay out what the path debounce
        // still owes on the way out (the web/iOS unmount-flush twin).
        cx.on_release(|this, cx| this.flush_pending_path(cx)).detach();
        let pane = cx.weak_entity();
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            for name in ["tab", "pick", "ultracode"] {
                let pane = pane.clone();
                binding.on_action(name, move |_, event, cx| {
                    let field = event.context.get("field").and_then(Value::as_str).unwrap_or_default().to_string();
                    let payload = event.payload.clone().unwrap_or(Value::Null);
                    let pane = pane.clone();
                    // Deferred: the binding is mid-update and these write it.
                    cx.defer(move |cx| {
                        let _ = pane.update(cx, |pane, cx| pane.action(name, &field, &payload, cx));
                    });
                    None
                });
            }
            binding
        });
        let plugin: Rc<dyn HostPlugin> = Rc::new(PanePlugin { pane: cx.weak_entity(), inner: super::doc::ide_plugin() });
        let view = super::doc::mount_with("settings-agents", document(&Ink::of(cx), &Chrome::of(cx)), &binding, plugin, true, window, cx);
        let defaults = Settings::default();
        let mut this = Self {
            controls: Controls::of(&defaults),
            shown_path: String::new(),
            agent_tab: defaults.default_agent,
            synced: None,
            path_save: None,
            save_error: None,
            binding,
            view,
            _subscriptions: subscriptions,
        };
        this.resync(cx);
        this
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::new(window, cx)
    }

    /// The pane in a pinned state: the hub's owned fields, the tab, then
    /// `typed` into the CLI path field.
    #[cfg(test)]
    pub(crate) fn show(&mut self, shot: AgentsShot, _window: &mut Window, cx: &mut gpui::Context<Self>) {
        let hub = CodingHub::global(cx);
        hub.update(cx, |hub, cx| {
            hub.settings.claude_path = shot.claude_path.to_string();
            hub.settings.claude_model = shot.claude_model.to_string();
            hub.settings.claude_effort = shot.claude_effort.to_string();
            hub.settings.codex_model = shot.codex_model.to_string();
            hub.settings.codex_effort = shot.codex_effort.to_string();
            hub.settings.claude_ultracode = shot.ultracode;
            cx.notify();
        });
        self.synced = None;
        self.resync(cx);
        self.agent_tab = shot.tab;
        if let Some(typed) = shot.typed {
            // A person typed it: the field's own text.
            self.controls.claude_path = typed.to_string();
            self.shown_path = typed.to_string();
        }
        cx.notify();
    }

    /// Overlay ONLY this pane's owned fields from `from` onto `onto` —
    /// the single definition `resync`/`save` both lean on so the two can
    /// never drift.
    fn overlay_owned(onto: &mut Settings, from: &Settings) {
        // EXP-1020: the launch defaults are `coding`'s ONE definition,
        // shared with the Device settings dialog so the two lists cannot
        // drift. This pane owns the two CLI paths on top of them.
        coding::overlay_launch_defaults(onto, from);
        onto.claude_path = from.claude_path.clone();
        onto.codex_path = from.codex_path.clone();
    }

    /// Mirror the hub's settings into the controls whenever they change out
    /// from under us. Unowned fields (Tools' repos/prefix/shell, the rail
    /// pref) are adopted into the baseline FIRST, so a sibling pane's save
    /// never wipes edits here; only owned-field changes rewrite the controls.
    fn resync(&mut self, cx: &mut gpui::Context<Self>) {
        let hub = CodingHub::global(cx);
        let settings = hub.read(cx).settings.clone();
        if let Some(synced) = self.synced.as_mut() {
            let mut adopted = settings.clone();
            Self::overlay_owned(&mut adopted, synced);
            *synced = adopted;
        }
        if self.synced.as_ref() == Some(&settings) {
            return;
        }
        // The persisted values are load-normalized into the choice sets.
        self.controls = Controls::of(&settings);
        self.shown_path = settings.claude_path.clone();
        // Open on the saved default agent (first sync only — later saves
        // must not yank the tab from under the user).
        if self.synced.is_none() {
            self.agent_tab = settings.default_agent;
        }
        self.synced = Some(settings);
        cx.notify();
    }

    /// The settings the controls currently describe: the synced baseline with
    /// ONLY this pane's fields overlaid. A blank CLI path degrades to the
    /// §7.7 default — a hand-blanked pane can never produce an unusable
    /// launcher (mirrors `Settings::load`); the selects are closed sets.
    fn drafted(&self) -> Settings {
        let defaults = Settings::default();
        let raw = self.controls.claude_path.trim();
        let mut drafted = self.synced.clone().unwrap_or_default();
        let owned = Settings {
            claude_path: if raw.is_empty() { defaults.claude_path.clone() } else { raw.to_string() },
            // EXP-1232: codex is a managed download — no field; a custom
            // `codexPath` in settings.json stays an escape hatch.
            codex_path: drafted.codex_path.clone(),
            claude_model: self.controls.claude_model.clone(),
            claude_effort: self.controls.claude_effort.clone(),
            codex_model: self.controls.codex_model.clone(),
            codex_effort: self.controls.codex_effort.clone(),
            claude_ultracode: self.controls.claude_ultracode,
            ..defaults
        };
        Self::overlay_owned(&mut drafted, &owned);
        drafted
    }

    fn dirty(&self) -> bool {
        self.synced.as_ref().map(|synced| *synced != self.drafted()).unwrap_or(false)
    }

    /// EXP-694: hold a typed CLI path until the typing settles
    /// ([`PATH_SAVE_DEBOUNCE`]); a newer keystroke replaces (and so cancels)
    /// the pending task.
    fn queue_path_save(&mut self, cx: &mut gpui::Context<Self>) {
        self.path_save = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PATH_SAVE_DEBOUNCE).await;
            let _ = this.update(cx, |this, cx| this.save(cx));
        }));
    }

    /// EXP-694: the autosave — every control writes through the hub the
    /// moment it changes, so the pane has no Save button. The baseline guard
    /// is what makes that safe: a rewrite `resync` performed drafts back to
    /// `synced` and writes nothing.
    fn save(&mut self, cx: &mut gpui::Context<Self>) {
        self.save_error = self.commit(cx);
        // FEED-79: a failed write is the error toast; Retry re-commits.
        if let Some(error) = &self.save_error {
            let pane = cx.entity().downgrade();
            crate::action_error::failed_action_with_retry(
                "save",
                "the agent settings",
                Some(error.to_string()),
                move |_, cx| {
                    let _ = pane.update(cx, |this, cx| this.save(cx));
                },
                cx,
            );
        }
        cx.notify();
    }

    /// The hub write itself, without the pane's error/notify plumbing — shared
    /// with the on-release flush, which has no `Context` left to notify.
    /// Returns the failure, if any.
    fn commit(&mut self, cx: &mut App) -> Option<SharedString> {
        // A failed write stays retryable: the hub adopts the value in memory
        // even when the file write fails, so the baseline reads clean and the
        // standing error is what re-arms the next commit.
        if !self.dirty() && self.save_error.is_none() {
            return None;
        }
        let drafted = self.drafted();
        let hub = CodingHub::global(cx);
        // Overlay ONLY the owned fields onto the hub's LIVE settings, so a
        // save here can never roll back a concurrent Tools-pane save (or the
        // rail pref).
        let mut settings = hub.read(cx).settings.clone();
        Self::overlay_owned(&mut settings, &drafted);
        let error = CodingHub::save_settings(&hub, settings.clone(), cx).err().map(SharedString::from);
        // `synced` follows the hub via the observer's resync; setting it here
        // too keeps the baseline honest when the observer coalesces.
        self.synced = Some(settings);
        error
    }

    /// EXP-694: the debounce's other half — the pane is rebuilt per Settings
    /// navigation, so leaving it drops [`Self::path_save`] and a CLI path
    /// typed inside the 800ms would never be written. TAKING the task first
    /// keeps this single-shot: a blur/Enter commit already cleared it, so the
    /// flush can never double-write.
    fn flush_pending_path(&mut self, cx: &mut App) {
        if self.path_save.take().is_some() {
            self.save_error = self.commit(cx);
            if let Some(error) = &self.save_error {
                crate::action_error::failed_action("save", "the agent settings", Some(error.to_string()), cx);
            }
        }
    }

    /// A document action: a tab, a picked model/effort, the Ultracode switch.
    fn action(&mut self, name: &str, field: &str, payload: &Value, cx: &mut gpui::Context<Self>) {
        // #FEED-247: a popup opening or closing fires `change` with no value.
        let picked = super::doc::picked_value(payload).is_some();
        let value = super::doc::picked_value(payload).unwrap_or_default().to_string();
        match name {
            "tab" => {
                self.agent_tab = if field == "codex" { CodingAgent::Codex } else { CodingAgent::Claude };
                cx.notify();
            }
            // EXP-694 autosave: confirming a choice IS the save.
            "pick" if picked => {
                let slot = match field {
                    "claudeModel" => &mut self.controls.claude_model,
                    "claudeEffort" => &mut self.controls.claude_effort,
                    "codexModel" => &mut self.controls.codex_model,
                    "codexEffort" => &mut self.controls.codex_effort,
                    _ => return,
                };
                *slot = value;
                self.save(cx);
            }
            "ultracode" => {
                self.controls.claude_ultracode = payload.get("checked").and_then(Value::as_bool).unwrap_or(!self.controls.claude_ultracode);
                self.save(cx);
            }
            _ => {}
        }
    }

    /// The CLI path field's edits: typing settles (the debounce), a blur or
    /// Enter commits at once.
    fn edited(&mut self, value: String, commit: bool, cx: &mut gpui::Context<Self>) {
        self.controls.claude_path = value;
        if commit {
            self.path_save = None;
            self.save(cx);
        } else {
            self.queue_path_save(cx);
        }
        cx.notify();
    }

    /// The state as the document binds it.
    fn project(&self) -> Value {
        let options = |choices: &[(&str, &str)]| choices.iter().map(|(label, value)| json!({"label": label, "value": value})).collect::<Vec<_>>();
        let claude = self.agent_tab == CodingAgent::Claude;
        json!({
            "claude": claude,
            "codex": !claude,
            "path": self.shown_path,
            "pathPlaceholder": coding::settings::DEFAULT_CLAUDE_PATH,
            "claudeModel": self.controls.claude_model,
            "claudeEffort": self.controls.claude_effort,
            "codexModel": self.controls.codex_model,
            "codexEffort": self.controls.codex_effort,
            "ultracode": self.controls.claude_ultracode,
            "claudeModels": options(model_choices_for(CodingAgent::Claude)),
            "claudeEfforts": options(effort_choices_for(CodingAgent::Claude)),
            "codexModels": options(model_choices_for(CodingAgent::Codex)),
            "codexEfforts": options(effort_choices_for(CodingAgent::Codex)),
        })
    }
}

impl Render for AgentsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "AgentsPane").entered();
        let projection = self.project();
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        // EXP-694: no Save button — every control autosaves; a write that
        // failed is a toast (FEED-79).
        div().w_full().min_w_0().child(self.view.clone())
    }
}

/// The pane's plugin: the IDE's, and the CLI path field's edits.
struct PanePlugin {
    pane: gpui::WeakEntity<AgentsPane>,
    inner: crate::exponential_ui_host::IdePlugin,
}

impl HostPlugin for PanePlugin {
    fn icon(&self, name: &str) -> Option<SharedString> {
        self.inner.icon(name)
    }

    fn on_unknown(&self, node: &eui_core::surface::PlacedNode) {
        self.inner.on_unknown(node)
    }

    fn on_input(&self, event: &eui_gpui::host::InputEvent, cx: &mut App) {
        let commit = event.kind == eui_gpui::host::InputKind::Commit;
        let value = event.value.as_str().unwrap_or_default().to_string();
        let pane = self.pane.clone();
        cx.defer(move |cx| {
            let _ = pane.update(cx, |pane, cx| pane.edited(value, commit, cx));
        });
    }
}

/// The colours the tab strip needs beyond [`Ink`]: the active segment's
/// glass stroke.
struct Chrome {
    stroke_active: String,
}

impl Chrome {
    fn of(_cx: &App) -> Self {
        Self { stroke_active: hex(crate::controls::ide_chrome().stroke_active) }
    }
}

/// One agent segment of the embedded tab strip (`glass_tab_item`: an equal
/// capsule, `px_2` / `py 6`, a 1 px border, `gap_1` between the 14 px mark
/// and the `text_sm` label); the active one wears the glass active fill and
/// stroke. Drawn twice, `active` picks.
fn tab(ink: &Ink, chrome: &Chrome, key: &str, icon: &str, label: &str, active: Value) -> [Value; 2] {
    let draw = |on: bool| {
        let id = format!("tab_{key}{}", if on { "_on" } else { "" });
        let glyph = ink.r(0.875);
        let mut node = json!({"id": id, "component": "Box", "visible": if on { active.clone() } else { not(active.clone()) },
            "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "center", "flexGrow": 1, "flexBasis": 0, "gap": ink.r(0.25), "paddingHorizontal": ink.r(0.5), "paddingVertical": 6, "borderRadius": 9999, "borderWidth": 1, "borderColor": if on { chrome.stroke_active.as_str() } else { "#00000000" }, "cursor": "pointer"},
            "children": [
                {"id": format!("{id}_icon"), "component": "Icon", "props": {"name": icon}, "style": {"width": glyph, "height": glyph, "flexShrink": 0}},
                ink.text(&format!("{id}_label"), json!(label), SM, &ink.foreground),
            ]});
        if on {
            node["style"]["backgroundColor"] = json!(ink.fill_active);
        } else {
            node["props"] = json!({"pressable": true});
            node["on"] = json!({"press": {"event": {"name": "tab", "context": {"field": key}}}});
        }
        node
    };
    [draw(false), draw(true)]
}

/// A `row` Select bound to `/pane/<field>` over `/pane/<options>`.
fn select(id: &str, label: &str, field: &str, options: &str, extra: Value) -> Value {
    let mut props = json!({"label": label, "name": id, "triggerVariant": "row", "value": path(&format!("/pane/{field}")), "options": path(&format!("/pane/{options}"))});
    for (k, v) in extra.as_object().into_iter().flatten() {
        props[k] = v.clone();
    }
    json!({"id": id, "component": "Select", "props": props, "on": {"change": {"event": {"name": "pick", "context": {"field": field}}}}})
}

/// The pane's tree: ONE glass group — the agent tabs as its first row, then
/// the active agent's rows (Claude: CLI path, Model, Effort, Ultracode;
/// Codex: Model, Reasoning).
fn document(ink: &Ink, chrome: &Chrome) -> Value {
    let claude = path("/pane/claude");
    let codex = path("/pane/codex");
    let ultracode = path("/pane/ultracode");
    let tabs: Vec<Value> = [tab(ink, chrome, "claude", "brand-claude", CodingAgent::Claude.label(), claude.clone()), tab(ink, chrome, "codex", "brand-codex", CodingAgent::Codex.label(), codex.clone())].into_iter().flatten().collect();
    let tabs = json!({"id": "tabs", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "padding": ink.r(0.5)}, "children": tabs});
    let path_row = json!({"id": "cli_path", "component": "Input", "props": {"variant": "row", "label": "CLI path", "name": "cliPath", "value": path("/pane/path"), "placeholder": path("/pane/pathPlaceholder")}});
    let mut effort_live = select("claude_effort", CodingAgent::Claude.effort_label(), "claudeEffort", "claudeEfforts", json!({}));
    effort_live["visible"] = not(ultracode.clone());
    let mut effort_locked = select("claude_effort_locked", CodingAgent::Claude.effort_label(), "claudeEffort", "claudeEfforts", json!({"description": ULTRACODE_HINT, "disabled": true}));
    effort_locked["visible"] = ultracode.clone();
    let switch = json!({"id": "ultracode_switch", "component": "Switch", "props": {"label": "", "name": "ultracode", "checked": ultracode}, "on": {"change": {"event": {"name": "ultracode"}}}});
    let mut switch_label = ink.text("ultracode_label", json!("Ultracode"), SM, &ink.foreground);
    switch_label["style"]["flexGrow"] = json!(1);
    switch_label["style"]["minWidth"] = json!(0);
    let ultracode_row = ink.shell("ultracode", false, json!(true), vec![switch_label, switch]);
    let claude_rows = json!({"id": "claude_rows", "component": "Box", "visible": claude, "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "children": [
            ink.divider("path_divider", json!(true)), path_row,
            ink.divider("claude_model_divider", json!(true)), select("claude_model", "Model", "claudeModel", "claudeModels", json!({})),
            ink.divider("claude_effort_divider", json!(true)), effort_live, effort_locked,
            ink.divider("ultracode_divider", json!(true)), ultracode_row,
        ]});
    let codex_rows = json!({"id": "codex_rows", "component": "Box", "visible": codex, "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "children": [
            ink.divider("codex_model_divider", json!(true)), select("codex_model", "Model", "codexModel", "codexModels", json!({})),
            ink.divider("codex_effort_divider", json!(true)), select("codex_effort", CodingAgent::Codex.effort_label(), "codexEffort", "codexEfforts", json!({})),
        ]});
    let group = ink.group("agents", vec![tabs, claude_rows, codex_rows]);
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0}, "children": [group]})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ink() -> Ink {
        Ink {
            rem: 14.,
            primary: "#fafafaff".into(),
            foreground: "#ffffffff".into(),
            hint: "#ffffffff".into(),
            value: "#ffffffff".into(),
            muted: "#a1a1aaff".into(),
            muted_glyph: "#a1a1aaff".into(),
            fill_row: "#ffffff0a".into(),
            stroke_row: "#ffffff14".into(),
            fill_card: "#ffffff0f".into(),
            stroke_card: "#ffffff1a".into(),
            code: "#27272aff".into(),
            radius_lg: 12.,
            radius_md: 8.,
            fill_section: "#ffffff0a".into(),
            fill_active: "#ffffff1a".into(),
            mono: "$type.family.mono".into(),
            pill_button_fill: "#ffffff0f".into(),
            pill_button_fill_disabled: "#ffffff0b".into(),
            primary_button_fill: "#f4f4f5ff".into(),
            primary_button_hover: "#e4e4e7ff".into(),
            primary_button_fill_disabled: "#f4f4f580".into(),
            primary_button_fg: "#18181bff".into(),
        }
    }

    /// #EXP-1629: the document raises no issue a debug build's mount check
    /// would refuse (only the IDE's literal chrome and the bound values).
    #[test]
    fn the_document_raises_no_unexpected_issue() {
        let tree = document(&ink(), &Chrome { stroke_active: "#ffffff29".into() });
        let bound = super::super::doc::bound_values(&tree);
        let mut surface = eui_core::surface::Surface::new("settings-agents", Default::default());
        let outcome = surface.set_nested(serde_json::from_value(tree).expect("parses"));
        let issues = super::super::doc::unexpected_issues(&bound, &outcome.issues);
        assert!(issues.is_empty(), "{issues:?}");
    }
}
