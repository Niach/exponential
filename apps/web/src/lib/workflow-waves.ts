// EXP-1103 — review WAVES, the one rule the server's `landNode` gate and the
// engine (`crates/coding/src/workflows/mod.rs`, `review_points` /
// `wave_gate`) share. Pure.
//
// A workflow is reviewed in waves over the LANDED result, never per node.
// The layers are the nodes' `wave` indexes (0-based, the server layout's
// depth position). By default ONE wave, after the last layer. A graph deeper
// than `REVIEW_WAVE_DEEP_DEPTH` layers adds a wave after the contract layer
// (0) and after every `REVIEW_WAVE_STRIDE`th layer after it, so later layers
// do not build on a broken foundation: a node of a later layer neither starts
// nor lands before every wave before its layer CLEARED. A wave clears when
// every landed node it covers carries `approved_at` (stamped by
// `workflows.clearReviewWave` once the wave's one fix run landed).

/** A graph with MORE than this many layers gets waves between layers. */
export const REVIEW_WAVE_DEEP_DEPTH = 3
/** In a deep graph, a wave follows layer 0 and every this-many-th layer after it. */
export const REVIEW_WAVE_STRIDE = 3

/** The layers a review wave follows, for a graph `depth` layers deep. */
export function reviewPoints(depth: number): number[] {
  if (!Number.isFinite(depth) || depth <= 0) return []
  const last = depth - 1
  const points: number[] = []
  if (depth > REVIEW_WAVE_DEEP_DEPTH) {
    for (let layer = 0; layer < last; layer += REVIEW_WAVE_STRIDE) points.push(layer)
  }
  points.push(last)
  return points
}

export interface WaveNode {
  wave: number
  /** contract `wfNodeState`. */
  state: string
  approvedAt: Date | string | null
}

/**
 * The first review point BEFORE `layer` that has not cleared — the wave a
 * node of that layer waits for. `null` = free. A wave has cleared when every
 * node up to its layer is final (`landed`/`skipped`) and every landed one in
 * its range carries `approved_at`.
 */
export function reviewWaveGate(nodes: readonly WaveNode[], layer: number): number | null {
  const depth = nodes.reduce((max, node) => Math.max(max, node.wave + 1), 0)
  let previous = -1
  for (const point of reviewPoints(depth)) {
    if (point >= layer) break
    const settled = nodes
      .filter((node) => node.wave <= point)
      .every((node) => node.state === `landed` || node.state === `skipped`)
    const stamped = nodes
      .filter((node) => node.wave > previous && node.wave <= point && node.state === `landed`)
      .every((node) => node.approvedAt !== null)
    if (!settled || !stamped) return point
    previous = point
  }
  return null
}

/** The literal `landNode` answers while a wave gates the node; shipped
 *  engines match it (`LandOutcome::is_waiting`). */
export const WAITING_FOR_REVIEW_WAVE = `Waiting for the review wave to clear`
/** The literal `landNode` answers while the node's run is still up. */
export const WAITING_FOR_RUN_TO_END = `Waiting for its run to end`
