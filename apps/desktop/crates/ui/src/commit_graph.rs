//! Lane layout for the Source Control history graph (EXP-509): the pure
//! commit → lane/edge assignment behind the gutter the history list paints.
//! Classic straight-branch layout over the loaded window, top (newest) to
//! bottom: each active lane "expects" a parent hash; a commit lands on the
//! leftmost lane expecting it, merge parents open new lanes, and joins close
//! them. Deterministic over a prefix — appending a page (`Load more`) never
//! re-lanes or recolors already-laid rows, so the caller recomputes over the
//! full vec on every append. Deliberately gpui-free (unit-tested as data).
//! Since EXP-518 the window is the multi-ref `scm::log_graph` walk (HEAD +
//! remote-tracking refs) and an optional trunk-tip seed pins HEAD's line to
//! lane 0 — squash-merged PR branches render as side lanes off the trunk.
//! EXP-537 closes those lanes at the top: a [`SquashLink`] declares "this
//! trunk commit squashed that branch tip", and the layout routes a SYNTHETIC
//! second parent from the squash commit's dot into the branch lane — drawn
//! dashed (the merge is by patch, not by ancestry) until the tip's own dot,
//! where the branch's real history continues solid.

use coding::scm::CommitInfo;

/// Render clamp for the sidebar gutter — lanes at or past this draw stacked
/// on the last position (dimmed), the ALGORITHM itself is unbounded.
pub const MAX_LANES: usize = 6;

/// How a row's edge is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeKind {
    /// Full-height vertical at `lane_top` (== `lane_bottom`): a lane passing
    /// this row by.
    Pass,
    /// Top edge → this row's dot: from `lane_top` at the row top into the dot
    /// (curved when the lanes differ).
    IntoDot,
    /// This row's dot → bottom edge at `lane_bottom` (curved when they
    /// differ — a merge parent branching out, or a join into another lane).
    OutOfDot,
}

/// One drawable edge of a row. Each row is self-contained: painting a row
/// paints ONLY its own edges (top half = `IntoDot`/`Pass` upper part, bottom
/// half = `OutOfDot`/`Pass` lower part), so the virtualized list never needs
/// neighbor rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    pub kind: EdgeKind,
    /// Lane at the row's top edge (`IntoDot`/`Pass`; for `OutOfDot` this is
    /// the dot's lane).
    pub lane_top: usize,
    /// Lane at the row's bottom edge (`OutOfDot`/`Pass`; for `IntoDot` this
    /// is the dot's lane).
    pub lane_bottom: usize,
    /// Palette index (stable per lane run — a branch keeps its color).
    pub color: usize,
    /// A [`SquashLink`] connector segment (EXP-537) — drawn dashed: the
    /// stretch from the squash commit's dot down to the squashed tip's dot,
    /// where the branch's real ancestry takes over solid.
    pub synthetic: bool,
}

/// EXP-537: "trunk commit `commit` squash-merged the branch whose tip is
/// `tip`" — provider knowledge (PR data), not git ancestry. The layout routes
/// `tip` like an extra merge parent of `commit`, marked [`Edge::synthetic`],
/// so a squash-merged branch's lane closes into the trunk instead of
/// dangling at its tip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SquashLink {
    /// The single-parent squash commit on the trunk.
    pub commit: String,
    /// The squashed branch's (kept) remote tip.
    pub tip: String,
}

/// One commit row's graph geometry.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GraphRow {
    /// The commit dot's lane.
    pub lane: usize,
    /// The dot's palette index.
    pub color: usize,
    pub edges: Vec<Edge>,
}

/// The laid-out graph for the loaded history window.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Graph {
    /// Row-parallel with the commit slice `layout` was given.
    pub rows: Vec<GraphRow>,
    /// Lanes still open after the last row — parents beyond the loaded
    /// window. The "Load more" row paints these as pass-throughs so lanes
    /// visually run off the bottom. `(lane, color, synthetic)` — synthetic
    /// lanes (a squash link whose tip is beyond the window) stay dashed.
    pub tail: Vec<(usize, usize, bool)>,
    /// Highest occupied lane over the window (gutter width driver).
    pub max_lane: usize,
}

/// A lane that is open between two rows: the next commit it wants to land on.
#[derive(Debug, Clone)]
struct Lane {
    expects: String,
    color: usize,
    /// Opened by a [`SquashLink`] and not yet resolved at the tip's dot —
    /// every segment it draws is dashed.
    synthetic: bool,
}

/// Lay the loaded commit window out into lanes. The walk runs `--date-order`,
/// which guarantees a child is emitted before its parents, so no topological
/// pre-sort is needed. `trunk_tip` is HEAD's hash at load time (EXP-518): it
/// seeds lane 0 so the trunk keeps lane 0 / color 0 even when a remote branch
/// tip is newer and rows above it exist. `links` are the window's squash
/// merges (EXP-537): each routes its tip like an extra, synthetic merge
/// parent of the squash commit — the caller guarantees a linked tip never
/// sits ABOVE its squash commit (else the lane would dangle open forever).
pub fn layout(commits: &[CommitInfo], trunk_tip: Option<&str>, links: &[SquashLink]) -> Graph {
    let mut lanes: Vec<Option<Lane>> = Vec::new();
    let mut next_color: usize = 0;
    let mut max_lane: usize = 0;
    let mut rows: Vec<GraphRow> = Vec::with_capacity(commits.len());

    // Seed lane 0 expecting the trunk tip — skipped when the tip IS the first
    // row (the fresh-tip path already lands it on lane 0 / color 0 without a
    // spurious top-edge stub, keeping the pre-seed output byte-identical).
    // When the tip sits deeper (a branch row is newer), the seeded lane draws
    // `Pass` through the rows above and a straight `IntoDot` at the tip; a tip
    // beyond the window survives into `tail` as (0, 0). Stable under append:
    // the condition depends only on `commits[0]` and the tip.
    if let Some(tip) = trunk_tip {
        if commits.first().is_some_and(|c| c.hash != tip) {
            lanes.push(Some(Lane {
                expects: tip.to_string(),
                color: take_color(&mut next_color),
                synthetic: false,
            }));
        }
    }

    for commit in commits {
        let mut edges: Vec<Edge> = Vec::new();

        // Lanes expecting THIS commit (its children's lines converge here).
        let incoming: Vec<usize> = lanes
            .iter()
            .enumerate()
            .filter_map(|(ix, lane)| {
                lane.as_ref().filter(|l| l.expects == commit.hash).map(|_| ix)
            })
            .collect();

        // The dot lands on the leftmost expecting lane; a fresh tip (first
        // row, or an unrelated head inside the window) opens the leftmost
        // free slot with a new color.
        let dot_lane = match incoming.first() {
            Some(&lane) => lane,
            None => {
                let lane = alloc_lane(&mut lanes);
                lanes[lane] = Some(Lane {
                    expects: String::new(),
                    color: take_color(&mut next_color),
                    synthetic: false,
                });
                lane
            }
        };
        let dot_color = lanes[dot_lane].as_ref().map(|l| l.color).unwrap_or(0);

        // Top halves: every incoming lane draws into the dot (the non-dot
        // ones curve — that child branched off here — and close); every other
        // occupied lane passes through. A synthetic lane resolving at its
        // tip's dot draws its last dashed segment here — below the dot the
        // branch's real history continues solid.
        for &ix in &incoming {
            edges.push(Edge {
                kind: EdgeKind::IntoDot,
                lane_top: ix,
                lane_bottom: dot_lane,
                color: lanes[ix].as_ref().map(|l| l.color).unwrap_or(dot_color),
                synthetic: lanes[ix].as_ref().is_some_and(|l| l.synthetic),
            });
            if ix != dot_lane {
                lanes[ix] = None;
            }
        }
        if let Some(lane) = lanes[dot_lane].as_mut() {
            lane.synthetic = false;
        }
        for (ix, lane) in lanes.iter().enumerate() {
            if let Some(lane) = lane {
                if ix != dot_lane {
                    edges.push(Edge {
                        kind: EdgeKind::Pass,
                        lane_top: ix,
                        lane_bottom: ix,
                        color: lane.color,
                        synthetic: lane.synthetic,
                    });
                }
            }
        }

        // Bottom halves: route the dot to its parents.
        let mut parents = commit.parents.iter();
        match parents.next() {
            None => {
                // Root commit — the lane ends at the dot.
                lanes[dot_lane] = None;
            }
            Some(first) => {
                // First parent: the lane continues straight down. Several
                // lanes may end up expecting the same parent — they run
                // parallel and all converge as `IntoDot`s at the parent's own
                // row (the classic layout: lines meet at the fork point's
                // dot, the trunk never re-routes into a feature lane).
                if let Some(lane) = lanes[dot_lane].as_mut() {
                    lane.expects = first.clone();
                }
                edges.push(Edge {
                    kind: EdgeKind::OutOfDot,
                    lane_top: dot_lane,
                    lane_bottom: dot_lane,
                    color: dot_color,
                    synthetic: false,
                });
            }
        }
        for parent in parents {
            // Merge parents: join the lane already expecting them, else open
            // the leftmost free lane with a fresh color.
            match expecting_lane(&lanes, parent) {
                Some(join) => {
                    let color = lanes[join].as_ref().map(|l| l.color).unwrap_or(dot_color);
                    edges.push(Edge {
                        kind: EdgeKind::OutOfDot,
                        lane_top: dot_lane,
                        lane_bottom: join,
                        color,
                        synthetic: false,
                    });
                }
                None => {
                    let lane = alloc_lane(&mut lanes);
                    let color = take_color(&mut next_color);
                    lanes[lane] =
                        Some(Lane { expects: parent.clone(), color, synthetic: false });
                    edges.push(Edge {
                        kind: EdgeKind::OutOfDot,
                        lane_top: dot_lane,
                        lane_bottom: lane,
                        color,
                        synthetic: false,
                    });
                }
            }
        }

        // Squash links (EXP-537): route each linked tip exactly like a merge
        // parent — dashed, on a synthetic lane that resolves at the tip's
        // dot. A tip that is also a real parent needs no synthetic line.
        for link in links.iter().filter(|l| l.commit == commit.hash) {
            if commit.parents.contains(&link.tip) {
                continue;
            }
            match expecting_lane(&lanes, &link.tip) {
                Some(join) => {
                    let color = lanes[join].as_ref().map(|l| l.color).unwrap_or(dot_color);
                    edges.push(Edge {
                        kind: EdgeKind::OutOfDot,
                        lane_top: dot_lane,
                        lane_bottom: join,
                        color,
                        synthetic: true,
                    });
                }
                None => {
                    let lane = alloc_lane(&mut lanes);
                    let color = take_color(&mut next_color);
                    lanes[lane] =
                        Some(Lane { expects: link.tip.clone(), color, synthetic: true });
                    edges.push(Edge {
                        kind: EdgeKind::OutOfDot,
                        lane_top: dot_lane,
                        lane_bottom: lane,
                        color,
                        synthetic: true,
                    });
                }
            }
        }

        while lanes.last().is_some_and(Option::is_none) {
            lanes.pop();
        }
        let row_max = edges
            .iter()
            .flat_map(|e| [e.lane_top, e.lane_bottom])
            .chain([dot_lane])
            .max()
            .unwrap_or(dot_lane);
        max_lane = max_lane.max(row_max);
        rows.push(GraphRow { lane: dot_lane, color: dot_color, edges });
    }

    let tail = lanes
        .iter()
        .enumerate()
        .filter_map(|(ix, lane)| lane.as_ref().map(|l| (ix, l.color, l.synthetic)))
        .collect();
    Graph { rows, tail, max_lane }
}

/// Leftmost free slot, growing the lane vec when all are occupied.
fn alloc_lane(lanes: &mut Vec<Option<Lane>>) -> usize {
    match lanes.iter().position(Option::is_none) {
        Some(ix) => ix,
        None => {
            lanes.push(None);
            lanes.len() - 1
        }
    }
}

/// The leftmost lane currently expecting `hash`, if any.
fn expecting_lane(lanes: &[Option<Lane>], hash: &str) -> Option<usize> {
    lanes
        .iter()
        .position(|lane| lane.as_ref().is_some_and(|l| l.expects == hash))
}

/// Cycle through the palette.
fn take_color(next_color: &mut usize) -> usize {
    let color = *next_color;
    *next_color += 1;
    color
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(hash: &str, parents: &[&str]) -> CommitInfo {
        CommitInfo {
            hash: hash.to_string(),
            parents: parents.iter().map(|p| p.to_string()).collect(),
            subject: format!("commit {hash}"),
            author: "t".to_string(),
            relative_time: "now".to_string(),
        }
    }

    fn link(commit: &str, tip: &str) -> SquashLink {
        SquashLink { commit: commit.to_string(), tip: tip.to_string() }
    }

    fn edge(row: &GraphRow, kind: EdgeKind) -> Vec<Edge> {
        row.edges.iter().copied().filter(|e| e.kind == kind).collect()
    }

    #[test]
    fn linear_chain_stays_on_lane_zero() {
        let graph =
            layout(&[commit("c", &["b"]), commit("b", &["a"]), commit("a", &[])], None, &[]);
        assert_eq!(graph.max_lane, 0);
        assert!(graph.tail.is_empty()); // root reached — nothing open
        for (ix, row) in graph.rows.iter().enumerate() {
            assert_eq!(row.lane, 0);
            assert_eq!(row.color, 0);
            assert!(edge(row, EdgeKind::Pass).is_empty());
            // Rows after the first draw the incoming line from above.
            assert_eq!(edge(row, EdgeKind::IntoDot).len(), usize::from(ix > 0));
        }
        // The root has no outgoing edge; the others continue straight.
        assert_eq!(edge(&graph.rows[0], EdgeKind::OutOfDot), vec![Edge {
            kind: EdgeKind::OutOfDot,
            lane_top: 0,
            lane_bottom: 0,
            color: 0,
            synthetic: false,
        }]);
        assert!(edge(&graph.rows[2], EdgeKind::OutOfDot).is_empty());
    }

    #[test]
    fn merge_opens_a_lane_and_join_closes_it() {
        // m merges f into the trunk: m → (b, f); f → a; b → a; a root.
        let graph = layout(&[
            commit("m", &["b", "f"]),
            commit("f", &["a"]),
            commit("b", &["a"]),
            commit("a", &[]),
        ], None, &[]);
        assert_eq!(graph.max_lane, 1);

        // Merge row: dot on lane 0, second parent branches out to lane 1
        // with a fresh color.
        let m = &graph.rows[0];
        assert_eq!(m.lane, 0);
        let out: Vec<Edge> = edge(m, EdgeKind::OutOfDot);
        assert!(out.contains(&Edge { kind: EdgeKind::OutOfDot, lane_top: 0, lane_bottom: 0, color: 0, synthetic: false }));
        assert!(out.contains(&Edge { kind: EdgeKind::OutOfDot, lane_top: 0, lane_bottom: 1, color: 1, synthetic: false }));

        // f sits on lane 1 and continues straight down toward "a"; the trunk
        // line passes it by.
        let f = &graph.rows[1];
        assert_eq!((f.lane, f.color), (1, 1));
        assert_eq!(edge(f, EdgeKind::OutOfDot), vec![Edge {
            kind: EdgeKind::OutOfDot,
            lane_top: 1,
            lane_bottom: 1,
            color: 1,
            synthetic: false,
        }]);
        assert_eq!(edge(f, EdgeKind::Pass), vec![Edge {
            kind: EdgeKind::Pass,
            lane_top: 0,
            lane_bottom: 0,
            color: 0,
            synthetic: false,
        }]);

        // b continues on lane 0; f's line (also expecting "a") passes by.
        let b = &graph.rows[2];
        assert_eq!(b.lane, 0);
        assert_eq!(edge(b, EdgeKind::IntoDot).len(), 1);
        assert_eq!(edge(b, EdgeKind::Pass), vec![Edge {
            kind: EdgeKind::Pass,
            lane_top: 1,
            lane_bottom: 1,
            color: 1,
            synthetic: false,
        }]);

        // Both lines converge at the fork point "a": a straight IntoDot from
        // lane 0 and a curved one from lane 1, then the root closes all.
        let a = &graph.rows[3];
        assert_eq!(a.lane, 0);
        let into: Vec<Edge> = edge(a, EdgeKind::IntoDot);
        assert!(into.contains(&Edge { kind: EdgeKind::IntoDot, lane_top: 0, lane_bottom: 0, color: 0, synthetic: false }));
        assert!(into.contains(&Edge { kind: EdgeKind::IntoDot, lane_top: 1, lane_bottom: 0, color: 1, synthetic: false }));
        assert!(graph.tail.is_empty());
    }

    #[test]
    fn branch_off_converges_both_children_at_the_shared_parent() {
        // Two heads inside the window sharing one parent: h1 → a, h2 → a.
        let graph =
            layout(&[commit("h1", &["a"]), commit("h2", &["a"]), commit("a", &[])], None, &[]);
        // h2 is a fresh tip → new lane 1, new color, running parallel.
        let h2 = &graph.rows[1];
        assert_eq!((h2.lane, h2.color), (1, 1));
        assert_eq!(edge(h2, EdgeKind::OutOfDot), vec![Edge {
            kind: EdgeKind::OutOfDot,
            lane_top: 1,
            lane_bottom: 1,
            color: 1,
            synthetic: false,
        }]);
        // Both lines converge at the shared parent's dot.
        let a = &graph.rows[2];
        assert_eq!(a.lane, 0);
        let into: Vec<Edge> = edge(a, EdgeKind::IntoDot);
        assert_eq!(into.len(), 2);
        assert!(into.contains(&Edge { kind: EdgeKind::IntoDot, lane_top: 1, lane_bottom: 0, color: 1, synthetic: false }));
        assert!(graph.tail.is_empty());
    }

    #[test]
    fn truncated_window_leaves_open_lanes_in_the_tail() {
        // A merge whose parents are both beyond the window.
        let graph = layout(&[commit("m", &["p0", "p1"]), commit("x", &["p2"])], None, &[]);
        // m keeps lane 0 open (expects p0) and opened lane 1 (expects p1);
        // x is a fresh tip on lane 2 (expects p2).
        assert_eq!(graph.rows[1].lane, 2);
        assert_eq!(graph.tail.len(), 3);
        assert_eq!(graph.tail[0], (0, 0, false));
        assert_eq!(graph.tail[1], (1, 1, false));
        assert_eq!(graph.tail[2], (2, 2, false));
        assert_eq!(graph.max_lane, 2);
    }

    #[test]
    fn prefix_layout_is_stable_under_append() {
        // Recomputing over an extended window never re-lanes the prefix —
        // the "Load more" guarantee.
        let full = [
            commit("m", &["b", "f"]),
            commit("f", &["a"]),
            commit("b", &["a"]),
            commit("a", &["z"]),
            commit("z", &[]),
        ];
        let prefix = layout(&full[..3], None, &[]);
        let whole = layout(&full, None, &[]);
        assert_eq!(prefix.rows[..], whole.rows[..3]);
    }

    #[test]
    fn freed_lane_is_reused_by_later_branches() {
        // Two sequential merges: the second's branch lane reuses slot 1
        // after the first closed it, with a FRESH color.
        let graph = layout(&[
            commit("m2", &["m1", "g"]),
            commit("g", &["m1"]),
            commit("m1", &["b", "f"]),
            commit("f", &["b"]),
            commit("b", &[]),
        ], None, &[]);
        assert_eq!(graph.max_lane, 1);
        let m1 = &graph.rows[2];
        // m1's merge parent f re-opens lane 1 with color 2 (0 = trunk,
        // 1 = g's branch, 2 = f's branch).
        assert!(edge(m1, EdgeKind::OutOfDot).contains(&Edge {
            kind: EdgeKind::OutOfDot,
            lane_top: 0,
            lane_bottom: 1,
            color: 2,
            synthetic: false,
        }));
    }

    #[test]
    fn seeded_trunk_keeps_lane_zero_when_branch_tip_is_newer() {
        // A branch tip ("bt") newer than the trunk tip ("mt"), both forking
        // from "a" — the EXP-518 shape. The seed pins the trunk to lane 0 /
        // color 0; the branch rides lane 1 with the next color.
        let graph = layout(
            &[commit("bt", &["a"]), commit("mt", &["a"]), commit("a", &[])],
            Some("mt"),
            &[],
        );
        let bt = &graph.rows[0];
        assert_eq!((bt.lane, bt.color), (1, 1));
        // The trunk's seeded lane passes the newer branch row by.
        assert_eq!(edge(bt, EdgeKind::Pass), vec![Edge {
            kind: EdgeKind::Pass,
            lane_top: 0,
            lane_bottom: 0,
            color: 0,
            synthetic: false,
        }]);
        // The trunk tip lands on the seeded lane with a straight IntoDot from
        // the list's top edge.
        let mt = &graph.rows[1];
        assert_eq!((mt.lane, mt.color), (0, 0));
        assert_eq!(edge(mt, EdgeKind::IntoDot), vec![Edge {
            kind: EdgeKind::IntoDot,
            lane_top: 0,
            lane_bottom: 0,
            color: 0,
            synthetic: false,
        }]);
        // Both lines converge at the shared fork point.
        let a = &graph.rows[2];
        assert_eq!(a.lane, 0);
        assert_eq!(edge(a, EdgeKind::IntoDot).len(), 2);
    }

    #[test]
    fn seed_matching_first_row_is_a_noop() {
        // The common case — the trunk tip IS the newest commit: seeding must
        // not change the layout (no spurious top-edge stub into row 0).
        let commits = [commit("c", &["b"]), commit("b", &["a"]), commit("a", &[])];
        assert_eq!(layout(&commits, Some("c"), &[]), layout(&commits, None, &[]));
    }

    #[test]
    fn seed_survives_in_tail_when_tip_is_beyond_the_window() {
        // A window of branch-only commits with the trunk tip past its end —
        // the seeded lane stays open and runs off the bottom.
        let graph = layout(&[commit("bt", &["a"])], Some("mt"), &[]);
        assert_eq!(graph.rows[0].lane, 1);
        assert!(graph.tail.contains(&(0, 0, false)));
    }

    #[test]
    fn seeded_prefix_is_stable_under_append() {
        // The Load-more guarantee holds with a seed: the caller reuses the
        // tip resolved at refresh time, so recomputing over an extended
        // window never re-lanes the prefix.
        let full = [
            commit("bt", &["x"]),
            commit("mt", &["a"]),
            commit("x", &["a"]),
            commit("a", &["z"]),
            commit("z", &[]),
        ];
        let prefix = layout(&full[..2], Some("mt"), &[]);
        let whole = layout(&full, Some("mt"), &[]);
        assert_eq!(prefix.rows[..], whole.rows[..2]);
    }

    #[test]
    fn squash_link_closes_the_branch_lane_into_the_trunk() {
        // EXP-537: s squash-merged the branch whose kept tip is t (s has ONE
        // real parent b). The link draws a dashed merge-back: s's dot → the
        // branch lane → t's dot; below t the branch's real history is solid.
        let graph = layout(
            &[
                commit("s", &["b"]),
                commit("t", &["a"]),
                commit("b", &["a"]),
                commit("a", &[]),
            ],
            None,
            &[link("s", "t")],
        );
        assert_eq!(graph.max_lane, 1);

        // The squash row branches a dashed connector out to lane 1 with a
        // fresh color, alongside its real (solid) trunk continuation.
        let s = &graph.rows[0];
        assert_eq!(s.lane, 0);
        let out = edge(s, EdgeKind::OutOfDot);
        assert!(out.contains(&Edge { kind: EdgeKind::OutOfDot, lane_top: 0, lane_bottom: 0, color: 0, synthetic: false }));
        assert!(out.contains(&Edge { kind: EdgeKind::OutOfDot, lane_top: 0, lane_bottom: 1, color: 1, synthetic: true }));

        // The tip lands on the synthetic lane: dashed above the dot, solid
        // real ancestry below it.
        let t = &graph.rows[1];
        assert_eq!((t.lane, t.color), (1, 1));
        assert_eq!(edge(t, EdgeKind::IntoDot), vec![Edge {
            kind: EdgeKind::IntoDot,
            lane_top: 1,
            lane_bottom: 1,
            color: 1,
            synthetic: true,
        }]);
        assert_eq!(edge(t, EdgeKind::OutOfDot), vec![Edge {
            kind: EdgeKind::OutOfDot,
            lane_top: 1,
            lane_bottom: 1,
            color: 1,
            synthetic: false,
        }]);

        // The branch lane passing b is solid (real history now) and both
        // lines converge solid at the fork point a.
        let b = &graph.rows[2];
        assert_eq!(edge(b, EdgeKind::Pass), vec![Edge {
            kind: EdgeKind::Pass,
            lane_top: 1,
            lane_bottom: 1,
            color: 1,
            synthetic: false,
        }]);
        let a = &graph.rows[3];
        assert!(edge(a, EdgeKind::IntoDot).contains(&Edge {
            kind: EdgeKind::IntoDot,
            lane_top: 1,
            lane_bottom: 0,
            color: 1,
            synthetic: false,
        }));
        assert!(graph.tail.is_empty());
    }

    #[test]
    fn squash_link_lane_stays_dashed_while_passing_rows() {
        // A trunk commit between the squash and the tip: the not-yet-resolved
        // synthetic lane passes it DASHED.
        let graph = layout(
            &[
                commit("s", &["m"]),
                commit("m", &["b"]),
                commit("t", &["a"]),
                commit("b", &["a"]),
                commit("a", &[]),
            ],
            None,
            &[link("s", "t")],
        );
        let m = &graph.rows[1];
        assert_eq!(edge(m, EdgeKind::Pass), vec![Edge {
            kind: EdgeKind::Pass,
            lane_top: 1,
            lane_bottom: 1,
            color: 1,
            synthetic: true,
        }]);
    }

    #[test]
    fn squash_link_tip_beyond_the_window_rides_the_tail_dashed() {
        // The linked tip is past the loaded window: the synthetic lane runs
        // off the bottom dashed, exactly like other open lanes.
        let graph = layout(&[commit("s", &["b"]), commit("b", &["a"])], None, &[link("s", "t")]);
        assert_eq!(graph.tail, vec![(0, 0, false), (1, 1, true)]);
    }

    #[test]
    fn squash_link_matching_a_real_parent_is_a_noop() {
        // A true merge commit needs no synthetic line — ancestry already
        // draws it.
        let commits = [
            commit("m", &["b", "t"]),
            commit("t", &["a"]),
            commit("b", &["a"]),
            commit("a", &[]),
        ];
        assert_eq!(layout(&commits, None, &[link("m", "t")]), layout(&commits, None, &[]));
    }

    #[test]
    fn squash_linked_prefix_is_stable_under_append() {
        // Links derived from pinned data only ever ADD lanes at or below
        // their squash row, so recomputing over an extended window keeps the
        // visible prefix byte-identical.
        let full = [
            commit("s2", &["s1"]),
            commit("t2", &["s1"]),
            commit("s1", &["b"]),
            commit("t1", &["b"]),
            commit("b", &[]),
        ];
        let links = [link("s2", "t2"), link("s1", "t1")];
        let prefix = layout(&full[..2], None, &links);
        let whole = layout(&full, None, &links);
        assert_eq!(prefix.rows[..], whole.rows[..2]);
    }
}

