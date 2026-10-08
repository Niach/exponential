//! Building the element tree: one absolutely positioned div per placed node
//! at its frame relative to the parent's frame, the box from the node's
//! cached `PaintStyle`, the leaf content inside, children nested; layers as
//! surface-coordinate overlays (with a scrim for Dialog / Drawer).

use std::rc::Rc;

use exponential_ui::layout_tree::NodeKind;
use exponential_ui::surface::{Layer, PlacedNode};
use gpui::{canvas, div, prelude::*, px, AnyElement, Context, Div, Hsla, MouseButton, SharedString, Stateful, Window};
use serde_json::{json, Value};

use super::state::is_text_field;
use super::SurfaceView;
use crate::extension::PaintContext;
use crate::measure::{date_label, display_text, select_label};
use crate::paint::icons::{self, Glyph};
use crate::paint::markdown::{self, MdPaint, MdStyles, TextSpec};
use crate::paint::natives::{self, styled_box, LeafCx};
use crate::paint::parts::{part_visual, spacing};
use crate::paint::PaintStyle;
use crate::measure::Shaper;

impl SurfaceView {
    fn owner_props(&self, index: u32) -> serde_json::Map<String, Value> {
        let o = self.cache.owner_of(index);
        self.cache.node(o).map(|n| n.props.clone()).unwrap_or_default()
    }

    /// The box style a node paints with; mirrored controls (unbound
    /// checkboxes, switches, radios, toggles) re-resolve theirs.
    fn box_style(&self, index: u32, n: &PlacedNode) -> PaintStyle {
        let base = self.styles.get(index as usize).cloned().unwrap_or_default();
        let owner_component = n.owner_component.as_deref().unwrap_or(&n.component);
        let theme = self.surface.theme().cloned();
        let mode = self.surface.mode();
        let states = self.interaction.get(&n.id).copied().unwrap_or_default().states();
        match (owner_component, n.part.as_deref()) {
            ("Checkbox", Some("box")) | ("Switch", Some("track")) => {
                let (checked, external) = self.checked_of(index);
                if checked == external {
                    return base;
                }
                let mut props = self.owner_props(index);
                props.insert("checked".into(), Value::Bool(checked));
                let mut st = states.clone();
                if checked {
                    st.push("checked".into());
                }
                PaintStyle::from_visual(&part_visual(theme.as_deref(), mode, owner_component, n.part.as_deref().unwrap_or("box"), &props, &st))
            }
            // The ROW is a plain option row; the `.dot` leaf wears the
            // `Radio/item` circle recipe (checked = the primary border) and
            // paints the centred inner dot when checked.
            ("Radio", Some("item")) => base,
            ("Radio", Some("dot")) => {
                let mut st = states;
                if self.radio_checked(index) {
                    st.push("checked".into());
                }
                PaintStyle::from_visual(&part_visual(theme.as_deref(), mode, "Radio", "item", &self.owner_props(index), &st))
            }
            ("Toggle", None) => {
                let external = n.props.get("pressed").cloned().unwrap_or(Value::Bool(false));
                let pressed = self.mirrored(&n.id, &external);
                if pressed == external {
                    return base;
                }
                let mut props = n.props.clone();
                props.insert("pressed".into(), pressed);
                PaintStyle::from_visual(&part_visual(theme.as_deref(), mode, "Toggle", "root", &props, &states))
            }
            ("Select" | "DatePicker", Some("field")) => {
                let mut st = states;
                if self.popup.as_ref().is_some_and(|p| p.field == n.id) {
                    st.push("open".into());
                }
                PaintStyle::from_visual(&part_visual(theme.as_deref(), mode, owner_component, "trigger", &self.owner_props(index), &st))
            }
            _ => base,
        }
    }

    /// (shown, external) checked state of a Checkbox / Switch part.
    fn checked_of(&self, index: u32) -> (bool, bool) {
        let o = self.cache.owner_of(index);
        let Some(owner) = self.cache.node(o) else { return (false, false) };
        let external = owner.props.get("checked").cloned().unwrap_or(Value::Bool(false));
        (self.mirrored(&owner.id, &external).as_bool().unwrap_or(false), external.as_bool().unwrap_or(false))
    }

    fn radio_checked(&self, index: u32) -> bool {
        let Some(n) = self.cache.node(index) else { return false };
        let o = self.cache.owner_of(index);
        let Some(owner) = self.cache.node(o) else { return false };
        let external = owner.props.get("value").cloned().unwrap_or(Value::Null);
        let value = self.mirrored(&owner.id, &external);
        let suffix = n.id.rsplit('.').next().unwrap_or_default();
        let row = self.cache.index_of(&format!("{}.item.{suffix}", owner.id)).and_then(|r| self.cache.node(r));
        row.and_then(|r| r.props.get("value")).is_some_and(|v| display_text(Some(v)) == display_text(Some(&value)))
    }

    /// The checked dot of a Radio row, centred in the row's circle.
    fn radio_dot(&self, index: u32, w: f32, h: f32) -> AnyElement {
        let theme = self.surface.theme().cloned();
        let props = crate::paint::parts::part_props(theme.as_deref(), self.surface.mode(), "Radio", "dot", &self.owner_props(index), &["checked".to_string()]);
        let dot = PaintStyle::from_visual(&exponential_ui::style::visual(&props, exponential_ui::style::BoxKind::Leaf));
        let d = crate::paint::parts::px_prop(&props, "width").unwrap_or(w.min(h) / 2.0);
        let ink = self.inks.get(index as usize).copied().unwrap_or(gpui::white());
        let st = PaintStyle { bg: dot.bg.or(Some(ink)), radius: d / 2.0, ..dot };
        styled_box(div().absolute().left(px((w - d) / 2.0)).top(px((h - d) / 2.0)).size(px(d)), &st, d, d).into_any_element()
    }

    /// One node and its subtree, at its frame relative to `origin`.
    pub(crate) fn paint_node(&self, index: u32, origin: (f32, f32), window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let i = index as usize;
        let n = &self.cache.nodes[i];
        let f = self.frames.get(i).copied().unwrap_or_default();
        let style = self.box_style(index, n);
        let id = self.cache.ids[i].clone();
        let mut el = div().id(id.clone()).absolute().left(px(f.x - origin.0)).top(px(f.y - origin.1)).w(px(f.w)).h(px(f.h));
        let r = style.radius.min(f.w.min(f.h) / 2.0).max(0.0);
        el = el.when_some(style.bg, |d, bg| d.bg(bg)).rounded(px(r));
        if !style.shadows.is_empty() {
            el = el.shadow(style.shadows.clone());
        }
        if let Some(o) = style.opacity {
            el = el.opacity(o);
        }
        if style.overflow_hidden {
            el = el.overflow_hidden();
        }
        if let Some(c) = style.color {
            el = el.text_color(c);
        }
        if let Some(role) = self.cache.roles[i] {
            el = el.role(role);
            if let Some(label) = self.cache.labels[i].clone() {
                el = el.aria_label(label);
            }
            let flags = &self.cache.flags[i];
            if role == gpui::Role::Tab {
                el = el.aria_selected(flags.selected);
            }
            if n.part.as_deref() == Some("trigger") && n.owner_component.as_deref() == Some("Accordion") {
                el = el.aria_expanded(flags.open);
            }
        }
        if let Some(h) = self.focus_handles.get(&n.id) {
            el = el.track_focus(h);
        }
        el = self.interactive(el, index, n, cx);
        // The border paints over the background, under the children, and
        // never offsets them (an overlay, not gpui's box border).
        if style.border_width > 0.0 {
            if let Some(bc) = style.border_color {
                el = el.child(div().absolute().top_0().left_0().w(px(f.w)).h(px(f.h)).border(px(style.border_width)).border_color(bc).rounded(px(r)));
            }
        }
        if n.owner_component.as_deref() == Some("Radio") && n.part.as_deref() == Some("dot") && self.radio_checked(index) {
            el = el.child(self.radio_dot(index, f.w, f.h));
        }
        let ink = self.inks.get(i).copied().unwrap_or(gpui::white());
        if n.component == "Extension" {
            return self.paint_extension(el, index, n, &style, f.w, f.h, window, cx);
        }
        if n.kind == NodeKind::Leaf {
            if let Some(content) = self.paint_leaf(index, n, &style, ink, f.w, f.h, window, cx) {
                el = el.child(content);
            }
        }
        let kids = self.children_elements(index, (f.x, f.y), window, cx);
        if let Some(list) = self.lists.get(&index).filter(|l| l.windowed).cloned() {
            return self.paint_windowed_list(el, n, f, list.content_height, kids).into_any_element();
        }
        el.children(kids).into_any_element()
    }

    fn children_elements(&self, index: u32, origin: (f32, f32), window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut out = Vec::new();
        for c in &self.cache.children[index as usize] {
            let child = &self.cache.nodes[*c as usize];
            if child.hidden || child.layer != self.cache.nodes[index as usize].layer {
                continue;
            }
            out.push(self.paint_node(*c, origin, window, cx));
        }
        out
    }

    /// A windowed list: its rows sit at CONTENT offsets; a probe reports the
    /// visible offset (the host scroller's clip or the list's own) so the
    /// core moves the window.
    fn paint_windowed_list(&self, el: Stateful<Div>, n: &PlacedNode, f: exponential_ui::surface::Frame, content_height: f32, kids: Vec<AnyElement>) -> Stateful<Div> {
        let this = self.this.clone();
        let list_id = n.id.clone();
        let offsets = self.list_offsets.clone();
        let probe = canvas(
            move |bounds, window, cx| {
                let mask = window.content_mask().bounds;
                let offset = (f32::from(mask.origin.y) - f32::from(bounds.origin.y)).max(0.0);
                let last = offsets.borrow().get(&list_id).copied();
                if last.is_none_or(|l| (l - offset).abs() >= 1.0) {
                    offsets.borrow_mut().insert(list_id.clone(), offset);
                    if let Some(this) = this.upgrade() {
                        this.update(cx, |this, cx| {
                            if this.surface.scroll(&list_id, offset) {
                                this.nodes_dirty = true;
                                cx.notify();
                            }
                        });
                    }
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .left_0()
        .w_full()
        .h(px(content_height.max(f.h)));
        let inner = div().relative().w(px(f.w)).h(px(content_height.max(f.h))).child(probe).children(kids);
        let scrolls = content_height > f.h + 0.5;
        match (scrolls, self.list_scrolls.get(&n.id)) {
            (true, Some(handle)) => el.overflow_y_scroll().track_scroll(handle).child(inner),
            _ => el.child(inner),
        }
    }

    /// Pointer / hover handlers of a node.
    fn interactive(&self, mut el: Stateful<Div>, index: u32, n: &PlacedNode, cx: &mut Context<Self>) -> Stateful<Div> {
        let id = n.id.clone();
        let tooltip_owner = n.trigger_for.clone().filter(|_| n.owner_component.as_deref() == Some("Tooltip") || n.part.as_deref() == Some("anchor"));
        if let Some(owner) = tooltip_owner {
            return el.on_hover(cx.listener(move |this, hovered: &bool, window, cx| this.tooltip_hover(&owner, *hovered, window, cx)));
        }
        let is_slider = n.component == "Slider" && n.part.as_deref() == Some("track");
        if is_slider {
            let (down_id, probe_id) = (id.clone(), id.clone());
            let bounds = self.slider_bounds.clone();
            return el
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, ev: &gpui::MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.drag_start(&down_id, f32::from(ev.position.x), cx)
                    }),
                )
                .child(canvas(move |b, _, _| {
                    bounds.borrow_mut().insert(probe_id.clone(), b);
                }, |_, _, _, _| {}).absolute().size_full());
        }
        let hoverable = n.pressable || matches!(n.component.as_str(), "Button" | "Link" | "Toggle") || is_text_field(n);
        if hoverable {
            let hid = id.clone();
            el = el.on_hover(cx.listener(move |this, hovered: &bool, _, cx| this.hover(&hid, *hovered, cx)));
        }
        let radio_label = n.owner_component.as_deref() == Some("Radio") && matches!(n.part.as_deref(), Some("label" | "dot")) && n.id.rsplit('.').next().is_some_and(|s| s.parse::<u32>().is_ok());
        let pressable = n.pressable || radio_label || matches!(n.component.as_str(), "Button" | "Link" | "Toggle");
        if pressable && !is_text_field(n) {
            let (down, up, out) = (id.clone(), id.clone(), id);
            let disabled = matches!(n.props.get("disabled"), Some(Value::Bool(true)));
            el = el
                .when(!disabled, |d| d.cursor_pointer())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.press_down(&down, cx)
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        if this.pressed.as_deref() == Some(up.as_str()) {
                            cx.stop_propagation();
                        }
                        this.press_up(&up, window, cx)
                    }),
                )
                .on_mouse_up_out(MouseButton::Left, cx.listener(move |this, _, _, cx| this.press_cancel(&out, cx)));
        }
        let _ = index;
        el
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_extension(&self, el: Stateful<Div>, index: u32, n: &PlacedNode, style: &PaintStyle, w: f32, h: f32, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let f = self.frames.get(index as usize).copied().unwrap_or_default();
        let children = self.children_elements(index, (f.x, f.y), window, cx);
        let kind = n.extension_kind.clone().unwrap_or_default();
        let Some(painter) = self.painters.get(&kind).cloned() else {
            // No painter registered: the placeholder note, children kept.
            let ink = self.inks.get(index as usize).copied().unwrap_or(gpui::white());
            return el.child(div().absolute().size_full().flex().items_center().justify_center().text_size(px(12.0)).text_color(ink.opacity(0.6)).child(SharedString::from(kind))).children(children).into_any_element();
        };
        let Some(visual) = self.surface.visual(index).cloned() else { return el.children(children).into_any_element() };
        let text_style = self.texts.get(index as usize).cloned().unwrap_or_default();
        let theme = self.surface.theme().cloned();
        let this = self.this.clone();
        let ctx = PaintContext {
            node: n,
            visual: &visual,
            text_style: &text_style,
            width: w,
            height: h,
            children,
            theme: theme.as_ref(),
            mode: self.surface.mode(),
            emit: Box::new(move |event, payload, _window, cx| {
                if let Some(this) = this.upgrade() {
                    let event = event.to_string();
                    this.update(cx, |this, cx| this.fire(index, &event, payload, cx));
                }
            }),
        };
        let _ = style;
        let painted = painter.paint(ctx, window, cx);
        el.child(painted).into_any_element()
    }

    /// The content of a measured leaf.
    #[allow(clippy::too_many_arguments)]
    fn paint_leaf(&self, index: u32, n: &PlacedNode, style: &PaintStyle, ink: Hsla, w: f32, h: f32, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let i = index as usize;
        let theme = self.surface.theme().cloned();
        let owner_props = self.owner_props(index);
        let states = self.interaction.get(&n.id).copied().unwrap_or_default().states();
        let text_style = self.texts.get(i).cloned().unwrap_or_default();
        let lcx = LeafCx {
            node: n,
            w,
            h,
            style,
            ink,
            text_style: &text_style,
            font: self.node_fonts.get(i).cloned().unwrap_or_else(|| self.default_font()),
            theme: theme.as_deref(),
            mode: self.surface.mode(),
            host: self.host.as_ref(),
            states: &states,
            owner_props: &owner_props,
        };
        let owner_component = n.owner_component.as_deref().unwrap_or("");
        let flags = &self.cache.flags[i];
        Some(match (n.component.as_str(), n.part.as_deref()) {
            ("Text", Some("tab")) => natives::tab(&lcx, flags.selected),
            ("Text", Some("trigger")) if owner_component == "Accordion" => natives::accordion_trigger(&lcx, flags.open),
            ("Text", Some("item")) if owner_component == "DropdownMenu" => natives::menu_item(&lcx),
            ("Text", _) => natives::text(&lcx, lcx.str("text"), n.lines),
            ("Markdown", _) => self.paint_markdown(&lcx, index, window, cx),
            ("Button" | "Toggle", _) => natives::button(&lcx, &n.component),
            ("Link", _) => natives::link(&lcx),
            ("Icon", _) => natives::icon(&lcx),
            ("Avatar", _) => natives::avatar(&lcx),
            ("Image", _) => natives::image(&lcx),
            ("Video", _) => natives::video(&lcx),
            ("AudioPlayer", _) => natives::audio(&lcx),
            ("Spinner", _) => natives::spinner(&lcx),
            ("Ring", _) => natives::ring(&lcx),
            ("Skeleton", _) => natives::skeleton(&lcx),
            ("Chart", _) => natives::chart(&lcx),
            ("TreeGuides", _) => natives::tree_guides(&lcx),
            ("Unknown", _) => natives::unknown(&lcx),
            ("Checkbox", Some("box")) => natives::check_box(&lcx, self.checked_of(index).0),
            ("Switch", Some("track")) => natives::switch_thumb(&lcx, self.checked_of(index).0),
            ("Radio", Some("dot")) => return None,
            ("Slider", Some("track")) => {
                let (min, max) = (lcx.num("min").unwrap_or(0.0), lcx.num("max").unwrap_or(100.0));
                let v = self.slider_value(index);
                let frac = if max > min { ((v - min) / (max - min)) as f32 } else { 0.0 };
                natives::slider(&lcx, frac)
            }
            ("Select", Some("field")) => {
                let owner = n.owner.clone().unwrap_or_else(|| n.id.clone());
                let external = n.props.get("value").cloned().unwrap_or(Value::Null);
                let mut props = n.props.clone();
                props.insert("value".into(), self.mirrored(&owner, &external));
                let chosen = !matches!(props.get("value"), None | Some(Value::Null)) && !matches!(props.get("value"), Some(Value::Array(a)) if a.is_empty());
                natives::trigger_content(&lcx, &select_label(&props), !chosen, Glyph::ChevronDown)
            }
            ("DatePicker", Some("field")) => {
                let owner = n.owner.clone().unwrap_or_else(|| n.id.clone());
                let external = n.props.get("value").cloned().unwrap_or(Value::Null);
                let value = display_text(Some(&self.mirrored(&owner, &external)));
                match date_label(&value) {
                    Some(label) => natives::trigger_content(&lcx, &label, false, Glyph::Calendar),
                    None => {
                        let ph = if lcx.str("placeholder").is_empty() { "Pick a date" } else { lcx.str("placeholder") };
                        natives::trigger_content(&lcx, ph, true, Glyph::Calendar)
                    }
                }
            }
            ("Input" | "Textarea", Some("field")) => self.field_element(&lcx)?,
            ("Composer", _) => self.paint_composer(&lcx, cx),
            ("ToggleGroup", _) => self.paint_toggle_group(&lcx, index, cx),
            ("Box", Some("indicator")) => self.paint_indicator(&lcx, index, cx),
            ("Input" | "Select" | "DatePicker" | "Textarea", None) => natives::text(&lcx, &placeholder_or_value(n), Some(1)),
            _ => return None,
        })
    }

    fn paint_markdown(&self, lcx: &LeafCx, index: u32, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let (x, y, w, _) = lcx.inner();
        let text = lcx.str("text").to_string();
        if let Some(el) = self.host.markdown(&text, lcx.text_style, w, window, cx) {
            return div().absolute().left(px(x)).top(px(y)).w(px(w)).child(el).into_any_element();
        }
        let blocks = self.cache.markdown.get(&index).cloned().unwrap_or_else(|| Rc::new(markdown::parse(&text)));
        let ts = lcx.text_style;
        let body = TextSpec { size: ts.font_size, line_height: ts.line_height, weight: ts.font_weight, family: ts.font_family.clone() };
        let styles = MdStyles::resolve(lcx.theme, lcx.mode, body, &lcx.node.props);
        let mut shaper = Shaper { window, fonts: &self.fonts, calls: 0 };
        let lay = markdown::layout(&blocks, &styles, w, &mut shaper);
        let (colors, muted, border, code_block_bg) = MdPaint::colors(lcx.theme, lcx.mode, lcx.ink, &lcx.node.props);
        let host = self.host.clone();
        let paint = MdPaint {
            family: self.fonts.family(styles.body.family.as_deref()),
            heading_family: self.fonts.family(styles.heading.family.as_deref()),
            mono: self.fonts.family(styles.code.family.as_deref().or(Some(self.fonts.mono.as_ref()))),
            styles,
            colors,
            muted,
            border,
            code_block_bg,
            on_link: Rc::new(move |href, _window, cx| host.open_url(&host.resolve_url(href), cx)),
        };
        div().absolute().left(px(x)).top(px(y)).child(markdown::paint(&lcx.node.id, &blocks, &lay, w, &paint)).into_any_element()
    }

    fn paint_composer(&self, lcx: &LeafCx, cx: &mut Context<Self>) -> AnyElement {
        let n = lcx.node;
        let (x, y, w, h) = lcx.inner();
        let send_props = lcx.part_props("Composer", "send", &[]);
        let send = crate::paint::parts::px_prop(&send_props, "height").unwrap_or(36.0);
        let busy = lcx.bool("busy");
        let send_style = lcx.part("Composer", "send", &[]);
        let field_props = lcx.part_props("Composer", "field", &[]);
        let fs = crate::paint::parts::px_prop(&field_props, "fontSize").unwrap_or(lcx.text_style.font_size);
        let field_ink = crate::paint::color::color_of(field_props.get("color").and_then(Value::as_str)).unwrap_or(lcx.ink);
        let gap = lcx.style.gap;
        let field_h = (h - send - gap).max(0.0);
        let empty = self.fields.get(&n.id).is_none_or(|f| f.value(cx).trim().is_empty());
        let mut out = div().absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h));
        if let Some(field) = self.fields.get(&n.id) {
            let el = match &field.input {
                super::input::FieldInput::Multi(s) => gpui_component::input::Textarea::new(s).appearance(false).font(lcx.font.clone()).text_size(px(fs)).text_color(field_ink).w(px(w)).h(px(field_h)).into_any_element(),
                super::input::FieldInput::Line(s) => gpui_component::input::Input::new(s).appearance(false).w(px(w)).h(px(field_h)).into_any_element(),
            };
            out = out.child(div().absolute().left_0().top_0().w(px(w)).h(px(field_h)).child(el));
        }
        let mut bar = div().absolute().left_0().bottom_0().w(px(w)).h(px(send)).flex().flex_row().items_center().justify_end().gap(px(spacing(lcx.theme, "xs")));
        if lcx.bool("attachments") {
            let att = lcx.part("Composer", "attachment", &[]);
            let idx = self.cache.index_of(&n.id).unwrap_or(0);
            bar = bar.child(
                styled_box(div().id(SharedString::from(format!("{}.attach", n.id))).h(px(send.min(28.0))).px(px(8.0)).flex().items_center().justify_center().cursor_pointer(), &att, 28.0, 28.0)
                    .role(gpui::Role::Button)
                    .aria_label("Attach")
                    .on_click(cx.listener(move |this, _, _, cx| this.fire(idx, "attach", None, cx)))
                    .child(icons::glyph(Glyph::Attach, 16.0, att.color.unwrap_or(lcx.ink))),
            );
        }
        let send_ink = send_style.color.unwrap_or(lcx.ink);
        let label = if busy { "Stop".to_string() } else if lcx.str("submitLabel").is_empty() { "Send".to_string() } else { lcx.str("submitLabel").to_string() };
        let fid = n.id.clone();
        let disabled = !busy && empty;
        bar = bar.child(
            styled_box(div().id(SharedString::from(format!("{}.send", n.id))).size(px(send)).flex().items_center().justify_center(), &send_style, send, send)
                .when(disabled, |d| d.opacity(0.5))
                .when(!disabled, |d| d.cursor_pointer())
                .role(gpui::Role::Button)
                .aria_label(SharedString::from(label))
                .on_click(cx.listener(move |this, _, window, cx| this.submit_composer(&fid, window, cx)))
                .child(icons::glyph(if busy { Glyph::Stop } else { Glyph::Send }, 16.0, send_ink)),
        );
        out.child(bar).into_any_element()
    }

    fn paint_toggle_group(&self, lcx: &LeafCx, index: u32, cx: &mut Context<Self>) -> AnyElement {
        let n = lcx.node;
        let items = n.props.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
        let fill = lcx.bool("fill");
        let external = n.props.get("value").cloned().unwrap_or(Value::Null);
        let current = self.mirrored(&n.id, &external);
        let chosen: Vec<String> = match &current {
            Value::Array(a) => a.iter().map(|v| display_text(Some(v))).collect(),
            Value::String(s) if lcx.str("type") == "multiple" => s.split(',').map(str::to_string).collect(),
            v => vec![display_text(Some(v))],
        };
        let item_props = lcx.part_props("ToggleGroup", "item", &[]);
        let ih = crate::paint::parts::px_prop(&item_props, "height").unwrap_or(lcx.h);
        let pad = crate::paint::parts::px_prop(&item_props, "paddingHorizontal").or_else(|| crate::paint::parts::px_prop(&item_props, "padding")).unwrap_or(12.0);
        let (x, y, w, h) = lcx.inner();
        let mut row = div().absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h)).flex().flex_row().items_center().gap(px(lcx.style.gap));
        for (k, it) in items.iter().enumerate() {
            let value = it.get("value").cloned().unwrap_or(Value::Null);
            let selected = chosen.contains(&display_text(Some(&value)));
            let disabled = matches!(it.get("disabled"), Some(Value::Bool(true)));
            let item_id = format!("{}.item.{k}", n.id);
            let hovered = self.interaction.get(&item_id).is_some_and(|s| s.hover);
            let mut st: Vec<String> = Vec::new();
            if selected {
                st.push("selected".into());
            }
            if disabled {
                st.push("disabled".into());
            }
            if hovered {
                st.push("hover".into());
            }
            let s = lcx.part("ToggleGroup", "item", &st);
            let color = s.color.unwrap_or(lcx.ink);
            let label = display_text(it.get("label"));
            let mut item = styled_box(div().id(SharedString::from(item_id.clone())).h(px(ih)).px(px(pad)).flex().flex_row().items_center().justify_center().gap(px(6.0)).whitespace_nowrap().text_color(color), &s, 80.0, ih)
                .when(fill, |d| d.flex_1())
                .role(gpui::Role::RadioButton)
                .aria_label(SharedString::from(label.clone()))
                .aria_selected(selected);
            if !disabled {
                let hid = item_id.clone();
                item = item
                    .cursor_pointer()
                    .on_hover(cx.listener(move |this, h: &bool, _, cx| this.hover(&hid, *h, cx)))
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_group_select(index, value.clone(), cx)));
            }
            if let Some(icon) = it.get("icon").and_then(Value::as_str) {
                item = item.child(icons::concept(self.host.as_ref(), icon, 16.0, color));
            }
            if !label.is_empty() {
                item = item.child(SharedString::from(label));
            }
            row = row.child(item);
        }
        row.into_any_element()
    }

    fn paint_indicator(&self, lcx: &LeafCx, index: u32, cx: &mut Context<Self>) -> AnyElement {
        let count = lcx.num("count").unwrap_or(0.0).max(0.0) as usize;
        let page = lcx.num("page").unwrap_or(0.0).max(0.0) as usize;
        let owner = self.cache.owner_of(index);
        let gap = spacing(lcx.theme, "xs");
        let mut row = div().absolute().left_0().bottom_0().w(px(lcx.w)).flex().flex_row().justify_center().gap(px(gap));
        for i in 0..count {
            let states: Vec<String> = if i == page { vec!["selected".into()] } else { vec![] };
            let s = lcx.part("Carousel", "indicator", &states);
            let props = lcx.part_props("Carousel", "indicator", &states);
            let size = crate::paint::parts::px_prop(&props, "width").unwrap_or(8.0);
            let fallback = if i == page { lcx.theme_color("primary") } else { lcx.theme_color("border") };
            let s = PaintStyle { bg: s.bg.or(fallback), radius: size / 2.0, ..s };
            row = row.child(
                styled_box(div().id(SharedString::from(format!("{}.dot.{i}", lcx.node.id))).size(px(size)), &s, size, size)
                    .cursor_pointer()
                    .role(gpui::Role::Tab)
                    .aria_label(SharedString::from(format!("Page {}", i + 1)))
                    .aria_selected(i == page)
                    .on_click(cx.listener(move |this, _, _, cx| this.fire(owner, "change", Some(json!({"page": i})), cx))),
            );
        }
        row.into_any_element()
    }

    /// An open layer: the scrim (Dialog / Drawer) and the content root at its
    /// surface-coordinate frame.
    pub(crate) fn paint_layer(&self, layer: &Layer, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let root = layer.root;
        let modal = matches!(layer.kind.as_str(), "Dialog" | "Drawer");
        let owner_index = self.cache.index_of(&layer.owner);
        let owner_props = owner_index.and_then(|o| self.cache.node(o)).map(|o| o.props.clone()).unwrap_or_default();
        let mut container = div().absolute().left_0().top_0();
        if modal {
            let theme = self.surface.theme().cloned();
            let overlay = PaintStyle::from_visual(&part_visual(theme.as_deref(), self.surface.mode(), &layer.kind, "overlay", &owner_props, &["open".to_string()]));
            let scrim = overlay.bg.unwrap_or(gpui::black().opacity(0.5));
            let dismissible = !matches!(owner_props.get("dismissible"), Some(Value::Bool(false)));
            let height = self.surface_height.max(self.viewport_height);
            let owner = layer.owner.clone();
            container = container.child(
                div()
                    .id(SharedString::from(format!("{}.overlay", layer.owner)))
                    .absolute()
                    .left_0()
                    .top_0()
                    .w(px(self.width))
                    .h(px(height))
                    .bg(scrim)
                    .occlude()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            if dismissible {
                                this.dismiss_layer(&owner, cx);
                            }
                        }),
                    ),
            );
        }
        if self.cache.node(root).is_some_and(|n| !n.hidden) {
            let f = self.frames.get(root as usize).copied().unwrap_or_default();
            let el = self.paint_node(root, (f.x, f.y), window, cx);
            let owner = layer.owner.clone();
            let mut wrap = div().id(SharedString::from(format!("{}.layer", layer.owner))).absolute().left(px(f.x)).top(px(f.y)).w(px(f.w)).h(px(f.h)).occlude().child(el);
            if matches!(layer.kind.as_str(), "Popover" | "DropdownMenu") {
                wrap = wrap.on_mouse_down_out(cx.listener(move |this, _, _, cx| this.dismiss_layer(&owner, cx)));
            }
            container = container.child(wrap);
        }
        container.into_any_element()
    }
}

fn placeholder_or_value(n: &PlacedNode) -> String {
    let v = display_text(n.props.get("value"));
    if v.is_empty() {
        n.props.get("placeholder").and_then(Value::as_str).unwrap_or("").to_string()
    } else {
        v
    }
}
