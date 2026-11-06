// EXP-1709 (merge-train.json): the worker's pure half on top of the
// contract's reference (`@exp/domain-contract/merge-train`): units from the
// repo's live PRs, the opener-subtree readiness input, and the run sequence
// a verdict leads to (re-run once, then bisect by units, halves in order on
// the base as it is then). `trainRun` in the contract is the oracle the
// tests replay this sequence against.
import type { TrainVerdict } from "@exp/domain-contract/merge-train"

export interface PlanPr {
  id: string
  number: number
  basePullRequestId: string | null
  createdAt: Date
}

/** The repo's live PRs as units: a lone PR, or a whole stack (every PR
 *  reachable through `base_pull_request_id` from a bottom whose base is not
 *  a live PR), parents before children, ties oldest first. */
export function buildUnits<T extends PlanPr>(prs: readonly T[]): T[][] {
  const byId = new Map(prs.map((pr) => [pr.id, pr]))
  const children = new Map<string, T[]>()
  const bottoms: T[] = []
  for (const pr of prs) {
    const parent = pr.basePullRequestId ? byId.get(pr.basePullRequestId) : undefined
    if (parent) children.set(parent.id, [...(children.get(parent.id) ?? []), pr])
    else bottoms.push(pr)
  }
  const byAge = (a: T, b: T) => a.createdAt.getTime() - b.createdAt.getTime() || a.number - b.number
  const units: T[][] = []
  const placed = new Set<string>()
  for (const bottom of [...bottoms].sort(byAge)) {
    const unit: T[] = []
    let frontier = [bottom]
    while (frontier.length > 0) {
      const next: T[] = []
      for (const pr of [...frontier].sort(byAge)) {
        if (placed.has(pr.id)) continue
        placed.add(pr.id)
        unit.push(pr)
        next.push(...(children.get(pr.id) ?? []))
      }
      frontier = next
    }
    units.push(unit)
  }
  // A cycle (never written): each leftover PR alone.
  for (const pr of [...prs].sort(byAge)) if (!placed.has(pr.id)) units.push([pr])
  return units
}

export interface PlanSession {
  id: string
  parentId: string | null
  status: string
  busy: boolean
}

/** merge-train.json READINESS: the opener and every descendant of it are
 *  idle (not queued, not busy). No opener (a PR opened outside a session)
 *  or an opener that is gone = idle. */
export function subtreeIdle(openerId: string | null, sessions: readonly PlanSession[]): boolean {
  if (!openerId) return true
  const kids = new Map<string, PlanSession[]>()
  for (const s of sessions) if (s.parentId) kids.set(s.parentId, [...(kids.get(s.parentId) ?? []), s])
  const opener = sessions.find((s) => s.id === openerId)
  if (!opener) return true
  const stack = [opener]
  const seen = new Set<string>()
  while (stack.length > 0) {
    const s = stack.pop()!
    if (seen.has(s.id)) continue
    seen.add(s.id)
    if (s.status === `queued` || (s.status !== `ended` && s.busy)) return false
    stack.push(...(kids.get(s.id) ?? []))
  }
  return true
}

/** One run as the worker keeps it: units of PR keys (ids in the worker,
 *  numbers in the replay), `attempt` 1 = already re-run, `bisect` = a half,
 *  `pending` = the halves still to test after it. */
export interface PlanRun<K> {
  units: K[][]
  attempt: number
  bisect: boolean
  pending: K[][][]
}

export interface RunDecision<K> {
  land: K[][]
  conflicts: Array<{ unit: K[]; files: string[] }>
  failing: K[][]
  /** Back to the queue (an error, or nothing decided). */
  requeue: K[][]
  next: PlanRun<K> | null
}

function popPending<K>(pending: K[][][]): PlanRun<K> | null {
  if (pending.length === 0) return null
  return { units: pending[0]!, attempt: 1, bisect: true, pending: pending.slice(1) }
}

/** What a settled verdict of `run` leads to. `unitOf` maps the verdict's
 *  PR-number units back to the run's units. A pending verdict decides
 *  nothing (null). */
export function decideRun<K>(
  run: PlanRun<K>,
  verdict: Pick<TrainVerdict, `state` | `composed` | `conflicts`>,
  unitOf: (numbers: number[]) => K[] | null
): RunDecision<K> | null {
  const empty: RunDecision<K> = { land: [], conflicts: [], failing: [], requeue: [], next: null }
  if (verdict.state === `pending`) return null
  if (verdict.state === `error`) {
    return { ...empty, requeue: [...run.units, ...run.pending.flat()] }
  }
  const conflicts = verdict.conflicts.flatMap((c) => {
    const unit = unitOf(c.prs)
    return unit ? [{ unit, files: c.files }] : []
  })
  const composed = verdict.composed.flatMap((prs) => {
    const unit = unitOf(prs)
    return unit ? [unit] : []
  })
  if (verdict.state === `conflict`) return { ...empty, conflicts, next: popPending(run.pending) }
  if (verdict.state === `green`) return { ...empty, land: composed, conflicts, next: popPending(run.pending) }
  // red
  if (composed.length === 0) return { ...empty, conflicts, next: popPending(run.pending) }
  if (!run.bisect && run.attempt === 0) {
    return { ...empty, conflicts, next: { units: composed, attempt: 1, bisect: false, pending: run.pending } }
  }
  if (composed.length === 1) return { ...empty, conflicts, failing: composed, next: popPending(run.pending) }
  const half = Math.ceil(composed.length / 2)
  return {
    ...empty,
    conflicts,
    next: { units: composed.slice(0, half), attempt: 1, bisect: true, pending: [composed.slice(half), ...run.pending] },
  }
}

/** The 1-based queue place per PR key: every member of a unit shares it. */
export function queuePositions<K>(queue: readonly K[][]): Map<K, number> {
  const positions = new Map<K, number>()
  queue.forEach((unit, i) => {
    for (const key of unit) positions.set(key, i + 1)
  })
  return positions
}
