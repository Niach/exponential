// EXP-981: what every client SAYS about a workflow. The graph's geometry is
// the server's (`wave`/`lane` on the synced nodes); this is the rest — bands,
// captions, the edges between nodes — mirrored ×4 (iOS `ExpCore/Sources/
// Domain/WorkflowView.swift`, Android `domain/WorkflowView.kt`, desktop
// `domain::workflow_view`) and locked by the contract fixture
// `domain-contract/fixtures/workflow-view.json`. All strings byte-identical.

// EXP-1033: a TYPE import — the edge style union belongs to the drawing half
// (`@exp/ui` `wave-graph.tsx`, one stroke per value) and is re-exported below
// so callers keep reading it beside `workflowEdgeStyle`, the function that
// decides it. EXP-1082: the display-state map is the domain's own table.
import type { WorkflowEdgeStyle } from "@exp/ui"
import {
  WF_NODE_DISPLAY_STATE,
  type WfNodeDisplayState,
  type WfNodeState,
} from "@exp/db-schema/domain"

export type WorkflowBand = `running` | `draft` | `done`

/** The list's three bands, in order. Flat rows under each, no row buttons. */
export const WORKFLOW_BANDS: ReadonlyArray<{ key: WorkflowBand; title: string }> = [
  { key: `running`, title: `Running` },
  { key: `draft`, title: `Draft` },
  { key: `done`, title: `Done` },
]

export const WORKFLOWS_TITLE = `Workflows`
export const WORKFLOWS_EMPTY_TITLE = `No workflows yet`
export const WORKFLOWS_EMPTY_BODY = `Select backlog issues on a board and choose Create workflow to plan them as one parallel run.`
export const PLAN_WORKFLOW_LABEL = `Plan`
export const DELETE_WORKFLOW_LABEL = `Delete workflow`
/** The bulk bar's play menu (EXP-981), in order. */
export const START_AS_BATCH_LABEL = `Start as batch`
export const START_AS_STACK_LABEL = `Start as stack`
export const CREATE_WORKFLOW_LABEL = `Create workflow…`

/** `paused` is a running workflow someone held; `cancelled` is over. An
 *  unknown status (a newer server) lands in Done rather than vanishing. */
export function workflowBand(status: string): WorkflowBand {
  if (status === `running` || status === `paused`) return `running`
  if (status === `draft`) return `draft`
  return `done`
}

export interface WorkflowShape {
  nodes: number
  depth: number
  width: number
  cycles: string[][]
}

/** `12 nodes · depth 3 · width 8`; `1 node · depth 1 · width 1`. */
export function workflowShapeLine(metrics: WorkflowShape): string {
  const nodes = metrics.nodes === 1 ? `1 node` : `${metrics.nodes} nodes`
  return `${nodes} · depth ${metrics.depth} · width ${metrics.width}`
}

/** Null while the workflow could start; else the cycles spelled out. */
export function workflowCycleNote(metrics: WorkflowShape): string | null {
  if (metrics.cycles.length === 0) return null
  const spelled = metrics.cycles.map((keys) => keys.join(`, `)).join(`; `)
  return `These issues block each other in a cycle: ${spelled}. Remove one relation to start.`
}

const STATE_LABELS: Record<string, string> = {
  proposed: `Proposed`,
  blocked: `Blocked`,
  ready: `Ready`,
  running: `Running`,
  waiting: `Waiting`,
  in_review: `In review`,
  updating: `Updating`,
  landed: `Landed`,
  failed: `Failed`,
  skipped: `Skipped`,
}

const KIND_LABELS: Record<string, string> = {
  contract: `Contract`,
  leaf: `Leaf`,
  integration: `Integration`,
}

export function workflowNodeStateLabel(state: string): string {
  return STATE_LABELS[state] ?? state
}

export function workflowNodeKindLabel(kind: string): string {
  return KIND_LABELS[kind] ?? kind
}

/** The tone a node's state paints in. `waiting` is the ONLY amber one (and
 *  the only one that pushes): amber means "a person is needed". */
export type WorkflowNodeTone = `muted` | `active` | `amber` | `success` | `danger`

export function workflowNodeTone(state: string): WorkflowNodeTone {
  if (state === `waiting`) return `amber`
  if (state === `failed`) return `danger`
  if (state === `landed`) return `success`
  if (state === `running` || state === `updating` || state === `in_review`) {
    return `active`
  }
  return `muted`
}

export interface CaptionNode {
  kind: string
  state: string
  risk: string
}

/**
 * The ONE caption under a node: the bare STATE label once the workflow has
 * started (`Running`, `In review`, `Landed`), nothing at all in a draft. The
 * kind and the risk are the node panel's (EXP-1014: no `Leaf`, no
 * `Contract · high risk` under the chips — the chip names the issue, the
 * caption says only what is happening to it).
 */
export function workflowNodeCaption(node: CaptionNode, workflowStatus: string): string {
  if (workflowStatus === `draft`) return ``
  return workflowNodeStateLabel(node.state)
}

/** `EXP-14 +3` for a compound node (a parent run as one batch with its
 *  sub-issues), the bare identifier otherwise. */
export function workflowNodeTitle(identifier: string, memberCount: number): string {
  return memberCount > 0 ? `${identifier} +${memberCount}` : identifier
}

export interface EdgeNode {
  id: string
  issueId: string
  memberIssueIds: readonly string[]
  /** EXP-983: engine-written serialization edges (`after_node_ids`). */
  afterNodeIds?: readonly string[]
}

export interface EdgeRelation {
  type: string
  issueId: string
  relatedIssueId: string
}

export interface WorkflowEdge {
  from: string
  to: string
  /** Inside a blocking cycle (`metrics.cycleEdges`): drawn red. */
  cycle: boolean
  /** EXP-983: not a `blocks` relation but a SERIALIZATION edge the engine
   *  added after two siblings' work collided: `to` merges `from` in first. */
  serial: boolean
}

/**
 * The edges between a workflow's nodes, from the synced `blocks` relations: a
 * relation between ANY two covered issues of two different nodes (the server's
 * `nodeEdges`). One edge per node pair, ordered by (from, to) node id.
 * `cycleEdges` = the workflow's `metrics.cycleEdges` (`<from>\n<to>`).
 */
export function workflowEdges(
  nodes: readonly EdgeNode[],
  relations: readonly EdgeRelation[],
  cycleEdges: readonly string[] = []
): WorkflowEdge[] {
  const nodeOf = new Map<string, string>()
  for (const node of nodes) {
    nodeOf.set(node.issueId, node.id)
    for (const member of node.memberIssueIds) nodeOf.set(member, node.id)
  }
  const onCycle = new Set(cycleEdges)
  const seen = new Set<string>()
  const edges: WorkflowEdge[] = []
  for (const relation of relations) {
    if (relation.type !== `blocks`) continue
    const from = nodeOf.get(relation.issueId)
    const to = nodeOf.get(relation.relatedIssueId)
    if (!from || !to || from === to) continue
    const key = `${from}\n${to}`
    if (seen.has(key)) continue
    seen.add(key)
    edges.push({ from, to, cycle: onCycle.has(key), serial: false })
  }
  // Serialization edges, unless a real edge already joins the pair.
  const known = new Set(nodes.map((node) => node.id))
  for (const node of nodes) {
    for (const from of node.afterNodeIds ?? []) {
      if (!known.has(from) || from === node.id) continue
      const key = `${from}\n${node.id}`
      if (seen.has(key)) continue
      seen.add(key)
      edges.push({ from, to: node.id, cycle: false, serial: true })
    }
  }
  return edges.sort((a, b) =>
    a.from < b.from ? -1 : a.from > b.from ? 1 : a.to < b.to ? -1 : a.to > b.to ? 1 : 0
  )
}

// ── Running a workflow (EXP-982) ────────────────────────────────────────────

export const START_WORKFLOW_LABEL = `Start`
export const PAUSE_WORKFLOW_LABEL = `Pause`
export const RESUME_WORKFLOW_LABEL = `Resume`
export const CANCEL_WORKFLOW_LABEL = `Cancel workflow`
export const CANCEL_WORKFLOW_CONFIRM = `Its live runs end and its branch is deleted. Nothing reached the default branch.`
export const FINAL_PR_TITLE = `Final pull request`
/** EXP-1033: the ONE human review of the whole run — squash-merging the
 *  workflow's final pull request from the workflow screen. */
export const MERGE_FINAL_PR_LABEL = `Merge`
export const MERGE_FINAL_PR_CONFIRM = `The workflow's branch is squash-merged into the default branch and the run is done.`
/** The strip over the graph that lists the runs that are up, one tap away. */
export const RUNNING_NOW_LABEL = `Running now`

export interface StartableWorkflow {
  status: string
  deviceId: string | null
  repositoryId: string | null
}

/**
 * Why Start is disabled, or null when the draft can start. One reason, the
 * most fundamental first; the server refuses with the same sentences.
 */
export function workflowStartBlocker(
  workflow: StartableWorkflow,
  metrics: WorkflowShape
): string | null {
  if (workflow.status !== `draft`) return `The workflow has already started.`
  if (metrics.nodes === 0) return `The workflow has no issues.`
  const cycle = workflowCycleNote(metrics)
  if (cycle) return cycle
  if (!workflow.repositoryId) return `The workflow's repository is gone.`
  if (!workflow.deviceId) return `Pick the device that runs this workflow first.`
  return null
}

/**
 * The final-PR node's caption, or null while the node is not drawn: it
 * appears once every node landed (or was skipped), after the last wave.
 */
export function workflowFinalPrCaption(
  nodes: ReadonlyArray<{ state: string }>,
  finalPrState: string | null,
  finalPrNumber: number | null
): string | null {
  // EXP-984: a `proposed` node was never admitted; it is not part of the run.
  const real = nodes.filter((node) => node.state !== `proposed`)
  if (real.length === 0) return null
  const allIn = real.every((node) => node.state === `landed` || node.state === `skipped`)
  if (!allIn && finalPrNumber === null) return null
  if (finalPrNumber === null) return `Opening the pull request`
  const label =
    finalPrState === `merged` ? `Merged` : finalPrState === `closed` ? `Closed` : `Open`
  return `#${finalPrNumber} · ${label}`
}

export const RETRY_NODE_LABEL = `Retry`
export const SKIP_NODE_LABEL = `Skip`
export const SKIP_NODE_CONFIRM = `Its dependents go on without it. The node's work is not part of the final pull request.`

/** A list row's secondary text: the shape line, led by the status word for
 *  the two statuses a band alone does not tell apart. */
export function workflowRowSubtitle(status: string, metrics: WorkflowShape): string {
  const shape = workflowShapeLine(metrics)
  if (status === `paused`) return `Paused · ${shape}`
  if (status === `cancelled`) return `Cancelled · ${shape}`
  return shape
}


// ── Speculative starts (EXP-983) ────────────────────────────────────────────

export type { WorkflowEdgeStyle }

const STARTED_STATES = new Set([`running`, `waiting`, `in_review`, `updating`])

/**
 * - `cycle` (red): inside a blocking cycle.
 * - `stale` (red): upstream moved and the dependent is merging it in (`to` is
 *   `updating`).
 * - `landed` (green): the blocker landed.
 * - `speculative` (dashed): the dependent started before its blocker landed,
 *   or the edge is a serialization edge.
 * - `plain` (grey): nothing to say yet.
 */
export function workflowEdgeStyle(
  edge: { cycle: boolean; serial: boolean },
  fromState: string,
  toState: string
): WorkflowEdgeStyle {
  if (edge.cycle) return `cycle`
  if (fromState === `landed`) return `landed`
  if (toState === `updating`) return `stale`
  if (edge.serial || STARTED_STATES.has(toState)) return `speculative`
  return `plain`
}

/** The node panel's line once a node announced its contract. */
export const CONTRACT_PUBLISHED_LABEL = `Contract published`

/** The node panel's chip line over `after_node_ids`. Byte-identical ×4. */
export const MERGES_IN_FIRST_LABEL = `Merges in first`


// ── Review gate, dynamic graphs, metrics (EXP-984) ─────────────────────────


// EXP-1033: the workflow screen configures NOTHING any more — the per-phase
// model pins of EXP-1002, the review model and the gate are gone with the
// settings block (`workflow-launch.ts` derives every model from the two the
// launch carries), so their labels went with them.

/** The node panel's read-only line: what THIS node's run spawns on
 *  (`modelForNode`). */
export const NODE_MODEL_LABEL = `Model`
/** EXP-1014: the chip of a node whose issue row has not synced yet — the
 *  identifier slot shows the first 8 characters of the issue id, the title
 *  this line. Byte-identical ×4. */
export const NODE_UNSYNCED_TITLE = `Not synced yet`

export interface ReviewLine {
  verdict: string
  round: number
  oracle: { passed: boolean } | null
}

/**
 * The node panel's one line about the latest agent review:
 * `Approved · round 1 · checks passed`, `Approved · round 1`,
 * `Changes requested · round 2 · checks failed`, `Changes requested · round 2`.
 * `approved` = the node's `approvedAt` is set. EXP-1010: an approval with no
 * oracle CLEARS the node, so `advisory` shows only when it did not.
 */
export function workflowReviewLine(review: ReviewLine, approved: boolean): string {
  const verdict = review.verdict === `approve` ? `Approved` : `Changes requested`
  const parts = [verdict, `round ${review.round}`]
  if (review.oracle) parts.push(review.oracle.passed ? `checks passed` : `checks failed`)
  else if (review.verdict === `approve` && !approved) parts.push(`advisory`)
  return parts.join(` · `)
}

// ── EXP-1082 §4: five display states + the `needs you` badge ───────────────
// A person sees FIVE node states (`wfNodeDisplayState`); the stored
// `wfNodeState` stays the engine's internal vocabulary. An unknown state (a
// newer server) reads `queued`. An open question is a BADGE beside the chip,
// never a state. Locked by the fixture's `displayStates` + `needsYouLabel`.

export type { WfNodeDisplayState }

export const NEEDS_YOU_LABEL = `needs you`

const DISPLAY_LABELS: Record<WfNodeDisplayState, string> = {
  queued: `Queued`,
  running: `Running`,
  done: `Done`,
  failed: `Failed`,
  skipped: `Skipped`,
}

export function workflowNodeDisplayState(state: string): WfNodeDisplayState {
  return (
    (WF_NODE_DISPLAY_STATE as Record<string, WfNodeDisplayState | undefined>)[
      state as WfNodeState
    ] ?? `queued`
  )
}

export function workflowNodeDisplayLabel(state: string): string {
  return DISPLAY_LABELS[workflowNodeDisplayState(state)]
}

// ── EXP-1082 §5 / EXP-1066: the workflow page view model ───────────────────
// The page is a picker (the node strip) over a face toggle; these four
// helpers are everything it asks the domain, locked ×4 by the fixture
// sections `nodeStrips`, `headerCaptions`, `primaryActions`, `chipMenus`.

export interface StripNodeInput {
  id: string
  identifier: string
  state: string
  wave: number
  lane: number
  members: number
  live: boolean
  needsYou: boolean
  note?: string | null
}

export interface NodeChip {
  id: string
  /** `workflowNodeTitle`. */
  title: string
  display: WfNodeDisplayState
  /** The node's note while one is set, else the display label. */
  caption: string
  stacked: boolean
  members: number
  live: boolean
  needsYou: boolean
}

/**
 * The strip IS the graph: waves left to right (only the waves that hold a
 * node), lanes top to bottom within a wave, ties by id. The edges are the
 * mini-graph popover's business; the strip only orders. A compound node is
 * `stacked` (the `IssueChipStack`); the caption is the node's note while it
 * has one, else its display label.
 */
export function workflowNodeStrip(
  nodes: readonly StripNodeInput[],
  _edges: readonly [string, string][]
): { wave: number; nodes: NodeChip[] }[] {
  const byWave = new Map<number, StripNodeInput[]>()
  for (const node of nodes) {
    const wave = byWave.get(node.wave) ?? []
    wave.push(node)
    byWave.set(node.wave, wave)
  }
  return [...byWave.entries()]
    .sort(([a], [b]) => a - b)
    .map(([wave, members]) => ({
      wave,
      nodes: members
        .sort((a, b) => a.lane - b.lane || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
        .map((node) => ({
          id: node.id,
          title: workflowNodeTitle(node.identifier, node.members),
          display: workflowNodeDisplayState(node.state),
          caption: node.note?.trim() || workflowNodeDisplayLabel(node.state),
          stacked: node.members > 0,
          members: node.members,
          live: node.live,
          needsYou: node.needsYou,
        })),
    }))
}

const STATUS_WORDS: Record<string, string> = {
  draft: `Draft`,
  done: `Done`,
  failed: `Failed`,
  cancelled: `Cancelled`,
}

/**
 * The one line under the workflow's name. A draft counts its ISSUES (members
 * included): `Draft · 8 issues`. A started workflow names its runner and
 * counts NODES: `on MacBook · 5 of 8 done · 2 running` (the running tail only
 * while something runs). Over, it counts what happened: `Done · 2 done · 1
 * skipped`, `Cancelled · 1 done · 1 failed`. A `proposed` node was never
 * admitted and is not counted.
 */
export function workflowHeaderCaption(
  status: string,
  nodes: readonly { state: string; members: number }[],
  deviceLabel: string | null
): string {
  const admitted = nodes.filter((node) => node.state !== `proposed`)
  if (status === `draft`) {
    const issues = admitted.reduce((sum, node) => sum + 1 + node.members, 0)
    return `Draft · ${issues === 1 ? `1 issue` : `${issues} issues`}`
  }
  const tally = (display: WfNodeDisplayState) =>
    admitted.filter((node) => workflowNodeDisplayState(node.state) === display).length
  const done = tally(`done`)
  if (status === `running` || status === `paused`) {
    const parts = [`${done} of ${admitted.length} done`]
    const running = tally(`running`)
    if (running > 0) parts.push(`${running} running`)
    return (deviceLabel ? [`on ${deviceLabel}`, ...parts] : parts).join(` · `)
  }
  const word = STATUS_WORDS[status] ?? status.charAt(0).toUpperCase() + status.slice(1)
  const parts = [word, `${done} done`]
  const failed = tally(`failed`)
  if (failed > 0) parts.push(`${failed} failed`)
  const skipped = tally(`skipped`)
  if (skipped > 0) parts.push(`${skipped} skipped`)
  return parts.join(` · `)
}

export type WorkflowPrimaryAction =
  | `pick_device`
  | `start`
  | `pause`
  | `resume`
  | `review_final_pr`

/**
 * The header's ONE primary button: a draft without a runner picks one (the
 * existing DevicePicker over own + team-shared online runners), a draft
 * starts, running pauses, paused resumes, done reviews the final PR. Failed
 * and cancelled offer nothing; Stop and Delete live in the overflow.
 */
export function workflowPrimaryAction(
  status: string,
  deviceLabel: string | null
): WorkflowPrimaryAction | null {
  if (status === `draft`) return deviceLabel ? `start` : `pick_device`
  if (status === `running`) return `pause`
  if (status === `paused`) return `resume`
  if (status === `done`) return `review_final_pr`
  return null
}

export type NodeChipMenuItem = `retry` | `skip` | `admit` | `dismiss`

/** What a node chip's overflow offers: Retry / Skip on a `failed` node,
 *  Admit / Dismiss on a `proposed` one (a follow-up filed mid-run), nothing
 *  else anywhere. */
export function nodeChipMenu(state: string): NodeChipMenuItem[] {
  if (state === `failed`) return [`retry`, `skip`]
  if (state === `proposed`) return [`admit`, `dismiss`]
  return []
}

export const ADMIT_NODE_LABEL = `Admit`
export const DISMISS_NODE_LABEL = `Dismiss`
export const PROPOSED_NODE_NOTE = `Filed during the run. Admit it into the workflow or dismiss it.`
