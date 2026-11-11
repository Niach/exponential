//! #EXP-1629: the agent pages' document pieces, shared by Settings → New
//! agent ([`super::new_agent`]) and one agent's page: the identity row
//! (genUI IconPicker / ColorPicker + the bare name), the row Selects
//! (Device, Account) with option glyphs and descriptions, the Schedule rows
//! and the Instructions field. Each piece draws today's gpui row at 0 px
//! (`settings_bench`).

use gpui::App;
use gpui_component::ActiveTheme as _;
use serde_json::{json, Value};

use domain::agent_settings::AGENT_SETTINGS;

use super::agent_detail::field_label;
use super::doc::{faded, hex, not, path, Ink, SM, VALUE_ALPHA, XS};

/// The colours the agent documents need beyond [`Ink`].
pub(super) struct Chrome {
    pub(super) radius: f32,
    pub(super) danger_bg: String,
    pub(super) danger_fg: String,
    pub(super) danger_hover: String,
    pub(super) danger_text: String,
    pub(super) list_hover: String,
    /// The Assistant's mark disc (`muted`).
    pub(super) disc: String,
    /// `feedback::skeleton_fill`.
    pub(super) skeleton: String,
    /// The `+ Add` outline button: its fill, stroke, hover fill and ink.
    pub(super) outline_fill: String,
    pub(super) outline: String,
    pub(super) outline_hover: String,
    pub(super) outline_fg: String,
    /// The primary `Button`: fill, hover and ink.
    pub(super) primary: String,
    pub(super) primary_hover: String,
    pub(super) primary_fg: String,
}

impl Chrome {
    pub(super) fn of(cx: &App) -> Self {
        let theme = cx.theme();
        Self {
            radius: f32::from(theme.radius),
            danger_bg: hex(theme.button_danger),
            danger_fg: hex(theme.button_danger_foreground),
            danger_hover: hex(theme.button_danger_hover),
            danger_text: hex(theme.danger),
            list_hover: hex(theme.list_hover),
            disc: hex(theme.muted),
            skeleton: hex(theme.muted_foreground.opacity(0.18)),
            outline_fill: hex(theme.input_background()),
            outline: hex(theme.input),
            outline_hover: hex(gpui_component::Colorize::mix_oklab(&theme.input, theme.transparent, 0.5)),
            outline_fg: hex(theme.button_foreground),
            primary: hex(theme.primary),
            primary_hover: hex(theme.primary_hover),
            primary_fg: hex(theme.primary_foreground),
        }
    }
}

#[cfg(test)]
impl Chrome {
    /// Plain colours for the document checks.
    pub(super) fn test() -> Self {
        let c = || "#808080".to_string();
        Self { radius: 6., danger_bg: c(), danger_fg: c(), danger_hover: c(), danger_text: c(), list_hover: c(), disc: c(), skeleton: c(), outline_fill: c(), outline: c(), outline_hover: c(), outline_fg: c(), primary: c(), primary_hover: c(), primary_fg: c() }
    }
}

/// A login's brand mark as a genUI icon (#EXP-1764).
pub(super) fn agent_mark_icon(agent: coding::CodingAgent) -> &'static str {
    match agent {
        coding::CodingAgent::Claude => "brand-claude",
        coding::CodingAgent::Codex => "brand-codex",
    }
}

/// A row Select (`glass_picker_row` + the IDE's Picker): genUI's `Select`
/// with the `row` trigger over `/pane/<base>/{value,options}`.
pub(super) fn select_row(id: &str, label: &str, base: &str, action: &str, extra: Value) -> Value {
    let mut props = json!({"triggerVariant": "row", "label": label, "name": id, "value": path(&format!("/pane/{base}/value")), "options": path(&format!("/pane/{base}/options"))});
    for (k, v) in extra.as_object().expect("the extra props") {
        props[k] = v.clone();
    }
    json!({"id": id, "component": "Select", "props": props, "on": {"change": {"event": {"name": action}}}})
}

/// A picker row whose value can't be picked here: the label, then the
/// value (no chevron) — `muted`, else 70% foreground.
pub(super) fn value_row(ink: &Ink, id: &str, visible: Value, label: &str, value: Value, muted: Value) -> Value {
    let column = json!({"id": format!("{id}_text"), "component": "Box", "style": {"display": "flex", "flexDirection": "column", "flexShrink": 0}, "children": [ink.text(&format!("{id}_label"), json!(label), SM, &ink.foreground)]});
    let mut plain = faded(ink.text(&format!("{id}_value"), value.clone(), SM, &ink.value), VALUE_ALPHA);
    plain["visible"] = not(muted.clone());
    let mut dim = ink.text(&format!("{id}_value_muted"), value, SM, &ink.muted);
    dim["visible"] = muted;
    let control = json!({"id": format!("{id}_control"), "component": "Box", "style": {"display": "flex", "flexDirection": "row", "justifyContent": "flex-end", "flexGrow": 1, "minWidth": 200}, "children": [plain, dim]});
    ink.shell(id, false, visible, vec![column, control])
}

/// The identity row: the icon picker, the colour picker and the name typed
/// straight into the row — the board form's.
pub(super) fn identity(ink: &Ink) -> Value {
    let icon = json!({"id": "icon_picker", "component": "IconPicker", "props": {"value": path("/pane/icon"), "color": path("/pane/color")}, "style": {"flexShrink": 0}, "on": {"change": {"event": {"name": "icon"}}}});
    let color = json!({"id": "color_picker", "component": "ColorPicker", "props": {"value": path("/pane/color")}, "style": {"flexShrink": 0}, "on": {"change": {"event": {"name": "color"}}}});
    let name = json!({"id": "name", "component": "Input", "props": {"variant": "bare", "label": "Name", "name": "name", "placeholder": AGENT_SETTINGS.copy.name_placeholder.clone(), "value": path("/pane/fields/name")}, "style": {"flexGrow": 1, "minWidth": 0}});
    let mut row = ink.shell("identity", false, json!(true), vec![icon, color, name]);
    row["style"]["gap"] = json!(ink.r(0.5));
    row
}

/// The Device row: THE device picker (each machine by its glyph, offline
/// ones say so).
pub(super) fn device_row() -> Value {
    select_row("device", field_label("device"), "device", "device", json!({"placeholder": "Select device…", "popupMinWidth": 170}))
}

/// The Account row (THE account picker: brand mark + email), and the muted
/// "Last used" read when the machine reports no login.
pub(super) fn account_rows(ink: &Ink) -> [Value; 2] {
    let mut pick = select_row("account", field_label("account"), "account", "account", json!({"triggerIcon": true, "popupMinWidth": 170}));
    pick["visible"] = path("/pane/account/has");
    // No login reported (offline, another member's box): the launch spends
    // the machine's last used one, so the row says so, muted.
    [pick, value_row(ink, "account_none", path("/pane/account/none"), field_label("account"), json!(AGENT_SETTINGS.copy.account_last_used.clone()), json!(true))]
}

/// `row` drawn with the hairline the IDE's group rows draw above themselves
/// (`glass_group_rows`), shown with the row.
pub(super) fn divided(ink: &Ink, row: Value) -> Vec<Value> {
    let id = row["id"].as_str().unwrap_or("row").to_string();
    let shown = row.get("visible").cloned().unwrap_or(json!(true));
    vec![ink.divider(&format!("{id}_divider"), shown), row]
}

/// The Schedule rows: none = ONE row reading "Waits for messages" that opens
/// the editor on a new trigger; else the "Triggers" row carrying "+ Add",
/// then one row per trigger (summary opens it, the compact switch pauses it,
/// the owner's ⋯ — `/pane/owner` — edits or deletes it).
pub(super) fn schedule_rows(ink: &Ink, chrome: &Chrome) -> [Value; 3] {
    let label = field_label("triggers");
    let glyph = ink.r(1.);
    // `picker_trigger`: the value + chevron (the Medium ghost button).
    // The ghost Button's run: gpui snaps it half a pixel up.
    let mut none_value = faded(ink.text("schedule_new_value", json!(AGENT_SETTINGS.copy.no_schedule.clone()), SM, &ink.value), VALUE_ALPHA);
    none_value["style"]["position"] = json!("relative");
    none_value["style"]["top"] = json!(-0.5);
    let none_trigger = json!({"id": "schedule_new_trigger", "component": "Box", "props": {"pressable": true}, "on": {"press": {"event": {"name": "scheduleNew"}}},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.5), "cursor": "pointer"},
        "children": [
            none_value,
            {"id": "schedule_new_chevron", "component": "Icon", "props": {"name": "ui-chevron-right"}, "style": {"width": glyph, "height": glyph, "color": ink.value, "opacity": VALUE_ALPHA, "flexShrink": 0}},
        ]});
    let label_col = |id: &str| json!({"id": format!("{id}_text"), "component": "Box", "style": {"display": "flex", "flexDirection": "column", "flexShrink": 0}, "children": [ink.text(&format!("{id}_label"), json!(label), SM, &ink.foreground)]});
    let control = |id: &str, child: Value| json!({"id": format!("{id}_control"), "component": "Box", "style": {"display": "flex", "flexDirection": "row", "justifyContent": "flex-end", "flexGrow": 1, "minWidth": 200}, "children": [child]});
    let none = ink.shell("schedule_none", false, path("/pane/noTriggers"), vec![label_col("schedule_none"), control("schedule_none", none_trigger)]);
    // `+ Add`: gpui-component's outline `xsmall` Button with its glyph
    // (`h_5`, `px_1`, `gap_1`, a `size_3` glyph, `text_xs`; the input fill
    // and stroke).
    let size = ink.r(XS);
    let add = json!({"id": "trigger_add", "component": "Box", "visible": path("/pane/canAdd"), "props": {"pressable": true}, "accessibility": {"label": AGENT_SETTINGS.copy.add_trigger.clone()}, "on": {"press": {"event": {"name": "triggerAdd"}}},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "gap": ink.r(0.25), "height": ink.r(1.25), "paddingHorizontal": ink.r(0.25), "borderRadius": chrome.radius, "borderWidth": 1, "borderColor": chrome.outline, "backgroundColor": chrome.outline_fill, "cursor": "pointer", ":hover": {"backgroundColor": chrome.outline_hover}},
        "children": [
            {"id": "trigger_add_icon", "component": "Icon", "props": {"name": "ui-add"}, "style": {"width": ink.r(0.75), "height": ink.r(0.75), "color": chrome.outline_fg, "flexShrink": 0}},
            {"id": "trigger_add_text", "component": "Text", "props": {"text": AGENT_SETTINGS.copy.add_trigger.clone()}, "style": {"fontSize": size, "lineHeight": size * super::doc::LINE, "color": chrome.outline_fg, "flexShrink": 0}},
        ]});
    let header = ink.shell("schedule_head", false, path("/pane/hasTriggers"), vec![label_col("schedule_head"), control("schedule_head", add)]);
    // One trigger row.
    // `picker_value_label`: one line, at most 240 px.
    let mut summary = ink.text("trigger_summary", path("summary"), SM, &ink.foreground);
    summary["props"]["lines"] = json!(1);
    summary["style"]["minWidth"] = json!(0);
    summary["style"]["maxWidth"] = json!(240);
    // The ghost Button's run sits on its own line box, under the compact
    // switch's 16 px: a member's row (no ⋯) is the switch's height.
    summary["style"]["lineHeight"] = json!(16);
    // gpui snaps the Button's run half a pixel up inside its 32 px row.
    summary["style"]["position"] = json!("relative");
    summary["style"]["top"] = json!(-0.5);
    let summary = json!({"id": "trigger_open", "component": "Box", "props": {"pressable": true}, "on": {"press": {"event": {"name": "triggerOpen", "context": {"key": path("key")}}}},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "flexGrow": 1, "flexShrink": 1, "minWidth": 0, "cursor": "pointer"}, "children": [summary]});
    let switch = json!({"id": "trigger_switch", "component": "Switch", "props": {"label": "", "name": "enabled", "size": "compact", "checked": path("enabled"), "disabled": path("/pane/busy")},
        "on": {"change": {"event": {"name": "triggerToggle", "context": {"key": path("key")}}}}});
    // a11y: named by the caller ("Trigger menu for {sentence}", the node's accessibility label).
    let mut more = ink.ghost_icon_button("trigger_more", "ui-more", path("menuLabel"), Value::Null, &chrome.list_hover);
    more.as_object_mut().expect("the trigger").remove("on");
    let menu = json!({"id": "trigger_menu", "component": "Menu", "visible": path("/pane/owner"), "props": {"items": path("items"), "align": "start"}, "on": {"select": {"event": {"name": "triggerMenu", "context": {"key": path("key")}}}}, "children": [more]});
    let trailing = json!({"id": "trigger_trailing", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "flexShrink": 0, "gap": ink.r(0.25)}, "children": [switch, menu]});
    let row = ink.shell("trigger", false, json!(true), vec![summary, trailing]);
    let row = json!({"id": "trigger_wrap", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"}, "children": [ink.divider("trigger_divider", json!(true)), row]});
    let list = json!({"id": "triggers", "component": "Box", "visible": path("/pane/hasTriggers"), "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "template": {"component": "trigger_wrap", "path": "/pane/triggers", "key": "key"}, "children": [row]});
    [none, header, list]
}

/// The Instructions field: the chrome-less Textarea (`row`, 4 rows growing
/// to 16) the card's glass group holds.
pub(super) fn instructions_field(visible: Value) -> Value {
    json!({"id": "instructions", "component": "Textarea", "visible": visible,
        "props": {"label": field_label("instructions"), "name": "instructions", "variant": "row", "rows": 4, "autosize": true, "maxRows": 16, "placeholder": AGENT_SETTINGS.copy.instructions_placeholder.clone(), "value": path("/pane/fields/instructions")}})
}

/// The Instructions caption under the card.
pub(super) fn instructions_caption(ink: &Ink) -> Value {
    let mut caption = ink.text("instructions_caption", json!(field_label("instructions")), XS, &ink.muted);
    caption["style"]["paddingHorizontal"] = json!(ink.r(0.25));
    caption
}

/// `action_error::field_error(..).px_1()`: the inline message (INVALID
/// INPUT), shown while `shown`.
pub(super) fn field_error(ink: &Ink, chrome: &Chrome, id: &str, text: Value, shown: Value) -> Value {
    json!({"id": id, "component": "Text", "visible": shown, "props": {"text": text}, "style": {"fontSize": crate::action_error::FIELD_ERROR_PX, "lineHeight": 20, "color": chrome.danger_text, "paddingHorizontal": ink.r(0.25)}})
}
