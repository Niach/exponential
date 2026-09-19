//! EXP-965 — the TREE CONNECTOR every nested list draws instead of bare
//! indentation (web `packages/ui/src/tree-guides.ts`, iOS
//! `ExpCore/Sources/Domain/TreeGuides.swift`, Android `domain/TreeGuides.kt`
//! — the same rule and the same tests).
//!
//! A nested list is a sequence of VISIBLE rows with a depth each (whatever a
//! fold left standing). From that sequence alone the connector falls out:
//!
//! * a row at depth `d ≥ 1` draws an ELBOW in gutter column `d - 1` — the
//!   band its PARENT's leading glyph sits in — running from the row's top
//!   edge down to its vertical centre and turning right into a short stub;
//! * the elbow becomes a TEE when the row is not its parent's last visible
//!   child: the vertical carries on to the row's bottom edge;
//! * every ANCESTOR level whose subtree continues below this row draws a
//!   straight PASS-THROUGH line for the full row height.
//!
//! Pure, so the geometry (which is per-platform) never has to re-derive the
//! facts — and so the rule is one unit test rather than four eyeballings.

/// The connector one row draws, in GUTTER LEVELS (a level is one indent step;
/// the caller turns a level into an x with its own padding and pitch).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Guides {
    /// The level whose gutter carries this row's elbow — `depth - 1`, and
    /// `None` for a root row (depth 0 draws nothing at all).
    pub elbow_at: Option<usize>,
    /// The elbow's vertical runs the WHOLE row height: another sibling
    /// follows below.
    pub tee: bool,
    /// Ancestor levels (all `< depth - 1`) that draw a full-height line
    /// because that ancestor has a later child.
    pub pass_through: Vec<usize>,
}

impl Guides {
    /// The row's nesting depth — the indent its list pads by. Derived, so a
    /// row carries its connector INSTEAD of a loose depth rather than beside
    /// one (the two can then never disagree).
    pub fn depth(&self) -> usize {
        self.elbow_at.map_or(0, |level| level + 1)
    }
}

/// The connector for every row of a visible, tree-ordered `depths` sequence.
///
/// One backward pass: `seen[x]` means "a later row at depth `x` exists with no
/// shallower row in between", which is exactly "that ancestor has another
/// child below". A row at depth `d` then closes every deeper level.
pub fn guides_for(depths: &[usize]) -> Vec<Guides> {
    let max_depth = depths.iter().copied().max().unwrap_or(0);
    let mut seen = vec![false; max_depth + 2];
    let mut out = vec![Guides::default(); depths.len()];
    for (index, &depth) in depths.iter().enumerate().rev() {
        if depth > 0 {
            out[index] = Guides {
                elbow_at: Some(depth - 1),
                tee: seen[depth],
                // Levels 0..depth-1 — the elbow's own level is not one of
                // them (its `tee` says whether it continues).
                pass_through: (0..depth.saturating_sub(1))
                    .filter(|level| seen[level + 1])
                    .collect(),
            };
        }
        seen[depth] = true;
        for level in seen.iter_mut().skip(depth + 1) {
            *level = false;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guides(depths: &[usize]) -> Vec<(Option<usize>, bool, Vec<usize>)> {
        guides_for(depths)
            .into_iter()
            .map(|guide| (guide.elbow_at, guide.tee, guide.pass_through))
            .collect()
    }

    /// A flat list draws nothing.
    #[test]
    fn roots_draw_no_connector() {
        assert_eq!(
            guides(&[0, 0, 0]),
            vec![(None, false, vec![]); 3]
        );
        assert_eq!(guides(&[]), Vec::new());
    }

    /// A straight chain: every child is its parent's LAST child, so every
    /// connector is an elbow and nothing passes through.
    #[test]
    fn a_chain_is_all_elbows() {
        assert_eq!(
            guides(&[0, 1, 2, 3]),
            vec![
                (None, false, vec![]),
                (Some(0), false, vec![]),
                (Some(1), false, vec![]),
                (Some(2), false, vec![]),
            ]
        );
    }

    /// Siblings under one parent: every one but the LAST is a tee.
    #[test]
    fn siblings_tee_until_the_last_one() {
        assert_eq!(
            guides(&[0, 1, 1, 1]),
            vec![
                (None, false, vec![]),
                (Some(0), true, vec![]),
                (Some(0), true, vec![]),
                (Some(0), false, vec![]),
            ]
        );
    }

    /// A grandchild under a NON-last child passes its grandparent's gutter
    /// through; under the last child it does not.
    #[test]
    fn a_grandchild_passes_a_continuing_ancestor_through() {
        // root ├ a │ └ a1 └ b └ b1
        assert_eq!(
            guides(&[0, 1, 2, 1, 2]),
            vec![
                (None, false, vec![]),
                (Some(0), true, vec![]),
                // `a` still has the sibling `b` below → level 0 passes through.
                (Some(1), false, vec![0]),
                (Some(0), false, vec![]),
                // `b` is the last child → nothing passes through.
                (Some(1), false, vec![]),
            ]
        );
    }

    /// Three levels deep: only the ancestors that CONTINUE draw.
    #[test]
    fn only_continuing_ancestors_pass_through() {
        // r ├ a │ ├ a1 │ │ └ x │ └ a2 └ b
        let depths = [0, 1, 2, 3, 2, 1];
        assert_eq!(
            guides(&depths),
            vec![
                (None, false, vec![]),
                (Some(0), true, vec![]),
                (Some(1), true, vec![0]),
                // `x` sits under `a1` (which has `a2` below) under `a`
                // (which has `b` below): both ancestors pass through.
                (Some(2), false, vec![0, 1]),
                (Some(1), false, vec![0]),
                (Some(0), false, vec![]),
            ]
        );
    }

    /// A row's depth rides its connector.
    #[test]
    fn the_depth_comes_off_the_elbow() {
        let depths: Vec<usize> = guides_for(&[0, 1, 2, 1])
            .iter()
            .map(Guides::depth)
            .collect();
        assert_eq!(depths, vec![0, 1, 2, 1]);
    }

    /// A row whose depth JUMPS back to a root closes every deeper level.
    #[test]
    fn a_new_root_closes_the_previous_subtree() {
        assert_eq!(
            guides(&[0, 1, 2, 0, 1]),
            vec![
                (None, false, vec![]),
                (Some(0), false, vec![]),
                (Some(1), false, vec![]),
                (None, false, vec![]),
                (Some(0), false, vec![]),
            ]
        );
    }
}
