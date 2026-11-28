//! #PERF-15: an `updateComponents` that only re-sends components IN PLACE
//! (same component, catalog, children, slots and `visible`: their own props,
//! style, actions changed) re-reduces just those components' subtrees and
//! splices them into the reduced tree, then rebuilds only those subtrees of
//! the layout tree ([`Surface::rebuild_subtree`]); reconcile + the engine's
//! dirty marks then restyle, remeasure and relayout only what changed.
//!
//! The splice is taken only where it is EXACTLY what the full reduce and
//! build would produce, else the update falls back to them:
//! - the surface has no issues (no id is missing, cyclic, placed twice or
//!   over a limit, so a subtree reduces the same on its own) and the new
//!   subtree reduces without one;
//! - every ancestor in the reduced tree is a plain `Box` (macro parts
//!   included, unless their macro hands props down with `$pass`) built from
//!   a non-basic component: nothing above reads the subtree (a Form's
//!   fields, a Tabs' panels, a List's rows, a basic component's children)
//!   or builds it in a special way (hidden panels, row wrappers);
//! - the subtree holds no data template and expands to as many nodes as
//!   before (the component budget is unchanged);
//! - Row tree guides: the sibling group is refilled (a guide is its
//!   siblings').
//!
//! `EUI_CHECK_INCREMENTAL=1` (debug builds): every splice is compared with
//! the full reduce, every subtree rebuild with the full build; a mismatch
//! panics (the test suites run with it).

use std::collections::HashMap;

use super::Surface;
use crate::layout_tree::LNode;
use crate::reducer::{reduce_subtree, ReduceOptions};
use crate::types::{FlatComponent, UiNode};

/// One step from a node to a child: a `children` index or a slot name.
#[derive(Debug, Clone)]
enum Step {
    Child(usize),
    Slot(String),
}

fn path_to(node: &UiNode, id: &str, path: &mut Vec<Step>) -> bool {
    if node.id == id {
        return true;
    }
    for (name, slot) in node.slots.iter().flatten() {
        path.push(Step::Slot(name.clone()));
        if path_to(slot, id, path) {
            return true;
        }
        path.pop();
    }
    for (i, child) in node.children.iter().enumerate() {
        path.push(Step::Child(i));
        if path_to(child, id, path) {
            return true;
        }
        path.pop();
    }
    false
}

fn step<'a>(node: &'a UiNode, s: &Step) -> Option<&'a UiNode> {
    match s {
        Step::Child(i) => node.children.get(*i),
        Step::Slot(name) => node.slots.as_ref()?.get(name),
    }
}

fn step_mut<'a>(node: &'a mut UiNode, s: &Step) -> Option<&'a mut UiNode> {
    match s {
        Step::Child(i) => node.children.get_mut(*i),
        Step::Slot(name) => node.slots.as_mut()?.get_mut(name),
    }
}

fn count(node: &UiNode) -> usize {
    let mut n = 0;
    node.walk(&mut |_| n += 1);
    n
}

/// The parts of a component the reduced tree's SHAPE depends on.
fn same_shape(old: &FlatComponent, new: &FlatComponent) -> bool {
    old.component == new.component && old.catalog_id == new.catalog_id && old.children == new.children && old.slots == new.slots && old.visible == new.visible
}

pub(super) fn checking() -> bool {
    cfg!(debug_assertions) && std::env::var_os("EUI_CHECK_INCREMENTAL").is_some()
}

impl Surface {
    /// The path to `id` in the reduced tree when its subtree may be reduced
    /// and built on its own: every ancestor a plain Box (its own or a macro
    /// part) of a non-basic component, no macro handing props down.
    fn unit_path<'r>(&self, root: &'r UiNode, id: &str) -> Option<(Vec<Step>, &'r UiNode)> {
        let mut path = Vec::new();
        if !path_to(root, id, &mut path) {
            return None;
        }
        let mut node = root;
        for s in &path {
            if node.component != "Box" {
                return None;
            }
            if let Some(r) = &node.recipe {
                if self.view.macros.get(&r.macro_).is_none_or(|m| !m.pass.is_empty()) {
                    return None;
                }
            }
            if let Some(&k) = self.component_index.get(&node.id) {
                let catalog = self.components[k].catalog_id.as_deref().unwrap_or(&self.catalog_id);
                if crate::functions::is_basic_catalog(catalog) {
                    return None;
                }
            }
            node = step(node, s)?;
        }
        Some((path, node))
    }

    /// A data write at `path` landed: rebuild the subtrees whose build read
    /// it (the binding dependents), else everything.
    pub(super) fn data_written(&mut self, path: &str) {
        self.data_version += 1;
        if self.needs_build || self.reduce_pending || !self.deps_ready || !self.build_issues.is_empty() {
            self.needs_build = true;
            return;
        }
        match self.data_units(path) {
            Some(units) => {
                if !units.is_empty() {
                    self.subtree_pending.extend(units);
                    self.in_place_pending = true;
                }
            }
            None => self.needs_build = true,
        }
    }

    /// The nearest rebuildable subtree (`unit_path`) of every node that read
    /// `path` or a pointer under or above it; `None` = rebuild everything (a
    /// read outside any node, the whole model, no unit up to the root).
    fn data_units(&self, path: &str) -> Option<Vec<String>> {
        let path = path.trim_end_matches('/');
        if path.is_empty() {
            return None;
        }
        let root = self.root.as_ref()?;
        let overlaps = |read: &str| {
            let read = read.trim_end_matches('/');
            read.is_empty() || read == path || read.strip_prefix(path).is_some_and(|r| r.starts_with('/')) || path.strip_prefix(read).is_some_and(|r| r.starts_with('/'))
        };
        let mut units: Vec<String> = Vec::new();
        for (owner, reads) in &self.data_deps {
            if !reads.iter().any(|r| overlaps(r)) {
                continue;
            }
            if owner.is_empty() {
                return None;
            }
            // A reader no longer in the tree reads nothing.
            let Some(&slot) = self.slot_of.get(owner) else { continue };
            if !self.live[slot as usize] || self.nodes[slot as usize].id != *owner {
                continue;
            }
            let mut s = slot;
            loop {
                let n = &self.nodes[s as usize];
                if n.source.is_none() && self.unit_path(root, &n.id).is_some() {
                    units.push(n.id.clone());
                    break;
                }
                s = n.parent?;
            }
        }
        units.sort_unstable();
        units.dedup();
        Some(units)
    }

    pub(super) fn reduce_options(&self) -> ReduceOptions {
        ReduceOptions { streaming: self.streaming, ..ReduceOptions::new(&self.catalog_id).with_view(self.view.clone()) }
    }

    /// Re-reduce the updated components in place (`replaced` = each id's
    /// previous version, `None` = new). `true` = done: the reduced tree is
    /// current and the ids' subtrees are queued for a subtree rebuild;
    /// `false` = nothing changed, reduce and build fully.
    pub(super) fn reduce_in_place(&mut self, replaced: &HashMap<String, Option<FlatComponent>>, order: &[String]) -> bool {
        if self.reduce_pending || self.needs_build || self.nested.is_some() || !self.issues.is_empty() || crate::functions::is_basic_catalog(&self.catalog_id) {
            return false;
        }
        let Some(mut root) = self.root.take() else { return false };
        let options = self.reduce_options();
        let ok = order.iter().all(|id| self.splice(&mut root, id, replaced.get(id).and_then(Option::as_ref), &options));
        if !ok {
            // The full reduce redoes every splice from `components`.
            self.root = Some(root);
            return false;
        }
        if checking() {
            let full = crate::reducer::reduce_surface(&self.components, &options);
            assert!(full.issues.is_empty(), "#PERF-15 in-place reduce: the full reduce has issues {:?}", full.issues);
            assert!(full.templates.as_ref().is_none_or(|t| t.iter().all(|(id, n)| self.template_cache.get(id).and_then(Option::as_ref) == Some(n))), "#PERF-15 in-place reduce: templates differ");
            assert_eq!(full.root, root, "#PERF-15 in-place reduce differs from the full reduce after updating {order:?}");
        }
        self.root = Some(root);
        self.subtree_pending.extend(order.iter().cloned());
        self.in_place_pending = true;
        true
    }

    fn splice(&self, root: &mut UiNode, id: &str, old: Option<&FlatComponent>, options: &ReduceOptions) -> bool {
        let Some(old) = old else { return false };
        let Some(&i) = self.component_index.get(id) else { return false };
        let new = &self.components[i];
        if !same_shape(old, new) {
            return false;
        }
        let Some((path, node)) = self.unit_path(root, id) else { return false };
        let old_count = count(node);
        let (components, index) = (&self.components, &self.component_index);
        let lookup = |id: &str| index.get(id).map(|&k| &components[k]);
        let Some(fresh) = reduce_subtree(id, path.len() + 1, &lookup, options) else { return false };
        if count(&fresh) != old_count || fresh.id != id {
            return false;
        }
        // Splice, then refill the guides of the node's sibling group.
        let Some((last, above)) = path.split_last() else {
            *root = fresh;
            crate::tree_guides::refill_siblings(std::slice::from_mut(root));
            return true;
        };
        let mut parent = &mut *root;
        for s in above {
            let Some(next) = step_mut(parent, s) else { return false };
            parent = next;
        }
        match last {
            Step::Child(k) => {
                parent.children[*k] = fresh;
                crate::tree_guides::refill_siblings(&mut parent.children);
            }
            Step::Slot(name) => {
                let Some(slot) = parent.slots.as_mut().and_then(|s| s.get_mut(name)) else { return false };
                *slot = fresh;
                crate::tree_guides::refill_siblings(std::slice::from_mut(slot));
            }
        }
        true
    }

    /// The pending build: the whole tree, else the queued subtrees (a
    /// subtree that cannot rebuild on its own rebuilds everything).
    pub(super) fn build_pending(&mut self) {
        if self.needs_build {
            self.subtree_pending.clear();
            self.in_place_pending = false;
            self.rebuild();
            return;
        }
        for id in std::mem::take(&mut self.subtree_pending) {
            if !self.rebuild_subtree(&id) {
                self.in_place_pending = false;
                self.rebuild();
                return;
            }
        }
        if std::mem::take(&mut self.in_place_pending) && checking() {
            self.check_against_full_build();
        }
    }

    /// `EUI_CHECK_INCREMENTAL`: the live slots against a full build.
    fn check_against_full_build(&mut self) {
        if !self.ghosts.is_empty() {
            return;
        }
        let Some(root) = self.root.take() else { return };
        let breakpoint = self.breakpoint.clone();
        let built = self.with_build_context(breakpoint, |ctx| crate::layout_tree::build(&root, ctx));
        self.root = Some(root);
        let norm = |n: &LNode, ids: &dyn Fn(u32) -> String| {
            let mut m = n.clone();
            m.index = 0;
            m.parent = None;
            m.children.clear();
            (m, n.parent.map(ids), n.children.iter().map(|c| ids(*c)).collect::<Vec<_>>())
        };
        let live = self.live.iter().filter(|l| **l).count();
        assert_eq!(live, built.nodes.len(), "#PERF-15 subtree rebuild: {live} live nodes, the full build has {}", built.nodes.len());
        for n in &built.nodes {
            let Some(&slot) = self.slot_of.get(&n.id) else { panic!("#PERF-15 subtree rebuild: {} missing", n.id) };
            let full = norm(n, &|t| built.nodes[t as usize].id.clone());
            let mine = norm(&self.nodes[slot as usize], &|s| self.nodes[s as usize].id.clone());
            assert_eq!(mine, full, "#PERF-15 subtree rebuild differs from the full build at {}", n.id);
        }
    }
}
