//! Interactions: local UI state (tabs, accordion, carousel, overlays,
//! popups, calendars, menus, tables, chips, files, forms), write-through of
//! bound props, `on` handlers (evaluated like `src/dynamic.ts runAction`:
//! context and function args against the data AS IT IS, then `set`, then
//! the event), scrolling and the host commands.

use serde_json::{json, Map, Value};

use super::{OutEvent, Surface, SurfaceCommand};
use crate::data::{absolute_path, is_binding, run_action, set_pointer, ResolveContext};
use crate::layout_tree::fields::{clamp_number_value, dates, number_prop, number_step_precision, parse_time};
use crate::layout_tree::{FieldErrors, LNode, FORM_FIELDS};
use crate::types::UiNode;

fn js(v: &Value) -> String {
    crate::json::to_js_string(v)
}

/// A check's resolved condition fails it (`failingChecks`: undefined,
/// null, false, "").
pub(crate) fn check_fails(v: Option<&Value>) -> bool {
    matches!(v, None | Some(Value::Null) | Some(Value::Bool(false))) || v.and_then(Value::as_str) == Some("")
}

/// The index suffix of a part id (`<owner>.item.3` → 3).
fn suffix_index(id: &str) -> Option<usize> {
    id.rsplit('.').next().and_then(|s| s.parse().ok())
}

impl Surface {
    // ------------------------------------------------------------------
    // Scrolling
    // ------------------------------------------------------------------

    /// A scroll container scrolled vertically (content offset in px): any
    /// `overflow: scroll|auto` node or a windowed List/Table (moves the
    /// window; only the rows entering/leaving change).
    pub fn scroll(&mut self, id: &str, offset: f32) -> bool {
        let x = self.scroll_offset(id).0;
        self.scroll_to(id, x, offset)
    }

    /// A scroll container named by its node id or its window key (a
    /// Table's body or the Table) → the key its offset lives under.
    fn scroll_id(&self, id: &str) -> String {
        self.slot_of.get(id).map(|&s| self.scroll_key(s)).filter(|k| k != id && self.lists.iter().any(|l| l.id == *k)).unwrap_or_else(|| id.to_string())
    }

    /// Both axes. Offsets clamp to the content the last pass measured (a
    /// fling past the end lands on the last page, never on blank rows).
    pub fn scroll_to(&mut self, id: &str, x: f32, y: f32) -> bool {
        let key = self.scroll_id(id);
        let id = key.as_str();
        let (mut x, mut y) = (x.max(0.0), y.max(0.0));
        if let Some((max_x, max_y)) = self.scroll_max(id) {
            x = x.min(max_x);
            y = y.min(max_y);
        }
        let current = self.local.scroll.get(id).copied().unwrap_or((0.0, 0.0));
        if (current.0 - x).abs() < 0.5 && (current.1 - y).abs() < 0.5 {
            return false;
        }
        self.local.scroll.insert(id.to_string(), (x, y));
        // A windowed container re-windows (its subtree only), and so does
        // every windowed list this container is the viewport of (round 2);
        // a plain one only moves paint.
        let lists: Vec<String> = self
            .lists
            .iter()
            .filter(|l| l.windowed)
            .filter(|l| l.id == id || matches!(self.local.list_views.get(&l.id).map(|v| &v.source), Some(crate::layout_tree::ListViewSource::Ancestor { id: a, .. }) if a == id))
            .map(|l| l.id.clone())
            .collect();
        self.subtree_pending.extend(lists);
        if (self.local.sticky_any || self.lists.iter().any(|l| l.sticky)) && !self.pending.iter().any(|e| matches!(e, OutEvent::Relayout)) {
            // Sticky offsets follow the scroll.
            self.pending.push(OutEvent::Relayout);
        }
        true
    }

    /// Round 2 (§5): the host's scroll offset of the WHOLE surface (a
    /// surface laid out taller than its host viewport, the host scrolls
    /// it). Lists without a bounded size window against it; sticky nodes
    /// without a scrolling ancestor pin against it. Returns whether
    /// anything must lay out again.
    pub fn set_surface_scroll(&mut self, x: f32, y: f32) -> bool {
        if self.local.surface_scroll == Some((x, y)) {
            return false;
        }
        self.local.surface_scroll = Some((x, y));
        let lists: Vec<String> = self.lists.iter().filter(|l| l.windowed && matches!(self.local.list_views.get(&l.id).map(|v| &v.source), Some(crate::layout_tree::ListViewSource::Host { .. }))).map(|l| l.id.clone()).collect();
        let any = !lists.is_empty() || self.local.sticky_any;
        self.subtree_pending.extend(lists);
        any
    }

    /// The largest offsets of a scroll container as the last pass laid it
    /// out (`None` before its first pass); an axis whose content fits is
    /// not clamped (a windowed list inside a scrolling PAGE takes the page's
    /// offset past its top).
    fn scroll_max(&self, id: &str) -> Option<(f32, f32)> {
        let slot = self.lists.iter().find(|l| l.id == id).map(|l| l.node).or_else(|| self.slot_of.get(id).copied())?;
        let s = self.scrolls.iter().find(|s| s.index == slot)?;
        let f = self.last_frames.get(slot as usize)?;
        let max = |content: f32, size: f32, on: bool| if on && content > size + 0.5 { content - size } else { f32::INFINITY };
        Some((max(s.content_width, f.w, s.scroll_x), max(s.content_height, f.h, s.scroll_y)))
    }

    /// The key a scroll container's offset is kept under: a windowed
    /// Table's BODY scrolls under the Table's id (its window key), every
    /// other container under its own id.
    pub(super) fn scroll_key(&self, slot: u32) -> String {
        self.lists.iter().find(|l| l.node == slot).map(|l| l.id.clone()).unwrap_or_else(|| self.nodes[slot as usize].id.clone())
    }

    /// The current offset of a scroll container.
    pub fn scroll_offset(&self, id: &str) -> (f32, f32) {
        self.local.scroll.get(&self.scroll_id(id)).copied().unwrap_or((0.0, 0.0))
    }

    // ------------------------------------------------------------------
    // Overlays
    // ------------------------------------------------------------------

    /// Open or close an overlay or popup (Dialog, Drawer, Popover, Tooltip,
    /// DropdownMenu, ContextMenu, Select, the pickers, Toast).
    pub fn set_open(&mut self, id: &str, open: bool) -> Vec<OutEvent> {
        self.local.open.insert(id.to_string(), open);
        if !open {
            self.local.submenu.remove(id);
            self.local.query.remove(id);
            self.local.range_anchor.remove(id);
        }
        self.needs_build = true;
        let mut out = vec![];
        out.extend(self.write_through(id, "open", Value::Bool(open)));
        out.extend(self.fire(id, "change", Some(json!({"open": open}))));
        out.push(OutEvent::Relayout);
        out
    }

    fn is_open_now(&self, id: &str) -> bool {
        self.local.open.get(id).copied().unwrap_or_else(|| self.node_by_id(id).and_then(|o| o.props.get("open").and_then(Value::as_bool)).unwrap_or(false))
    }

    /// A Toast's timeout (or its close button): `open: false`, `dismiss` +
    /// `change`.
    pub fn dismiss_toast(&mut self, id: &str) -> Vec<OutEvent> {
        let mut out = self.fire(id, "dismiss", None);
        out.extend(self.set_open(id, false));
        out
    }

    // ------------------------------------------------------------------
    // Lookup
    // ------------------------------------------------------------------

    pub(super) fn node_by_id(&self, id: &str) -> Option<&LNode> {
        self.slot_of.get(id).map(|s| &self.nodes[*s as usize])
    }

    /// The SOURCE node of `id` (its `on`, its raw bindings, its `checks`),
    /// renamed to `id`: an instance node (inside a template row or a Table
    /// slot cell, at any depth) resolves through the source the build
    /// recorded on it.
    fn source_node(&self, id: &str) -> Option<UiNode> {
        let source = self.node_by_id(id).and_then(|n| n.source.as_ref());
        let (source_id, tree) = match source {
            Some(s) => (s.id.as_str(), match &s.template {
                Some(t) => self.template_cache.get(t)?.as_ref()?,
                None => self.root.as_ref()?,
            }),
            None => (id, self.root.as_ref()?),
        };
        let mut found = None;
        tree.walk(&mut |n| {
            if found.is_none() && n.id == source_id {
                found = Some(n.clone());
            }
        });
        found.map(|mut n| {
            n.id = id.to_string();
            n
        })
    }

    /// The instance suffix of `id` (what its build appended to the source
    /// id; empty for a node of the surface's tree).
    fn instance_suffix(&self, id: &str) -> String {
        self.node_by_id(id).and_then(|n| n.source.as_ref()).and_then(|s| id.strip_prefix(s.id.as_str())).unwrap_or("").to_string()
    }

    fn scope_of(&self, id: &str) -> String {
        self.node_by_id(id).map(|n| n.scope.clone()).unwrap_or_default()
    }

    /// The resolve context of a scope: the data model, the strings and, for
    /// a literal Table row's scope, the row standing in for its pointer.
    fn resolve_ctx<'s>(&'s self, scope: &'s str) -> ResolveContext<'s> {
        let ctx = ResolveContext::new(&self.data, scope).with_strings(&self.strings).with_formatter(&*self.formatter);
        let ctx = match self.clock {
            Some(now) => ctx.with_now(now),
            None => ctx,
        };
        match crate::layout_tree::row_scope_root(scope).and_then(|root| self.seed.row_scopes.get_key_value(root)) {
            Some((root, row)) => ctx.with_overlay(root, row),
            None => ctx,
        }
    }

    /// The binding path of `prop` on `id`, made absolute. A RELATIVE path
    /// inside a literal Table row (`/$row/…`) has no place in the data model:
    /// the control keeps its value locally like an unbound one.
    fn binding_path(&self, id: &str, prop: &str) -> Option<String> {
        let node = self.source_node(id)?;
        let binding = node.props.get(prop).filter(|v| is_binding(v))?;
        Some(absolute_path(binding["path"].as_str().unwrap_or(""), &self.scope_of(id))).filter(|p| !p.starts_with("/$row/"))
    }

    /// If `prop` on `id` is bound, write `value` to the data model.
    fn write_through(&mut self, id: &str, prop: &str, value: Value) -> Vec<OutEvent> {
        let Some(path) = self.binding_path(id, prop) else { return vec![] };
        set_pointer(&mut self.data, &path, Some(value.clone()));
        self.data_version += 1;
        self.needs_build = true;
        vec![OutEvent::DataChanged { path, value }]
    }

    /// Write a field's new value: through its binding, else into the local
    /// field state (so the control shows it and a Form collects it).
    fn write_field(&mut self, id: &str, prop: &str, value: Value) -> Vec<OutEvent> {
        let out = self.write_through(id, prop, value.clone());
        if out.is_empty() {
            self.local.field_values.insert(id.to_string(), value);
            self.needs_build = true;
        } else {
            self.local.field_values.remove(id);
        }
        out
    }

    /// Fire a node's `on.<event>` handler: the context and the function's
    /// args resolve against the data AS IT IS, then `set` writes, then the
    /// event goes out with the payload merged into its context.
    fn fire(&mut self, id: &str, event: &str, payload: Option<Value>) -> Vec<OutEvent> {
        let Some(node) = self.source_node(id) else { return vec![] };
        let Some(action) = node.on.as_ref().and_then(|o| o.get(event)).cloned() else { return vec![] };
        let scope = self.scope_of(id);
        let outcome = {
            let ctx = self.resolve_ctx(&scope);
            run_action(&action, &ctx)
        };
        let mut out = vec![];
        if let Some((path, value)) = outcome.written {
            self.data = outcome.data;
            self.data_version += 1;
            self.needs_build = true;
            out.push(OutEvent::DataChanged { path, value });
        }
        if let Some(ev) = outcome.event {
            let mut context = match ev.context {
                Some(Value::Object(m)) => m,
                _ => Map::new(),
            };
            if let Some(Value::Object(p)) = &payload {
                for (k, v) in p {
                    context.insert(k.clone(), v.clone());
                }
            }
            out.push(OutEvent::Action { name: ev.name, component_id: id.to_string(), event: event.to_string(), context: Value::Object(context), payload });
        }
        if let Some(call) = outcome.call {
            if call.call == "openUrl" {
                if let Some(url) = call.args.get("url").and_then(Value::as_str) {
                    out.push(OutEvent::OpenUrl { url: url.to_string() });
                }
            } else if !call.call.is_empty() && !crate::host::is_builtin_function(&call.call) {
                // A host function: the host's registry + policy gate decide
                // (VAPP-91). The other built-ins are value functions: no effect.
                out.push(OutEvent::FunctionCall { component_id: id.to_string(), name: call.call, args: Value::Object(call.args) });
            }
        }
        out
    }

    // ------------------------------------------------------------------
    // Events
    // ------------------------------------------------------------------

    /// An interaction on layout node `index`: `press`, `change` (with a
    /// payload such as `{value}` / `{open}` / `{page}`), `select`, `submit`,
    /// `commit`, `dismiss`, `contextmenu {x, y}`, `upload {files}`. Local UI
    /// state is updated here; bound props write through; `on` handlers fire.
    pub fn event(&mut self, index: u32, event: &str, payload: Option<Value>) -> Vec<OutEvent> {
        if !self.live.get(index as usize).copied().unwrap_or(false) {
            return vec![];
        }
        let n = self.nodes[index as usize].clone();
        // A disabled (or loading) control is inert whatever the host
        // dispatches (keyboard activation, an accessibility action): only
        // closing and leaving go through.
        if !matches!(event, "dismiss" | "blur" | "reset") && self.is_inert(&n) {
            return vec![];
        }
        if event == "contextmenu" {
            return self.context_menu(&n, payload);
        }
        if event == "press" {
            if let Some(target) = n.trigger_for.clone() {
                let mut out = self.fire(&n.id, "press", None);
                let open = !self.is_open_now(&target);
                out.extend(self.set_open(&target, open));
                return out;
            }
        }
        let owner_id = n.owner.clone().unwrap_or_else(|| n.id.clone());
        let owner_component = n.owner_component.clone().unwrap_or_else(|| n.component.clone());
        let part = n.part.clone();
        let mut out = vec![];
        match (owner_component.as_str(), part.as_deref(), event) {
            ("Tabs", Some("tab"), "press") => {
                let value = n.props.get("value").map(js).unwrap_or_default();
                self.local.tabs.insert(owner_id.clone(), value.clone());
                self.subtree_pending.insert(owner_id.clone());
                out.extend(self.write_through(&owner_id, "value", Value::String(value.clone())));
                out.extend(self.fire(&owner_id, "change", Some(json!({"value": value}))));
                out.push(OutEvent::Relayout);
            }
            ("Accordion", Some("trigger"), "press") => {
                let Some(owner) = self.node_by_id(&owner_id).cloned() else { return out };
                let value = n.props.get("value").map(js).unwrap_or_default();
                let multiple = owner.props.get("type").and_then(Value::as_str) == Some("multiple");
                let mut open = self.local.accordion.get(&owner_id).cloned().unwrap_or_else(|| owner.props.get("value").map(js).map(|v| v.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect()).unwrap_or_default());
                if open.contains(&value) {
                    open.retain(|v| *v != value);
                } else if multiple {
                    open.push(value.clone());
                } else {
                    open = vec![value.clone()];
                }
                let joined = open.join(",");
                self.local.accordion.insert(owner_id.clone(), open);
                self.subtree_pending.insert(owner_id.clone());
                out.extend(self.write_through(&owner_id, "value", Value::String(joined.clone())));
                out.extend(self.fire(&owner_id, "change", Some(json!({"value": joined}))));
                out.push(OutEvent::Relayout);
            }
            ("Carousel", _, "change") | ("Carousel", Some("previous" | "next"), "press") => {
                // A previous/next button carries the page it goes to.
                let page = if event == "press" { n.props.get("target").and_then(Value::as_i64) } else { payload.as_ref().and_then(|p| p.get("page")).and_then(Value::as_i64) }.unwrap_or(0);
                self.local.carousel.insert(owner_id.clone(), page);
                self.subtree_pending.insert(owner_id.clone());
                out.extend(self.write_through(&owner_id, "page", json!(page)));
                out.extend(self.fire(&owner_id, "change", Some(json!({"page": page}))));
                out.push(OutEvent::Relayout);
            }
            ("DropdownMenu" | "ContextMenu", Some("item"), "press") => out.extend(self.menu_select(&n, &owner_id)),
            ("Toast", Some("action"), "press") => {
                out.extend(self.fire(&owner_id, "action", None));
                out.extend(self.dismiss_toast(&owner_id));
            }
            ("Toast", Some("close"), "press") | ("Toast", _, "dismiss") => out.extend(self.dismiss_toast(&owner_id)),
            ("Dialog" | "Drawer" | "Popover" | "Tooltip" | "DropdownMenu" | "ContextMenu", part, "press") if part.is_some() || n.id == owner_id => {
                if part == Some("close") {
                    out.extend(self.set_open(&owner_id, false));
                } else if part.is_none() || part == Some("trigger") {
                    let open = !self.is_open_now(&owner_id);
                    out.extend(self.set_open(&owner_id, open));
                }
            }
            (_, _, "dismiss") => {
                // Escape, a scrim press, a drag: close the overlay if allowed.
                let dismissible = self.layers.iter().find(|l| l.owner == owner_id).map(|l| l.dismissible).unwrap_or(true);
                if dismissible {
                    out.extend(self.set_open(&owner_id, false));
                }
            }
            ("Dialog" | "Drawer" | "Popover" | "Tooltip" | "DropdownMenu" | "ContextMenu" | "Select" | "DatePicker" | "DateRangePicker" | "TimePicker", _, "change") if payload.as_ref().and_then(|p| p.get("open")).is_some() => {
                let open = payload.as_ref().and_then(|p| p.get("open")).and_then(Value::as_bool).unwrap_or(false);
                out.extend(self.set_open(&owner_id, open));
            }
            ("Select", Some("item"), "press") => out.extend(self.select_option(&n, &owner_id)),
            ("Select", Some("search"), "change") => {
                let query = payload.as_ref().and_then(|p| p.get("value")).map(js).unwrap_or_default();
                self.local.query.insert(owner_id.clone(), query.clone());
                self.needs_build = true;
                out.extend(self.fire(&owner_id, "search", Some(json!({"query": query}))));
                out.push(OutEvent::Relayout);
            }
            ("DatePicker" | "DateRangePicker", Some("previous" | "next"), "press") => {
                let delta = if part.as_deref() == Some("next") { 1 } else { -1 };
                let current = self.calendar_month(&owner_id);
                self.local.month.insert(owner_id.clone(), dates::shift_month(current.0, current.1, delta));
                self.needs_build = true;
                out.push(OutEvent::Relayout);
            }
            ("DatePicker", Some("day"), "press") => {
                let date = n.props.get("date").map(js).unwrap_or_default();
                out.extend(self.write_field(&owner_id, "value", Value::String(date.clone())));
                out.extend(self.fire(&owner_id, "change", Some(json!({"value": date}))));
                out.extend(self.set_open(&owner_id, false));
            }
            ("DateRangePicker", Some("day"), "press") => out.extend(self.range_pick(&n, &owner_id)),
            ("TimePicker", Some("item"), "press") => {
                let value = n.props.get("value").map(js).unwrap_or_default();
                out.extend(self.write_field(&owner_id, "value", Value::String(value.clone())));
                out.extend(self.fire(&owner_id, "change", Some(json!({"value": value}))));
                out.extend(self.set_open(&owner_id, false));
            }
            ("NumberField", Some("decrement" | "increment"), "press") => {
                let up = part.as_deref() == Some("increment");
                out.extend(self.number_step(&owner_id, up));
            }
            ("NumberField", _, "submit") => {
                out.extend(self.fire(&owner_id, "submit", payload.clone()));
                if let Some(form) = n.form.clone() {
                    out.extend(self.submit_form(&form));
                }
            }
            ("NumberField", _, "change" | "commit") => {
                // Text parses in the surface formatter's separators and
                // digits (what the field shows); unreadable text changes
                // nothing (the reference's NumberField), empty text clears.
                let raw = payload.as_ref().and_then(|p| p.get("value")).cloned().unwrap_or(Value::Null);
                let parsed = match &raw {
                    Value::Number(_) => raw.as_f64(),
                    Value::String(s) => match crate::format::parse_number(self.formatter.as_ref(), s) {
                        None if !s.trim().is_empty() => return out,
                        v => v,
                    },
                    _ => None,
                };
                let value = parsed.map(|v| if event == "commit" { self.clamp_number(&owner_id, v) } else { v });
                let v = value.map(crate::json::number).unwrap_or(Value::Null);
                self.local.errors.remove(&owner_id);
                out.extend(self.write_field(&owner_id, "value", v.clone()));
                out.extend(self.fire(&owner_id, "change", Some(json!({"value": v}))));
                out.push(OutEvent::Relayout);
            }
            ("ChipInput", Some("input"), "change") => {
                let text = payload.as_ref().and_then(|p| p.get("value")).map(js).unwrap_or_default();
                if let Some(entry) = text.strip_suffix(',') {
                    out.extend(self.chip_add(&owner_id, entry.trim()));
                } else {
                    self.local.query.insert(owner_id.clone(), text);
                    self.needs_build = true;
                    out.push(OutEvent::Relayout);
                }
            }
            ("ChipInput", Some("input"), "submit" | "commit") => {
                let text = payload.as_ref().and_then(|p| p.get("value")).map(js).unwrap_or_default();
                out.extend(self.chip_add(&owner_id, text.trim()));
            }
            ("ChipInput", Some("suggestion"), "press") => {
                let value = n.props.get("value").map(js).unwrap_or_default();
                out.extend(self.chip_add(&owner_id, &value));
            }
            ("ChipInput", Some("remove"), "press") => {
                let value = n.props.get("value").map(js).unwrap_or_default();
                out.extend(self.chip_remove(&owner_id, &value));
            }
            ("FileUpload", Some("dropzone" | "browse"), "press") => {
                let owner = self.node_by_id(&owner_id).cloned();
                let accept = owner.as_ref().and_then(|o| o.props.get("accept")).and_then(Value::as_str).map(str::to_string);
                let multiple = owner.as_ref().and_then(|o| o.props.get("multiple")).and_then(Value::as_bool).unwrap_or(false);
                out.push(OutEvent::PickFiles { component_id: owner_id.clone(), accept, multiple });
            }
            ("FileUpload", _, "upload") => out.extend(self.files_upload(&owner_id, payload)),
            ("FileUpload", Some("remove"), "press") => {
                let i = suffix_index(&n.id).unwrap_or(0);
                out.extend(self.file_remove(&owner_id, i));
            }
            ("Table", Some("headerCell"), "press") => out.extend(self.table_sort(&n, &owner_id)),
            ("Table", Some("row"), "press") => out.extend(self.table_row(&n, &owner_id)),
            ("Table", Some("checkbox"), "press") => out.extend(self.table_check(&n, &owner_id)),
            ("CodeBlock", Some("copy"), "press") => {
                let code = self.node_by_id(&owner_id).and_then(|o| o.props.get("code")).map(js).unwrap_or_default();
                self.local.copied.insert(owner_id.clone(), true);
                self.subtree_pending.insert(owner_id.clone());
                out.push(OutEvent::Copy { text: code });
                out.push(OutEvent::Announce { text: self.strings.get("copied").cloned().unwrap_or_default(), live: "polite".into() });
                out.push(OutEvent::Relayout);
            }
            ("CodeBlock", Some("copy"), "reset") => {
                self.local.copied.remove(&owner_id);
                self.subtree_pending.insert(owner_id.clone());
                out.push(OutEvent::Relayout);
            }
            ("Resizable", Some("handle"), "drag" | "key") => out.extend(self.resize_event(&n, &owner_id, event, payload.as_ref())),
            ("Checkbox" | "Switch", _, "press" | "change") => {
                let current = self.current_prop(&owner_id, "checked").and_then(|v| v.as_bool()).unwrap_or(false);
                let checked = payload.as_ref().and_then(|p| p.get("checked")).and_then(Value::as_bool).unwrap_or(!current);
                self.local.errors.remove(&owner_id);
                out.extend(self.write_field(&owner_id, "checked", Value::Bool(checked)));
                out.extend(self.fire(&owner_id, "change", Some(json!({"checked": checked}))));
                out.push(OutEvent::Relayout);
            }
            ("Radio", Some("item" | "dot" | "label"), "press") => {
                // The option index is the id suffix (`<owner>.dot.2`); the
                // ROW (`<owner>.item.2`) carries the option's value.
                let suffix = n.id.rsplit('.').next().unwrap_or_default();
                let row_id = format!("{owner_id}.item.{suffix}");
                let value = self.node_by_id(&row_id).and_then(|row| row.props.get("value").cloned()).unwrap_or(Value::Null);
                self.local.errors.remove(&owner_id);
                out.extend(self.write_field(&owner_id, "value", value.clone()));
                out.extend(self.fire(&owner_id, "change", Some(json!({"value": value}))));
                out.push(OutEvent::Relayout);
            }
            ("Toggle", _, "press") => {
                let current = self.local.pressed_toggles.get(&owner_id).copied().unwrap_or_else(|| n.props.get("pressed").and_then(Value::as_bool).unwrap_or(false));
                self.local.pressed_toggles.insert(owner_id.clone(), !current);
                self.subtree_pending.insert(owner_id.clone());
                out.extend(self.write_through(&owner_id, "pressed", Value::Bool(!current)));
                out.extend(self.fire(&owner_id, "change", Some(json!({"pressed": !current}))));
                out.push(OutEvent::Relayout);
            }
            ("Link", _, "press") => {
                if let Some(url) = n.props.get("href").and_then(Value::as_str) {
                    out.push(OutEvent::OpenUrl { url: url.to_string() });
                }
                out.extend(self.fire(&owner_id, "press", None));
            }
            ("Button", None, "press") if n.props.get("submit").and_then(Value::as_bool) == Some(true) => {
                out.extend(self.fire(&owner_id, "press", None));
                if let Some(form) = n.form.clone() {
                    out.extend(self.submit_form(&form));
                }
            }
            ("Input" | "Textarea" | "Select" | "DatePicker" | "Slider" | "Composer" | "ToggleGroup" | "TimePicker" | "DateRangePicker", _, "change" | "commit" | "submit") => {
                let value = payload.as_ref().and_then(|p| p.get("value")).cloned().unwrap_or(Value::Null);
                let name = n.props.get("name").or_else(|| self.node_by_id(&owner_id).and_then(|o| o.props.get("name"))).and_then(Value::as_str).unwrap_or(&owner_id).to_string();
                let path = self.binding_path(&owner_id, "value");
                if event != "submit" {
                    out.extend(self.write_field(&owner_id, "value", value.clone()));
                    // `validateOn: change` checks as the user types; a field
                    // already showing errors re-checks on every change.
                    if event == "change" && (self.validate_on(&owner_id) == "change" || self.local.errors.contains_key(&owner_id)) {
                        self.validate_field(&owner_id);
                    }
                }
                out.push(OutEvent::Input { component_id: owner_id.clone(), name, path, value: value.clone(), commit: event != "change" });
                out.extend(self.fire(&owner_id, event, Some(json!({"value": value}))));
                // Enter in a single-line field submits its Form.
                if event == "submit" && owner_component != "Textarea" {
                    if let Some(form) = n.form.clone() {
                        out.extend(self.submit_form(&form));
                    }
                }
            }
            ("Form", _, "submit") => out.extend(self.submit_form(&owner_id)),
            ("Input" | "Textarea" | "NumberField" | "Select" | "DatePicker" | "TimePicker" | "ChipInput", _, "blur") => {
                if self.validate_on(&owner_id) == "blur" {
                    self.validate_field(&owner_id);
                    out.push(OutEvent::Relayout);
                }
                out.extend(self.fire(&owner_id, "blur", None));
            }
            _ => {
                out.extend(self.fire(&owner_id, event, payload));
            }
        }
        out
    }

    /// The node, or the native owning it, is `disabled`/`loading` (its own
    /// props, a Form's injection, or the host's `disabled` state).
    fn is_inert(&self, n: &LNode) -> bool {
        let flagged = |p: &crate::types::Props| p.get("disabled").and_then(Value::as_bool) == Some(true) || p.get("loading").and_then(Value::as_bool) == Some(true);
        if flagged(&n.props) || self.states.get(&n.id).is_some_and(|s| s.iter().any(|x| x == "disabled")) {
            return true;
        }
        n.owner.as_ref().and_then(|o| self.node_by_id(o)).is_some_and(|o| flagged(&o.props))
    }

    fn context_menu(&mut self, n: &LNode, payload: Option<Value>) -> Vec<OutEvent> {
        // The nearest ContextMenu at or above the node.
        let mut cur = Some(n.index);
        while let Some(c) = cur {
            let node = &self.nodes[c as usize];
            if node.component == "ContextMenu" {
                let id = node.id.clone();
                let x = payload.as_ref().and_then(|p| p.get("x")).and_then(Value::as_f64);
                let y = payload.as_ref().and_then(|p| p.get("y")).and_then(Value::as_f64);
                match (x, y) {
                    (Some(x), Some(y)) => {
                        self.local.context_point.insert(id.clone(), (x as f32, y as f32));
                    }
                    _ => {
                        self.local.context_point.remove(&id);
                    }
                }
                return self.set_open(&id, true);
            }
            cur = node.parent;
        }
        vec![]
    }

    /// A menu entry pressed: a checkbox toggles (its `checked` binding
    /// written), a submenu opens beside its row, an action selects + closes.
    fn menu_select(&mut self, n: &LNode, owner_id: &str) -> Vec<OutEvent> {
        let kind = n.props.get("kind").and_then(Value::as_str).unwrap_or("item").to_string();
        let path: Vec<usize> = n.id.strip_prefix(&format!("{owner_id}.item.")).map(|s| s.split('.').filter_map(|p| p.parse().ok()).collect()).unwrap_or_default();
        let mut out = vec![];
        if kind == "submenu" {
            let i = path.first().copied().unwrap_or(0);
            if self.local.submenu.get(owner_id) == Some(&i) {
                self.local.submenu.remove(owner_id);
            } else {
                self.local.submenu.insert(owner_id.to_string(), i);
            }
            self.needs_build = true;
            out.push(OutEvent::Relayout);
            return out;
        }
        let value = n.props.get("value").cloned().unwrap_or(Value::Null);
        if kind == "checkbox" {
            let checked = !n.props.get("checked").and_then(Value::as_bool).unwrap_or(false);
            // The item's `checked` binding inside the source `items`.
            if let Some(source) = self.source_node(owner_id) {
                let mut item = source.props.get("items");
                for (depth, i) in path.iter().enumerate() {
                    let at = item.and_then(|a| a.get(*i));
                    item = if depth + 1 < path.len() { at.and_then(|x| x.get("items")) } else { at };
                }
                if let Some(binding) = item.and_then(|i| i.get("checked")).filter(|b| is_binding(b)) {
                    let p = absolute_path(binding["path"].as_str().unwrap_or(""), &self.scope_of(owner_id));
                    set_pointer(&mut self.data, &p, Some(Value::Bool(checked)));
                    self.data_version += 1;
                    self.needs_build = true;
                    out.push(OutEvent::DataChanged { path: p, value: Value::Bool(checked) });
                }
            }
            out.extend(self.fire(owner_id, "select", Some(json!({"value": value, "checked": checked}))));
        } else {
            out.extend(self.fire(owner_id, "select", Some(json!({"value": value}))));
        }
        out.extend(self.set_open(owner_id, false));
        out
    }

    fn select_option(&mut self, n: &LNode, owner_id: &str) -> Vec<OutEvent> {
        let v = n.props.get("value").map(js).unwrap_or_default();
        let owner = self.node_by_id(owner_id).cloned();
        let multiple = owner.as_ref().and_then(|o| o.props.get("multiple")).and_then(Value::as_bool).unwrap_or(false);
        let value = if multiple {
            let current = self.current_prop(owner_id, "value").filter(|v| !v.is_null()).map(|v| js(&v)).unwrap_or_default();
            let mut set: Vec<String> = current.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect();
            if set.contains(&v) {
                set.retain(|x| *x != v);
            } else {
                set.push(v.clone());
            }
            set.join(",")
        } else {
            v
        };
        self.local.errors.remove(owner_id);
        let mut out = self.write_field(owner_id, "value", Value::String(value.clone()));
        out.extend(self.fire(owner_id, "change", Some(json!({"value": value}))));
        if multiple {
            out.push(OutEvent::Relayout);
        } else {
            out.extend(self.set_open(owner_id, false));
        }
        out
    }

    fn calendar_month(&self, id: &str) -> (i32, u32) {
        if let Some(m) = self.local.month.get(id) {
            return *m;
        }
        let owner = self.node_by_id(id);
        let start = owner.and_then(|o| o.props.get("value").or_else(|| o.props.get("start"))).and_then(Value::as_str).and_then(dates::parse);
        start.map(|(y, m, _)| (y, m)).or_else(|| self.settings.today.as_deref().and_then(dates::parse).map(|(y, m, _)| (y, m))).unwrap_or((2026, 1))
    }

    /// DateRangePicker: the first pick anchors, the second completes the
    /// range (ordered) and closes.
    fn range_pick(&mut self, n: &LNode, owner_id: &str) -> Vec<OutEvent> {
        let date = n.props.get("date").map(js).unwrap_or_default();
        let mut out = vec![];
        match self.local.range_anchor.remove(owner_id) {
            None => {
                self.local.range_anchor.insert(owner_id.to_string(), date);
                self.needs_build = true;
                out.push(OutEvent::Relayout);
            }
            Some(first) => {
                let (start, end) = if first <= date { (first, date) } else { (date, first) };
                let a = self.write_through(owner_id, "start", Value::String(start.clone()));
                let b = self.write_through(owner_id, "end", Value::String(end.clone()));
                if a.is_empty() && b.is_empty() {
                    self.local.field_values.insert(owner_id.to_string(), json!({"start": start, "end": end}));
                }
                out.extend(a);
                out.extend(b);
                out.extend(self.fire(owner_id, "change", Some(json!({"start": start, "end": end}))));
                out.extend(self.set_open(owner_id, false));
            }
        }
        out
    }

    /// The reference's `clamp`: into `[min, max]`, then rounded to the
    /// precision (`precision`, else the step's decimals) like `toFixed`.
    fn clamp_number(&self, id: &str, v: f64) -> f64 {
        let Some(owner) = self.node_by_id(id) else { return v };
        let (_, precision) = number_step_precision(&owner.props);
        clamp_number_value(v, number_prop(&owner.props, "min"), number_prop(&owner.props, "max"), precision)
    }

    /// A stepper press: from the value, else `min` when it is above 0, else
    /// 0 (the reference's `stepBy`).
    fn number_step(&mut self, id: &str, up: bool) -> Vec<OutEvent> {
        let owner = self.node_by_id(id).cloned();
        let (step, _) = owner.as_ref().map(|o| number_step_precision(&o.props)).unwrap_or((1.0, 0));
        let min = owner.as_ref().and_then(|o| number_prop(&o.props, "min"));
        let value = self.current_prop(id, "value").filter(|v| !v.is_null() && v.as_str() != Some("")).map(|v| match v {
            Value::Number(n) => n.as_f64().unwrap_or(0.0),
            other => crate::json::to_number(&other),
        });
        let current = value.unwrap_or_else(|| min.filter(|m| *m > 0.0).unwrap_or(0.0));
        let next = self.clamp_number(id, if up { current + step } else { current - step });
        let v = crate::json::number(next);
        self.local.errors.remove(id);
        let mut out = self.write_field(id, "value", v.clone());
        out.extend(self.fire(id, "change", Some(json!({"value": v}))));
        out.push(OutEvent::Relayout);
        out
    }

    /// A prop's CURRENT value: the data model when bound, else the local
    /// state an event left, else the node's props (events may arrive
    /// several times between two layouts).
    fn current_prop(&self, id: &str, prop: &str) -> Option<Value> {
        if let Some(path) = self.binding_path(id, prop) {
            let scope = self.scope_of(id);
            return self.resolve_ctx(&scope).read(&path).cloned();
        }
        let local = match prop {
            "values" => self.local.chips.get(id).map(|v| Value::Array(v.iter().cloned().map(Value::String).collect())),
            "files" => self.local.files.get(id).map(|f| Value::Array(f.clone())),
            "selected" => self.local.selected.get(id).map(|v| Value::Array(v.iter().cloned().map(Value::String).collect())),
            "sort" => self.local.sort.get(id).map(|(k, d)| json!({"key": k, "direction": d})),
            "checked" | "value" => self.local.field_values.get(id).cloned(),
            _ => None,
        };
        local.or_else(|| self.node_by_id(id).and_then(|o| o.props.get(prop)).cloned())
    }

    fn chip_values(&self, id: &str) -> Vec<String> {
        self.current_prop(id, "values").and_then(|v| v.as_array().map(|a| a.iter().map(js).collect())).unwrap_or_default()
    }

    fn chips_write(&mut self, id: &str, values: Vec<String>) -> Vec<OutEvent> {
        let value = Value::Array(values.iter().cloned().map(Value::String).collect());
        let out = self.write_through(id, "values", value);
        if out.is_empty() {
            self.local.chips.insert(id.to_string(), values);
        } else {
            self.local.chips.remove(id);
        }
        self.needs_build = true;
        out
    }

    fn chip_add(&mut self, id: &str, value: &str) -> Vec<OutEvent> {
        self.local.query.remove(id);
        self.needs_build = true;
        let mut values = self.chip_values(id);
        let max = self.node_by_id(id).and_then(|o| o.props.get("max")).and_then(Value::as_f64);
        if value.is_empty() || values.iter().any(|v| v == value) || max.is_some_and(|m| values.len() as f64 >= m) {
            return vec![OutEvent::Relayout];
        }
        values.push(value.to_string());
        let mut out = self.chips_write(id, values.clone());
        out.extend(self.fire(id, "add", Some(json!({"value": value}))));
        out.extend(self.fire(id, "change", Some(json!({"values": values}))));
        out.push(OutEvent::Relayout);
        out
    }

    fn chip_remove(&mut self, id: &str, value: &str) -> Vec<OutEvent> {
        let mut values = self.chip_values(id);
        values.retain(|v| v != value);
        let mut out = self.chips_write(id, values.clone());
        out.extend(self.fire(id, "remove", Some(json!({"value": value}))));
        out.extend(self.fire(id, "change", Some(json!({"values": values}))));
        out.push(OutEvent::Relayout);
        out
    }

    fn files_of(&self, id: &str) -> Vec<Value> {
        self.current_prop(id, "files").and_then(|v| v.as_array().cloned()).unwrap_or_default()
    }

    fn files_write(&mut self, id: &str, files: Vec<Value>) -> Vec<OutEvent> {
        let out = self.write_through(id, "files", Value::Array(files.clone()));
        if out.is_empty() {
            self.local.files.insert(id.to_string(), files);
        } else {
            self.local.files.remove(id);
        }
        self.needs_build = true;
        out
    }

    /// Picked or dropped files (`{files: [{name, size, type}]}`): too-large
    /// ones are refused (the field shows `fileTooLargeNamed`), the rest attached
    /// and announced to the host as `upload`.
    fn files_upload(&mut self, id: &str, payload: Option<Value>) -> Vec<OutEvent> {
        let incoming: Vec<Value> = payload.as_ref().and_then(|p| p.get("files")).and_then(Value::as_array).cloned().unwrap_or_default();
        let owner = self.node_by_id(id).cloned();
        let max = owner.as_ref().and_then(|o| o.props.get("maxSize")).and_then(Value::as_f64);
        let multiple = owner.as_ref().and_then(|o| o.props.get("multiple")).and_then(Value::as_bool).unwrap_or(false);
        let (ok, refused): (Vec<Value>, Vec<Value>) = incoming.into_iter().partition(|f| !max.is_some_and(|m| f.get("size").and_then(Value::as_f64).is_some_and(|s| s > m)));
        if !refused.is_empty() {
            let name = owner.as_ref().and_then(|o| o.props.get("name")).map(js).unwrap_or_default();
            let template = self.strings.get("fileTooLargeNamed").cloned().unwrap_or_default();
            let messages = refused
                .iter()
                .map(|f| {
                    let mut params = serde_json::Map::new();
                    params.insert("name".into(), Value::String(f.get("name").map(js).unwrap_or_default()));
                    crate::strings::format_string(&template, &params)
                })
                .collect();
            self.local.errors.insert(id.to_string(), FieldErrors { name, messages });
        } else {
            self.local.errors.remove(id);
        }
        let mut files = if multiple { self.files_of(id) } else { Vec::new() };
        let accepted: Vec<Value> = if multiple { ok } else { ok.into_iter().take(1).collect() };
        files.extend(accepted.iter().cloned());
        let mut out = self.files_write(id, files);
        if !accepted.is_empty() {
            out.extend(self.fire(id, "upload", Some(json!({"files": accepted}))));
        }
        out.push(OutEvent::Relayout);
        out
    }

    fn file_remove(&mut self, id: &str, i: usize) -> Vec<OutEvent> {
        let mut files = self.files_of(id);
        if i >= files.len() {
            return vec![];
        }
        let removed = files.remove(i);
        let mut out = self.files_write(id, files);
        out.extend(self.fire(id, "remove", Some(json!({"name": removed.get("name").cloned().unwrap_or(Value::Null)}))));
        out.push(OutEvent::Relayout);
        out
    }

    fn table_sort(&mut self, n: &LNode, id: &str) -> Vec<OutEvent> {
        if n.props.get("sortable").and_then(Value::as_bool) != Some(true) {
            return vec![];
        }
        let key = n.props.get("key").map(js).unwrap_or_default();
        let current = self.current_prop(id, "sort");
        let same = current.as_ref().and_then(|s| s.get("key")).map(js).as_deref() == Some(key.as_str());
        let dir = if same && current.as_ref().and_then(|s| s.get("direction")).and_then(Value::as_str) == Some("asc") { "desc" } else { "asc" };
        let sort = json!({"key": key, "direction": dir});
        let mut out = self.write_through(id, "sort", sort.clone());
        if out.is_empty() {
            self.local.sort.insert(id.to_string(), (key, dir.to_string()));
        }
        self.needs_build = true;
        out.extend(self.fire(id, "sort", Some(json!({"sort": sort}))));
        out.push(OutEvent::Relayout);
        out
    }

    fn table_selected(&self, id: &str) -> Vec<String> {
        self.current_prop(id, "selected").and_then(|v| v.as_array().map(|a| a.iter().map(js).collect())).unwrap_or_default()
    }

    fn table_select(&mut self, id: &str, selected: Vec<String>) -> Vec<OutEvent> {
        let value = Value::Array(selected.iter().cloned().map(Value::String).collect());
        let mut out = self.write_through(id, "selected", value.clone());
        if out.is_empty() {
            self.local.selected.insert(id.to_string(), selected);
        }
        self.needs_build = true;
        out.extend(self.fire(id, "select", Some(json!({"selected": value}))));
        out.push(OutEvent::Relayout);
        out
    }

    fn table_row(&mut self, n: &LNode, id: &str) -> Vec<OutEvent> {
        let key = n.props.get("key").map(js).unwrap_or_default();
        let row = self.node_by_id(id).and_then(|o| o.props.get("rows")).and_then(|r| r.get(n.props.get("index").and_then(Value::as_u64).unwrap_or(0) as usize)).cloned().unwrap_or(Value::Null);
        let mut out = self.fire(id, "rowPress", Some(json!({"key": key, "row": row})));
        let selectable = self.node_by_id(id).and_then(|o| o.props.get("selectable")).and_then(Value::as_str).unwrap_or("none").to_string();
        if selectable == "single" {
            let current = self.table_selected(id);
            let next = if current == [key.clone()] { vec![] } else { vec![key] };
            out.extend(self.table_select(id, next));
        }
        out
    }

    fn table_check(&mut self, n: &LNode, id: &str) -> Vec<OutEvent> {
        let mut selected = self.table_selected(id);
        if n.props.get("header").and_then(Value::as_bool) == Some(true) {
            let all: Vec<String> = self.nodes.iter().enumerate().filter(|(i, r)| self.live[*i] && r.owner.as_deref() == Some(id) && r.part.as_deref() == Some("row")).filter_map(|(_, r)| r.props.get("key").map(js)).collect();
            let rows = self.node_by_id(id).and_then(|o| o.props.get("rows")).and_then(Value::as_array).map(Vec::len).unwrap_or(0);
            let checked = n.props.get("checked").and_then(Value::as_bool).unwrap_or(false);
            selected = if checked {
                vec![]
            } else if all.len() == rows {
                all
            } else {
                let key = self.node_by_id(id).and_then(|o| o.props.get("rowKey")).map(js).unwrap_or_else(|| "id".into());
                self.node_by_id(id).and_then(|o| o.props.get("rows")).and_then(Value::as_array).map(|rs| rs.iter().enumerate().map(|(i, r)| r.get(&key).map(js).unwrap_or_else(|| i.to_string())).collect()).unwrap_or_default()
            };
        } else {
            let key = n.props.get("key").map(js).unwrap_or_default();
            if selected.contains(&key) {
                selected.retain(|k| *k != key);
            } else {
                selected.push(key);
            }
        }
        self.table_select(id, selected)
    }

    // ------------------------------------------------------------------
    // Resizable (round 2 §1)
    // ------------------------------------------------------------------

    /// A Resizable's sizes as shown, its limits, axis, rtl and slot.
    fn resizable_state(&self, owner_id: &str) -> Option<ResizableState> {
        let slot = *self.slot_of.get(owner_id)?;
        let owner = &self.nodes[slot as usize];
        let sizes: Vec<f64> = owner.props.get("sizes").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_f64).collect())?;
        let limits = crate::resizable::PanelLimits::list(owner.props.get("panels"));
        let orientation = if owner.props.get("direction").and_then(Value::as_str) == Some("vertical") { crate::resizable::Orientation::Vertical } else { crate::resizable::Orientation::Horizontal };
        let rtl = self.node_state.get(slot as usize).is_some_and(|s| s.direction == taffy::style::Direction::Rtl);
        Some((sizes, limits, orientation, rtl, slot))
    }

    /// A handle drag or key. `drag {phase: start|move|end, delta}` (`delta`
    /// = px on the main axis since the drag STARTED, screen direction):
    /// sizes follow from the START sizes (no drift), the end writes a bound
    /// `sizes` and fires `change {sizes}`. `key {key}` (Arrow*/Home/End/
    /// Enter, `keyboardResize`) writes and fires at once. Like the
    /// reference, nothing is written or fired when the sizes did not change
    /// (a click without movement, a key at a limit, any other key).
    fn resize_event(&mut self, n: &LNode, owner_id: &str, event: &str, payload: Option<&Value>) -> Vec<OutEvent> {
        let Some((sizes, limits, orientation, rtl, slot)) = self.resizable_state(owner_id) else { return vec![] };
        let handle = n.props.get("handle").and_then(Value::as_u64).unwrap_or(0) as usize;
        let hairline = self.effective.as_ref().and_then(|t| t.tokens.control.get("hairline").copied()).unwrap_or(1.0);
        let next = if event == "key" {
            let key = payload.and_then(|p| p.get("key")).and_then(Value::as_str).unwrap_or("");
            if !crate::resizable::RESIZE_KEYS.contains(&key) {
                return vec![];
            }
            let next = crate::resizable::keyboard_resize(&sizes, handle, key, orientation, rtl, &limits);
            if next == sizes {
                return vec![];
            }
            next
        } else {
            let phase = payload.and_then(|p| p.get("phase")).and_then(Value::as_str).unwrap_or("move");
            if phase == "start" {
                self.local.drags.insert(n.id.clone(), sizes);
                return vec![];
            }
            let start = self.local.drags.get(&n.id).cloned().unwrap_or(sizes.clone());
            let px = payload.and_then(|p| p.get("delta")).and_then(Value::as_f64).unwrap_or(0.0);
            let frame = self.last_frames.get(slot as usize).copied().unwrap_or_default();
            let container = if orientation == crate::resizable::Orientation::Vertical { frame.h } else { frame.w } as f64;
            let delta = crate::resizable::drag_delta(px, container, start.len(), orientation, rtl, hairline);
            let next = crate::resizable::resize_panels(&start, handle, delta, &limits);
            if phase != "end" {
                if next != sizes {
                    self.local.sizes.insert(owner_id.to_string(), next);
                    self.subtree_pending.insert(owner_id.to_string());
                    return vec![OutEvent::Relayout];
                }
                return vec![];
            }
            self.local.drags.remove(&n.id);
            if next == start {
                // No net movement: drop the live preview, commit nothing.
                if sizes != start {
                    if self.binding_path(owner_id, "sizes").is_some() {
                        self.local.sizes.remove(owner_id);
                    } else {
                        self.local.sizes.insert(owner_id.to_string(), start);
                    }
                    self.subtree_pending.insert(owner_id.to_string());
                    return vec![OutEvent::Relayout];
                }
                return vec![];
            }
            next
        };
        let value = json!(next);
        let mut out = self.write_through(owner_id, "sizes", value.clone());
        if out.is_empty() {
            self.local.sizes.insert(owner_id.to_string(), next);
        } else {
            self.local.sizes.remove(owner_id);
        }
        self.needs_build = true;
        out.extend(self.fire(owner_id, "change", Some(json!({"sizes": value}))));
        out.push(OutEvent::Relayout);
        out
    }

    // ------------------------------------------------------------------
    // Forms
    // ------------------------------------------------------------------

    /// Validation `checks` of a field evaluated against the data model: the
    /// messages of the failing ones. The reference's `failingChecks`: a check
    /// WITH a `condition` fails when it resolves to nothing (an unseeded
    /// path), `null`, `false` or `""` (0 passes); a message that is not a
    /// string reads `$string.invalidValue` (round 2).
    pub fn failing_checks(&self, id: &str) -> Vec<String> {
        let Some(node) = self.source_node(id) else { return vec![] };
        let scope = self.scope_of(id);
        let ctx = self.resolve_ctx(&scope);
        node.props
            .get("checks")
            .and_then(Value::as_array)
            .map(|checks| {
                checks
                    .iter()
                    .filter(|c| {
                        let Some(cond) = c.get("condition") else { return false };
                        check_fails(crate::data::resolve_value(cond, &ctx).as_ref())
                    })
                    .map(|c| c.get("message").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| self.strings.get("invalidValue").cloned().unwrap_or_else(|| "Invalid value".into())))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// A field's `validateOn` (its prop, else the catalog default, else blur).
    fn validate_on(&self, id: &str) -> String {
        let node = self.node_by_id(id);
        node.and_then(|n| n.props.get("validateOn"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| node.and_then(|n| self.view.component(&n.component)).and_then(|d| d.props.get("validateOn")).and_then(|p| p.default.as_ref()).and_then(Value::as_str).map(str::to_string))
            .unwrap_or_else(|| "blur".into())
    }

    /// Run one field's checks now (its errors show or clear).
    fn validate_field(&mut self, id: &str) {
        let failed = self.failing_checks(id);
        let name = self.node_by_id(id).and_then(|n| n.props.get("name")).map(js).unwrap_or_default();
        let before = self.local.errors.get(id).map(|e| e.messages.clone());
        if failed.is_empty() {
            self.local.errors.remove(id);
        } else {
            self.local.errors.insert(id.to_string(), FieldErrors { name, messages: failed.clone() });
        }
        if before.unwrap_or_default() != failed {
            self.needs_build = true;
        }
    }

    /// The value a field contributes to its Form's `submit`.
    fn field_value(&self, id: &str, component: &str) -> Value {
        if let Some(v) = self.local.field_values.get(id) {
            return v.clone();
        }
        let props = self.node_by_id(id).map(|n| n.props.clone()).unwrap_or_default();
        match component {
            "Checkbox" | "Switch" => props.get("checked").cloned().unwrap_or(Value::Bool(false)),
            "ChipInput" => props.get("values").cloned().unwrap_or(json!([])),
            "FileUpload" => props.get("files").cloned().unwrap_or(json!([])),
            "DateRangePicker" => json!({"start": props.get("start").cloned().unwrap_or(Value::Null), "end": props.get("end").cloned().unwrap_or(Value::Null)}),
            "TimePicker" => props.get("value").filter(|v| v.as_str().and_then(parse_time).is_some()).cloned().unwrap_or(Value::Null),
            _ => props.get("value").cloned().unwrap_or(Value::Null),
        }
    }

    /// Submit a Form: every field's checks run; any failure → no `submit`,
    /// `invalid {errors}`, every failing field shows its messages, focus
    /// moves to the first and `invalidFields` is announced. A busy form
    /// refuses.
    pub fn submit_form(&mut self, form_id: &str) -> Vec<OutEvent> {
        let Some(form) = self.source_node(form_id) else { return vec![] };
        if self.node_by_id(form_id).and_then(|n| n.props.get("busy")).and_then(Value::as_bool) == Some(true) {
            return vec![];
        }
        let mut fields: Vec<(String, String, String)> = Vec::new();
        fn walk(n: &UiNode, out: &mut Vec<(String, String, String)>, top: bool) {
            if !top && n.component == "Form" {
                return;
            }
            if FORM_FIELDS.contains(&n.component.as_str()) {
                if let Some(name) = n.props.get("name").and_then(Value::as_str) {
                    out.push((n.id.clone(), n.component.clone(), name.to_string()));
                }
            }
            for s in n.slots.iter().flat_map(|s| s.values()) {
                walk(s, out, false);
            }
            for c in &n.children {
                walk(c, out, false);
            }
        }
        walk(&form, &mut fields, true);
        // A Form inside a template row / slot cell: its fields are instances
        // with the same suffix as the Form.
        let suffix = self.instance_suffix(form_id);
        if !suffix.is_empty() {
            for f in &mut fields {
                f.0.push_str(&suffix);
            }
        }
        let mut errors = Vec::new();
        let mut first_invalid: Option<String> = None;
        for (id, _, name) in &fields {
            let failed = self.failing_checks(id);
            if failed.is_empty() {
                self.local.errors.remove(id);
                continue;
            }
            first_invalid.get_or_insert_with(|| id.clone());
            for m in &failed {
                errors.push(json!({"name": name, "message": m}));
            }
            self.local.errors.insert(id.clone(), FieldErrors { name: name.clone(), messages: failed });
        }
        self.needs_build = true;
        let mut out = vec![];
        if let Some(first) = first_invalid {
            out.extend(self.fire(form_id, "invalid", Some(json!({"errors": errors}))));
            let index = self.slot_of.get(&first).copied().unwrap_or(0);
            out.push(OutEvent::Focus { id: first, index });
            out.push(OutEvent::Announce { text: self.strings.get("invalidFields").cloned().unwrap_or_default(), live: "assertive".into() });
            out.push(OutEvent::Relayout);
            return out;
        }
        let mut values = Map::new();
        for (id, component, name) in &fields {
            values.insert(name.clone(), self.field_value(id, component));
        }
        out.extend(self.fire(form_id, "submit", Some(json!({"values": values}))));
        out.push(OutEvent::Relayout);
        out
    }

    // ------------------------------------------------------------------
    // Host commands
    // ------------------------------------------------------------------

    /// `focus {id}`, `announce {text, live}`, `scrollIntoView {id}`.
    pub fn command(&mut self, command: &SurfaceCommand) -> Vec<OutEvent> {
        match command {
            SurfaceCommand::Announce { text, live } => vec![OutEvent::Announce { text: text.clone(), live: live.clone().unwrap_or_else(|| "polite".into()) }],
            SurfaceCommand::Focus { id } => {
                let Some(&slot) = self.slot_of.get(id) else { return vec![] };
                // The first focusable node at or below it.
                let mut stack = vec![slot];
                while let Some(s) = stack.pop() {
                    let n = &self.nodes[s as usize];
                    if n.hidden {
                        continue;
                    }
                    if n.pressable || matches!(n.part.as_deref(), Some("field" | "input" | "trigger" | "search")) {
                        return vec![OutEvent::Focus { id: n.id.clone(), index: s }];
                    }
                    stack.extend(n.children.iter().rev());
                }
                vec![OutEvent::Focus { id: id.clone(), index: slot }]
            }
            SurfaceCommand::ScrollIntoView { id } => self.scroll_into_view(id),
            SurfaceCommand::ScrollToIndex { id, index, align } => self.scroll_to_index(id, *index as usize, align.as_deref().and_then(crate::list::ScrollAlign::parse).unwrap_or_default()),
        }
    }

    /// Round 2 (§5): bring item `index` (DATA order, before a local sort)
    /// of the List or Table `id` into view, `align`ed, the pinned section
    /// header subtracted. Its own, its scrolling ancestor's or the host's
    /// offset moves (the host's: [`OutEvent::ScrollSurface`]); a windowed
    /// list renders the item on the next pass.
    pub fn scroll_to_index(&mut self, id: &str, index: usize, align: crate::list::ScrollAlign) -> Vec<OutEvent> {
        use crate::layout_tree::ListViewSource;
        let Some(spec) = self.lists.iter().find(|l| l.id == id).cloned() else { return vec![] };
        let pos = match &spec.data_index {
            Some(map) => map.iter().position(|d| *d == Some(index)),
            None => Some(spec.static_count + index),
        };
        let Some(pos) = pos.filter(|p| *p < spec.keys.len()) else { return vec![] };
        // Over the window's cached offsets: a row's extent = the next
        // offset − the gap (O(1) past the cache).
        let Some(window) = self.local.lists.get_mut(id) else { return vec![] };
        window.gap = spec.gap;
        let offsets = window.offsets_cached(&spec.keys);
        let count = spec.keys.len();
        let extent = |r: usize| (offsets[r + 1] - offsets[r] - if r + 1 < count { spec.gap } else { 0.0 }) as f64;
        let h = spec.horizontal;
        let axis = |p: (f32, f32)| if h { p.0 } else { p.1 };
        let own = self.local.scroll.get(id).copied().map(axis).unwrap_or(0.0);
        let view = self.local.list_views.get(id).cloned();
        let (source, viewport) = match view {
            Some(v) => (v.source, v.viewport),
            None => (ListViewSource::Own, if h { self.viewport.0 } else if self.viewport.1 > 0.0 { self.viewport.1 } else { self.max_height.unwrap_or(2000.0) }),
        };
        let scroll = match &source {
            ListViewSource::Own => own,
            ListViewSource::Ancestor { id: a, rel } => self.local.scroll.get(a).copied().map(axis).unwrap_or(0.0) - rel,
            ListViewSource::Host { rel } => self.local.surface_scroll.map(|p| axis(p) - rel).unwrap_or(own),
        };
        // The pinned header over the item: its section's header extent.
        let inset = if spec.sticky {
            let before = spec.headers.partition_point(|&hp| hp < pos);
            before.checked_sub(1).map(|i| extent(spec.headers[i])).unwrap_or(0.0)
        } else {
            0.0
        };
        let target = crate::list::scroll_offset_at(offsets[pos] as f64, extent(pos), offsets[count] as f64, viewport as f64, scroll.max(0.0) as f64, align, inset) as f32;
        let with_axis = |p: (f32, f32), v: f32| if h { (v, p.1) } else { (p.0, v) };
        let mut out = vec![];
        match source {
            ListViewSource::Own => {
                let (x, y) = with_axis(self.local.scroll.get(id).copied().unwrap_or_default(), target);
                self.local.scroll.insert(id.to_string(), (x, y));
            }
            ListViewSource::Ancestor { id: a, rel } => {
                let (x, y) = with_axis(self.local.scroll.get(&a).copied().unwrap_or_default(), target + rel);
                self.local.scroll.insert(a.clone(), (x, y));
            }
            ListViewSource::Host { rel } => match self.local.surface_scroll {
                Some(p) => {
                    let (x, y) = with_axis(p, target + rel);
                    self.local.surface_scroll = Some((x, y));
                    out.push(OutEvent::ScrollSurface { x, y });
                }
                None => {
                    let (x, y) = with_axis(self.local.scroll.get(id).copied().unwrap_or_default(), target);
                    self.local.scroll.insert(id.to_string(), (x, y));
                }
            },
        }
        if spec.windowed {
            self.subtree_pending.insert(id.to_string());
        }
        out.push(OutEvent::Relayout);
        out
    }

    /// Scroll every scroll container above `id` so the node is in view (a
    /// windowed row that is not rendered scrolls by its key's offset).
    pub fn scroll_into_view(&mut self, id: &str) -> Vec<OutEvent> {
        let mut out = vec![];
        if let Some(&slot) = self.slot_of.get(id) {
            let target = self.last_frames.get(slot as usize).copied().unwrap_or_default();
            let mut cur = self.nodes[slot as usize].parent;
            while let Some(p) = cur {
                if let Some(s) = self.scrolls.iter().find(|s| s.index == p).copied() {
                    let frame = self.last_frames[p as usize];
                    let pid = self.scroll_key(p);
                    let (mut ox, mut oy) = (s.offset_x, s.offset_y);
                    let top = target.y - frame.y;
                    let left = target.x - frame.x;
                    if s.scroll_y {
                        if top < oy {
                            oy = top;
                        } else if top + target.h > oy + frame.h {
                            oy = top + target.h - frame.h;
                        }
                    }
                    if s.scroll_x {
                        if left < ox {
                            ox = left;
                        } else if left + target.w > ox + frame.w {
                            ox = left + target.w - frame.w;
                        }
                    }
                    if self.scroll_to(&pid, ox, oy) {
                        out.push(OutEvent::Relayout);
                    }
                }
                cur = self.nodes[p as usize].parent;
            }
            return out;
        }
        // A windowed row outside the window: scroll its list to it.
        for l in self.lists.clone() {
            if let Some(pos) = l.keys.iter().position(|k| k == id || id.ends_with(&format!(".{k}"))) {
                let y = l.offsets.get(pos).copied().unwrap_or(0.0);
                if self.scroll_to(&l.id, 0.0, y) {
                    out.push(OutEvent::Relayout);
                }
            }
        }
        out
    }
}

/// What a Resizable event reads: sizes, limits, orientation, rtl, slot.
type ResizableState = (Vec<f64>, Vec<crate::resizable::PanelLimits>, crate::resizable::Orientation, bool, u32);
