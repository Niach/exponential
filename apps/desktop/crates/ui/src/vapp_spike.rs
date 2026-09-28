//! VAPP-4 spike (throwaway): the shared kitchen-sink fixture painted by gpui
//! from the Rust core's taffy frames.
//!
//! gpui's `Styled` cannot express grid areas or mixed track lists, so this
//! screen does NOT map the fixture's styles onto gpui. The core
//! (`crates/vapp-spike`) resolves the styles, lays the tree out with taffy
//! and a gpui text-system `Measure`, and hands back ABSOLUTE frames in
//! pre-order; every node is painted as an absolutely positioned div at its
//! frame (nested like the tree, so clipping and opacity inherit), leaves as
//! the app's own primitives. Opened with `EXP_DEV_SCREEN=kitchen-sink`
//! (`EXP_DEV_VAPP_BENCH=<n>` swaps in the synthetic n-node bench tree).

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use gpui::{
    canvas, div, prelude::FluentBuilder as _, px, AnyElement, App, AppContext as _, Context,
    Entity, Font,
    FontWeight, Hsla, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _,
    Render, Role, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, TextRun, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    input::{InputEvent, InputState, Textarea, TextareaState},
    select::Select,
    ActiveTheme as _, Icon, Sizable as _,
};
use serde_json::{Map, Value};
use theme::tokens as t;
use vapp_spike::taffy::Size as TSize;
use vapp_spike::{AvailableSpace, Kind, LayoutResult, Measure, MeasureRequest, Surface, TextStyle};

use crate::controls::WebControl as _;
use crate::surface::{BadgeTone, PillMode, PillSize};

/// The surface width until the first paint reports the pane's real width.
const DEFAULT_WIDTH: f32 = 900.;
/// The fake host round trip of the typing test (LANES.md "typing test").
pub(crate) const ECHO_DELAY: Duration = Duration::from_millis(150);

/// Contract paddings (LANES.md fixed-measure table) added to REAL text widths.
const BUTTON_PAD_X: f32 = 12.;
const PILL_PAD_X: f32 = 10.;
const LISTROW_PAD_X: f32 = 8.;
const LISTROW_GAP: f32 = 8.;
const TOGGLE_GAP: f32 = 8.;
const SWITCH_W: f32 = 36.;
const LIVE_DOT_GAP: f32 = 4.;
/// Gap the markdown estimate adds between rendered blocks (paragraphs and
/// list items), eyeballed off MarkdownView's document rhythm.
const MARKDOWN_BLOCK_GAP: f32 = 8.;

/// The host-owned echo bookkeeping (LANES.md "typing test"): every local edit
/// bumps the revision; an echo applies only when it carries the latest one.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct EchoState {
    pub latest: u64,
    pub applied: u64,
    pub dropped: u64,
}

impl EchoState {
    /// A local edit: returns the revision its echo will carry.
    pub fn edit(&mut self) -> u64 {
        self.latest += 1;
        self.latest
    }

    /// An echo `{rev, value}` arrived: `true` = apply it, `false` = stale.
    pub fn arrive(&mut self, rev: u64) -> bool {
        if rev == self.latest {
            self.applied += 1;
            true
        } else {
            self.dropped += 1;
            false
        }
    }
}

/// One node's view model, parsed ONCE from the surface.
struct NodeVm {
    id: SharedString,
    kind: Kind,
    props: Map<String, Value>,
    children: Vec<usize>,
    /// The accessibility label (text/label/title/placeholder/alt).
    label: Option<SharedString>,
}

impl NodeVm {
    fn str_prop(&self, key: &str) -> &str {
        self.props.get(key).and_then(Value::as_str).unwrap_or("")
    }

    fn bool_prop(&self, key: &str) -> bool {
        self.props.get(key).and_then(Value::as_bool).unwrap_or(false)
    }
}

fn a11y_label(kind: Kind, props: &Map<String, Value>) -> Option<SharedString> {
    let get = |k: &str| props.get(k).and_then(Value::as_str).filter(|s| !s.is_empty());
    let label = match kind {
        Kind::Box | Kind::Card | Kind::Divider => None,
        Kind::Badge => props.get("count").map(|c| format!("{c} new")),
        Kind::Progress => props
            .get("value")
            .and_then(Value::as_f64)
            .map(|v| format!("{}%", (v * 100.).round())),
        Kind::Avatar => get("name").map(str::to_string),
        Kind::Image => get("alt").map(str::to_string),
        Kind::Select => get("value").map(str::to_string),
        Kind::ListRow => get("title").map(|title| match get("meta") {
            Some(meta) => format!("{title}, {meta}"),
            None => title.to_string(),
        }),
        _ => get("text")
            .or_else(|| get("label"))
            .or_else(|| get("title"))
            .or_else(|| get("placeholder"))
            .map(str::to_string),
    };
    label.map(SharedString::from)
}

pub struct VappKitchenSink {
    surface: Surface,
    nodes: Vec<NodeVm>,
    pressed: HashSet<String>,
    width: f32,
    last: Option<LayoutResult>,
    inputs: HashMap<String, Entity<InputState>>,
    textareas: HashMap<String, Entity<TextareaState>>,
    selects: HashMap<String, crate::coding_selects::ChoiceSelect>,
    toggles: HashMap<String, bool>,
    bench_caption: String,
    pub(crate) echo: EchoState,
    #[cfg_attr(not(test), allow(dead_code))]
    echo_field: Option<Entity<InputState>>,
    scroll: ScrollHandle,
    renders: u64,
    _subscriptions: Vec<Subscription>,
}

impl VappKitchenSink {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let bench = std::env::var("EXP_DEV_VAPP_BENCH")
            .ok()
            .and_then(|n| n.trim().parse::<usize>().ok())
            .filter(|n| *n > 0);
        let json = match bench {
            Some(n) => vapp_spike::bench::bench_tree(n),
            None => vapp_spike::KITCHEN_SINK_JSON.to_string(),
        };
        let surface = Surface::new(&json).expect("vapp fixture parses");
        eprintln!(
            "[vapp desktop] surface built: {} nodes in {} µs",
            surface.node_count(),
            surface.build_ns / 1_000
        );
        let nodes: Vec<NodeVm> = surface
            .nodes()
            .iter()
            .map(|n| NodeVm {
                id: SharedString::from(n.id.clone()),
                kind: n.kind,
                label: a11y_label(n.kind, &n.props),
                props: n.props.clone(),
                children: n.children.iter().map(|c| *c as usize).collect(),
            })
            .collect();

        let mut inputs = HashMap::new();
        let mut textareas = HashMap::new();
        let mut selects = HashMap::new();
        let mut toggles = HashMap::new();
        let mut subscriptions = Vec::new();
        let mut echo_field = None;
        for node in &nodes {
            match node.kind {
                Kind::TextField => {
                    let placeholder = node.str_prop("placeholder").to_string();
                    let state =
                        cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
                    if node.bool_prop("echo") {
                        subscriptions.push(cx.subscribe_in(
                            &state,
                            window,
                            |this, state, event: &InputEvent, window, cx| {
                                if let InputEvent::Change = event {
                                    this.local_edit(state.clone(), window, cx);
                                }
                            },
                        ));
                        echo_field = Some(state.clone());
                    }
                    inputs.insert(node.id.to_string(), state);
                }
                Kind::Textarea => {
                    let placeholder = node.str_prop("placeholder").to_string();
                    let state = cx.new(|cx| {
                        crate::controls::web_textarea(3, 12, window, cx).placeholder(placeholder)
                    });
                    textareas.insert(node.id.to_string(), state);
                }
                Kind::Select => {
                    let options: Vec<String> = node
                        .props
                        .get("options")
                        .and_then(Value::as_array)
                        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                        .unwrap_or_default();
                    let choices: Vec<(&str, &str)> =
                        options.iter().map(|o| (o.as_str(), o.as_str())).collect();
                    let state = crate::coding_selects::choice_select(
                        &choices,
                        node.str_prop("value"),
                        window,
                        cx,
                    );
                    selects.insert(node.id.to_string(), state);
                }
                Kind::Toggle => {
                    toggles.insert(node.id.to_string(), node.bool_prop("checked"));
                }
                _ => {}
            }
        }

        Self {
            surface,
            nodes,
            pressed: HashSet::new(),
            width: DEFAULT_WIDTH,
            last: None,
            inputs,
            textareas,
            selects,
            toggles,
            bench_caption: String::new(),
            echo: EchoState::default(),
            echo_field,
            scroll: ScrollHandle::new(),
            renders: 0,
            _subscriptions: subscriptions,
        }
    }

    /// The echo field's InputState (tests type into it).
    #[cfg(test)]
    pub(crate) fn echo_field(&self) -> Option<Entity<InputState>> {
        self.echo_field.clone()
    }

    /// A local edit of the host-owned field: bump the revision and schedule
    /// the fake host echo `{rev, value}` [`ECHO_DELAY`] later.
    fn local_edit(&mut self, state: Entity<InputState>, window: &mut Window, cx: &mut Context<Self>) {
        let rev = self.echo.edit();
        let value = state.read(cx).value().to_string();
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(ECHO_DELAY).await;
            let _ = this.update_in(cx, |this, window, cx| {
                if !this.echo.arrive(rev) {
                    return;
                }
                // The echo matches what the field shows unless something
                // rewrote it; `set_value` resets the caret, so only on drift.
                if state.read(cx).value().as_ref() != value.as_str() {
                    state.update(cx, |state, cx| state.set_value(value.clone(), window, cx));
                }
            });
        })
        .detach();
    }

    fn set_width(&mut self, width: f32, cx: &mut Context<Self>) {
        if width > 0. && (width - self.width).abs() > 0.5 {
            self.width = width;
            cx.notify();
        }
    }

    fn press(&mut self, id: &str, down: bool, cx: &mut Context<Self>) {
        let changed = if down {
            self.pressed.insert(id.to_string())
        } else {
            self.pressed.remove(id)
        };
        if changed {
            cx.notify();
        }
    }

    fn layout(&mut self, window: &Window, cx: &App) -> LayoutResult {
        self.surface.set_viewport(self.width, 0.);
        let pressed: Vec<String> = self.pressed.iter().cloned().collect();
        self.surface.set_pressed(&pressed);
        let mut measure = GpuiMeasure {
            window,
            family: cx.theme().font_family.clone(),
        };
        let started = Instant::now();
        let result = self.surface.layout(&mut measure);
        let wall = started.elapsed();
        self.renders += 1;
        self.bench_caption = format!(
            "{} nodes · {} measure calls · taffy {} µs · wall {:.2} ms",
            result.nodes.len(),
            result.measure_calls,
            result.layout_ns / 1_000,
            wall.as_secs_f64() * 1_000.
        );
        eprintln!(
            "[vapp desktop] render {} width {:.0} pressed {:?}: {} nodes · {} measure calls · taffy {} µs · wall {} µs · echo {:?}",
            self.renders,
            self.width,
            pressed,
            result.nodes.len(),
            result.measure_calls,
            result.layout_ns / 1_000,
            wall.as_micros(),
            self.echo,
        );
        result
    }

    fn build(&self, ix: usize, result: &LayoutResult, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let node = &self.nodes[ix];
        let placed = &result.nodes[ix];
        let frame = placed.frame;
        let (ox, oy) = match ix {
            0 => (0., 0.),
            _ => self
                .parent_of(ix)
                .map(|p| (result.nodes[p].frame.x, result.nodes[p].frame.y))
                .unwrap_or((0., 0.)),
        };
        let visual = &placed.visual;
        let mut el = match node.kind {
            Kind::Card => crate::surface::glass_card().id(node.id.clone()),
            _ => div().id(node.id.clone()),
        }
        .absolute()
        .left(px(frame.x - ox))
        .top(px(frame.y - oy))
        .w(px(frame.w))
        .h(px(frame.h));
        if let Some(bg) = visual.background_color.as_deref().and_then(color) {
            el = el.bg(bg);
        }
        if let Some(width) = visual.border_width.filter(|w| *w > 0.) {
            el = el.border(px(width)).border_color(
                visual
                    .border_color
                    .as_deref()
                    .and_then(color)
                    .unwrap_or(t::BORDER.to_hsla()),
            );
        }
        if let Some(radius) = visual.border_radius {
            el = el.rounded(px(radius));
        }
        if let Some(opacity) = visual.opacity {
            el = el.opacity(opacity);
        }
        if visual.overflow_hidden {
            el = el.overflow_hidden();
        }
        if node.kind == Kind::Card {
            el = el.role(Role::Group);
        }
        if let Some(label) = node.label.clone() {
            el = el.aria_label(label);
        }
        if !node.kind.is_container() {
            el = self.leaf(el, node, placed.text_style, visual, frame.w, frame.h, window, cx);
        }
        for child in &node.children {
            el = el.child(self.build(*child, result, window, cx));
        }
        el.into_any_element()
    }

    fn parent_of(&self, ix: usize) -> Option<usize> {
        self.surface.nodes()[ix].parent.map(|p| p as usize)
    }

    #[allow(clippy::too_many_arguments)]
    fn leaf(
        &self,
        el: gpui::Stateful<gpui::Div>,
        node: &NodeVm,
        ts: TextStyle,
        visual: &vapp_spike::Visual,
        w: f32,
        h: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = cx.theme();
        let foreground = theme.foreground;
        let muted = theme.muted_foreground;
        let id = node.id.clone();
        let typed = |d: gpui::Div| {
            d.text_size(px(ts.font_size))
                .line_height(px(ts.line_height))
                .font_weight(weight(ts.font_weight))
        };
        match node.kind {
            Kind::Text => {
                let variant = node.str_prop("variant");
                let ink = visual.color.as_deref().and_then(color).unwrap_or(match variant {
                    "muted" | "label" => muted,
                    _ => foreground,
                });
                let text = typed(div())
                    .size_full()
                    .text_color(ink)
                    .when(visual.text_align.as_deref() == Some("center"), |d| d.text_center())
                    .when(visual.text_align.as_deref() == Some("right"), |d| d.text_right())
                    .child(SharedString::from(node.str_prop("text").to_string()));
                el.role(Role::Label).child(text)
            }
            Kind::Button => {
                let label = node.str_prop("label").to_string();
                let mut button = Button::new(SharedString::from(format!("{id}-button")))
                    .label(label)
                    .web_md()
                    .px(px(BUTTON_PAD_X))
                    .text_size(px(ts.font_size))
                    .w(px(w))
                    .h(px(h));
                button = match node.str_prop("variant") {
                    "outline" => button.outline(),
                    "ghost" => button.ghost(),
                    _ => button.primary(),
                };
                let (down, up, out) = (id.to_string(), id.to_string(), id.to_string());
                el.role(Role::Button)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| this.press(&down, true, cx)),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| this.press(&up, false, cx)),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| this.press(&out, false, cx)),
                    )
                    .child(button)
            }
            Kind::TextField => match self.inputs.get(id.as_ref()) {
                Some(state) => el.role(Role::TextInput).child(
                    crate::controls::glass_input(state, window, cx)
                        .w(px(w))
                        .h(px(h)),
                ),
                None => el,
            },
            Kind::Textarea => match self.textareas.get(id.as_ref()) {
                Some(state) => el
                    .role(Role::MultilineTextInput)
                    .child(Textarea::new(state).w(px(w)).h(px(h))),
                None => el,
            },
            Kind::Select => match self.selects.get(id.as_ref()) {
                Some(state) => el
                    .role(Role::ComboBox)
                    .child(Select::new(state).with_size(gpui_component::Size::Medium).w(px(w)).h(px(h))),
                None => el,
            },
            Kind::Toggle => {
                let checked = self.toggles.get(id.as_ref()).copied().unwrap_or(false);
                let key = id.to_string();
                el.role(Role::Switch).child(
                    typed(div())
                        .size_full()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(TOGGLE_GAP))
                        .text_color(foreground)
                        .child(SharedString::from(node.str_prop("label").to_string()))
                        .child(
                            crate::controls::web_switch(SharedString::from(format!("{id}-switch")))
                                .checked(checked)
                                .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                                    this.toggles.insert(key.clone(), *checked);
                                    cx.notify();
                                })),
                        ),
                )
            }
            Kind::ListRow => el.role(Role::ListItem).child(
                typed(crate::surface::flat_row())
                    .id(SharedString::from(format!("{id}-row")))
                    .size_full()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap(px(LISTROW_GAP))
                    .px(px(LISTROW_PAD_X))
                    .cursor_pointer()
                    .hover(|s| s.bg(t::glass::FILL_ACTIVE.to_hsla()))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_color(foreground)
                            .child(SharedString::from(node.str_prop("title").to_string())),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(muted)
                            .child(SharedString::from(node.str_prop("meta").to_string())),
                    ),
            ),
            Kind::Badge => {
                let count = node.props.get("count").and_then(Value::as_u64).unwrap_or(0) as usize;
                el.role(Role::Status).child(
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .children(crate::surface::count_badge(count, BadgeTone::Primary, cx)),
                )
            }
            Kind::Pill => {
                let live = node.str_prop("tone") == "live";
                let mode = if live {
                    PillMode::Readonly
                } else {
                    PillMode::Select {
                        selected: node.bool_prop("selected"),
                    }
                };
                let pill = crate::surface::glass_pill(
                    SharedString::from(format!("{id}-pill")),
                    PillSize::Sm,
                    mode,
                    cx,
                )
                .w(px(w))
                .h(px(h))
                .px(px(PILL_PAD_X))
                .gap(px(LIVE_DOT_GAP))
                .justify_center()
                .text_size(px(ts.font_size))
                .line_height(px(ts.line_height))
                .font_weight(weight(ts.font_weight))
                .when(live, |pill| {
                    pill.bg(t::BACKGROUND.to_hsla().opacity(0.72))
                        .text_color(foreground)
                        .child(crate::surface::live_dot(t::GREEN.to_hsla(), false))
                })
                .child(SharedString::from(node.str_prop("label").to_string()));
                el.role(Role::Label).child(pill)
            }
            Kind::Avatar => {
                let name = node.str_prop("name").to_string();
                let size = node.props.get("size").and_then(Value::as_f64).unwrap_or(32.) as f32;
                el.role(Role::Image).child(crate::user_avatar::avatar_element(
                    &name,
                    &name,
                    None,
                    gpui_component::Size::Size(px(size)),
                ))
            }
            Kind::Image => {
                let tint = color(node.str_prop("placeholder")).unwrap_or(t::MUTED.to_hsla());
                el.role(Role::Image).child(
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(tint)
                        .child(
                            Icon::from(crate::icons::registry::EDITOR_IMAGE)
                                .with_size(gpui_component::Size::Large)
                                .text_color(t::FOREGROUND.to_hsla().opacity(0.7)),
                        ),
                )
            }
            Kind::Divider => el.child(div().size_full().bg(t::BORDER.to_hsla())),
            Kind::Progress => {
                let value = node
                    .props
                    .get("value")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.)
                    .clamp(0., 1.) as f32;
                el.role(Role::ProgressIndicator).child(
                    div()
                        .size_full()
                        .rounded_full()
                        .overflow_hidden()
                        .bg(t::glass::FILL_ACTIVE.to_hsla())
                        .child(div().h_full().w(gpui::relative(value)).rounded_full().bg(theme.primary)),
                )
            }
            Kind::Markdown => el.role(Role::Article).child(
                div().w(px(w)).child(crate::markdown::MarkdownView::new(
                    SharedString::from(format!("{id}-md")),
                    node.str_prop("text").to_string(),
                )),
            ),
            Kind::Box | Kind::Card => el,
        }
    }
}

impl Render for VappKitchenSink {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let result = self.layout(window, cx);
        let surface = if result.nodes.is_empty() {
            div().into_any_element()
        } else {
            self.build(0, &result, window, cx)
        };
        let height = result.surface_height;
        self.last = Some(result);
        let this = cx.entity().downgrade();
        let muted = cx.theme().muted_foreground;
        gpui_component::v_flex()
            .id("vapp-kitchen-sink")
            .size_full()
            .child(
                gpui_component::h_flex()
                    .flex_shrink_0()
                    .justify_between()
                    .px_4()
                    .py_1()
                    .text_xs()
                    .text_color(muted)
                    .child("vApp kitchen sink")
                    .child(SharedString::from(self.bench_caption.clone())),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    // One frame of lag: the pane reports its width at
                    // prepaint and the next render lays out at it.
                    .child(
                        canvas(
                            move |bounds, _, cx| {
                                let width = bounds.size.width / px(1.);
                                if let Some(this) = this.upgrade() {
                                    this.update(cx, |this, cx| this.set_width(width, cx));
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    )
                    .child(crate::scroll_pane::v_scroll_pane(
                        "vapp-kitchen-sink-scroll",
                        &self.scroll,
                        div().relative().w_full().h(px(height)).child(surface),
                    )),
            )
    }
}

/// `#rrggbb` or `$palette.x` / `$semantic.x` into the design tokens.
fn color(spec: &str) -> Option<Hsla> {
    if let Some(hex) = spec.strip_prefix('#') {
        return (hex.len() == 6)
            .then(|| u32::from_str_radix(hex, 16).ok())
            .flatten()
            .map(|v| gpui::rgb(v).into());
    }
    let (group, name) = spec.strip_prefix('$')?.split_once('.')?;
    let token = match (group, name) {
        ("palette", "background") => t::BACKGROUND,
        ("palette", "foreground") => t::FOREGROUND,
        ("palette", "card") => t::CARD,
        ("palette", "cardForeground") => t::CARD_FOREGROUND,
        ("palette", "popover") => t::POPOVER,
        ("palette", "primary") => t::PRIMARY,
        ("palette", "primaryForeground") => t::PRIMARY_FOREGROUND,
        ("palette", "secondary") => t::SECONDARY,
        ("palette", "muted") => t::MUTED,
        ("palette", "mutedForeground") => t::MUTED_FOREGROUND,
        ("palette", "accent") => t::ACCENT,
        ("palette", "destructive") => t::DESTRUCTIVE,
        ("palette", "border") => t::BORDER,
        ("palette", "input") => t::INPUT,
        ("palette", "ring") => t::RING,
        ("semantic", "neutral") => t::NEUTRAL,
        ("semantic", "yellow") => t::YELLOW,
        ("semantic", "green") => t::GREEN,
        ("semantic", "red") => t::RED,
        ("semantic", "orange") => t::ORANGE,
        ("semantic", "blue") => t::BLUE,
        _ => return None,
    };
    Some(token.to_hsla())
}

fn weight(w: u16) -> FontWeight {
    match w {
        0..=449 => FontWeight::NORMAL,
        450..=549 => FontWeight::MEDIUM,
        550..=649 => FontWeight::SEMIBOLD,
        _ => FontWeight::BOLD,
    }
}

/// The host measure: gpui's text system for every text width/height, the
/// LANES.md control sizes around REAL label widths.
struct GpuiMeasure<'a> {
    window: &'a Window,
    family: SharedString,
}

impl GpuiMeasure<'_> {
    fn run(&self, text: &str, ts: TextStyle) -> TextRun {
        TextRun {
            len: text.len(),
            font: Font {
                weight: weight(ts.font_weight),
                ..gpui::font(self.family.clone())
            },
            color: gpui::black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }
    }

    /// Max-content width of ONE line (no newlines).
    fn line_width(&self, text: &str, ts: TextStyle) -> f32 {
        if text.is_empty() {
            return 0.;
        }
        let run = self.run(text, ts);
        let line = self.window.text_system().shape_line(
            SharedString::from(text.to_string()),
            px(ts.font_size),
            &[run],
            None,
        );
        (line.width / px(1.)).ceil()
    }

    fn max_content(&self, text: &str, ts: TextStyle) -> f32 {
        text.lines().map(|l| self.line_width(l, ts)).fold(0., f32::max)
    }

    /// Min-content: the widest single word.
    fn min_content(&self, text: &str, ts: TextStyle) -> f32 {
        text.split_whitespace().map(|w| self.line_width(w, ts)).fold(0., f32::max)
    }

    /// Height of `text` wrapped at `wrap`: the sum of every wrapped line.
    fn wrapped_height(&self, text: &str, ts: TextStyle, wrap: f32) -> f32 {
        if text.is_empty() {
            return ts.line_height;
        }
        let run = self.run(text, ts);
        match self.window.text_system().shape_text(
            SharedString::from(text.to_string()),
            px(ts.font_size),
            &[run],
            Some(px(wrap)),
            None,
        ) {
            Ok(lines) => lines
                .iter()
                .map(|line| line.size(px(ts.line_height)).height / px(1.))
                .sum(),
            Err(_) => ts.line_height,
        }
    }

    /// CSS-like fit-content: min(max-content, max(min-content, available)).
    fn text_width(&self, req: &MeasureRequest, text: &str, ts: TextStyle) -> f32 {
        if let Some(known) = req.known_width {
            return known;
        }
        let max = self.max_content(text, ts);
        match req.available_width {
            AvailableSpace::MaxContent => max,
            AvailableSpace::MinContent => self.min_content(text, ts),
            AvailableSpace::Definite(avail) => max.min(avail.max(self.min_content(text, ts))),
        }
    }

    fn text(&self, req: &MeasureRequest, text: &str, ts: TextStyle) -> (f32, f32) {
        let w = self.text_width(req, text, ts);
        (w, self.wrapped_height(text, ts, w))
    }

    fn markdown(&self, req: &MeasureRequest, source: &str) -> (f32, f32) {
        let ts = TextStyle {
            font_size: 14.,
            font_weight: 400,
            line_height: 20.,
        };
        let plain: Vec<String> = source.lines().map(plain_markdown_line).collect();
        let joined = plain.join("\n");
        let w = self.text_width(req, &joined, ts);
        // MarkdownView renders every paragraph AND every list item as its
        // own block with a gap between blocks; blank source lines only
        // separate them.
        let blocks: Vec<&String> = plain.iter().filter(|l| !l.trim().is_empty()).collect();
        let mut h = 0.;
        for (i, line) in blocks.iter().enumerate() {
            if i > 0 {
                h += MARKDOWN_BLOCK_GAP;
            }
            h += self.wrapped_height(line, ts, w);
        }
        (w, h.max(ts.line_height))
    }
}

/// Strip the GFM markers the measure does not shape (emphasis, list bullets).
fn plain_markdown_line(line: &str) -> String {
    let trimmed = line.trim_start();
    let body = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .map(|rest| format!("•  {rest}"))
        .unwrap_or_else(|| trimmed.to_string());
    body.replace("**", "").replace('`', "")
}

impl Measure for GpuiMeasure<'_> {
    fn measure(&mut self, req: &MeasureRequest) -> TSize<f32> {
        let ts = req.text_style;
        let str_prop = |k: &str| req.props.get(k).and_then(Value::as_str).unwrap_or("");
        let (w, h) = match req.kind {
            Kind::Text => self.text(req, str_prop("text"), ts),
            Kind::Markdown => self.markdown(req, str_prop("text")),
            Kind::Button => (self.line_width(str_prop("label"), ts) + 2. * BUTTON_PAD_X, 36.),
            Kind::Pill => {
                let live = str_prop("tone") == "live";
                let dot = if live {
                    crate::surface::LIVE_DOT_PX + LIVE_DOT_GAP
                } else {
                    0.
                };
                (self.line_width(str_prop("label"), ts) + 2. * PILL_PAD_X + dot + 2., 28.)
            }
            Kind::ListRow => (
                self.line_width(str_prop("title"), ts)
                    + self.line_width(str_prop("meta"), ts)
                    + 2. * LISTROW_PAD_X
                    + LISTROW_GAP,
                32.,
            ),
            Kind::Toggle => (self.line_width(str_prop("label"), ts) + TOGGLE_GAP + SWITCH_W, 24.),
            Kind::TextField | Kind::Select => (160., 36.),
            Kind::Textarea => (160., 72.),
            Kind::Badge => (20., 20.),
            Kind::Avatar => {
                let s = req.props.get("size").and_then(Value::as_f64).unwrap_or(32.) as f32;
                (s, s)
            }
            Kind::Image => (320., 180.),
            Kind::Divider => (0., 1.),
            Kind::Progress => (160., 8.),
            Kind::Box | Kind::Card => (0., 0.),
        };
        TSize {
            width: req.known_width.unwrap_or(w),
            height: req.known_height.unwrap_or(h),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vapp_echo_applies_only_the_latest_revision() {
        let mut echo = EchoState::default();
        let r1 = echo.edit();
        let r2 = echo.edit();
        let r3 = echo.edit();
        assert_eq!((r1, r2, r3), (1, 2, 3));
        // Echoes land in order; only the last one carries the latest revision.
        assert!(!echo.arrive(r1));
        assert!(!echo.arrive(r2));
        assert!(echo.arrive(r3));
        assert_eq!((echo.applied, echo.dropped), (1, 2));
        // A late stale echo after a newer edit is dropped too.
        let r4 = echo.edit();
        assert!(!echo.arrive(r3));
        assert!(echo.arrive(r4));
    }

    #[test]
    fn vapp_colours_resolve_hex_and_tokens() {
        assert_eq!(color("$semantic.green"), Some(t::GREEN.to_hsla()));
        assert_eq!(color("$palette.background"), Some(t::BACKGROUND.to_hsla()));
        assert_eq!(color("#ff0000"), Some(gpui::rgb(0xff0000).into()));
        assert_eq!(color("$palette.nope"), None);
        assert_eq!(color("red"), None);
    }

    #[test]
    fn vapp_markdown_plain_lines() {
        assert_eq!(plain_markdown_line("**Bold** text"), "Bold text");
        assert_eq!(plain_markdown_line("- item"), "•  item");
    }

    /// The typing test's host: holds the screen WITHOUT laying it out. A
    /// laid-out, focused gpui-component `Input` asks a gpui test window for
    /// its native `NSView` (`gpui_base::input::native::macos::ns_view`
    /// panics: "Test Windows are not backed by a real platform window"), so
    /// `simulate_input` cannot reach it headlessly.
    struct HeadlessHost {
        sink: Entity<VappKitchenSink>,
    }

    impl Render for HeadlessHost {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    /// LANES.md typing test: 40 characters into the host-owned echo field,
    /// 5 ms apart (so early echoes fire mid-typing and must be dropped as
    /// stale), then 400 ms. Injected one character per call through the
    /// field's `EntityInputHandler::replace_text_in_range`, the entry point
    /// the platform text-input client calls for every committed keystroke.
    #[gpui::test]
    async fn vapp_typing_test_echo_field_keeps_every_character(cx: &mut gpui::TestAppContext) {
        use gpui::EntityInputHandler as _;
        const TYPED: &str = "abcdefghijklmnopqrstuvwxyz0123456789ABCD";
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        let host = cx.add_window(|window, cx| HeadlessHost {
            sink: cx.new(|cx| VappKitchenSink::new(window, cx)),
        });
        cx.run_until_parked();
        let (sink, field) = host
            .update(cx, |host, _, cx| {
                let field = host.sink.read(cx).echo_field().expect("echo field");
                (host.sink.clone(), field)
            })
            .unwrap();
        for ch in TYPED.chars() {
            host.update(cx, |_, window, cx| {
                field.update(cx, |state, cx| {
                    state.replace_text_in_range(None, &ch.to_string(), window, cx)
                });
            })
            .unwrap();
            cx.executor().advance_clock(Duration::from_millis(5));
            cx.run_until_parked();
        }
        cx.executor().advance_clock(Duration::from_millis(400));
        cx.run_until_parked();
        let shown = cx.update(|cx| field.read(cx).value().to_string());
        assert_eq!(shown, TYPED);
        let echo = cx.update(|cx| sink.read(cx).echo.clone());
        assert_eq!(echo.latest, 40, "one revision per keystroke: {echo:?}");
        assert!(echo.applied >= 1, "the last echo applied: {echo:?}");
        assert!(echo.dropped >= 1, "mid-typing echoes were stale: {echo:?}");
        assert_eq!(echo.applied + echo.dropped, 40, "every echo arrived: {echo:?}");
    }
}
