//! #EXP-1950 (`fixtures/install-trust.json`, decisions a1 + b1): the
//! approval of a new install and a team-mate's reset on the desktop, drawn
//! ONLY from core genUI parts (Section, Row, Badge, Button, Stack, the page
//! EmptyState) painted by the SDK's gpui painter, so the four platforms paint
//! the same tree (web `components/devices/install-approval.tsx`, iOS
//! `InstallApprovalView`).
//!
//! - The ROWS (Needs you, on top of the list): one per install the pin store
//!   yields ([`steer::install_trust::TrustKeeper`]): the Row (label, the code,
//!   the Badge), Approve and Remove on their own line under it, so the code
//!   keeps the row's width and is never truncated (`row`, `neverTruncated`).
//!   The code is derived from the row's endpoint id on THIS install
//!   (`reconcile_trust`), never read from the server.
//! - The WAITING state: this install not approved yet = THE page EmptyState
//!   with the `ui-private` glyph, "Waiting for approval" and its own code.
//! - A press runs the keeper's Approve / Remove off the UI thread; a failure
//!   is the failed-action toast (feedback-states.json).
//!
//! DEV-ONLY `EXP_DEV_INSTALL_TRUST=install-approval|install-waiting` feeds the
//! fixture's rows (N "Pixel 9" New install, T3 "Mira's new iPhone" Reset) or
//! the waiting state (N's code) into the real page, for the captures.

use std::rc::Rc;
use std::sync::Arc;

use eui_gpui::host::{ActionEvent, HostPlugin};
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{
    div, px, App, AppContext as _, Context, Entity, Global, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    WeakEntity, Window,
};
use serde_json::{json, Value};
use steer::install_trust::{copy, install_code, RowKind, TrustKeeper, TrustRow, TrustSnapshot, TrustState, WAITING_ICON};
use sync::Store;

/// install-trust.json `waiting`: the column (web `max-w-md`) and its top
/// offset, a share of the window's height (web `pt-[20vh]`).
const WAITING_W: f32 = 448.;
const WAITING_TOP: f32 = 0.2;

pub(crate) const EVENT_APPROVE: &str = "approve";
pub(crate) const EVENT_REMOVE: &str = "remove";

// ---------------------------------------------------------------------------
// The trees (install-trust.json `row`)
// ---------------------------------------------------------------------------

/// One install's row: the Row (title, the code, the Badge), the two Buttons
/// on their own line under it.
pub(crate) fn pending_row(row: &TrustRow, title: &str) -> Value {
    let id = &row.install_id;
    json!({
        "id": format!("install-{id}"),
        "component": "Stack",
        "props": {"direction": "vertical", "gap": "sm"},
        "children": [
            {
                "id": format!("install-{id}-row"),
                "component": "Row",
                "props": {"title": title, "subtitle": row.code},
                "slots": {
                    "trailing": {
                        "id": format!("install-{id}-badge"),
                        "component": "Badge",
                        "props": {"text": row.kind.badge(), "variant": "secondary"},
                    },
                },
            },
            {
                "id": format!("install-{id}-actions"),
                "component": "Stack",
                "props": {"direction": "horizontal", "gap": "sm"},
                "children": [
                    {
                        "id": format!("install-{id}-approve"),
                        "component": "Button",
                        "props": {"label": copy::APPROVE, "size": "sm"},
                        "on": {"press": {"event": {"name": EVENT_APPROVE, "context": {"id": id}}}},
                    },
                    {
                        "id": format!("install-{id}-remove"),
                        "component": "Button",
                        "props": {"label": copy::REMOVE, "size": "sm", "variant": "ghost"},
                        "on": {"press": {"event": {"name": EVENT_REMOVE, "context": {"id": id}}}},
                    },
                ],
            },
        ],
    })
}

/// The band of installs waiting for this install's approval.
pub(crate) fn pending_tree(rows: &[TrustRow], title: &dyn Fn(&TrustRow) -> String) -> Value {
    json!({
        "id": "pending-installs",
        "component": "Section",
        "props": {},
        "children": rows.iter().map(|row| pending_row(row, &title(row))).collect::<Vec<_>>(),
    })
}

/// This install, not approved yet: THE page EmptyState with its own code.
pub(crate) fn waiting_tree(code: &str) -> Value {
    json!({
        "id": "install-waiting",
        "component": "EmptyState",
        "props": {"icon": WAITING_ICON, "title": copy::WAITING, "description": code},
    })
}

/// A row's title: the install's label; a team-mate's reset leads with
/// their name. The code when nothing else is known.
pub(crate) fn row_title(row: &TrustRow, name_of: &dyn Fn(&str) -> Option<String>) -> String {
    let label = row.label.clone().unwrap_or_default();
    if row.kind != RowKind::Reset {
        return if label.is_empty() { row.code.clone() } else { label };
    }
    let parts: Vec<String> = [name_of(&row.user_id).unwrap_or_default(), label].into_iter().filter(|part| !part.is_empty()).collect();
    if parts.is_empty() {
        row.code.clone()
    } else {
        parts.join(" · ")
    }
}

// ---------------------------------------------------------------------------
// The state: the active account's keeper, or the dev specimen
// ---------------------------------------------------------------------------

/// DEV-ONLY: which specimen `EXP_DEV_INSTALL_TRUST` asks for.
fn dev_specimen() -> Option<String> {
    std::env::var("EXP_DEV_INSTALL_TRUST").ok().filter(|value| value == "install-approval" || value == "install-waiting")
}

const INSTALL_TRUST_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/install-trust.json");

/// The fixture's install `key` as a row of `kind`.
fn fixture_row(key: &str, kind: RowKind) -> TrustRow {
    let fixture: Value = serde_json::from_str(INSTALL_TRUST_JSON).expect("install-trust.json parses");
    let entry = &fixture["keys"][key];
    let endpoint_id = entry["endpointId"].as_str().unwrap_or_default().to_string();
    TrustRow {
        kind,
        install_id: format!("install-{key}"),
        code: install_code(&endpoint_id).unwrap_or_default(),
        endpoint_id,
        user_id: entry["userId"].as_str().unwrap_or_default().to_string(),
        label: entry["label"].as_str().map(str::to_string),
    }
}

/// The specimen's snapshot (`install-approval`: N + T3, as web's picture;
/// `install-waiting`: N waiting).
pub(crate) fn specimen_snapshot(id: &str) -> TrustSnapshot {
    let n = fixture_row("N", RowKind::NewInstall);
    if id == "install-waiting" {
        return TrustSnapshot { state: TrustState::Waiting, rows: Vec::new(), code: Some(n.code) };
    }
    TrustSnapshot { state: TrustState::Trusted, rows: vec![n, fixture_row("T3", RowKind::Reset)], code: None }
}

/// The active account's keeper (its node runs once its key holder does).
pub(crate) fn keeper(cx: &App) -> Option<Arc<TrustKeeper>> {
    crate::enet_wiring::install_trust(cx)
}

/// What Needs you draws now (None = no keeper yet: nothing).
pub(crate) fn snapshot(cx: &App) -> Option<TrustSnapshot> {
    if let Some(id) = dev_specimen() {
        return Some(specimen_snapshot(&id));
    }
    keeper(cx).map(|keeper| keeper.snapshot())
}

/// The rows that count in Needs you (rows only exist on a trusted install).
pub(crate) fn pending_count(cx: &App) -> usize {
    counted(snapshot(cx).as_ref())
}

/// The Needs-you rows a snapshot adds: its pending rows (none while waiting).
pub(crate) fn counted(snapshot: Option<&TrustSnapshot>) -> usize {
    snapshot.map_or(0, |snapshot| if snapshot.waiting() { 0 } else { snapshot.rows.len() })
}

/// Bumped (by [`crate::enet_wiring`]'s watcher) when the keeper's rows or
/// state moved: the Needs you page and the counts observe it.
#[derive(Default)]
pub(crate) struct InstallTrustTick(pub u64);
impl Global for InstallTrustTick {}

// ---------------------------------------------------------------------------
// The surface
// ---------------------------------------------------------------------------

struct ActionHost {
    on_action: Rc<dyn Fn(ActionEvent, &mut App)>,
}

impl HostPlugin for ActionHost {
    fn icon(&self, name: &str) -> Option<SharedString> {
        crate::exponential_ui_host::icon_path(name)
    }

    fn on_action(&self, event: &ActionEvent, cx: &mut App) {
        let (on_action, event) = (self.on_action.clone(), event.clone());
        cx.defer(move |cx| on_action(event, cx));
    }
}

/// The band (or the waiting state) on Needs you: one genUI document.
pub(crate) struct InstallTrustSurface {
    view: Entity<SurfaceView>,
    last: Value,
    /// An install whose press runs (no second press until it answers).
    busy: Option<String>,
}

impl InstallTrustSurface {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let weak: WeakEntity<Self> = cx.entity().downgrade();
        let on_action: Rc<dyn Fn(ActionEvent, &mut App)> = Rc::new(move |event, cx| {
            let _ = weak.update(cx, |this, cx| this.on_action(event, cx));
        });
        let options = SurfaceViewOptions {
            surface_id: format!("install-trust-{}", cx.entity_id()),
            theme: eui_core::themes::builtin_theme(crate::exponential_ui_host::THEME_ID),
            mode: eui_core::theme::Mode::Dark,
            host: Rc::new(ActionHost { on_action }),
            width: None,
            settings: Some(eui_core::surface::SurfaceSettings {
                mode: eui_core::theme::ModeSetting::Dark,
                reduced_motion: theme::motion::reduce_motion(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let view = cx.new(|cx| SurfaceView::new(options, window, cx));
        Self { view, last: Value::Null, busy: None }
    }

    fn on_action(&mut self, event: ActionEvent, cx: &mut Context<Self>) {
        let Some(id) = event.context["id"].as_str().map(str::to_string) else { return };
        if self.busy.is_some() || dev_specimen().is_some() {
            return;
        }
        let approve = match event.name.as_str() {
            EVENT_APPROVE => true,
            EVENT_REMOVE => false,
            _ => return,
        };
        let Some(keeper) = keeper(cx) else { return };
        // Only a row this install's pin store yields (never a server's code).
        let Some(row) = keeper.snapshot().rows.into_iter().find(|row| row.install_id == id) else { return };
        self.busy = Some(id);
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { if approve { keeper.approve(&row) } else { keeper.remove(&row) } })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = None;
                if let Err(err) = result {
                    let verb = if approve { "approve" } else { "remove" };
                    crate::action_error::failed_action(verb, "the install", Some(err), cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn show(&mut self, tree: Value, cx: &mut App) {
        if self.last == tree {
            return;
        }
        let nested: eui_core::NestedNode = match serde_json::from_value(tree.clone()) {
            Ok(nested) => nested,
            Err(error) => {
                log::warn!("[install-trust] the document does not parse: {error}");
                return;
            }
        };
        self.last = tree;
        self.view.update(cx, |view, cx| {
            let outcome = view.set_nested(nested, cx);
            if !outcome.issues.is_empty() {
                log::debug!("[install-trust] document issues: {:?}", outcome.issues);
            }
        });
    }
}

impl Render for InstallTrustSurface {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "InstallTrustSurface").entered();
        let Some(snapshot) = snapshot(cx) else { return div() };
        if snapshot.waiting() {
            self.show(waiting_tree(snapshot.code.as_deref().unwrap_or_default()), cx);
            // Web `mx-auto max-w-md pt-[20vh]`.
            let top = window.viewport_size().height * WAITING_TOP;
            return div()
                .w_full()
                .flex()
                .justify_center()
                .pt(top)
                .child(div().w_full().max_w(px(WAITING_W)).child(self.view.clone()));
        }
        if snapshot.rows.is_empty() {
            return div();
        }
        let names: Vec<(String, String)> = Store::try_global(cx)
            .map(|store| {
                store.collections().users.read(cx).iter().filter_map(|user| Some((user.id.clone(), user.name.clone()?))).collect()
            })
            .unwrap_or_default();
        let name_of = |user_id: &str| names.iter().find(|(id, _)| id == user_id).map(|(_, name)| name.clone());
        self.show(pending_tree(&snapshot.rows, &|row| row_title(row, &name_of)), cx);
        div().w_full().min_w_0().child(self.view.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<TrustRow> {
        specimen_snapshot("install-approval").rows
    }

    /// The fixture's codes, derived here (never served).
    #[test]
    fn the_specimen_rows_are_the_fixture_installs() {
        let rows = rows();
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].kind, rows[0].code.as_str(), rows[0].label.as_deref()), (RowKind::NewInstall, "HNMBN A0319", Some("Pixel 9")));
        assert_eq!((rows[1].kind, rows[1].code.as_str(), rows[1].label.as_deref()), (RowKind::Reset, "776AF T3NNF", Some("Mira's new iPhone")));
        let waiting = specimen_snapshot("install-waiting");
        assert!(waiting.waiting());
        assert_eq!(waiting.code.as_deref(), Some("HNMBN A0319"));
    }

    /// install-trust.json `row`: the same tree and node ids as web.
    #[test]
    fn the_row_is_the_fixture_tree() {
        let row = &rows()[0];
        let tree = pending_row(row, "Pixel 9");
        assert_eq!(tree["id"], "install-install-N");
        assert_eq!(tree["component"], "Stack");
        assert_eq!(tree["props"], json!({"direction": "vertical", "gap": "sm"}));
        let line = &tree["children"][0];
        assert_eq!((line["id"].as_str(), line["component"].as_str()), (Some("install-install-N-row"), Some("Row")));
        assert_eq!(line["props"], json!({"title": "Pixel 9", "subtitle": "HNMBN A0319"}));
        let badge = &line["slots"]["trailing"];
        assert_eq!(badge["id"], "install-install-N-badge");
        assert_eq!(badge["props"], json!({"text": "New install", "variant": "secondary"}));
        let actions = &tree["children"][1];
        assert_eq!(actions["id"], "install-install-N-actions");
        assert_eq!(actions["props"], json!({"direction": "horizontal", "gap": "sm"}));
        assert_eq!(actions["children"][0]["props"], json!({"label": "Approve", "size": "sm"}));
        assert_eq!(actions["children"][1]["props"], json!({"label": "Remove", "size": "sm", "variant": "ghost"}));
        assert_eq!(actions["children"][0]["on"]["press"]["event"], json!({"name": "approve", "context": {"id": "install-N"}}));
        assert_eq!(actions["children"][1]["on"]["press"]["event"], json!({"name": "remove", "context": {"id": "install-N"}}));
        let band = pending_tree(&rows(), &|row| row.label.clone().unwrap_or_default());
        assert_eq!((band["id"].as_str(), band["component"].as_str()), (Some("pending-installs"), Some("Section")));
        assert_eq!(band["children"][1]["children"][0]["slots"]["trailing"]["props"]["text"], "Reset");
        // Every tree parses as a catalog document.
        serde_json::from_value::<eui_core::NestedNode>(band).unwrap();
        serde_json::from_value::<eui_core::NestedNode>(waiting_tree("HNMBN A0319")).unwrap();
    }

    #[test]
    fn the_waiting_state_is_the_page_empty_state() {
        assert_eq!(
            waiting_tree("HNMBN A0319"),
            json!({"id": "install-waiting", "component": "EmptyState", "props": {"icon": "ui-private", "title": "Waiting for approval", "description": "HNMBN A0319"}})
        );
    }

    /// install-trust.json `waiting`.
    #[test]
    fn the_waiting_state_stands_where_the_fixture_says() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/install-trust.json")).unwrap();
        assert_eq!(fixture["waiting"]["topOffset"].as_f64().unwrap() as f32, WAITING_TOP);
        assert_eq!(fixture["waiting"]["maxWidth"].as_f64().unwrap() as f32, WAITING_W);
        assert_eq!(fixture["waiting"]["listGap"], 12, "needs_you.rs gap_3");
    }

    /// The pending rows count in Needs you; the waiting state and no keeper add none.
    #[test]
    fn the_pending_rows_count_in_needs_you() {
        assert_eq!(counted(Some(&specimen_snapshot("install-approval"))), 2);
        assert_eq!(counted(Some(&specimen_snapshot("install-waiting"))), 0);
        assert_eq!(counted(None), 0);
    }

    /// A reset leads with the team-mate's name; a new install is its label.
    #[test]
    fn a_rows_title() {
        let rows = rows();
        let named = |user: &str| (user == "user-mate").then(|| "Mira".to_string());
        assert_eq!(row_title(&rows[0], &named), "Pixel 9");
        assert_eq!(row_title(&rows[1], &named), "Mira · Mira's new iPhone");
        assert_eq!(row_title(&rows[1], &|_| None), "Mira's new iPhone");
        let bare = TrustRow { label: None, ..rows[0].clone() };
        assert_eq!(row_title(&bare, &|_| None), "HNMBN A0319");
    }
}
