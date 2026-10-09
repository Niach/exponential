//! Rebuild = build the layout tree (the bind pass) + RECONCILE it onto the
//! stable slots by id: unchanged nodes keep their slot, style, visual and
//! measurement; the rest is reported as a `NodeDelta`.

use std::collections::{HashMap, HashSet};


use super::{Carried, NodeState, Surface};
use crate::layout_tree::{self, BuildContext, LNode, LayerPlacement, NodeKind};
use crate::types::UiNode;

/// Compact (renumber in pre-order) once this many slots are dead and they
/// outnumber the live ones.
const COMPACT_MIN_DEAD: usize = 64;

impl Surface {
    /// Rebuild the layout tree and reconcile it onto the slots.
    pub(super) fn rebuild(&mut self) {
        crate::roomy(|| self.rebuild_now())
    }

    fn rebuild_now(&mut self) {
        self.needs_build = false;
        self.rebuilt = true;
        // The tree is borrowed for the build, never cloned (a 2,000-row
        // list's rebuild costs its window, not its rows).
        let Some(root) = self.root.take() else {
            self.clear_slots();
            self.main_root = None;
            self.layers.clear();
            self.lists.clear();
            self.toasts.clear();
            return;
        };
        let breakpoint = self.active_breakpoint();
        let built = self.with_build_context(breakpoint.clone(), |ctx| layout_tree::build(&root, ctx));
        self.root = Some(root);
        self.note_build_issues(&built.issues);
        self.built_nodes += built.nodes.len() as u32;
        self.responsive = built.responsive;
        self.breakpoint = breakpoint;
        self.reconcile(built);
        let dead = self.live.iter().filter(|l| !**l).count();
        if dead >= COMPACT_MIN_DEAD && dead > self.live.len() - dead {
            self.compact();
        }
    }

    /// The build's refusals join the surface issues (once each).
    fn note_build_issues(&mut self, issues: &[crate::types::ReduceIssue]) {
        for issue in issues {
            if !self.issues.contains(issue) {
                self.issues.push(issue.clone());
            }
        }
    }

    /// Run `f` with the build context of this surface (templates reduced,
    /// gaps, breakpoint, strings, locale…).
    fn with_build_context<R>(&mut self, breakpoint: Option<String>, f: impl FnOnce(&mut BuildContext) -> R) -> R {
        let templates = std::mem::take(&mut self.template_cache);
        let (vw, vh) = self.viewport;
        let window_height = if vh > 0.0 { vh } else { self.max_height.unwrap_or(2000.0) };
        let gaps: HashMap<&str, f32> = ["none", "xxs", "xs", "sm", "md", "lg", "xl", "xl2", "xl3"].iter().map(|g| (*g, self.gap_px(g))).collect();
        let wide = self.breakpoints().get("md").is_some_and(|md| vw >= *md);
        let token = |name: &str, default: f32| self.effective.as_ref().and_then(|t| t.tokens.control.get(name).copied()).map(|v| v as f32).unwrap_or(default);
        let (hairline, row_extent) = (token("hairline", 1.0), token("row", crate::list::DEFAULT_ESTIMATED_ITEM_HEIGHT));
        let expand = self.expand_controls.unwrap_or(self.theme.is_some());
        let out = {
            let template = |id: &str| -> Option<UiNode> { templates.get(id).cloned().flatten() };
            let gap = |name: &str| -> f32 { gaps.get(name).copied().unwrap_or(0.0) };
            let mut ctx = BuildContext {
                view: &self.view,
                recipes: &self.recipes,
                data: &self.data,
                local: &mut self.local,
                template: &template,
                viewport_height: window_height,
                viewport_width: if vw > 0.0 { vw } else { 2000.0 },
                hairline,
                row_extent,
                gap_px: &gap,
                expand_controls: expand,
                strings: &self.strings,
                breakpoint,
                locale: &self.settings.locale,
                wide,
                today: self.settings.today.clone(),
                formatter: &*self.formatter,
                now: self.clock.unwrap_or_else(crate::format::now_ms),
                data_version: self.data_version,
            };
            f(&mut ctx)
        };
        self.template_cache = templates;
        out
    }

    /// Rebuild ONE subtree: a scrolled List/Table re-windows, a Tabs /
    /// Accordion / Carousel / Toggle / CodeBlock whose LOCAL state changed
    /// re-renders; only that subtree is built and reconciled (the rest of
    /// the surface untouched). `false` when that is not possible (an overlay
    /// or a toast lives inside, the node is the root or a template item):
    /// the caller rebuilds fully.
    pub(super) fn rebuild_subtree(&mut self, list_id: &str) -> bool {
        crate::roomy(|| self.rebuild_subtree_now(list_id))
    }

    fn rebuild_subtree_now(&mut self, list_id: &str) -> bool {
        let Some(&slot) = self.slot_of.get(list_id) else { return false };
        let Some(parent) = self.nodes[slot as usize].parent else { return false };
        let old = self.subtree(slot);
        let old_ids: HashSet<&str> = old.iter().map(|s| self.nodes[*s as usize].id.as_str()).collect();
        if self.layers.iter().any(|l| old_ids.contains(l.owner.as_str())) || self.toasts.iter().any(|t| old_ids.contains(t.id.as_str())) {
            return false;
        }
        let Some(root) = self.root.take() else { return false };
        let Some(node) = find_ui(&root, list_id) else {
            self.root = Some(root);
            return false;
        };
        let parent_node = self.nodes[parent as usize].clone();
        let scope = self.nodes[slot as usize].scope.clone();
        let breakpoint = self.breakpoint.clone();
        let seed = std::mem::take(&mut self.seed);
        let mut built = self.with_build_context(breakpoint, |ctx| layout_tree::build_subtree(node, &parent_node, &scope, seed, ctx));
        self.root = Some(root);
        self.note_build_issues(&built.issues);
        self.seed = layout_tree::BuildSeed { row_scopes: std::mem::take(&mut built.row_scopes), forms: std::mem::take(&mut built.forms) };
        if !built.layers.is_empty() || !built.toasts.is_empty() {
            return false;
        }
        self.rebuilt = true;
        self.built_nodes += built.nodes.len() as u32 - 1;
        let old_lists: HashSet<u32> = old.iter().copied().collect();
        let (map, structure_changed) = self.reconcile_core(built.nodes, Some(parent), old);
        self.lists.retain(|l| !old_lists.contains(&l.node));
        self.lists.extend(built.lists.into_iter().map(|mut l| {
            l.node = map[l.node as usize];
            l
        }));
        if structure_changed {
            self.structure_version += 1;
        }
        true
    }

    /// A slot and every descendant.
    fn subtree(&self, slot: u32) -> Vec<u32> {
        let mut out = vec![slot];
        let mut i = 0;
        while i < out.len() {
            out.extend(self.nodes[out[i] as usize].children.iter().copied());
            i += 1;
        }
        out
    }

    /// Forget every slot (a new surface, a compaction).
    fn clear_slots(&mut self) {
        self.archive_all();
        if !self.nodes.is_empty() {
            self.renumbered = true;
            self.structure_version += 1;
        }
        self.nodes.clear();
        self.live.clear();
        self.slot_of.clear();
        self.free.clear();
        self.node_state.clear();
        self.engine.clear();
        self.memo.clear();
        self.memo_versions.clear();
        self.style_dirty.clear();
        self.visuals_dirty.clear();
        self.delta_added.clear();
        self.delta_removed.clear();
        self.delta_changed.clear();
    }

    /// Renumber in pre-order: every index moves (`NodeDelta::renumbered`).
    fn compact(&mut self) {
        self.clear_slots();
        self.restyle_all = true;
        if self.root.is_some() {
            self.rebuild();
        }
    }

    /// Every memo entry into the id-keyed archive.
    fn archive_all(&mut self) {
        for i in 0..self.nodes.len() {
            if self.live.get(i).copied().unwrap_or(false) {
                self.archive_slot(i as u32);
            }
        }
    }

    fn archive_slot(&mut self, slot: u32) {
        let Some(intr) = self.memo.intrinsics.get(&slot).copied() else { return };
        let heights: Vec<(i64, f32)> = self.memo.heights.iter().filter(|((i, _), _)| *i == slot).map(|((_, w), h)| (*w, *h)).collect();
        let n = &self.nodes[slot as usize];
        self.archive.insert(n.id.clone(), Carried { intrinsics: intr, heights, props: n.props.clone(), lines: n.lines });
    }

    fn alloc(&mut self, reusable: &mut Vec<u32>) -> u32 {
        if let Some(slot) = reusable.pop() {
            return slot;
        }
        let slot = self.nodes.len() as u32;
        self.nodes.push(LNode {
            index: slot,
            id: String::new(),
            component: "Box".into(),
            part: None,
            owner: None,
            owner_component: None,
            catalog_id: None,
            extension_kind: None,
            depth: 0,
            parent: None,
            children: Vec::new(),
            layer: 0,
            kind: NodeKind::Container,
            props: Default::default(),
            base_style: Default::default(),
            default_keys: Vec::new(),
            own_query: None,
            part_query: None,
            hidden: true,
            lines: None,
            pressable: false,
            scope: String::new(),
            source: None,
            on: None,
            accessibility: None,
            trigger_for: None,
            form: None,
            live: None,
        });
        self.live.push(false);
        self.node_state.push(NodeState::default());
        self.engine.resize(self.nodes.len());
        slot
    }

    fn kill(&mut self, slot: u32) {
        self.archive_slot(slot);
        let id = self.nodes[slot as usize].id.clone();
        if self.slot_of.get(&id) == Some(&slot) {
            self.slot_of.remove(&id);
            // A node that left the tree keeps no hover/focus (a closed hover
            // card's content must not hold it open when it opens again).
            self.states.remove(&id);
        }
        self.live[slot as usize] = false;
        self.engine.set_children(slot, &[]);
        self.memo.forget(slot);
        self.memo_versions.remove(&slot);
        self.node_state[slot as usize] = NodeState::default();
        self.style_dirty.remove(&slot);
        self.visuals_dirty.remove(&slot);
        self.delta_changed.remove(&slot);
        if !self.delta_added.remove(&slot) {
            self.delta_removed.insert(slot);
        }
        self.free.push(slot);
    }

    fn reconcile(&mut self, mut built: layout_tree::Built) {
        self.seed = layout_tree::BuildSeed { row_scopes: std::mem::take(&mut built.row_scopes), forms: std::mem::take(&mut built.forms) };
        let candidates: Vec<u32> = (0..self.nodes.len() as u32).filter(|s| self.live[*s as usize]).collect();
        let (map, structure_changed) = self.reconcile_core(built.nodes, None, candidates);
        self.main_root = map.first().copied();
        self.layers = built
            .layers
            .into_iter()
            .map(|mut l| {
                l.root = map[l.root as usize];
                l.scroll_body = l.scroll_body.map(|b| map[b as usize]);
                if let LayerPlacement::Anchored { anchor, .. } = &mut l.placement {
                    *anchor = map[*anchor as usize];
                }
                l
            })
            .collect();
        self.lists = built
            .lists
            .into_iter()
            .map(|mut l| {
                l.node = map[l.node as usize];
                l
            })
            .collect();
        // A toast that just opened speaks (assertive for errors); it never
        // takes focus.
        for t in &built.toasts {
            if !self.toasts.iter().any(|old| old.id == t.id) {
                if let Some(n) = self.node_by_id(&t.id) {
                    // Round 2: non-string values show as display strings.
                    let title = crate::format::display_opt(n.props.get("title"));
                    let text = match n.props.get("description").filter(|d| !d.is_null()) {
                        Some(d) => format!("{title}. {}", crate::format::display_string(d)),
                        None => title,
                    };
                    let live = if t.kind == "error" { "assertive" } else { "polite" };
                    self.pending.push(super::OutEvent::Announce { text, live: live.into() });
                }
            }
        }
        self.toasts = built.toasts;
        if structure_changed {
            self.structure_version += 1;
        }
    }

    /// Map built nodes (pre-order, temp indices) onto slots by id, write
    /// them, kill the `candidates` nobody claimed. `pinned` = temp node 0 is
    /// an existing slot (the parent of a rebuilt subtree) that stays as it is.
    fn reconcile_core(&mut self, nodes: Vec<LNode>, pinned: Option<u32>, candidates: Vec<u32>) -> (Vec<u32>, bool) {
        let mut reusable = std::mem::take(&mut self.free);
        // Lowest slots first (pop from the end).
        reusable.sort_unstable_by(|a, b| b.cmp(a));
        let mut claimed: HashSet<u32> = HashSet::with_capacity(nodes.len());
        let mut map: Vec<u32> = Vec::with_capacity(nodes.len());
        let reclaimable: HashSet<u32> = if pinned.is_some() { candidates.iter().copied().collect() } else { HashSet::new() };
        for (t, n) in nodes.iter().enumerate() {
            let slot = match (t, pinned) {
                (0, Some(p)) => p,
                _ => match self.slot_of.get(&n.id).copied() {
                    Some(s) if !claimed.contains(&s) && self.live[s as usize] && (pinned.is_none() || reclaimable.contains(&s)) => s,
                    _ => self.alloc(&mut reusable),
                },
            };
            claimed.insert(slot);
            map.push(slot);
        }
        let mut structure_changed = false;
        let stale: Vec<u32> = candidates.into_iter().filter(|s| self.live[*s as usize] && !claimed.contains(s)).collect();
        for s in stale {
            self.kill(s);
            structure_changed = true;
        }
        // Slots freed now are reusable from the NEXT rebuild on (a delta
        // never names one slot as removed and added).
        let freed_now = std::mem::take(&mut self.free);
        self.free = reusable;
        self.free.extend(freed_now);

        for (t, mut n) in nodes.into_iter().enumerate() {
            if t == 0 && pinned.is_some() {
                continue;
            }
            let slot = map[t];
            n.index = slot;
            n.parent = n.parent.map(|p| map[p as usize]);
            n.children = n.children.iter().map(|c| map[*c as usize]).collect();
            let existing = self.live[slot as usize] && self.nodes[slot as usize].id == n.id;
            if existing {
                let old = &self.nodes[slot as usize];
                let static_changed = old.props != n.props
                    || old.lines != n.lines
                    || old.pressable != n.pressable
                    || old.hidden != n.hidden
                    || old.accessibility != n.accessibility
                    || old.trigger_for != n.trigger_for
                    || old.component != n.component
                    || old.part != n.part
                    || old.kind != n.kind
                    || old.layer != n.layer
                    || old.parent != n.parent
                    || old.children != n.children
                    || old.form != n.form
                    || old.live != n.live
                    || old.part_query.as_ref().map(|q| &q.states) != n.part_query.as_ref().map(|q| &q.states)
                    || old.own_query.as_ref().map(|q| &q.states) != n.own_query.as_ref().map(|q| &q.states);
                let style_changed = old.base_style != n.base_style || old.default_keys != n.default_keys || old.own_query != n.own_query || old.part_query != n.part_query || old.hidden != n.hidden || old.kind != n.kind || old.component != n.component;
                let measure_changed = old.props != n.props || old.lines != n.lines || old.component != n.component || old.kind != n.kind || old.part != n.part;
                if old.parent != n.parent || old.children != n.children || old.layer != n.layer {
                    structure_changed = true;
                }
                if live_text_changed(old, &n) {
                    let text = crate::format::display_opt(n.props.get("text"));
                    let live = n.live.clone().unwrap_or_else(|| "polite".into());
                    self.pending.push(super::OutEvent::Announce { text, live });
                }
                if static_changed && !self.delta_added.contains(&slot) {
                    self.delta_changed.insert(slot);
                }
                if style_changed {
                    self.style_dirty.insert(slot);
                }
                if measure_changed {
                    self.node_state[slot as usize].content_version += 1;
                    self.memo.forget(slot);
                    self.memo_versions.remove(&slot);
                    self.engine.mark_dirty(slot);
                }
            } else {
                structure_changed = true;
                self.delta_removed.remove(&slot);
                self.delta_changed.remove(&slot);
                self.delta_added.insert(slot);
                self.style_dirty.insert(slot);
                self.node_state[slot as usize] = NodeState::default();
                self.memo.forget(slot);
                self.memo_versions.remove(&slot);
                if let Some(c) = self.archive.get(&n.id) {
                    if c.props == n.props && c.lines == n.lines {
                        self.memo.intrinsics.insert(slot, c.intrinsics);
                        for (w, h) in &c.heights {
                            self.memo.heights.insert((slot, *w), *h);
                        }
                        self.memo_versions.insert(slot, 0);
                    }
                }
            }
            self.engine.set_measured(slot, n.kind == NodeKind::Leaf && !n.hidden);
            self.slot_of.insert(n.id.clone(), slot);
            self.live[slot as usize] = true;
            self.nodes[slot as usize] = n;
        }
        for (t, slot) in map.iter().enumerate() {
            if t == 0 && pinned.is_some() {
                continue;
            }
            let children = self.nodes[*slot as usize].children.clone();
            self.engine.set_children(*slot, &children);
        }
        (map, structure_changed)
    }
}

/// The node with `id` in a normalized tree (slots first, like `walk`).
fn find_ui<'a>(node: &'a UiNode, id: &str) -> Option<&'a UiNode> {
    if node.id == id {
        return Some(node);
    }
    node.slots.iter().flat_map(|s| s.values()).chain(node.children.iter()).find_map(|c| find_ui(c, id))
}

/// A live region whose text changed (an announcement for screen readers).
fn live_text_changed(old: &LNode, new: &LNode) -> bool {
    new.live.is_some() && new.component == "Text" && old.props.get("text") != new.props.get("text") && !new.hidden
}
