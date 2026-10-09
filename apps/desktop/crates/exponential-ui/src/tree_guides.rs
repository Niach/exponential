//! Round 3 (VAPP-102, `docs/round-3-contract.md` §2): the tree connector rule
//! the CORE computes for nested rows, mirroring
//! `packages/exponential-ui/src/tree-guides.ts` line by line. `tree_guides`
//! is the ×4 rule of the app (`packages/ui/src/tree-guides.ts`, desktop
//! `domain::tree_guides`, iOS `TreeGuides.swift`, Android `TreeGuides.kt`),
//! copied verbatim so a Section of Rows connects like a session tree or a PR
//! stack. `apply_tree_guides` is the ONE post-expansion pass (the end of
//! [`crate::macros::expand_macros`]): over every node's children in order,
//! each Row root contributes its numeric `recipe.props.depth` (anything else
//! counts as 0 and ends every subtree), and the rule fills the Row's `guides`
//! part (a TreeGuides native) with `elbowAt` / `tee` / `passThrough`. Locked
//! by `fixtures/tree-guides.json` and the Row cases of `catalog-macros.json`.

use serde_json::Value;

use crate::types::{Props, UiNode};

/// Guide geometry for ONE row, in gutter LEVELS (not pixels).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TreeGuide {
    /// The gutter level the elbow sits in (`depth - 1`); `None` for a root.
    pub elbow_at: Option<usize>,
    /// The elbow's vertical continues past the centre: a later sibling follows.
    pub tee: bool,
    /// Ancestor gutter levels carrying a straight full-height line, ascending.
    pub pass_through: Vec<usize>,
}

/// Whether the subtree hanging off gutter `level` continues AFTER row
/// `index`: another row at depth `level + 1` follows before the walk leaves
/// that subtree (a row at depth `≤ level`).
fn continues_after(depths: &[usize], index: usize, level: usize) -> bool {
    for &depth in &depths[index + 1..] {
        if depth <= level {
            return false;
        }
        if depth == level + 1 {
            return true;
        }
    }
    false
}

/// The guides for a flat list of VISIBLE rows, given their depths.
pub fn tree_guides(depths: &[usize]) -> Vec<TreeGuide> {
    depths
        .iter()
        .enumerate()
        .map(|(index, &depth)| {
            if depth == 0 {
                return TreeGuide::default();
            }
            let pass_through = (0..depth - 1).filter(|&level| continues_after(depths, index, level)).collect();
            TreeGuide { elbow_at: Some(depth - 1), tee: continues_after(depths, index, depth - 1), pass_through }
        })
        .collect()
}

/// The expanded root of a `Row` macro.
pub fn is_row_root(node: &UiNode) -> bool {
    node.recipe.as_ref().is_some_and(|r| r.macro_ == "Row" && r.part == "root")
}

/// A Row root's depth: its recipe prop when a finite number > 0 (floored),
/// else 0 (a bound depth cannot be computed against its siblings).
fn row_depth(node: &UiNode) -> usize {
    let depth = node.recipe.as_ref().and_then(|r| r.props.get("depth")).and_then(Value::as_f64);
    match depth {
        Some(d) if d.is_finite() && d > 0.0 => d.floor() as usize,
        _ => 0,
    }
}

/// The Row's `guides` part, when its depth emitted one.
fn guides_part(row: &mut UiNode) -> Option<&mut UiNode> {
    row.children
        .iter_mut()
        .find(|c| c.component == "TreeGuides" && c.recipe.as_ref().is_some_and(|r| r.part == "guides"))
}

/// Fill every Row's guides from the rows around it. Mutates `root` in place
/// (the expander hands it a fresh tree).
pub fn apply_tree_guides(root: &mut UiNode) {
    let depths: Vec<usize> = root.children.iter().map(|c| if is_row_root(c) { row_depth(c) } else { 0 }).collect();
    let guides = tree_guides(&depths);
    for (i, child) in root.children.iter_mut().enumerate() {
        if !is_row_root(child) {
            continue;
        }
        let Some(part) = guides_part(child) else { continue };
        let g = &guides[i];
        let mut props = Props::new();
        props.insert("depth".into(), Value::from(depths[i] as u64));
        if let Some(elbow) = g.elbow_at {
            props.insert("elbowAt".into(), Value::from(elbow as u64));
        }
        props.insert("tee".into(), Value::Bool(g.tee));
        props.insert("passThrough".into(), Value::Array(g.pass_through.iter().map(|&l| Value::from(l as u64)).collect()));
        part.props = props;
    }
    if let Some(slots) = root.slots.as_mut() {
        for slot in slots.values_mut() {
            apply_tree_guides(slot);
        }
    }
    for child in &mut root.children {
        apply_tree_guides(child);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sibling_after_the_subtree_tees_and_passes_through() {
        let g = tree_guides(&[0, 1, 2, 1]);
        assert_eq!(g[0], TreeGuide::default());
        assert_eq!(g[1], TreeGuide { elbow_at: Some(0), tee: true, pass_through: vec![] });
        assert_eq!(g[2], TreeGuide { elbow_at: Some(1), tee: false, pass_through: vec![0] });
        assert_eq!(g[3], TreeGuide { elbow_at: Some(0), tee: false, pass_through: vec![] });
    }
}
