//! #EXP-1770: THE device readiness block (`device_readiness`'s row model,
//! `packages/domain-contract/fixtures/device-doctor.json`) as ONE eUI
//! document, for every host: Settings → Tools and the first-run devices step
//! (`doctor_section`, THIS device), the device settings dialog, the Agent
//! composer's one row, and the styleguide.
//!
//! - [`block`] builds the tree once; everything it shows is bound to the
//!   state [`project`] writes (the loading skeleton, one band per group with
//!   its tag, the rows as a template ladder, or ONE row alone).
//! - [`ReadinessBlock`] mounts it on the native binding for the hosts that
//!   are gpui views; a document host (Settings → Tools) puts [`block`] in its
//!   own tree and routes [`EVENTS`] itself.
//!
//! A row: the state glyph in its tone, the label, the device-written detail
//! at the right edge in its tone, at most ONE action pill (the block's filled
//! one, or glass; its disabled face while the action runs). The Computer use row
//! is the switch row. A permission row is indented. The Computer use band's
//! host rows (the model and Work screen pickers) stay the host's own gpui
//! rows, as named `host_leaf` slots ([`TAIL_SLOTS`]).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use eui_gpui::{binding::Binding, host::HostPlugin, view::SurfaceView};
use gpui::{div, AnyElement, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, SharedString, Styled as _, Window};
use gpui_component::ActiveTheme as _;
use serde_json::{json, Value};

use coding::device_doctor::{DoctorAction, DoctorGroup};

use crate::device_readiness::{BlockProps, ReadinessRow, ReadinessSection, Tone};

use super::doc::{faded, hex, path, Ink, PillPaint, SM, XS};

/// The Computer use band's host rows (EXP-1236 / NEW-59), in order: what
/// `BlockProps::computer_use_tail` holds.
pub(crate) const TAIL_SLOTS: [&str; 2] = ["computer_use_model", "work_screen"];

/// The document's actions (a pill, the switch, the install row's Custom
/// path pill, its field and Save path, Check again).
pub(crate) const EVENTS: [&str; 6] = ["readinessAction", "readinessToggle", "readinessCustomPath", "readinessPathEdit", "readinessSavePath", "readinessRecheck"];

/// What a person did in the block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    Action { key: String, action: DoctorAction },
    Toggle(bool),
    CustomPath(String),
    PathEdit { key: String, value: String },
    SavePath(String),
    Recheck,
}

/// One of [`EVENTS`] as an [`Event`].
pub(crate) fn event(name: &str, context: &Value, payload: &Value) -> Option<Event> {
    let key = || context.get("key").and_then(Value::as_str).map(str::to_string);
    match name {
        "readinessAction" => Some(Event::Action { key: key()?, action: serde_json::from_value(context.get("action")?.clone()).ok()? }),
        "readinessToggle" => Some(Event::Toggle(payload.get("checked").and_then(Value::as_bool)?)),
        "readinessCustomPath" => Some(Event::CustomPath(key()?)),
        "readinessPathEdit" => Some(Event::PathEdit { key: key()?, value: payload.get("value").and_then(Value::as_str).unwrap_or_default().to_string() }),
        "readinessSavePath" => Some(Event::SavePath(key()?)),
        "readinessRecheck" => Some(Event::Recheck),
        _ => None,
    }
}

/// The colours the block needs beyond [`Ink`].
pub(crate) struct Paint {
    success: String,
    warning: String,
    danger: String,
    skeleton: String,
    fill_active: String,
    stroke_active: String,
}

impl Paint {
    pub(crate) fn of(cx: &App) -> Self {
        let theme = cx.theme();
        let chrome = crate::controls::ide_chrome();
        Self {
            success: hex(theme.success),
            warning: hex(theme.warning),
            danger: hex(theme.danger),
            skeleton: hex(theme.muted_foreground.opacity(0.18)),
            fill_active: hex(chrome.fill_active),
            stroke_active: hex(chrome.stroke_active),
        }
    }
}

fn group_key(group: DoctorGroup) -> &'static str {
    match group {
        DoctorGroup::Required => "required",
        DoctorGroup::Agents => "agents",
        DoctorGroup::ComputerUse => "computer_use",
        DoctorGroup::Voice => "voice",
        DoctorGroup::Storage => "storage",
    }
}

fn column() -> Value {
    json!({"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0})
}

/// The tones a glyph and a detail are drawn in.
const TONES: [&str; 4] = ["success", "warning", "muted", "destructive"];

fn tone_key(tone: Tone) -> &'static str {
    match tone {
        Tone::Success => "success",
        Tone::Warning => "warning",
        Tone::Muted => "muted",
        Tone::Destructive => "destructive",
    }
}

/// The glyph each tone's state draws (`device_readiness::state_glyph`:
/// one tone per glyph).
fn tone_icon(tone: &str) -> &'static str {
    match tone {
        "success" => "ui-check",
        "warning" => "ui-warning",
        "muted" => "ui-minus",
        _ => "ui-close",
    }
}

fn tone_color<'a>(ink: &'a Ink, paint: &'a Paint, tone: &str) -> &'a str {
    match tone {
        "success" => &paint.success,
        "warning" => &paint.warning,
        "muted" => &ink.muted,
        _ => &paint.danger,
    }
}

/// A `glass_pill_button` / `glass_pill_button_primary` at `PillSize::Sm`
/// whose label comes from `label`, shown where `on`; its disabled face
/// where `busy` (gpui-component's `loading` on a pill with no glyph draws
/// no spinner: the faded face is all it shows).
pub(super) fn pill(ink: &Ink, id: &str, paint: PillPaint, label: Value, on: Value, busy: Value, event: Value) -> [Value; 2] {
    let [mut live, mut dim] = ink.pill_button_with(id, paint, None, None, label, json!(false), None, event);
    live["visible"] = on;
    dim["visible"] = busy;
    for face in [&mut live, &mut dim] {
        // The label sits a pixel lower in gpui-component's button.
        if let Some(text) = face["children"].as_array_mut().and_then(|children| children.iter_mut().find(|child| child["component"] == "Text")) {
            text["style"]["paddingTop"] = json!(1);
        }
    }
    [live, dim]
}

/// ONE row of a ladder (a template instance): the switch row, the line,
/// the note under it, and (THIS device's install rows) the Custom path
/// field.
fn row(ink: &Ink, paint: &Paint, id: &str, custom_path: bool) -> Vec<Value> {
    // The fixture's switch row: label + switch, NO description; read-only
    // (no toggle) dims the label with the switch.
    let label = |id: String, visible: &str| {
        let mut text = ink.text(&id, path("label"), SM, &ink.foreground);
        text["visible"] = path(visible);
        text["style"]["flexGrow"] = json!(1);
        text["style"]["flexShrink"] = json!(1);
        text["style"]["minWidth"] = json!(0);
        text
    };
    let switch = json!({"id": format!("{id}_switch"), "component": "Switch", "props": {"label": "", "name": "computer_use", "checked": path("on"), "disabled": path("readOnly")},
        "on": {"change": {"event": {"name": "readinessToggle"}}}});
    let switch_row = json!({"id": format!("{id}_sw"), "component": "Box", "visible": path("switch"),
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "gap": ink.r(0.75), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.5)},
        "children": [label(format!("{id}_sw_label"), "toggle"), faded(label(format!("{id}_sw_label_dim"), "readOnly"), 0.5), switch]});

    // The line: glyph, label, the detail at the right edge, the pills.
    let mut children = vec![json!({"id": format!("{id}_indent"), "component": "Box", "visible": path("indent"), "style": {"width": 36. - ink.r(0.75) - ink.r(0.5), "flexShrink": 0}})];
    let glyph = ink.r(0.875);
    for tone in TONES {
        children.push(json!({"id": format!("{id}_glyph_{tone}"), "component": "Icon", "visible": path(&format!("glyph_{tone}")), "props": {"name": tone_icon(tone)},
            "style": {"width": glyph, "height": glyph, "color": tone_color(ink, paint, tone), "flexShrink": 0}}));
    }
    let mut name = ink.text(&format!("{id}_label"), path("label"), SM, &ink.foreground);
    name["style"]["flexShrink"] = json!(0);
    children.push(name);
    for tone in TONES {
        let mut detail = ink.text(&format!("{id}_detail_{tone}"), path("detail"), XS, tone_color(ink, paint, tone));
        detail["visible"] = path(&format!("detail_{tone}"));
        detail["props"]["lines"] = json!(1);
        detail["style"]["flexGrow"] = json!(1);
        detail["style"]["flexShrink"] = json!(1);
        detail["style"]["minWidth"] = json!(0);
        detail["style"]["textAlign"] = json!("right");
        children.push(detail);
    }
    if custom_path {
        // The install row's second pill: `glass_pill` in Select mode.
        let event = json!({"name": "readinessCustomPath", "context": {"key": path("key")}});
        let mut closed = ink.pill(&format!("{id}_custom"), json!(crate::device_readiness::CUSTOM_PATH), false, Some(event.clone()));
        closed["visible"] = path("customPath");
        closed["style"].as_object_mut().expect("the pill's style").remove(":hover");
        let mut open = ink.pill(&format!("{id}_custom_on"), json!(crate::device_readiness::CUSTOM_PATH), false, Some(event));
        open["visible"] = path("pathOpen");
        open["style"].as_object_mut().expect("the pill's style").remove(":hover");
        open["style"]["backgroundColor"] = json!(paint.fill_active);
        open["style"]["borderColor"] = json!(paint.stroke_active);
        open["children"][0]["style"]["opacity"] = json!(1.0);
        children.extend([closed, open]);
    }
    let event = json!({"name": "readinessAction", "context": {"key": path("key"), "action": path("action")}});
    children.extend(pill(ink, &format!("{id}_pill"), PillPaint::Glass, path("pillLabel"), path("glassOn"), path("glassBusy"), event.clone()));
    children.extend(pill(ink, &format!("{id}_primary"), PillPaint::Primary, path("pillLabel"), path("primaryOn"), path("primaryBusy"), event));
    let line = json!({"id": format!("{id}_line"), "component": "Box", "visible": path("line"),
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0, "gap": ink.r(0.5), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.5), "borderRadius": ink.radius_md},
        "children": children});

    // Under the row (`pl 36`, `px_3`, `pb_2`): a host's note (a finished
    // command's outcome).
    let under = |id: String, visible: &str, children: Vec<Value>| {
        json!({"id": id, "component": "Box", "visible": path(visible),
            "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0, "gap": ink.r(0.5), "paddingLeft": 36, "paddingRight": ink.r(0.75), "paddingBottom": ink.r(0.5)},
            "children": children})
    };
    let mut note = ink.text(&format!("{id}_note_text"), path("note"), XS, &ink.muted);
    note["style"]["flexGrow"] = json!(1);
    note["style"]["minWidth"] = json!(0);
    let mut out = vec![switch_row, line, under(format!("{id}_note"), "hasNote", vec![note])];
    if custom_path {
        // Open: the CLI path field and Save path.
        // `glass_input` at `web_input_sm`: the `box` field, `sm`.
        let input = json!({"id": format!("{id}_path_input"), "component": "Input", "props": {"variant": "box", "size": "sm", "label": "", "name": "path", "value": path("path"), "placeholder": path("placeholder")},
            "on": {"change": {"event": {"name": "readinessPathEdit", "context": {"key": path("key")}}}},
            "style": {"flexGrow": 1, "flexShrink": 1, "minWidth": 0}});
        let [save, _] = pill(ink, &format!("{id}_save"), PillPaint::Glass, json!(crate::device_readiness::SAVE_PATH), json!(true), json!(false), json!({"name": "readinessSavePath", "context": {"key": path("key")}}));
        out.push(under(format!("{id}_path"), "pathOpen", vec![input, save]));
    }
    out
}

/// The rows at `list` as a flat ladder.
fn ladder(ink: &Ink, paint: &Paint, id: &str, list: &str, custom_path: bool) -> Value {
    json!({"id": format!("{id}_rows"), "component": "List", "props": {"gap": "none"},
        "template": {"component": format!("{id}_item"), "path": list, "key": "key"},
        "children": [{"id": format!("{id}_item"), "component": "Box", "style": column(), "children": row(ink, paint, id, custom_path)}]})
}

/// The Computer use band's host rows: `host_leaf` slots named
/// `<id>_<slot>` ([`TAIL_SLOTS`]).
fn tail(id: &str, at: &str) -> Vec<Value> {
    TAIL_SLOTS
        .iter()
        .map(|slot| json!({"id": format!("{id}_{slot}"), "component": "Embed", "visible": path(&format!("{at}/tail/{slot}")), "props": {"surface": {}}, "style": {"width": "100%"}}))
        .collect()
}

/// THE block's tree, its state at `at` ([`project`]), its node ids under
/// `id`. `custom_path`: THIS device's install rows carry the Custom path
/// pill and field.
pub(crate) fn block(ink: &Ink, paint: &Paint, id: &str, at: &str, custom_path: bool) -> Value {
    let mut groups = Vec::new();
    for group in DoctorGroup::ALL {
        let g = group_key(group);
        let gid = format!("{id}_{g}");
        // The band's trailing tag (`optional`): plain muted text.
        let tag = group.tag().map(|tag| ink.text(&format!("{gid}_tag"), json!(tag), XS, &ink.muted));
        let mut children = vec![ink.band(&format!("{gid}_band"), group.label(), tag), ladder(ink, paint, &gid, &format!("{at}/groups/{g}"), custom_path)];
        if group == DoctorGroup::ComputerUse {
            children.extend(tail(&gid, at));
        }
        groups.push(json!({"id": gid, "component": "Box", "visible": path(&format!("{at}/shown/{g}")), "style": column(), "children": children}));
    }
    let mut sections = json!({"id": format!("{id}_sections"), "component": "Box", "visible": path(&format!("{at}/sectioned")), "style": column(), "children": groups});
    sections["style"]["gap"] = json!(ink.r(1.));
    let mut single = vec![ladder(ink, paint, &format!("{id}_single"), &format!("{at}/single"), custom_path)];
    single.extend(tail(&format!("{id}_single"), at));
    let single = json!({"id": format!("{id}_one"), "component": "Box", "visible": path(&format!("{at}/singled")), "style": column(), "children": single});
    json!({"id": id, "component": "Box", "style": column(), "children": [ink.skeleton_rows(&format!("{id}_skeleton"), &paint.skeleton, path(&format!("{at}/loading"))), sections, single]})
}

/// What the block shows.
pub(crate) enum Shape<'a> {
    /// No report yet: the skeleton rows.
    Loading,
    Sections(&'a [ReadinessSection]),
    /// ONE row alone (the composer's, the bare switch row).
    Single(&'a ReadinessRow),
}

/// An install row's Custom path field (THIS device).
#[derive(Clone, Debug, Default)]
pub(crate) struct PathField {
    pub open: bool,
    pub value: String,
    pub placeholder: String,
}

/// The host facts a projection reads beside the rows.
pub(crate) struct Facts<'a> {
    pub busy: &'a HashSet<String>,
    /// The switch is wired (else read-only).
    pub toggle: bool,
    pub notes: &'a HashMap<String, String>,
    pub paths: &'a HashMap<String, PathField>,
    /// How many [`TAIL_SLOTS`] the host fills.
    pub tail: usize,
}

impl<'a> Facts<'a> {
    pub(crate) fn of(props: &'a BlockProps, paths: &'a HashMap<String, PathField>) -> Self {
        Self { busy: &props.busy, toggle: props.on_toggle.is_some(), notes: &props.notes, paths, tail: props.computer_use_tail.len() }
    }
}

fn row_state(row: &ReadinessRow, facts: &Facts) -> Value {
    let busy = facts.busy.contains(&row.key);
    let (glass, primary) = match row.pill {
        Some(pill) if pill.primary => (false, true),
        Some(_) => (true, false),
        None => (false, false),
    };
    let field = facts.paths.get(&row.key);
    let note = facts.notes.get(&row.key);
    let mut state = json!({
        "key": row.key,
        "label": row.label,
        "switch": row.switch.is_some(),
        "line": row.switch.is_none(),
        "on": row.switch.unwrap_or(false),
        "toggle": facts.toggle,
        "readOnly": !facts.toggle,
        "indent": row.indent,
        "detail": row.detail.clone().unwrap_or_default(),
        "glassOn": glass && !busy,
        "glassBusy": glass && busy,
        "primaryOn": primary && !busy,
        "primaryBusy": primary && busy,
        "pillLabel": row.pill.map(|pill| pill.action.label()).unwrap_or_default(),
        "action": row.pill.map(|pill| serde_json::to_value(pill.action).unwrap_or_default()).unwrap_or_default(),
        "hasNote": note.is_some(),
        "note": note.cloned().unwrap_or_default(),
        "customPath": field.is_some_and(|field| !field.open),
        "pathOpen": field.is_some_and(|field| field.open),
        "path": field.map(|field| field.value.clone()).unwrap_or_default(),
        "placeholder": field.map(|field| field.placeholder.clone()).unwrap_or_default(),
    });
    let glyph = row.glyph.map(|(_, tone)| tone_key(tone));
    for tone in TONES {
        state[format!("glyph_{tone}")] = json!(glyph == Some(tone));
        state[format!("detail_{tone}")] = json!(row.switch.is_none() && tone_key(row.detail_tone) == tone);
    }
    state
}

/// The state [`block`] binds.
pub(crate) fn project(shape: Shape, facts: &Facts) -> Value {
    let mut groups = serde_json::Map::new();
    let mut shown = serde_json::Map::new();
    for group in DoctorGroup::ALL {
        groups.insert(group_key(group).into(), json!([]));
        shown.insert(group_key(group).into(), json!(false));
    }
    let mut single = Vec::new();
    let mut tail = false;
    match &shape {
        Shape::Loading => {}
        Shape::Sections(sections) => {
            for section in sections.iter() {
                let rows: Vec<Value> = section.rows.iter().map(|row| row_state(row, facts)).collect();
                shown.insert(group_key(section.group).into(), json!(!rows.is_empty()));
                groups.insert(group_key(section.group).into(), Value::Array(rows));
                tail |= section.group == DoctorGroup::ComputerUse;
            }
        }
        Shape::Single(row) => {
            single.push(row_state(row, facts));
            tail = true;
        }
    }
    let slots: serde_json::Map<String, Value> = TAIL_SLOTS.iter().enumerate().map(|(ix, slot)| (slot.to_string(), json!(tail && ix < facts.tail))).collect();
    json!({
        "loading": matches!(shape, Shape::Loading),
        "sectioned": matches!(shape, Shape::Sections(_)),
        "singled": matches!(shape, Shape::Single(_)),
        "groups": groups,
        "shown": shown,
        "single": single,
        "tail": slots,
    })
}

// ---------------------------------------------------------------------------
// The mount (the gpui hosts)
// ---------------------------------------------------------------------------

/// The block on the native binding, for a host that is a gpui view: the
/// host hands it the rows and its wiring every render ([`Self::show`]).
pub(crate) struct ReadinessBlock {
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    props: Rc<RefCell<BlockProps>>,
}

impl ReadinessBlock {
    pub(crate) fn new(surface_id: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let props: Rc<RefCell<BlockProps>> = Rc::default();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new("/block", Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(super::doc::IdeToasts));
            for name in EVENTS {
                let props = props.clone();
                binding.on_action(name, move |_, action, cx| {
                    let event = event(name, &action.context, action.payload.as_ref().unwrap_or(&Value::Null))?;
                    let props = props.clone();
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| dispatch(&props, event, window, cx));
                    });
                    None
                });
            }
            binding
        });
        let plugin: Rc<dyn HostPlugin> = Rc::new(TailPlugin { inner: super::doc::ide_plugin(), props: props.clone(), heights: RefCell::default() });
        let tree = block(&Ink::of(cx), &Paint::of(cx), "readiness", "/block", false);
        let view = super::doc::mount_with(surface_id, tree, &binding, plugin, false, window, cx);
        Self { binding, view, props }
    }

    /// The rows to show and the host's wiring.
    pub(crate) fn show(&mut self, shape: Shape, props: BlockProps, cx: &mut Context<Self>) {
        let state = project(shape, &Facts::of(&props, &HashMap::new()));
        *self.props.borrow_mut() = props;
        self.binding.update(cx, |binding, cx| binding.update_state(|current| *current = state, cx));
        cx.notify();
    }
}

impl Render for ReadinessBlock {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "ReadinessBlock").entered();
        div().w_full().min_w_0().child(self.view.clone())
    }
}

/// A host's [`ReadinessBlock`] for `id`, kept across renders (the styleguide's
/// functional entries).
pub(crate) fn keyed(id: &str, window: &mut Window, cx: &mut App) -> Entity<ReadinessBlock> {
    let surface = id.to_string();
    window.use_keyed_state(SharedString::from(id.to_string()), cx, move |window, cx| ReadinessBlock::new(&surface, window, cx))
}

fn dispatch(props: &Rc<RefCell<BlockProps>>, event: Event, window: &mut Window, cx: &mut App) {
    match event {
        Event::Action { key, action } => {
            let handler = props.borrow().on_action.clone();
            if let Some(handler) = handler {
                handler(&key, action, window, cx);
            }
        }
        Event::Toggle(on) => {
            let handler = props.borrow().on_toggle.clone();
            if let Some(handler) = handler {
                handler(on, window, cx);
            }
        }
        _ => {}
    }
}

/// The IDE's plugin plus the Computer use band's host rows.
struct TailPlugin {
    inner: crate::exponential_ui_host::IdePlugin,
    props: Rc<RefCell<BlockProps>>,
    heights: RefCell<HashMap<usize, f32>>,
}

impl TailPlugin {
    fn slot(component_id: &str) -> Option<usize> {
        TAIL_SLOTS.iter().position(|slot| component_id.ends_with(&format!("_{slot}")))
    }

    fn row(&self, slot: usize, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        let build = self.props.borrow().computer_use_tail.get(slot).cloned()?;
        Some(crate::surface::list_row(build(window, cx)).into_any_element())
    }
}

impl HostPlugin for TailPlugin {
    fn icon(&self, name: &str) -> Option<SharedString> {
        self.inner.icon(name)
    }

    fn on_unknown(&self, node: &eui_core::surface::PlacedNode) {
        self.inner.on_unknown(node)
    }

    fn host_leaf_height(&self, component_id: &str, _width: f32, _window: &mut Window, _cx: &mut App) -> Option<f32> {
        let slot = Self::slot(component_id)?;
        Some(self.heights.borrow().get(&slot).copied().unwrap_or(0.))
    }

    fn host_leaf_prepaint(&self, component_id: &str, width: f32, window: &mut Window, cx: &mut App) -> bool {
        let Some(slot) = Self::slot(component_id) else { return false };
        let Some(row) = self.row(slot, window, cx) else { return false };
        let height = crate::chat_tab_transcript::measure(row, width, window, cx);
        let old = self.heights.borrow_mut().insert(slot, height);
        old.is_none_or(|old| (old - height).abs() > 0.01)
    }

    // drop with EXP-1771: the device settings' Computer use model and Work
    // screen pickers are the IDE's own `Select` rows until they move.
    fn host_leaf(&self, component_id: &str, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        self.row(Self::slot(component_id)?, window, cx)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn test_ink() -> Ink {
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

    pub(crate) fn test_paint() -> Paint {
        Paint { success: "#22c55eff".into(), warning: "#f59e0bff".into(), danger: "#ef4444ff".into(), skeleton: "#a1a1aa2e".into(), fill_active: "#ffffff1a".into(), stroke_active: "#ffffff33".into() }
    }

    /// #EXP-1629: the document raises no issue a debug build's mount check
    /// refuses (#EXP-1767's class), with and without the Custom path rows.
    #[test]
    fn the_document_raises_no_unexpected_issue() {
        for custom_path in [false, true] {
            let tree = block(&test_ink(), &test_paint(), "readiness", "/block", custom_path);
            let bound = super::super::doc::bound_values(&tree);
            let mut surface = eui_core::surface::Surface::new("readiness", Default::default());
            let outcome = surface.set_nested(serde_json::from_value(tree).expect("parses"));
            let issues = super::super::doc::unexpected_issues(&bound, &outcome.issues);
            assert!(issues.is_empty(), "{issues:?}");
        }
    }

    /// Every action round-trips through the document's event.
    #[test]
    fn a_pill_press_names_its_row_and_action() {
        for action in [DoctorAction::Install, DoctorAction::Update, DoctorAction::SignIn, DoctorAction::Grant] {
            let context = json!({"key": "claude", "action": serde_json::to_value(action).unwrap()});
            assert_eq!(event("readinessAction", &context, &Value::Null), Some(Event::Action { key: "claude".into(), action }));
        }
        assert_eq!(event("readinessToggle", &Value::Null, &json!({"checked": true})), Some(Event::Toggle(true)));
    }

    /// The projection: one band per group with rows, the primary pill
    /// filled, a busy row's spinner, the tail only under Computer use.
    #[test]
    fn the_projection_follows_the_row_model() {
        let fixture: Value = serde_json::from_str(include_str!("../../../../../../packages/domain-contract/fixtures/device-doctor.json")).unwrap();
        let case = fixture["cases"].as_array().unwrap().iter().find(|case| case["name"].as_str().is_some_and(|name| name.starts_with("claude too old"))).unwrap();
        let doctor = crate::device_readiness::parse(Some(&case["doctor"])).unwrap();
        let sections = crate::device_readiness::sections(&doctor, true, None);
        let busy: HashSet<String> = ["claude".to_string()].into();
        let (notes, paths) = (HashMap::new(), HashMap::new());
        let facts = Facts { busy: &busy, toggle: true, notes: &notes, paths: &paths, tail: 2 };
        let state = project(Shape::Sections(&sections), &facts);
        assert_eq!(state["shown"]["required"], json!(true));
        assert_eq!(state["shown"]["voice"], json!(false));
        let claude = state["groups"]["agents"].as_array().unwrap().iter().find(|row| row["key"] == "claude").unwrap();
        assert_eq!((claude["primaryBusy"].clone(), claude["primaryOn"].clone(), claude["pillLabel"].clone()), (json!(true), json!(false), json!("Update")));
        let switch = &state["groups"]["computer_use"][0];
        assert_eq!((switch["switch"].clone(), switch["on"].clone()), (json!(true), json!(true)));
        assert_eq!(state["tail"]["work_screen"], json!(true));
        let loading = project(Shape::Loading, &facts);
        assert_eq!((loading["loading"].clone(), loading["tail"]["work_screen"].clone()), (json!(true), json!(false)));
    }
}
