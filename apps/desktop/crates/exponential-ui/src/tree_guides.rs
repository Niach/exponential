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
/// A sibling's depth: a Row root's recipe prop, else any node's own numeric
/// `depth` prop (an extension row such as SessionRow or PrRow), when a finite
/// number > 0, else 0 (a bound depth cannot be computed against its siblings).
fn sibling_depth(node: &UiNode) -> usize {
    let depth = if is_row_root(node) { node.recipe.as_ref().and_then(|r| r.props.get("depth")) } else { node.props.get("depth") }.and_then(Value::as_f64);
    match depth {
        Some(d) if d.is_finite() && d > 0.0 => d.floor() as usize,
        _ => 0,
    }
}

/// A non-Row sibling that takes part: its own numeric `depth` prop.
fn has_depth(node: &UiNode) -> bool {
    !is_row_root(node) && node.component != "TreeGuides" && node.props.get("depth").is_some_and(Value::is_number)
}

/// The filled guide as props: `{depth, elbowAt?, tee, passThrough}` (TS `guideProps`).
fn guide_props(depth: usize, g: &TreeGuide) -> Props {
    let mut props = Props::new();
    props.insert("depth".into(), Value::from(depth as u64));
    if let Some(elbow) = g.elbow_at {
        props.insert("elbowAt".into(), Value::from(elbow as u64));
    }
    props.insert("tee".into(), Value::Bool(g.tee));
    props.insert("passThrough".into(), Value::Array(g.pass_through.iter().map(|&l| Value::from(l as u64)).collect()));
    props
}

/// Fill the guides of ONE sibling list (TS `fillSiblings`): a Row root's
/// `guides` part takes the props; any other node with a numeric `depth` gets
/// them (minus `depth`) as its `guide` prop for an extension painter to draw.
fn fill_siblings(nodes: &mut [UiNode]) {
    let depths: Vec<usize> = nodes.iter().map(sibling_depth).collect();
    let guides = tree_guides(&depths);
    for (i, node) in nodes.iter_mut().enumerate() {
        let g = &guides[i];
        if is_row_root(node) {
            if let Some(part) = guides_part(node) {
                part.props = guide_props(depths[i], g);
            }
        } else if has_depth(node) {
            let mut guide = guide_props(depths[i], g);
            guide.shift_remove("depth");
            node.props.insert("guide".into(), Value::Object(guide));
        }
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
/// TS `applyTreeGuides`: the root itself and every slot value are one-element
/// lists; every node's children are one list; recurse slots then children.
pub fn apply_tree_guides(root: &mut UiNode) {
    fill_siblings(std::slice::from_mut(root));
    visit(root);
}

fn visit(node: &mut UiNode) {
    fill_siblings(&mut node.children);
    if let Some(slots) = node.slots.as_mut() {
        for slot in slots.values_mut() {
            fill_siblings(std::slice::from_mut(slot));
            visit(slot);
        }
    }
    for child in &mut node.children {
        visit(child);
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
