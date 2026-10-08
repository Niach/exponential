//! Overlay natives (Dialog, Drawer, Popover, Tooltip, DropdownMenu,
//! ContextMenu), menus with checkbox / label / separator entries and ONE
//! level of submenus, and the Toast band. The trigger stays inline; the
//! content is a LAYER with its own root (`<owner>.content`), placed by the
//! surface against the viewport, an anchor or a point.

use serde_json::{json, Value};

use super::{bool_prop, js, str_prop, Builder, LNode, LayerPlacement, NodeKind, ToastSpec};
use crate::overlay::{OverlayAlign, OverlaySide};
use crate::theme::RecipeQuery;
use crate::types::UiNode;

/// Toasts visible at once (the newest three).
pub const MAX_TOASTS: usize = 3;

impl Builder<'_, '_> {
    /// A layer root with an id suffix (`<owner>.<part><suffix>`).
    fn layer_root_suffixed(&mut self, owner: &LNode, part: &str, suffix: &str, style: Value) -> (u32, u32) {
        let (layer, root) = self.layer_root(owner, part, style);
        if !suffix.is_empty() {
            let id = format!("{}.{part}{suffix}", owner.id);
            self.nodes[root as usize].id = id;
        }
        (layer, root)
    }

    pub(crate) fn overlay(&mut self, index: u32, node: &UiNode, scope: &str) {
        let owner = self.nodes[index as usize].clone();
        let layer_parent = owner.layer;
        let kind = owner.component.clone();
        // Round 2 §7: the overlay is LAYOUT-TRANSPARENT, its frame is the
        // trigger's. A control trigger (Button, Toggle) keeps its own size;
        // any other trigger (a Link, a Box) is the flex item: the wrapper
        // takes the parent's alignment and the trigger fills it.
        let slot_trigger = node.slots.as_ref().and_then(|s| s.get("trigger"));
        let fills = slot_trigger.is_some_and(|t| !matches!(t.component.as_str(), "Button" | "Toggle"));
        if kind == "ContextMenu" {
            self.style_default(index, &[("display", json!("flex")), ("flexDirection", json!("column"))]);
        } else if fills {
            self.style_default(index, &[("display", json!("flex")), ("flexDirection", json!("row")), ("alignItems", json!("stretch"))]);
        } else {
            self.style_default(index, &[("display", json!("flex")), ("flexDirection", json!("row")), ("alignSelf", json!("flex-start"))]);
        }
        let mut anchor: Option<u32> = None;
        if let Some(trigger) = slot_trigger {
            anchor = self.add(trigger, Some(index), layer_parent, scope, false);
            if let Some(a) = anchor.filter(|_| fills) {
                self.style_default(a, &[("flexGrow", json!(1)), ("minWidth", json!(0))]);
            }
        } else if kind == "DropdownMenu" {
            let label = owner.props.get("label").cloned().unwrap_or_else(|| Value::String(self.string("menu")));
            let mut props = json!({"label": label, "variant": "outline"});
            if let Some(icon) = owner.props.get("icon") {
                props["icon"] = icon.clone();
                if owner.props.get("label").is_none() {
                    props["size"] = json!("icon");
                }
            }
            let t = self.part(&owner, "trigger", "Button", NodeKind::Leaf, json!({}), props);
            anchor = Some(t);
        } else if kind == "Tooltip" {
            // A tooltip wraps its children as the anchor.
            let wrap = self.part(&owner, "anchor", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "row"}), json!({}));
            self.nodes[wrap as usize].part_query = None;
            self.children_of(wrap, node, scope, false);
            anchor = Some(wrap);
        } else if kind == "ContextMenu" {
            // The target region: the child, inline.
            self.children_of(index, node, scope, false);
        }
        let open_on_hover = kind == "Tooltip" || str_prop(&owner.props, "openOn") == Some("hover");
        if let Some(a) = anchor {
            self.nodes[a as usize].pressable |= !open_on_hover;
            self.nodes[a as usize].trigger_for = Some(owner.id.clone());
        }
        let open = self.is_open(&owner.id, &owner.props, false);
        if let Some(a) = anchor.filter(|_| open) {
            self.add_state(a, "open");
        }
        if !open {
            return;
        }
        let side = self.responsive_prop(&owner.props, "side").and_then(|v| v.as_str().and_then(OverlaySide::parse));
        let point = self.ctx.local.context_point.get(&owner.id).copied();
        let placement = match kind.as_str() {
            "Dialog" => LayerPlacement::Centered,
            "Drawer" => LayerPlacement::Edge(side.unwrap_or(OverlaySide::Bottom)),
            "Tooltip" => LayerPlacement::Anchored { anchor: anchor.unwrap_or(index), side: side.unwrap_or(OverlaySide::Top), align: OverlayAlign::Center },
            "DropdownMenu" => LayerPlacement::Anchored { anchor: anchor.unwrap_or(index), side: OverlaySide::Bottom, align: OverlayAlign::Start },
            "ContextMenu" => match point {
                Some((x, y)) => LayerPlacement::AtPoint { x, y },
                None => LayerPlacement::Anchored { anchor: index, side: OverlaySide::Bottom, align: OverlayAlign::Start },
            },
            _ => LayerPlacement::Anchored { anchor: anchor.unwrap_or(index), side: side.unwrap_or(OverlaySide::Bottom), align: OverlayAlign::Center },
        };
        let content_style = match kind.as_str() {
            "Dialog" => json!({"display": "flex", "flexDirection": "column", "gap": "$spacing.md", "padding": "$spacing.lg", "borderRadius": "$radius.xl", "width": "100%"}),
            "Drawer" => match placement {
                LayerPlacement::Edge(OverlaySide::Top | OverlaySide::Bottom) => json!({"display": "flex", "flexDirection": "column", "gap": "$spacing.md", "padding": "$spacing.lg", "width": "100%"}),
                _ => json!({"display": "flex", "flexDirection": "column", "gap": "$spacing.md", "padding": "$spacing.lg", "height": "100%"}),
            },
            "Tooltip" => json!({"display": "flex", "flexDirection": "row", "paddingHorizontal": "$spacing.sm", "paddingVertical": "$spacing.xs", "borderRadius": "$radius.md"}),
            "DropdownMenu" | "ContextMenu" => json!({"display": "flex", "flexDirection": "column", "padding": "$spacing.xs", "borderRadius": "$radius.md", "minWidth": 160}),
            _ => json!({"display": "flex", "flexDirection": "column", "gap": "$spacing.md", "padding": "$spacing.md", "borderRadius": "$radius.md"}),
        };
        let (layer, root) = self.layer_root(&owner, "content", content_style);
        if matches!(kind.as_str(), "Dialog" | "Drawer") {
            // Round 2: the dialog's name — its title, else `$string.dialog`.
            let label = str_prop(&owner.props, "title").map(str::to_string).unwrap_or_else(|| self.string("dialog"));
            self.default_a11y(root, "label", json!(label));
        }
        let owner_now = self.nodes[index as usize].clone();
        let mut scroll_body = None;
        let dismissible = owner.props.get("dismissible").and_then(Value::as_bool).unwrap_or(true);
        match kind.as_str() {
            "Dialog" | "Drawer" => {
                if kind == "Drawer" && matches!(placement, LayerPlacement::Edge(OverlaySide::Bottom)) && owner.props.get("dragToDismiss").and_then(Value::as_bool).unwrap_or(true) {
                    self.part_in(root, &owner_now, "handle", "Box", NodeKind::Container, json!({"width": 40, "height": 4, "borderRadius": "$radius.full", "alignSelf": "center", "flexShrink": 0}), json!({}), "");
                }
                if let Some(t) = str_prop(&owner.props, "title") {
                    let t = t.to_string();
                    let n = self.text_part(root, &owner_now, "title", &t, "title", "");
                    self.nodes[n as usize].base_style.insert("flexShrink".into(), json!(0));
                }
                if let Some(d) = str_prop(&owner.props, "description") {
                    let d = d.to_string();
                    let n = self.text_part(root, &owner_now, "description", &d, "muted", "");
                    self.nodes[n as usize].base_style.insert("flexShrink".into(), json!(0));
                }
                // The body scrolls when the content is taller than the viewport.
                let body = self.part_in(root, &owner_now, "body", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "column", "gap": "$spacing.md", "flexShrink": 1, "minHeight": 0, "overflowY": "auto"}), json!({}), "");
                self.nodes[body as usize].part_query = None;
                scroll_body = Some(body);
                self.children_of(body, node, scope, false);
                if let Some(footer) = node.slots.as_ref().and_then(|s| s.get("footer")) {
                    let f = self.part_in(root, &owner_now, "footer", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "row", "justifyContent": "flex-end", "gap": "$spacing.sm", "flexShrink": 0}), json!({}), "");
                    self.add(footer, Some(f), layer, scope, false);
                }
                if kind == "Dialog" && dismissible {
                    let label = self.string("close");
                    let close = self.part_in(root, &owner_now, "close", "Button", NodeKind::Leaf, json!({"position": "absolute", "top": "$spacing.sm", "insetInlineEnd": "$spacing.sm"}), json!({"icon": super::builtin_icon("Dialog.close"), "variant": "ghost", "size": "icon", "label": label}), "");
                    self.nodes[close as usize].pressable = true;
                }
            }
            "Tooltip" => {
                let text = owner.props.get("content").map(js).unwrap_or_default();
                self.text_part(root, &owner_now, "label", &text, "caption", "");
            }
            "DropdownMenu" | "ContextMenu" => {
                let items = owner.props.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
                self.menu_items(root, &owner_now, &items, "", true);
                scroll_body = Some(root);
            }
            _ => {
                self.children_of(root, node, scope, false);
            }
        }
        let modal = kind == "Dialog" || kind == "Drawer";
        self.push_layer(layer, root, &owner_now, placement, modal, dismissible, false, scroll_body);
    }

    /// Menu entries (`menuItem.kind`): an action or checkbox ROW (check,
    /// label, shortcut, submenu indicator), a separator, a group label; an
    /// open submenu becomes its own layer beside its row.
    pub(crate) fn menu_items(&mut self, parent: u32, owner: &LNode, items: &[Value], prefix: &str, top: bool) {
        let open_sub = if top { self.ctx.local.submenu.get(&owner.id).copied() } else { None };
        for (i, item) in items.iter().enumerate() {
            let suffix = format!("{prefix}.{i}");
            let kind = item.get("kind").and_then(Value::as_str).unwrap_or(if item.get("separator").and_then(Value::as_bool) == Some(true) { "separator" } else { "item" });
            match kind {
                "separator" => {
                    self.part_in(parent, owner, "separator", "Box", NodeKind::Container, json!({"height": 1, "marginVertical": "$spacing.xs", "flexShrink": 0}), json!({}), &suffix);
                }
                "label" => {
                    let text = item.get("label").map(js).unwrap_or_default();
                    self.text_part(parent, owner, "label", &text, "caption", &suffix);
                }
                _ => {
                    let disabled = item.get("disabled").and_then(Value::as_bool).unwrap_or(false);
                    let checked = item.get("checked").and_then(Value::as_bool).unwrap_or(false);
                    let label = item.get("label").map(js).unwrap_or_default();
                    let mut props = json!({"text": label, "value": item.get("value").cloned().unwrap_or(Value::Null), "kind": kind, "disabled": disabled, "lines": 1});
                    if kind == "checkbox" {
                        props["checked"] = json!(checked);
                    }
                    if item.get("destructive").and_then(Value::as_bool) == Some(true) {
                        props["destructive"] = json!(true);
                    }
                    let row = self.part_in(parent, owner, "item", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": "$spacing.sm"}), props, &suffix);
                    self.nodes[row as usize].pressable = !disabled;
                    if disabled {
                        self.add_state(row, "disabled");
                    }
                    if kind == "checkbox" {
                        if checked {
                            self.add_state(row, "checked");
                        }
                        let glyph = format!("{}.check", owner.component);
                        let c = self.icon_part(row, owner, "check", &glyph, &suffix);
                        if !checked {
                            self.nodes[c as usize].base_style.insert("visibility".into(), json!("hidden"));
                        }
                    }
                    let mut label_props = json!({"text": label, "lines": 1});
                    if let Some(icon) = item.get("icon") {
                        label_props["icon"] = icon.clone();
                    }
                    let l = self.part_in(row, owner, "itemLabel", "Text", NodeKind::Leaf, json!({"flexGrow": 1, "flexShrink": 1, "minWidth": 0}), label_props, &suffix);
                    self.nodes[l as usize].part_query = None;
                    if let Some(shortcut) = item.get("shortcut").map(js) {
                        self.text_part(row, owner, "shortcut", &shortcut, "muted", &suffix);
                    }
                    if kind == "submenu" {
                        let glyph = format!("{}.submenuIndicator", owner.component);
                        self.icon_part(row, owner, "submenuIndicator", &glyph, &suffix);
                        if open_sub == Some(i) {
                            self.add_state(row, "open");
                            let sub_items = item.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
                            let (layer, root) = self.layer_root_suffixed(owner, "content", &suffix, json!({"display": "flex", "flexDirection": "column", "padding": "$spacing.xs", "borderRadius": "$radius.md", "minWidth": 160}));
                            self.menu_items(root, owner, &sub_items, &suffix, false);
                            self.push_layer(layer, root, owner, LayerPlacement::Anchored { anchor: row, side: OverlaySide::Right, align: OverlayAlign::Start }, false, true, false, Some(root));
                            if let Some(spec) = self.layers.last_mut() {
                                spec.mirror = true;
                            }
                        }
                    }
                }
            }
        }
    }

    /// A Toast: hidden in the main tree; open ones go to the toast band.
    pub(crate) fn toast(&mut self, index: u32, node: &UiNode) {
        self.nodes[index as usize].hidden = true;
        self.nodes[index as usize].kind = NodeKind::Leaf;
        let open = self.is_open(&node.id, &self.nodes[index as usize].props.clone(), true);
        if open {
            self.pending_toasts.push((index, node.clone()));
        }
    }

    /// The toast band: the newest three open toasts, newest nearest the edge.
    pub(crate) fn flush_toasts(&mut self) {
        let pending = std::mem::take(&mut self.pending_toasts);
        let skip = pending.len().saturating_sub(MAX_TOASTS);
        for (order, (index, _node)) in pending.into_iter().skip(skip).rev().enumerate() {
            let owner = self.nodes[index as usize].clone();
            let kind = str_prop(&owner.props, "type").unwrap_or("info").to_string();
            let (layer, root) = self.layer_root(&owner, "root", json!({"display": "flex", "flexDirection": "row", "alignItems": "flex-start", "gap": "$spacing.sm", "padding": "$spacing.md", "borderRadius": "$radius.lg", "minWidth": 240, "maxWidth": 420}));
            self.nodes[root as usize].live = Some(if kind == "error" { "assertive" } else { "polite" }.into());
            self.query_prop(root, "type", Value::String(kind.clone()));
            let glyph = format!("Toast.icon.{kind}");
            self.icon_part(root, &owner, "icon", &glyph, "");
            let body = self.part_in(root, &owner, "body", "Box", NodeKind::Container, json!({"display": "flex", "flexDirection": "column", "gap": "$spacing.xxs", "flexGrow": 1, "flexShrink": 1, "minWidth": 0}), json!({}), "");
            self.nodes[body as usize].part_query = None;
            let title = owner.props.get("title").map(js).unwrap_or_default();
            self.text_part(body, &owner, "title", &title, "label", "");
            if let Some(d) = owner.props.get("description").map(js) {
                self.text_part(body, &owner, "description", &d, "muted", "");
            }
            if let Some(label) = owner.props.get("actionLabel").map(js) {
                let a = self.part_in(root, &owner, "action", "Button", NodeKind::Leaf, json!({"flexShrink": 0}), json!({"label": label, "variant": "outline", "size": "sm"}), "");
                self.nodes[a as usize].pressable = true;
                self.nodes[a as usize].own_query = Some(RecipeQuery::new("Button", "root", self.ctx.recipes.native_recipe_props("Button", &self.nodes[a as usize].props.clone()), Vec::new()));
            }
            let dismissible = owner.props.get("dismissible").and_then(Value::as_bool).unwrap_or(true);
            if dismissible {
                let label = self.string("dismiss");
                let c = self.part_in(root, &owner, "close", "Button", NodeKind::Leaf, json!({"flexShrink": 0}), json!({"icon": super::builtin_icon("Toast.close"), "label": label, "variant": "ghost", "size": "icon"}), "");
                self.nodes[c as usize].pressable = true;
            }
            self.push_layer(layer, root, &owner, LayerPlacement::Toast { order: order as u32 }, false, dismissible, false, None);
            let duration = owner.props.get("duration").and_then(Value::as_f64).unwrap_or(5000.0);
            self.toasts.push(ToastSpec { id: owner.id.clone(), duration_ms: duration, kind });
        }
        let _ = bool_prop;
    }
}
