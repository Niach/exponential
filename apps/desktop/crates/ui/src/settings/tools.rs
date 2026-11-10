//! Settings → Tools (EXP-288 — renamed from "Coding" and slimmed to the
//! non-agent, this-device knobs; the per-agent settings live in
//! [`super::agents`]).
//!
//! | Section        | Meaning                                              |
//! |----------------|------------------------------------------------------|
//! | Repos root     | Where repositories/worktrees live (`~` works)        |
//! | Branch prefix  | Prepended to the issue identifier (`exp/EXP-42`)     |
//! | Terminal shell | Program new `+` terminal tabs spawn (blank = auto)   |
//! | Tooling doctor | The shared [`super::doctor_section::DoctorPanel`]    |
//! |                | (EXP-367 — moved here from Agents; also the wizard's |
//! |                | tools step)                                          |
//!
//! Settings persist through [`crate::coding_flow::CodingHub`] to the local
//! per-install `settings.json` — never synced. This pane and the Agents pane
//! share that ONE settings struct but own DISJOINT fields: `drafted` overlays
//! only this pane's fields onto its synced baseline, `save` overlays them
//! onto the hub's LIVE settings (a Tools save can never roll back an Agents
//! save), and `resync` adopts unowned-field changes into the baseline first
//! so a sibling save never wipes edits in flight here.
//!
//! The keep-alive switch is deliberately NOT one of the three DRAFTED
//! fields: it writes straight through on click (the hub's `save_ui_prefs`,
//! the notifications pane's OS-notification switch twin), so it stays out
//! of `overlay_owned` and out of the Save-changes button.

//!
//! #EXP-1629: the pane is ONE document on eUI's native host binding: its
//! three row Inputs, Save, the doctor (#EXP-1770: the shared readiness
//! document, `doctor_section::document`) and the Danger zone.

use std::rc::Rc;

use eui_gpui::{binding::Binding, view::SurfaceView};
use gpui::{div, AppContext as _, Entity, IntoElement, ParentElement, Render, Styled, Subscription, Window};
use gpui_component::{button::ButtonVariant, ActiveTheme as _};
use serde_json::{json, Value};

use coding::Settings;

use crate::coding_flow::CodingHub;
use crate::native_dialog::{self, AlertSpec};

use super::doc::{hex, not, path, IdeToasts, Ink, SM};
use super::doctor_section::LocalDoctor;
use super::readiness::{self, Paint};

/// A state the pictures show (`settings_bench`).
#[cfg(test)]
pub(crate) struct ToolsShot {
    pub repos: &'static str,
    pub prefix: &'static str,
    pub shell: Option<&'static str>,
    /// Typed into the Branch prefix field after the sync (dirty).
    pub typed: Option<&'static str>,
    /// A `device-doctor.json` case name; `None` = the doctor is running.
    pub doctor: Option<&'static str>,
    /// Claude Code is not installed and its Custom path field is open.
    pub path_open: bool,
}

/// The three fields as the controls hold them (repos root, branch prefix,
/// shell).
#[derive(Clone, Default, PartialEq)]
struct Fields {
    repos: String,
    prefix: String,
    shell: String,
}

impl Fields {
    fn of(settings: &Settings) -> Self {
        Self { repos: settings.repos_root.clone(), prefix: settings.branch_prefix.clone(), shell: settings.terminal_shell.clone().unwrap_or_default() }
    }
}

pub struct ToolsPane {
    /// The fields as typed (the document's edits).
    typed: Fields,
    /// The fields the inputs were last given (their bound values).
    shown: Fields,
    /// THIS device's readiness block (EXP-367 — also the onboarding tools
    /// step).
    doctor: LocalDoctor,
    /// The hub settings the controls were last synced from (dirty baseline).
    synced: Option<Settings>,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    _subscriptions: Vec<Subscription>,
}

impl ToolsPane {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let hub = CodingHub::global(cx);
        let subscriptions = vec![cx.observe(&hub, |this, _, cx| {
            this.resync(cx);
            this.doctor.resync(cx);
            cx.notify();
        })];
        let pane = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new("/pane", Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            for name in ["edit", "save", "reset"] {
                let pane = pane.clone();
                binding.on_action(name, move |_, event, cx| {
                    let field = event.context.get("field").and_then(Value::as_str).unwrap_or_default().to_string();
                    let value = event.payload.as_ref().and_then(|p| p.get("value")).and_then(Value::as_str).unwrap_or_default().to_string();
                    let pane = pane.clone();
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| {
                            let _ = pane.update(cx, |pane, cx| pane.action(name, &field, value, window, cx));
                        });
                    });
                    None
                });
            }
            for name in readiness::EVENTS {
                let pane = pane.clone();
                binding.on_action(name, move |_, action, cx| {
                    let event = readiness::event(name, &action.context, action.payload.as_ref().unwrap_or(&Value::Null))?;
                    let pane = pane.clone();
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| {
                            let _ = pane.update(cx, |pane, cx| {
                                pane.doctor.handle(event, window, cx);
                                cx.notify();
                            });
                        });
                    });
                    None
                });
            }
            binding
        });
        let theme = cx.theme();
        let chrome = Chrome {
            radius: f32::from(theme.radius),
            danger: hex(theme.danger),
            primary: hex(theme.button_primary),
            primary_fg: hex(theme.button_primary_foreground),
            primary_hover: hex(theme.button_primary_hover),
            danger_bg: hex(theme.button_danger),
            danger_fg: hex(theme.button_danger_foreground),
            danger_hover: hex(theme.button_danger_hover),
        };
        let view = super::doc::mount_with("settings-tools", document(&Ink::of(cx), &Paint::of(cx), &chrome), &binding, Rc::new(super::doc::ide_plugin()), true, window, cx);
        let mut this = Self {
            typed: Fields::default(),
            shown: Fields::default(),
            doctor: LocalDoctor::new(cx),
            synced: None,
            binding,
            view,
            _subscriptions: subscriptions,
        };
        this.resync(cx);
        this
    }

    /// Overlay ONLY this pane's owned fields from `from` onto `onto` — the
    /// single definition `resync`, `drafted` and `save` all lean on, so the
    /// three can never drift (the [`super::agents`] pane's twin).
    fn overlay_owned(onto: &mut Settings, from: &Settings) {
        onto.repos_root = from.repos_root.clone();
        onto.branch_prefix = from.branch_prefix.clone();
        onto.terminal_shell = from.terminal_shell.clone();
    }

    /// Mirror the hub's settings into the controls whenever they change out
    /// from under us. Unowned fields (agents, rail…) are adopted into the
    /// baseline FIRST, so a sibling pane's save never wipes edits here; only
    /// owned-field changes rewrite the inputs.
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
        self.typed = Fields::of(&settings);
        self.shown = self.typed.clone();
        self.synced = Some(settings);
        cx.notify();
    }

    /// The settings the controls currently describe: the synced baseline with
    /// ONLY this pane's fields overlaid. Blank repos/prefix degrade to the
    /// §7.7 defaults (mirrors `Settings::load`); a blank shell is `None`
    /// (= auto), NOT a default program.
    fn drafted(&self) -> Settings {
        let defaults = Settings::default();
        let value = |raw: &str, default: &str| {
            let raw = raw.trim();
            if raw.is_empty() { default.to_string() } else { raw.to_string() }
        };
        let mut drafted = self.synced.clone().unwrap_or_default();
        drafted.repos_root = value(&self.typed.repos, &defaults.repos_root);
        drafted.branch_prefix = value(&self.typed.prefix, &defaults.branch_prefix);
        let shell = self.typed.shell.trim().to_string();
        drafted.terminal_shell = (!shell.is_empty()).then_some(shell);
        drafted
    }

    fn dirty(&self) -> bool {
        self.synced.as_ref().map(|synced| *synced != self.drafted()).unwrap_or(false)
    }

    fn save(&mut self, cx: &mut gpui::Context<Self>) {
        let drafted = self.drafted();
        let hub = CodingHub::global(cx);
        // Overlay ONLY the owned fields onto the hub's LIVE settings, so a
        // save here can never roll back a concurrent Agents-pane save.
        let mut settings = hub.read(cx).settings.clone();
        Self::overlay_owned(&mut settings, &drafted);
        // FEED-79: a failed write is the error toast; Retry saves again.
        if let Err(error) = CodingHub::save_settings(&hub, settings.clone(), cx) {
            let pane = cx.entity().downgrade();
            crate::action_error::failed_action_with_retry(
                "save",
                "the tools settings",
                Some(error.to_string()),
                move |_, cx| {
                    let _ = pane.update(cx, |this, cx| this.save(cx));
                },
                cx,
            );
        }
        // `synced` follows the hub via the observer's resync; setting it here
        // too keeps the Save button honest when the observer coalesces.
        self.synced = Some(settings);
        cx.notify();
    }

    /// A document action: a field's edit, Save, Reset IDE data.
    fn action(&mut self, name: &str, field: &str, value: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match name {
            "edit" => {
                match field {
                    "repos" => self.typed.repos = value,
                    "prefix" => self.typed.prefix = value,
                    "shell" => self.typed.shell = value,
                    _ => return,
                }
                cx.notify();
            }
            "save" => self.save(cx),
            "reset" => self.confirm_reset(window, cx),
            _ => {}
        }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::new(window, cx)
    }

    /// The pane in a pinned state: the hub's three fields and THIS device's
    /// readiness block (a `device-doctor.json` case, `None` = still running),
    /// then `typed` into the Branch prefix field (dirty).
    #[cfg(test)]
    pub(crate) fn show(&mut self, shot: ToolsShot, _window: &mut Window, cx: &mut gpui::Context<Self>) {
        let doctor = shot.doctor.and_then(|name| {
            let fixture: serde_json::Value = serde_json::from_str(include_str!("../../../../../../packages/domain-contract/fixtures/device-doctor.json")).ok()?;
            let case = fixture["cases"].as_array()?.iter().find(|case| case["name"].as_str() == Some(name))?.clone();
            crate::device_readiness::parse(Some(&case["doctor"]))
        });
        let doctor = doctor.map(|mut doctor| {
            if shot.path_open {
                super::doctor_section::claude_not_installed(&mut doctor);
            }
            doctor
        });
        let hub = CodingHub::global(cx);
        hub.update(cx, |hub, cx| {
            hub.settings.repos_root = shot.repos.to_string();
            hub.settings.branch_prefix = shot.prefix.to_string();
            hub.settings.terminal_shell = shot.shell.map(str::to_string);
            hub.doctor.device = doctor;
            // The run `CodingHub::global` kicked lands superseded.
            hub.doctor.generation += 1;
            hub.doctor.running = false;
            cx.notify();
        });
        self.synced = None;
        self.resync(cx);
        if let Some(typed) = shot.typed {
            // A person typed it: the field's own text.
            self.typed.prefix = typed.to_string();
            self.shown.prefix = typed.to_string();
        }
        if shot.path_open {
            self.doctor.open_path(coding::CodingAgent::Claude);
        }
        cx.notify();
    }

    /// The "Reset IDE data" confirm (EXP-367, built for testing fresh-install
    /// flows): destructive-local-only, so a plain danger confirm suffices —
    /// everything server-side survives and re-syncs on the next sign-in.
    /// EXP-369: the reset now also wipes the clones, so the confirm names the
    /// repos root — the only IDE data that is expensive to recreate.
    fn confirm_reset(&self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let repos_root = CodingHub::global(cx).read(cx).settings.repos_root.clone();
        let spec = AlertSpec::new(
            "Reset IDE data",
            format!(
                "This signs you out on this device and deletes ALL local IDE data: \
                 settings, accounts, and synced caches. Cloned repositories and \
                 worktrees under {repos_root} are deleted too, including any \
                 uncommitted work in them. The app restarts onto the sign-in screen."
            ),
            "Reset and restart",
        )
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, cx| {
            crate::session::reset_ide_data(cx);
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }

    /// The state as the document binds it.
    fn project(&self, cx: &gpui::App) -> Value {
        let defaults = Settings::default();
        json!({
            "fields": {"repos": self.shown.repos, "prefix": self.shown.prefix, "shell": self.shown.shell},
            "placeholders": {"repos": defaults.repos_root, "prefix": defaults.branch_prefix, "shell": terminal::manager::default_shell()},
            "dirty": self.dirty(),
            "doctor": self.doctor.project(cx),
        })
    }
}

impl Render for ToolsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "ToolsPane").entered();
        let projection = self.project(cx);
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        div().w_full().min_w_0().child(self.view.clone())
    }
}

/// The colours the document needs beyond [`Ink`]: the primary and danger
/// `Button`s' paints.
struct Chrome {
    radius: f32,
    danger: String,
    primary: String,
    primary_fg: String,
    primary_hover: String,
    danger_bg: String,
    danger_fg: String,
    danger_hover: String,
}

/// A `web_sm` `Button` (32 px, `px 12`, `text_sm` on a line of 1, the theme
/// radius) in a solid fill.
fn sm_button(ink: &Ink, id: &str, label: &str, fill: &str, hover: &str, fg: &str, event: &str, radius: f32) -> Value {
    let size = ink.r(SM);
    json!({"id": id, "component": "Box", "props": {"pressable": true}, "accessibility": {"label": label}, "on": {"press": {"event": {"name": event}}},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "height": 32, "paddingHorizontal": 12, "borderRadius": radius, "backgroundColor": fill, "cursor": "pointer", ":hover": {"backgroundColor": hover}},
        "children": [{"id": format!("{id}_text"), "component": "Text", "props": {"text": label}, "style": {"fontSize": size, "lineHeight": size, "color": fg, "flexShrink": 0}}]})
}

/// One field row (EXP-862: the label inside the row, the value typed on its
/// right): the `row` Input over the field.
fn field(id: &str, label: &str) -> Value {
    json!({"id": id, "component": "Input", "props": {"variant": "row", "label": label, "name": id, "value": path(&format!("/pane/fields/{id}")), "placeholder": path(&format!("/pane/placeholders/{id}"))},
        "on": {"change": {"event": {"name": "edit", "context": {"field": id}}}}})
}

/// The pane's tree (`gap_6`): the Tools band over its three fields, Save,
/// the doctor, the Danger zone.
fn document(ink: &Ink, paint: &Paint, chrome: &Chrome) -> Value {
    let group = ink.group("fields", vec![
        field("repos", "Repos & worktrees root"),
        ink.divider("prefix_divider", json!(true)),
        field("prefix", "Branch prefix"),
        ink.divider("shell_divider", json!(true)),
        field("shell", "Terminal shell"),
    ]);
    let card = json!({"id": "card", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%", "minWidth": 0}, "children": [ink.band("band", "Tools", None), group]});
    let mut save = sm_button(ink, "save", "Save changes", &chrome.primary, &chrome.primary_hover, &chrome.primary_fg, "save", chrome.radius);
    save["visible"] = path("/pane/dirty");
    // Disabled: the fill at half under the solid label.
    let mut save_off = sm_button(ink, "save_off", "Save changes", "#00000000", "#00000000", &chrome.primary_fg, "save", chrome.radius);
    save_off["visible"] = not(path("/pane/dirty"));
    save_off["props"]["pressable"] = json!(false);
    save_off["style"]["cursor"] = json!("default");
    save_off["style"]["position"] = json!("relative");
    save_off["children"].as_array_mut().expect("the button's children").insert(0, json!({"id": "save_off_fill", "component": "Box",
        "style": {"position": "absolute", "top": 0, "left": 0, "right": 0, "bottom": 0, "borderRadius": chrome.radius, "backgroundColor": chrome.primary, "opacity": 0.5}}));
    let save_area = json!({"id": "save_area", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "justifyContent": "flex-end"}, "children": [save, save_off]});
    let doctor = super::doctor_section::document(ink, paint, "doctor", "/pane/doctor");
    // EXP-367: local-only destructive hatch for testing fresh-install flows.
    let mut label = ink.text("danger_band_label", json!("Danger zone"), SM, &chrome.danger);
    label["style"]["fontWeight"] = json!(500);
    let mut sentence = ink.text("danger_text", json!("Sign out and delete all local settings, accounts, caches, and cloned repositories."), SM, &ink.muted);
    sentence["style"]["flexGrow"] = json!(1);
    sentence["style"]["flexShrink"] = json!(1);
    sentence["style"]["minWidth"] = json!(0);
    let button = sm_button(ink, "reset", "Reset IDE data", &chrome.danger_bg, &chrome.danger_hover, &chrome.danger_fg, "reset", chrome.radius);
    let mut row = ink.shell("danger_row", false, json!(true), vec![sentence, button]);
    row["style"]["justifyContent"] = json!("space-between");
    let danger = json!({"id": "danger", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0},
        "children": [ink.band_with("danger_band", Some(label), "", None, None), ink.group("danger_group", vec![row])]});
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(1.5), "width": "100%", "minWidth": 0}, "children": [card, save_area, doctor, danger]})
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #EXP-1629: the document raises no issue a debug build's mount check
    /// refuses (#EXP-1767's class).
    #[test]
    fn the_document_raises_no_unexpected_issue() {
        let ink = Ink {
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
        };
        let chrome = Chrome {
            radius: 6.,
            danger: "#f87171ff".into(),
            primary: "#f4f4f5ff".into(),
            primary_fg: "#18181bff".into(),
            primary_hover: "#e4e4e7ff".into(),
            danger_bg: "#dc2626ff".into(),
            danger_fg: "#ffffffff".into(),
            danger_hover: "#b91c1cff".into(),
        };
        let tree = document(&ink, &super::super::readiness::tests::test_paint(), &chrome);
        let bound = super::super::doc::bound_values(&tree);
        let mut surface = eui_core::surface::Surface::new("settings-tools", Default::default());
        let outcome = surface.set_nested(serde_json::from_value(tree).expect("parses"));
        let issues = super::super::doc::unexpected_issues(&bound, &outcome.issues);
        assert!(issues.is_empty(), "{issues:?}");
    }

    /// The pane's whole contract with its siblings: it owns THREE settings
    /// fields and touches nothing else, so a save here can never roll back a
    /// concurrent Agents-pane save (and vice versa). A fourth field silently
    /// joining it (or one dropping out of `save`'s overlay) is exactly the
    /// bug this locks.
    #[test]
    fn the_pane_owns_exactly_three_fields() {
        let mut source = Settings::default();
        source.repos_root = "/tmp/repos".to_string();
        source.branch_prefix = "wip/".to_string();
        source.terminal_shell = Some("/bin/zsh".to_string());
        // Fields the AGENTS pane owns — they must not travel.
        source.default_agent = coding::CodingAgent::Codex;
        source.claude_model = "opus".to_string();

        let mut target = Settings::default();
        ToolsPane::overlay_owned(&mut target, &source);

        let mut expected = Settings::default();
        expected.repos_root = source.repos_root.clone();
        expected.branch_prefix = source.branch_prefix.clone();
        expected.terminal_shell = source.terminal_shell.clone();
        assert_eq!(target, expected);
        assert_eq!(target.default_agent, Settings::default().default_agent);
        assert_eq!(target.claude_model, Settings::default().claude_model);
    }
}
