//! #EXP-1629: what the settings panes' documents share: the IDE's chrome
//! as literal styles ([`Ink`], gpui's rems and line heights in px), the
//! expression helpers the trees bind with, the theme they paint in and the
//! IDE's toasts for a failed write.

use gpui::{App, Hsla};
use gpui_component::ActiveTheme as _;
use serde_json::{json, Value};

use eui_gpui::binding::Toasts;

/// `#rrggbbaa` of a chrome colour (the document's literal styles).
pub(super) fn hex(color: Hsla) -> String {
    let c = color.to_rgb();
    let byte = |v: f32| (v.clamp(0., 1.) * 255.).round() as u8;
    format!("#{:02x}{:02x}{:02x}{:02x}", byte(c.r), byte(c.g), byte(c.b), byte(c.a))
}

pub(super) fn path(p: &str) -> Value {
    json!({"@path": p})
}

pub(super) fn not(value: Value) -> Value {
    json!({"@call": "not", "args": {"value": value}})
}

pub(super) fn eq(a: Value, b: Value) -> Value {
    json!({"@call": "eq", "args": {"a": a, "b": b}})
}

pub(super) fn and(a: Value, b: Value) -> Value {
    json!({"@call": "and", "args": {"values": [a, b]}})
}

/// The colours and metrics the document writes in: the IDE's glass chrome
/// (`surface::glass_group_rows`, `glass_toggle_row`, `glass_picker_row`) in
/// px. gpui's sizes are rems and the IDE's root sets the rem to the theme's
/// font size, so `px_4` / `text_sm` are read through [`Ink::rem`]; a gpui
/// text line is `phi` × its size ([`LINE`]).
pub(super) struct Ink {
    pub(super) rem: f32,
    pub(super) foreground: String,
    pub(super) hint: String,
    pub(super) value: String,
    pub(super) muted: String,
    pub(super) muted_glyph: String,
    pub(super) primary: String,
    pub(super) fill_row: String,
    pub(super) stroke_row: String,
    pub(super) fill_card: String,
    pub(super) stroke_card: String,
    pub(super) code: String,
    pub(super) radius_lg: f32,
    pub(super) radius_md: f32,
    pub(super) fill_section: String,
    pub(super) fill_active: String,
    pub(super) mono: String,
    /// A glass pill `Button`'s fill as gpui-component paints it (the custom
    /// variant mixes its pre-divided colour back with transparent), and its
    /// disabled fill (`color.opacity(0.15)`).
    pub(super) pill_button_fill: String,
    pub(super) pill_button_fill_disabled: String,
}

/// The IDE's translucent inks (`foreground.opacity(…)`) as the node's
/// opacity over the solid colour: a `#rrggbbaa` byte rounds 0.5 to 0.502.
pub(super) const HINT_ALPHA: f32 = 0.5;
pub(super) const VALUE_ALPHA: f32 = 0.7;
pub(super) const GLYPH_ALPHA: f32 = 0.6;

/// `node` drawn at `alpha` (see [`HINT_ALPHA`]).
pub(super) fn faded(mut node: Value, alpha: f32) -> Value {
    node["style"]["opacity"] = json!(alpha);
    node
}

/// gpui's default line height (`phi()`), relative to the font size.
pub(super) const LINE: f32 = 1.618_034;
/// gpui `text_sm` / `text_xs`, in rems.
pub(super) const SM: f32 = 0.875;
pub(super) const XS: f32 = 0.75;

impl Ink {
    pub(super) fn of(cx: &App) -> Self {
        let chrome = crate::controls::ide_chrome();
        let theme = cx.theme();
        Self {
            rem: f32::from(theme.font_size),
            foreground: hex(theme.foreground),
            hint: hex(theme.foreground),
            value: hex(theme.foreground),
            muted: hex(theme.muted_foreground),
            muted_glyph: hex(theme.muted_foreground),
            primary: hex(theme.primary),
            fill_row: hex(chrome.fill_row),
            stroke_row: hex(chrome.stroke_row),
            fill_card: hex(chrome.fill_card),
            stroke_card: hex(chrome.stroke_card),
            code: hex(theme.muted),
            radius_lg: chrome.radius_lg,
            radius_md: chrome.radius_md,
            fill_section: hex(chrome.fill_section),
            fill_active: hex(chrome.fill_active),
            mono: theme.mono_font_family.to_string(),
            pill_button_fill: hex(gpui_component::Colorize::mix_oklab(&eui_gpui::controls::custom_variant_fill(chrome.fill_card), theme.transparent, eui_gpui::controls::CUSTOM_VARIANT_ALPHA_FACTOR)),
            pill_button_fill_disabled: hex(eui_gpui::controls::custom_variant_fill(chrome.fill_card).opacity(0.15)),
        }
    }

    /// `rems` in px.
    pub(super) fn r(&self, rems: f32) -> f32 {
        self.rem * rems
    }

    /// A text run at `rems` (`SM`, `XS`) in gpui's line height.
    pub(super) fn text(&self, id: &str, text: Value, rems: f32, color: &str) -> Value {
        let size = self.r(rems);
        json!({"id": id, "component": "Text", "props": {"text": text}, "style": {"fontSize": size, "lineHeight": size * LINE, "color": color}})
    }

    /// `glass_group`: the radius-12 `fill_row` block.
    pub(super) fn group(&self, id: &str, children: Vec<Value>) -> Value {
        json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "borderRadius": self.radius_lg, "backgroundColor": self.fill_row, "overflow": "hidden"}, "children": children})
    }

    /// The hairline a group row draws above itself (`glass_row_divider`).
    pub(super) fn divider(&self, id: &str, visible: Value) -> Value {
        json!({"id": id, "component": "Box", "visible": visible, "style": {"height": 1, "flexShrink": 0, "backgroundColor": self.stroke_row}})
    }

    /// The row's hairline where `shown`: the IDE draws it as the row's top
    /// border, so a disabled row's dims with it (two nodes, `disabled`
    /// picks).
    pub(super) fn row_dividers(&self, id: &str, shown: Value, disabled: Value) -> [Value; 2] {
        [self.divider(id, and(shown.clone(), not(disabled.clone()))), faded(self.divider(&format!("{id}_dim"), and(shown, disabled)), 0.5)]
    }

    /// `glass_row_shell`: `px_4` / `py_3`, centred, `gap_3`. A disabled row
    /// dims WHOLE (EXP-1557), so each row is drawn twice and `dim` picks.
    pub(super) fn shell(&self, id: &str, dim: bool, visible: Value, children: Vec<Value>) -> Value {
        json!({"id": id, "component": "Box", "visible": visible, "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": self.r(0.75), "paddingHorizontal": self.r(1.), "paddingVertical": self.r(0.75), "opacity": if dim { 0.5 } else { 1.0 }}, "children": children})
    }

    /// The label column (`gap_0p5`): the label over its muted hint.
    pub(super) fn label_column(&self, id: &str, label: Value, hint: Value, grow: bool) -> Value {
        let mut hint = faded(self.text(&format!("{id}_hint"), hint, XS, &self.hint), HINT_ALPHA);
        hint["style"]["maxWidth"] = json!(eui_gpui::controls::ROW_DESCRIPTION_MAX_W);
        json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": self.r(0.125), "minWidth": 0, "flexGrow": if grow { 1 } else { 0 }, "flexShrink": 1},
            "children": [self.text(&format!("{id}_label"), label, SM, &self.foreground), hint]})
    }
}

/// The IDE's toasts for a failed write.
pub(super) struct IdeToasts;

impl Toasts for IdeToasts {
    fn failed(&self, title: &str, detail: Option<&str>, cx: &mut App) {
        let toast = crate::toast::Toast::error(title.to_string());
        let toast = match detail.and_then(crate::action_error::description_of) {
            Some(detail) => toast.description(detail),
            None => toast,
        };
        crate::toast::show_in_active_window(toast, cx);
    }
}

/// The pane's skeleton rows (`settings_skeleton`: the entity row's height).
pub(super) fn skeleton(id: &str, visible: Value) -> Value {
    json!({"id": id, "component": "Skeleton", "visible": visible, "props": {"rows": crate::feedback::SKELETON_ROWS, "rowHeight": super::SETTINGS_ROW_H}})
}

/// `controls::empty_state_list`: a 20px muted glyph over the `text_sm`
/// title, `gap_1p5`, `px_3` / `py_6`, the column `text_center` (each run
/// centres inside its box, which gpui rounds up to a whole pixel).
pub(super) fn empty_state(ink: &Ink, id: &str, icon: &str, title: &str, visible: Value) -> Value {
    let mut text = ink.text(&format!("{id}_title"), json!(title), SM, &ink.muted);
    text["style"]["textAlign"] = json!("center");
    json!({"id": id, "component": "Box", "visible": visible,
        "style": {"display": "flex", "flexDirection": "column", "alignItems": "center", "gap": ink.r(0.375), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(1.5), "width": "100%"},
        "children": [
            {"id": format!("{id}_icon"), "component": "Icon", "props": {"name": icon}, "style": {"width": 20, "height": 20, "color": ink.muted_glyph, "opacity": GLYPH_ALPHA}},
            text,
        ]})
}

/// `action_error::load_failed_state`: [`empty_state`] with the `text_xs`
/// primary Retry link under it (the `retry` action).
pub(super) fn failed_state(ink: &Ink, id: &str, icon: &str, title: &str, visible: Value) -> Value {
    let mut node = empty_state(ink, id, icon, title, visible);
    let mut retry = ink.text(&format!("{id}_retry_text"), json!(crate::feedback::RETRY), XS, &ink.primary);
    retry["style"]["cursor"] = json!("pointer");
    retry["style"]["textAlign"] = json!("center");
    node["children"].as_array_mut().expect("the empty state's children").push(json!({"id": format!("{id}_retry"), "component": "Box", "props": {"pressable": true}, "on": {"press": {"event": {"name": "retry"}}}, "children": [retry]}));
    node
}

impl Ink {
    /// `glass_picker_row`: the label column (no hint: it never shrinks,
    /// picker-presentation.json (3)), then the value at 70% with its chevron
    /// (a small ghost `Button`: `text_sm`, `gap_1`, a `size_4` glyph),
    /// opening `items` as the single-select Menu that fires `action` with
    /// the picked value. A `dim` row draws the trigger only.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn picker_row(&self, id: &str, dim: bool, visible: Value, label: &str, hint: Option<&str>, value: Value, action: &str, items: Vec<Value>) -> Value {
        let glyph = self.r(1.);
        let trigger = json!({"id": format!("{id}_trigger"), "component": "Box", "props": {"pressable": !dim},
            "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": self.r(0.25), "cursor": "pointer"},
            "children": [
                faded(self.text(&format!("{id}_value"), value, SM, &self.value), VALUE_ALPHA),
                {"id": format!("{id}_chevron"), "component": "Icon", "props": {"name": "ui-chevron-right"}, "style": {"width": glyph, "height": glyph, "color": self.value, "opacity": VALUE_ALPHA}},
            ]});
        let control = if dim {
            trigger
        } else {
            json!({"id": format!("{id}_menu"), "component": "Menu", "props": {"items": items, "align": "end"}, "on": {"select": {"event": {"name": action}}}, "children": [trigger]})
        };
        let control = json!({"id": format!("{id}_control"), "component": "Box", "style": {"display": "flex", "flexDirection": "row", "justifyContent": "flex-end", "flexGrow": 1, "minWidth": 200}, "children": [control]});
        let column = match hint {
            Some(hint) => self.label_column(&format!("{id}_text"), json!(label), json!(hint), false),
            None => {
                let mut column = json!({"id": format!("{id}_text"), "component": "Box", "style": {"display": "flex", "flexDirection": "column", "minWidth": 0, "flexShrink": 0},
                    "children": [self.text(&format!("{id}_text_label"), json!(label), SM, &self.foreground)]});
                column["style"]["gap"] = json!(self.r(0.125));
                column
            }
        };
        self.shell(id, dim, visible, vec![column, control])
    }

    /// `glass_section_header`: the `fill_section` strip (`px_3` / `py_1p5`,
    /// `gap_1p5`, `mb_1`, `radius_md`), the `text_sm` MEDIUM label at 85%,
    /// then a spacer and the trailing control.
    pub(super) fn band(&self, id: &str, label: &str, trailing: Option<Value>) -> Value {
        self.band_with(id, None, label, None, trailing)
    }

    /// [`Ink::band`] with a leading glyph and `glass_section_band_loading`'s
    /// reload spinner (shown where `loading`), BEFORE the trailing control.
    pub(super) fn band_with(&self, id: &str, leading: Option<Value>, label: &str, loading: Option<Value>, trailing: Option<Value>) -> Value {
        let mut text = faded(self.text(&format!("{id}_label"), json!(label), SM, &self.foreground), 0.85);
        text["props"]["lines"] = json!(1);
        text["style"]["fontWeight"] = json!(500);
        text["style"]["minWidth"] = json!(0);
        text["style"]["flexShrink"] = json!(1);
        let mut children: Vec<Value> = leading.into_iter().collect();
        children.push(text);
        children.push(json!({"id": format!("{id}_spacer"), "component": "Box", "style": {"flexGrow": 1}}));
        if let Some(loading) = loading {
            let size = eui_gpui::controls::INLINE_SPINNER_PX;
            children.push(json!({"id": format!("{id}_spinner"), "component": "Spinner", "visible": loading, "style": {"width": size, "height": size, "color": self.muted, "flexShrink": 0}}));
        }
        children.extend(trailing);
        json!({"id": id, "component": "Box",
            "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0, "gap": self.r(0.375), "paddingHorizontal": self.r(0.75), "paddingVertical": self.r(0.375), "marginBottom": self.r(0.25), "borderRadius": self.radius_md, "backgroundColor": self.fill_section},
            "children": children})
    }

    /// `glass_pill` at `PillSize::Sm`: the 24px capsule (`px 8`, `gap 4`),
    /// `fill_card` over a 1px `stroke_card`, `text_xs` MEDIUM at 70%. `mono`
    /// sets the label in the theme's mono family; `press` makes it an
    /// action pill (the active fill under the pointer).
    pub(super) fn pill(&self, id: &str, label: Value, mono: bool, press: Option<Value>) -> Value {
        let mut text = faded(self.text(&format!("{id}_text"), label, XS, &self.foreground), VALUE_ALPHA);
        text["style"]["fontWeight"] = json!(500);
        if mono {
            text["style"]["fontFamily"] = json!(self.mono);
        }
        let mut pill = json!({"id": id, "component": "Box",
            "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "flexShrink": 0, "height": 24, "paddingHorizontal": 8, "gap": 4, "borderRadius": 9999, "borderWidth": 1, "borderColor": self.stroke_card, "backgroundColor": self.fill_card},
            "children": [text]});
        if let Some(event) = press {
            pill["props"] = json!({"pressable": true});
            pill["on"] = json!({"press": {"event": event}});
            pill["style"]["cursor"] = json!("pointer");
            pill["style"][":hover"] = json!({"backgroundColor": self.fill_active});
        }
        pill
    }

    /// `glass_pill_button` at `PillSize::Sm` (gpui-component's `XSmall`
    /// button under `web_xs`): 24px tall, `px 8`, capsule, a 1px
    /// `stroke_card`; inside, `gap_1` between the `size_3` glyph and the
    /// `text_xs` label (line height 1); no glyph without `icon`. Disabled: the faded fill and the
    /// half-muted ink. Drawn twice, `disabled` picks.
    pub(super) fn pill_button(&self, id: &str, icon: Option<&str>, label: Value, disabled: Value, event: Value) -> [Value; 2] {
        let draw = |dim: bool| {
            let id = if dim { format!("{id}_dim") } else { id.to_string() };
            let (ink, alpha) = if dim { (&self.muted, 0.5) } else { (&self.foreground, VALUE_ALPHA) };
            let size = self.r(XS);
            let glyph = icon.map(|icon| json!({"id": format!("{id}_icon"), "component": "Icon", "props": {"name": icon}, "style": {"width": self.r(0.75), "height": self.r(0.75), "color": ink, "opacity": alpha, "flexShrink": 0}}));
            let text = json!({"id": format!("{id}_text"), "component": "Text", "props": {"text": label.clone()}, "style": {"fontSize": size, "lineHeight": size, "color": ink, "opacity": alpha, "flexShrink": 0}});
            let mut button = json!({"id": id, "component": "Box", "visible": if dim { disabled.clone() } else { not(disabled.clone()) },
                "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "gap": self.r(0.25), "height": 24, "paddingHorizontal": 8, "borderRadius": 9999, "borderWidth": 1, "borderColor": self.stroke_card, "backgroundColor": if dim { &self.pill_button_fill_disabled } else { &self.pill_button_fill }},
                "children": glyph.into_iter().chain([text]).collect::<Vec<_>>()});
            if !dim {
                button["props"] = json!({"pressable": true});
                button["on"] = json!({"press": {"event": event.clone()}});
                button["style"]["cursor"] = json!("pointer");
                button["style"][":hover"] = json!({"backgroundColor": self.fill_active});
            }
            button
        };
        [draw(false), draw(true)]
    }

    /// `controls::ghost_icon_button`: a 32px transparent square on the row
    /// rung's corner, its `size_3p5` glyph at 70%, the list hover wash.
    pub(super) fn ghost_icon_button(&self, id: &str, icon: &str, label: Value, event: Value, list_hover: &str) -> Value {
        let glyph = self.r(0.875);
        json!({"id": id, "component": "Box", "props": {"pressable": true}, "accessibility": {"label": label}, "on": {"press": {"event": event}},
            "style": {"display": "flex", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "width": 32, "height": 32, "borderRadius": self.radius_md, "cursor": "pointer", ":hover": {"backgroundColor": list_hover}},
            "children": [{"id": format!("{id}_icon"), "component": "Icon", "props": {"name": icon}, "style": {"width": glyph, "height": glyph, "color": self.foreground, "opacity": VALUE_ALPHA}}]})
    }
}

/// The pane's document as a [`SurfaceView`] in the Exponential theme, its actions
/// and inputs routed to `binding`, everything else to the IDE's plugin.
pub(super) fn mount<S: 'static>(surface_id: &str, tree: Value, binding: &gpui::Entity<eui_gpui::binding::Binding<S>>, window: &mut gpui::Window, cx: &mut App) -> gpui::Entity<eui_gpui::view::SurfaceView> {
    mount_with(surface_id, tree, binding, std::rc::Rc::new(ide_plugin()), false, window, cx)
}

/// The IDE's plugin as the panes' documents use it (no media origin, no
/// uploads).
pub(super) fn ide_plugin() -> crate::exponential_ui_host::IdePlugin {
    crate::exponential_ui_host::IdePlugin { media_origin: String::new(), uploads: None }
}

/// [`mount`] with the pane's own plugin under the binding; `lift_layers`
/// opens its popups at window level, as the IDE's picker popovers do.
pub(super) fn mount_with<S: 'static>(surface_id: &str, tree: Value, binding: &gpui::Entity<eui_gpui::binding::Binding<S>>, inner: std::rc::Rc<dyn eui_gpui::host::HostPlugin>, lift_layers: bool, window: &mut gpui::Window, cx: &mut App) -> gpui::Entity<eui_gpui::view::SurfaceView> {
    use gpui::AppContext as _;
    use std::rc::Rc;
    let host: Rc<dyn eui_gpui::host::HostPlugin> = Rc::new(eui_gpui::binding::BindingHost::new(binding, inner));
    let options = eui_gpui::view::SurfaceViewOptions {
        surface_id: surface_id.into(),
        theme: eui_core::themes::builtin_theme(crate::exponential_ui_host::THEME_ID),
        mode: eui_core::theme::Mode::Dark,
        host,
        lift_layers,
        settings: Some(eui_core::surface::SurfaceSettings { mode: eui_core::theme::ModeSetting::Dark, reduced_motion: theme::motion::reduce_motion(), ..Default::default() }),
        ..Default::default()
    };
    let tree: eui_core::NestedNode = serde_json::from_value(tree).expect("a settings document parses");
    let view = cx.new(|cx| {
        let mut view = eui_gpui::view::SurfaceView::new(options, window, cx);
        let outcome = view.set_nested(tree, cx);
        // The documents write the IDE's chrome as literal colours ([`Ink`]):
        // the core reports each one (a node style wants `$color.<name>`) and
        // paints it; anything else is a real mistake.
        let issues: Vec<_> = outcome.issues.iter().filter(|issue| !issue.message.ends_with("expected $color.<name>")).collect();
        debug_assert!(issues.is_empty(), "{surface_id}: {issues:?}");
        view
    });
    binding.update(cx, |binding, cx| binding.attach(&view, cx));
    view
}
