//! EXP-778 personal pins — the ONE place the desktop reads the per-user
//! `pins` shape and fires `pins.toggle`.
//!
//! A pin names an issue, a coding session or an action (contract `pinKind`).
//! The shape is static per user and NOT team/trash scoped, so every reader
//! here filters to the ACTIVE team and to targets that still resolve in the
//! sibling collections (`issues` / `coding_sessions` / `actions`): a pin
//! whose issue was trashed with its board, or whose session left the
//! caller's window, is simply hidden — never an empty row, never an error.
//!
//! The toggle is fire-and-forget: the server pins when absent and unpins
//! when present, and the row re-streams over the shape, so the button's
//! glyph settles off the collection (the mark-read pattern in `sidebar`).

use gpui::App;
use gpui_component::{button::Button, Icon};
use sync::Store;

use domain::rows::Pin;

use crate::icons::registry;
use crate::queries;

/// The target a pin row names: `(kind, id)` — or `None` when the row is
/// malformed (unknown kind, or the id column that kind needs is empty).
/// Pure so the resolution rule is testable without a store.
pub(crate) fn pin_target(pin: &Pin) -> Option<(&'static str, &str)> {
    let kind = pin.kind.as_deref()?;
    let (kind, id) = match kind {
        domain::contract::PIN_KIND_ISSUE => {
            (domain::contract::PIN_KIND_ISSUE, pin.issue_id.as_deref())
        }
        domain::contract::PIN_KIND_SESSION => {
            (domain::contract::PIN_KIND_SESSION, pin.session_id.as_deref())
        }
        domain::contract::PIN_KIND_ACTION => {
            (domain::contract::PIN_KIND_ACTION, pin.action_id.as_deref())
        }
        _ => return None,
    };
    id.filter(|id| !id.is_empty()).map(|id| (kind, id))
}

/// Display order: `sort_order` ascending (a new pin appends), then
/// `created_at`, then id — total, so the rail never reorders between
/// repaints. Pure so it is testable without a store.
pub(crate) fn sort_pins(pins: &mut [Pin]) {
    pins.sort_by(|a, b| {
        let order = a
            .sort_order
            .unwrap_or(f64::MAX)
            .total_cmp(&b.sort_order.unwrap_or(f64::MAX));
        order
            .then_with(|| a.created_at.cmp(&b.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });
}

/// The caller's pins in `team_id`, in display order, malformed rows dropped.
/// Target resolution is the caller's business (the rail needs the rows it
/// resolves against anyway).
pub(crate) fn pins_in_team(team_id: &str, cx: &App) -> Vec<Pin> {
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let mut pins: Vec<Pin> = store
        .collections()
        .pins
        .read(cx)
        .iter()
        .filter(|pin| pin.team_id.as_deref() == Some(team_id))
        .filter(|pin| pin_target(pin).is_some())
        .cloned()
        .collect();
    sort_pins(&mut pins);
    pins
}

/// Whether the caller has `target_id` pinned as `kind`. The collection is
/// per user already (`user_id = me` server-side), so no owner check here.
pub(crate) fn is_pinned(kind: &str, target_id: &str, cx: &App) -> bool {
    let Some(store) = Store::try_global(cx) else {
        return false;
    };
    store
        .collections()
        .pins
        .read(cx)
        .iter()
        .any(|pin| pin_target(pin) == Some((kind, target_id)))
}

/// Fire `pins.toggle` for `target_id` as `kind` in `team_id`. Fire-and-forget
/// off the foreground: the synced row is the source of truth for the glyph.
pub(crate) fn toggle_pin(team_id: String, kind: &'static str, target_id: String, cx: &mut App) {
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = api::pins::pins_toggle(&trpc, &team_id, kind, &target_id) {
                log::warn!("[ui] pins.toggle({kind} {target_id}) failed: {err}");
            }
        })
        .detach();
}

/// The small glass pin toggle every pinnable header wears (issue detail,
/// session screen, action row): `ui-pin` while unpinned, `ui-unpin` while
/// pinned, tooltip to match. The click stops propagation so a row-level
/// click handler underneath never fires too.
pub(crate) fn pin_toggle_button(
    id: impl Into<gpui::ElementId>,
    team_id: String,
    kind: &'static str,
    target_id: String,
    cx: &App,
) -> Button {
    let pinned = is_pinned(kind, &target_id, cx);
    let (glyph, tooltip) = if pinned {
        (registry::UI_UNPIN, "Unpin")
    } else {
        (registry::UI_PIN, "Pin")
    };
    crate::controls::glass_icon_button(id, Icon::from(glyph), cx)
        .tooltip(tooltip)
        .on_click(move |_, _window, cx| {
            cx.stop_propagation();
            toggle_pin(team_id.clone(), kind, target_id.clone(), cx);
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin(id: &str, kind: Option<&str>, order: Option<f64>) -> Pin {
        Pin {
            id: id.to_string(),
            user_id: Some("me".to_string()),
            team_id: Some("t-1".to_string()),
            kind: kind.map(str::to_string),
            issue_id: Some("i-1".to_string()),
            session_id: Some("s-1".to_string()),
            action_id: Some("a-1".to_string()),
            sort_order: order,
            created_at: Some("2026-09-11T00:00:00Z".to_string()),
            updated_at: None,
        }
    }

    #[test]
    fn target_follows_kind_and_tolerates_junk() {
        assert_eq!(pin_target(&pin("p", Some("issue"), None)), Some(("issue", "i-1")));
        assert_eq!(
            pin_target(&pin("p", Some("session"), None)),
            Some(("session", "s-1"))
        );
        assert_eq!(pin_target(&pin("p", Some("action"), None)), Some(("action", "a-1")));
        // Unknown kind / missing kind → hidden, never a panic.
        assert_eq!(pin_target(&pin("p", Some("board"), None)), None);
        assert_eq!(pin_target(&pin("p", None, None)), None);
        // The kind's own id column is the one that must be set.
        let mut orphan = pin("p", Some("session"), None);
        orphan.session_id = None;
        assert_eq!(pin_target(&orphan), None);
        let mut blank = pin("p", Some("issue"), None);
        blank.issue_id = Some(String::new());
        assert_eq!(pin_target(&blank), None);
    }

    #[test]
    fn sort_is_by_sort_order_then_stable() {
        let mut pins = vec![
            pin("c", Some("issue"), Some(3.0)),
            pin("a", Some("issue"), Some(1.0)),
            pin("z", Some("issue"), None),
            pin("b", Some("issue"), Some(2.0)),
            pin("b2", Some("issue"), Some(2.0)),
        ];
        sort_pins(&mut pins);
        let ids: Vec<&str> = pins.iter().map(|p| p.id.as_str()).collect();
        // A missing sort_order sinks to the end; equal orders fall back to
        // created_at (equal here) then id.
        assert_eq!(ids, ["a", "b", "b2", "c", "z"]);
    }
}
