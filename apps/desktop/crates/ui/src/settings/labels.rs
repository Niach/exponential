//! Settings → Labels (masterplan-v3 §4.2).
//!
//! Web parity: `components/team/labels-section.tsx` — inline label rows
//! (color swatch `Popover` + borderless name `Input` persisting on
//! blur/Enter + the delete confirm) and the create form (name input +
//! `ColorSwatchGrid` + Create/Cancel).
//!
//! #EXP-1629: the pane is an eUI document on the native binding: the
//! "Labels" band ("New label" its trailing pill) over one row per label (the
//! genUI ColorPicker behind the row's swatch, the bare name field, the
//! delete), the empty state, and the create form (the boxed name field, the
//! inline ColorPicker grid, Create / Cancel). The swatch grids are web's
//! `ColorSwatchGrid` (28 px cells).
//!
//! Reads come from the synced `labels` collection; create/update/delete are
//! §4.1 un-gated tRPC mutations reflected by the Electric echo. A row's typed
//! name is its draft (like the web's per-row `useState(label.name)`): an
//! untouched row follows a teammate's rename, a half-typed one stays the
//! user's until it persists.

use std::collections::HashMap;
use std::rc::Rc;

use gpui::{div, App, AppContext as _, Entity, IntoElement, ParentElement, Render, Styled, Subscription, Window};
use gpui_component::ActiveTheme as _;
use serde_json::{json, Value};
use sync::Store;

use domain::rows::Label;
use eui_gpui::binding::Binding;
use eui_gpui::view::SurfaceView;

use crate::native_dialog::{self, AlertSpec};
use crate::navigation::{active_team_id, Navigation};

use super::doc::{empty_state, hex, mount, not, path, IdeToasts, Ink, PillPaint, XS};
use super::{parse_hex_color, spawn_trpc};

/// Web `LABEL_COLORS` (packages/ui/src/label-colors.ts) — verbatim.
pub(crate) const LABEL_COLORS: [&str; 20] = [
    "#ef4444", "#dc2626", "#f97316", "#f59e0b", "#eab308", "#84cc16", "#22c55e", "#10b981",
    "#14b8a6", "#06b6d4", "#0ea5e9", "#3b82f6", "#6366f1", "#8b5cf6", "#a855f7", "#ec4899",
    "#f43f5e", "#78716c", "#64748b", "#a3a3a3",
];

/// Web `STATUS_COLORS` — the label palette plus white (EXP-685: the retired
/// builtin Todo was the one white status, so a team that wants it back
/// recreates it byte-identical). Status-only on purpose: labels and boards
/// keep [`LABEL_COLORS`].
pub(crate) const STATUS_COLORS: [&str; 21] = [
    "#ef4444", "#dc2626", "#f97316", "#f59e0b", "#eab308", "#84cc16", "#22c55e", "#10b981",
    "#14b8a6", "#06b6d4", "#0ea5e9", "#3b82f6", "#6366f1", "#8b5cf6", "#a855f7", "#ec4899",
    "#f43f5e", "#78716c", "#64748b", "#a3a3a3", "#fafafa",
];

/// The data-model path the projection is written at.
const PATH: &str = "/pane";

/// Web `A label with this name already exists.` — shown for the local
/// duplicate pre-check on create and rename (EXP-254).
const DUPLICATE_NAME_MESSAGE: &str = "A label with this name already exists.";

pub struct LabelsPane {
    nav: Entity<Navigation>,
    /// label id → the name typed into its row and not yet persisted.
    drafts: HashMap<String, String>,
    creating: bool,
    /// The create form's typed name (the field is the document's; edits
    /// arrive as `newName` actions).
    new_name: String,
    new_color: String,
    submitting: bool,
    /// Inline error under the create form (duplicate name / server reject).
    create_error: Option<String>,
    /// (label id, message) — inline error under a row whose rename failed.
    row_error: Option<(String, String)>,
    /// The bench's labels (headless has no team or synced rows).
    #[cfg(test)]
    shot_labels: Option<Vec<Label>>,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    _subscriptions: Vec<Subscription>,
}

/// The pane in a pinned state (the bench's).
#[cfg(test)]
pub(crate) struct LabelsShot {
    /// The team's labels (`None` = no team).
    pub labels: Option<Vec<Label>>,
    pub creating: bool,
    /// The create form's typed name and picked colour.
    pub typed: String,
    pub color: String,
    pub submitting: bool,
    pub create_error: Option<String>,
    pub row_error: Option<(String, String)>,
    /// A row's half-typed rename (label id, text).
    pub draft: Option<(String, String)>,
}

impl LabelsPane {
    pub fn new(nav: Entity<Navigation>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&nav, |this, _, cx| {
                // Team switch: the drafts and inline errors belong to the
                // old scope.
                this.create_error = None;
                this.row_error = None;
                this.prune_drafts(cx);
                cx.notify();
            }),
            cx.observe(&collections.labels, |this, _, cx| {
                this.prune_drafts(cx);
                cx.notify();
            }),
        ];
        // Every action leaves the binding for the pane, deferred (the
        // binding is mid-update and the pane writes it; the delete confirm
        // needs the window).
        let pane = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            for name in ["new", "cancel", "create", "newName", "newColor", "rowName", "persist", "rowColor", "delete"] {
                let pane = pane.clone();
                binding.on_action(name, move |_, event, cx| {
                    let key = event.context.get("key").and_then(Value::as_str).unwrap_or_default().to_string();
                    let value = event.payload.as_ref().and_then(|p| p.get("value")).and_then(Value::as_str).unwrap_or_default().to_string();
                    let pane = pane.clone();
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| {
                            let _ = pane.update(cx, |pane, cx| pane.action(name, key, value, window, cx));
                        });
                    });
                    None
                });
            }
            binding
        });
        let view = mount("settings-labels", document(&Ink::of(cx), &Chrome::of(cx)), &binding, window, cx);
        Self {
            nav,
            drafts: HashMap::new(),
            creating: false,
            new_name: String::new(),
            new_color: LABEL_COLORS[6].to_string(),
            submitting: false,
            create_error: None,
            row_error: None,
            #[cfg(test)]
            shot_labels: None,
            binding,
            view,
            _subscriptions: subscriptions,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = crate::navigation::nav_for_window(window, cx);
        Self::new(nav, window, cx)
    }

    #[cfg(test)]
    pub(crate) fn show(&mut self, shot: LabelsShot, _window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.shot_labels = Some(shot.labels.unwrap_or_default());
        self.creating = shot.creating;
        self.new_name = shot.typed;
        self.new_color = shot.color;
        self.submitting = shot.submitting;
        self.create_error = shot.create_error;
        self.row_error = shot.row_error;
        self.drafts = shot.draft.into_iter().collect();
        cx.notify();
    }

    /// The bench's press on node `id` (a row's swatch; `|` separates
    /// candidates), its popup's enter fade run out.
    #[cfg(test)]
    pub(crate) fn press_for_shots(&mut self, id: &str, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.view.update(cx, |view, cx| {
            let index = id.split('|').find_map(|id| view.index_of(id)).expect("the pressed node");
            view.press(index, window, cx);
            view.advance_clock(std::time::Duration::from_millis(500), cx);
        });
    }

    /// Runs an open popup's fade out after the press's frames.
    #[cfg(test)]
    pub(crate) fn press_for_shots_settle(&mut self, cx: &mut gpui::Context<Self>) {
        self.view.update(cx, |view, cx| view.advance_clock(std::time::Duration::from_millis(500), cx));
    }

    /// A document action: its name, the row `key` and the payload's `value`.
    fn action(&mut self, name: &str, key: String, value: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match name {
            "new" => {
                self.creating = true;
                self.new_name.clear();
            }
            "cancel" => {
                self.creating = false;
                self.new_name.clear();
                self.reset_form();
            }
            "create" => self.create(cx),
            // Typing clears a stale server error (the live duplicate check
            // re-derives in the projection).
            "newName" => {
                self.new_name = value;
                self.create_error = None;
            }
            "newColor" if !value.is_empty() => self.new_color = value,
            "rowName" => {
                // Typing clears the row's stale duplicate error.
                if self.row_error.as_ref().is_some_and(|(id, _)| id == &key) {
                    self.row_error = None;
                }
                self.drafts.insert(key, value);
            }
            "persist" => self.persist_name(&key, cx),
            "rowColor" if !value.is_empty() => {
                let Some(label) = self.scoped_labels(cx).into_iter().find(|label| label.id == key) else { return };
                spawn_trpc(cx, "labels.update(color)", move |trpc| api::labels::labels_update(trpc, &label.team_id, &label.id, None, Some(&value)));
            }
            "delete" => {
                let Some(label) = self.scoped_labels(cx).into_iter().find(|label| label.id == key) else { return };
                // dialog stays native: THE Prompt (`delete-label`) confirms a destructive delete.
                Self::confirm_delete(label, window, cx);
            }
            _ => return,
        }
        cx.notify();
    }

    /// EXP-1230: the delete confirms with the prompts fixture's
    /// `delete-label`.
    fn confirm_delete(label: Label, window: &mut Window, cx: &mut App) {
        let spec = AlertSpec::from_prompt("Delete label", &domain::prompts::delete_label(&label.name)).on_ok(move |_, cx| {
            let (team_id, label_id) = (label.team_id.clone(), label.id.clone());
            spawn_trpc(cx, "labels.delete", move |trpc| api::labels::labels_delete(trpc, &team_id, &label_id));
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }

    /// Drop the drafts of labels that left the scope, and those the synced
    /// name has caught up with (the row follows the label again).
    fn prune_drafts(&mut self, cx: &App) {
        let labels: HashMap<String, String> = self.scoped_labels(cx).into_iter().map(|label| (label.id, label.name)).collect();
        self.drafts.retain(|id, draft| labels.get(id).is_some_and(|name| name != draft));
    }

    /// Web `isDuplicateName`: another label in the team already has this name
    /// (case-insensitive, matching the server's (team_id, lower(name)) unique).
    fn is_duplicate_name(&self, name: &str, exclude_id: Option<&str>, cx: &App) -> bool {
        let needle = name.trim().to_lowercase();
        self.scoped_labels(cx).iter().any(|label| Some(label.id.as_str()) != exclude_id && label.name.trim().to_lowercase() == needle)
    }

    /// The team's labels, `sortOrder` then name (web `orderBy`).
    fn scoped_labels(&self, cx: &App) -> Vec<Label> {
        #[cfg(test)]
        if let Some(labels) = &self.shot_labels {
            return labels.clone();
        }
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return Vec::new();
        };
        let mut labels: Vec<Label> = Store::global(cx).collections().labels.read(cx).iter().filter(|label| label.team_id == team_id).cloned().collect();
        labels.sort_by(|a, b| {
            a.sort_order
                .unwrap_or(f64::MAX)
                .total_cmp(&b.sort_order.unwrap_or(f64::MAX))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        labels
    }

    /// Web `persistName` (blur / Enter): trim; empty or unchanged resets the
    /// row to the synced name, otherwise `labels.update`. Nothing typed =
    /// nothing to persist, so a teammate's rename is never written back.
    fn persist_name(&mut self, label_id: &str, cx: &mut gpui::Context<Self>) {
        let Some(label) = self.scoped_labels(cx).into_iter().find(|label| label.id == label_id) else { return };
        let Some(typed) = self.drafts.get(label_id).map(|draft| draft.trim().to_string()) else { return };
        if typed.is_empty() || typed == label.name {
            self.drafts.remove(label_id);
            self.row_error = None;
            return;
        }
        // Web `persistName` duplicate pre-check: keep the typed name so the
        // user can fix it, show the inline error, skip the mutation (the
        // server enforces the same unique either way — EXP-254).
        if self.is_duplicate_name(&typed, Some(label_id), cx) {
            self.row_error = Some((label_id.to_string(), DUPLICATE_NAME_MESSAGE.to_string()));
            return;
        }
        self.row_error = None;
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            return;
        };
        let (team_id, label_id) = (label.team_id.clone(), label_id.to_string());
        // Web `LabelRow`: a rejected rename says so under the row instead of
        // vanishing into the log.
        cx.spawn(async move |this, cx| {
            let call_id = label_id.clone();
            let result = cx.background_executor().spawn(async move { api::labels::labels_update(&trpc, &team_id, &call_id, Some(&typed), None) }).await;
            let _ = this.update(cx, |this, cx| {
                if let Err(err) = &result {
                    log::warn!("[ui] labels.update(name) failed: {err}");
                    // FEED-79: a name already taken is INVALID INPUT (under
                    // the row); anything else is the error toast.
                    if crate::action_error::is_name_conflict(err) {
                        this.row_error = Some((label_id.clone(), super::form_error(err, "Failed to rename label.")));
                    } else {
                        crate::action_error::failed_action_err("rename", "the label", err, cx);
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn create(&mut self, cx: &mut gpui::Context<Self>) {
        if self.submitting {
            return;
        }
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return;
        };
        let name = self.new_name.trim().to_string();
        if name.is_empty() || self.is_duplicate_name(&name, None, cx) {
            return;
        }
        let color = self.new_color.clone();
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            return;
        };

        self.submitting = true;
        self.create_error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { api::labels::labels_create(&trpc, &team_id, &name, Some(&color)) }).await;
            let _ = this.update(cx, |this, cx| {
                this.submitting = false;
                if let Err(err) = &result {
                    log::warn!("[ui] labels.create failed: {err}");
                    // The duplicate-name CONFLICT racing a not-yet-synced
                    // label is INVALID INPUT (inline); everything else is the
                    // error toast (FEED-79). A create can duplicate: no Retry.
                    if crate::action_error::is_name_conflict(err) {
                        this.create_error = Some(super::form_error(err, "Failed to create label."));
                    } else {
                        crate::action_error::failed_action_err("create", "the label", err, cx);
                    }
                } else {
                    this.creating = false;
                    this.new_name.clear();
                    this.reset_form();
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Web `resetForm`: random color for the next label.
    fn reset_form(&mut self) {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos() as usize).unwrap_or(0);
        self.new_color = LABEL_COLORS[nanos % LABEL_COLORS.len()].to_string();
        self.create_error = None;
    }

    /// The state as the document binds it.
    fn project(&self, cx: &App) -> Value {
        let labels = self.scoped_labels(cx);
        let muted = hex(cx.theme().muted_foreground);
        let name = self.new_name.trim();
        // Web `newNameIsDuplicate`: live check against the synced labels;
        // the message shows and Create disables before any round-trip.
        let duplicate = !name.is_empty() && self.is_duplicate_name(name, None, cx);
        let form_error = if duplicate { Some(DUPLICATE_NAME_MESSAGE.to_string()) } else { self.create_error.clone() };
        json!({
            "creating": self.creating,
            "empty": labels.is_empty() && !self.creating,
            "rows": labels.iter().map(|label| {
                let color = label.color.clone().unwrap_or_default();
                let swatch = if parse_hex_color(&color).is_some() { color.trim().to_string() } else { muted.clone() };
                let error = self.row_error.as_ref().filter(|(id, _)| id == &label.id).map(|(_, message)| message.clone());
                json!({
                    "key": label.id,
                    "name": self.drafts.get(&label.id).unwrap_or(&label.name),
                    "color": color,
                    "swatch": swatch,
                    "deleteLabel": format!("Delete label {}", label.name),
                    "hasError": error.is_some(),
                    "error": error.unwrap_or_default(),
                })
            }).collect::<Vec<_>>(),
            "form": {
                "name": self.new_name,
                "color": self.new_color,
                "hasError": form_error.is_some(),
                "error": form_error.unwrap_or_default(),
                // Web copy, ellipsis and all: `Creating...` is three dots
                // there, not `…`.
                "create": if self.submitting { "Creating..." } else { "Create label" },
                "createOff": name.is_empty() || self.submitting || duplicate,
                "submitting": self.submitting,
            },
        })
    }
}

/// The colours the document needs beyond [`Ink`].
struct Chrome {
    danger: String,
    list_hover: String,
    stroke: String,
    /// The theme radius (a `web_xs` Button's corner).
    radius: f32,
}

impl Chrome {
    fn of(cx: &App) -> Self {
        let theme = cx.theme();
        Self { danger: hex(theme.danger), list_hover: hex(theme.list_hover), stroke: hex(super::row_stroke(cx)), radius: f32::from(theme.radius) }
    }
}

/// `action_error::field_error`: 13 px in the danger ink, a 20 px line.
fn field_error(chrome: &Chrome, id: &str, message: Value, visible: Value, inset: f32) -> Value {
    let size = crate::action_error::FIELD_ERROR_PX;
    json!({"id": id, "component": "Text", "visible": visible, "props": {"text": message}, "style": {"fontSize": size, "lineHeight": 20, "color": chrome.danger, "paddingHorizontal": inset}})
}

/// One label row (`flat_row`: `px_3` / `py_1p5`, `gap_3`): the swatch (a
/// genUI ColorPicker behind the IDE's `xsmall` ghost trigger's 14 px dot),
/// the borderless name field (`web_input_sm`: 32 px, the field's 10 px
/// inset) and the delete; the rename error under it (`gap_1`, `px_1`).
fn row(ink: &Ink, chrome: &Chrome) -> Value {
    let dot = json!({"id": "swatch_dot", "component": "Box", "style": {"width": ink.r(1.), "height": ink.r(1.), "flexShrink": 0, "borderRadius": 9999, "borderWidth": 1, "borderColor": chrome.stroke, "backgroundColor": path("swatch")}});
    // The picker's trigger is a 24 px circle; the IDE's `xsmall` button is
    // `px_1` around the dot (21 px): the slot is that wide, the circle
    // centred on it.
    let picker = json!({"id": "swatch", "component": "ColorPicker", "props": {"value": path("color")}, "style": {"flexShrink": 0, "marginLeft": (ink.r(1.5) - 24.) / 2.},
        "on": {"change": {"event": {"name": "rowColor", "context": {"key": path("key")}}}}, "children": [dot]});
    let slot = json!({"id": "swatch_slot", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": ink.r(1.5), "flexShrink": 0}, "children": [picker]});
    let input = json!({"id": "name", "component": "Input", "props": {"variant": "bare", "label": "Name", "name": "name", "value": path("name")}, "style": {"flexGrow": 1, "minWidth": 0},
        "on": {"change": {"event": {"name": "rowName", "context": {"key": path("key")}}}, "submit": {"event": {"name": "persist", "context": {"key": path("key")}}}, "blur": {"event": {"name": "persist", "context": {"key": path("key")}}}}});
    let field = json!({"id": "name_box", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "flexGrow": 1, "flexShrink": 1, "flexBasis": 0, "minWidth": 0, "height": 32, "paddingHorizontal": 10}, "children": [input]});
    // a11y: named by the caller (the third argument is the genUI label)
    let delete = ink.ghost_icon_button("delete", "ui-delete", path("deleteLabel"), json!({"name": "delete", "context": {"key": path("key")}}), &chrome.list_hover);
    let row = json!({"id": "row", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0, "gap": ink.r(0.75), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.375), "borderRadius": ink.radius_md},
        "children": [slot, field, delete]});
    json!({"id": "row_wrap", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "gap": ink.r(0.25)},
        "children": [row, field_error(chrome, "row_error", path("error"), path("hasError"), ink.r(0.25))]})
}

/// The inline create form (`flat_row`, `p_3`, `gap_3`): the boxed name
/// field, its error, the "Color" caption over the inline ColorPicker grid
/// (`gap_1p5`), then Create (a `web_xs` primary `Button`) and Cancel.
fn form(ink: &Ink, chrome: &Chrome) -> Value {
    let input = json!({"id": "new_name", "component": "Input", "props": {"variant": "box", "size": "sm", "label": "Label name", "name": "newName", "placeholder": "Label name", "value": path("/pane/form/name")},
        "on": {"change": {"event": {"name": "newName"}}, "submit": {"event": {"name": "create"}}}});
    let color = json!({"id": "new_color_column", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.375)},
        "children": [
            ink.text("new_color_label", json!("Color"), XS, &ink.muted),
            {"id": "new_color", "component": "ColorPicker", "props": {"inline": true, "value": path("/pane/form/color")}, "on": {"change": {"event": {"name": "newColor"}}}},
        ]});
    let mut create = ink.pill_button_with("create", PillPaint::Primary, None, None, path("/pane/form/create"), path("/pane/form/createOff"), None, json!({"name": "create"}));
    for button in create.iter_mut() {
        button["style"]["borderRadius"] = json!(chrome.radius);
        for child in button["children"].as_array_mut().into_iter().flatten() {
            if child["style"].get("position").is_some() {
                child["style"]["borderRadius"] = json!(chrome.radius);
            }
        }
    }
    let [create, create_off] = create;
    let [cancel, cancel_off] = ink.pill_button("cancel", None, json!("Cancel"), path("/pane/form/submitting"), json!({"name": "cancel"}));
    let buttons = json!({"id": "form_buttons", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.5)}, "children": [create, create_off, cancel, cancel_off]});
    json!({"id": "form", "component": "Box", "visible": path("/pane/creating"), "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.75), "padding": ink.r(0.75), "borderRadius": ink.radius_md, "width": "100%", "minWidth": 0},
        "children": [input, field_error(chrome, "form_error", path("/pane/form/error"), path("/pane/form/hasError"), 0.), color, buttons]})
}

/// The pane's tree: the band (New label while no form is open) over the
/// rows or the empty state, then the create form (`section`: `gap_2`).
fn document(ink: &Ink, chrome: &Chrome) -> Value {
    let [mut new_label, _] = ink.pill_button("new", Some("ui-add"), json!("New label"), json!(false), json!({"name": "new"}));
    // Beside a glyph the IDE's pill label sits on the lower device pixel.
    new_label["children"][1]["style"]["paddingTop"] = json!(1);
    let new_label = json!({"id": "new_slot", "component": "Box", "visible": not(path("/pane/creating")), "style": {"display": "flex", "flexShrink": 0}, "children": [new_label]});
    let band = ink.band("band", "Labels", Some(new_label));
    let rows = json!({"id": "rows", "component": "List", "props": {"gap": "none"},
        "template": {"component": "row_wrap", "path": "/pane/rows", "key": "key"},
        "children": [row(ink, chrome)]});
    // The list is always there (an empty one still takes its `gap_2`).
    let list = json!({"id": "list", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0},
        "children": [rows, empty_state(ink, "empty", "settings-labels", crate::empty_state_copy::LABELS, path("/pane/empty"))]});
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%", "minWidth": 0},
        "children": [band, list, form(ink, chrome)]})
}

impl Render for LabelsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "LabelsPane").entered();
        let projection = self.project(cx);
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        div().w_full().min_w_0().child(self.view.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The document parses and raises no issue beyond the literal chrome.
    #[test]
    fn the_document_parses() {
        let ink = Ink {
            rem: 14.,
            foreground: "#fafafaff".into(),
            hint: "#fafafaff".into(),
            value: "#fafafaff".into(),
            muted: "#a1a1aaff".into(),
            muted_glyph: "#a1a1aaff".into(),
            primary: "#fafafaff".into(),
            fill_row: "#ffffff0d".into(),
            stroke_row: "#ffffff14".into(),
            fill_card: "#ffffff0d".into(),
            stroke_card: "#ffffff14".into(),
            code: "#27272aff".into(),
            radius_lg: 12.,
            radius_md: 10.,
            fill_section: "#ffffff0a".into(),
            fill_active: "#ffffff14".into(),
            mono: "$type.family.mono".into(),
            pill_button_fill: "#ffffff0d".into(),
            pill_button_fill_disabled: "#ffffff05".into(),
            primary_button_fill: "#fafafaff".into(),
            primary_button_hover: "#e4e4e7ff".into(),
            primary_button_fill_disabled: "#fafafa80".into(),
            primary_button_fg: "#18181bff".into(),
        };
        let chrome = Chrome { danger: "#f43f5eff".into(), list_hover: "#ffffff0d".into(), stroke: "#ffffff14".into(), radius: 10. };
        let tree = document(&ink, &chrome);
        let _: eui_core::NestedNode = serde_json::from_value(tree).expect("the labels document parses");
    }
}
