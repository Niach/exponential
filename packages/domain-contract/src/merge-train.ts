import contract from "../contract.json" with { type: "json" }
import fixture from "../fixtures/merge-train.json" with { type: "json" }

// EXP-1707 (fixtures/merge-train.json, epic #EXP-1704): the merge train's
// pure half. Readiness, the per-PR state machine, batching + bisect, the
// generated-file resolver policy, the `pr_merge` texts and their routing,
// the Needs you row, the status pill and the `…` menu. The server worker
// (#EXP-1709) and the train pipeline (#EXP-1708) apply it; the clients
// (#EXP-1710) replay the UI cases ×4.

/** contract prMergeState (the replay test pins the two lists together). */
export type PrMergeState = `draft` | `checks` | `queued` | `testing` | `merged` | `conflict` | `failing` | `held`
/** contract teamMergeMode. */
export type TeamMergeMode = `auto` | `approval`
export const PR_MERGE_STATES: readonly string[] = contract.prMergeState.values
export const TEAM_MERGE_MODES: readonly string[] = contract.teamMergeMode.values
export type HoldReason = `person` | `bounces` | `timeout`
export type MergeOutcome = `merged` | `conflict` | `failing` | `held`

export const MERGE_TRAIN_LIMITS = fixture.limits
export const MERGE_TRAIN_COPY = fixture.copy
export const MERGE_TRAIN_TEST_IDS = fixture.testIds
export const MERGE_NOW_PROMPT = fixture.mergeNowPrompt

const HOUR = 3_600_000

// ── READINESS ──────────────────────────────────────────────────────────────

export type MergeWait = `draft` | `failing` | `checks` | `session` | `approval`

export interface ReadinessInput {
  draft: boolean
  /** contract prChecksState of the PR's own head. */
  checksState: string | null
  /** The opener and every descendant of it are idle (not queued, not busy). */
  subtreeIdle: boolean
  mode: TeamMergeMode
  /** A person pressed Needs approval (approval mode only). */
  approved: boolean
}

/** What a PR still waits for, first blocker wins; null = ready. */
export function mergeWait(input: ReadinessInput): MergeWait | null {
  if (input.draft) return `draft`
  if (input.checksState === `failure`) return `failing`
  if (input.checksState === `pending`) return `checks`
  if (!input.subtreeIdle) return `session`
  if (input.mode === `approval` && !input.approved) return `approval`
  return null
}

/** The pre-train state a readiness verdict maps to. */
export function preTrainState(wait: MergeWait | null): PrMergeState {
  if (wait === null) return `queued`
  if (wait === `draft`) return `draft`
  if (wait === `failing`) return `failing`
  return `checks`
}

// ── THE STATE MACHINE ──────────────────────────────────────────────────────

export interface MergePr {
  state: PrMergeState
  /** Train bounces (conflict or failing found by the train). */
  attempts: number
  /** When the PR first entered `queued` (the 24 h clock), ISO or null. */
  queuedSince: string | null
  holdReason: HoldReason | null
  /** An owner's Merge now: next, alone, untested. */
  mergeNow: boolean
}

export type MergeEvent =
  | { type: `readiness`; wait: MergeWait | null; at: string }
  | { type: `push` }
  | { type: `train_start` }
  | { type: `train_landed` }
  | { type: `train_requeued` }
  | { type: `train_unlanded` }
  | { type: `train_conflict` }
  | { type: `train_failing` }
  | { type: `tick`; now: string }
  | { type: `hold` }
  | { type: `resume` }
  | { type: `merge_now` }

export interface MergeStepResult {
  pr: MergePr
  /** The `pr_merge` outcome to post, or null. */
  post: MergeOutcome | null
}

const FROZEN: ReadonlySet<PrMergeState> = new Set([`testing`, `merged`, `held`])

function bounce(pr: MergePr, outcome: `conflict` | `failing`): MergeStepResult {
  const attempts = pr.attempts + 1
  if (attempts >= MERGE_TRAIN_LIMITS.bounces) {
    return { pr: { ...pr, state: `held`, attempts, holdReason: `bounces`, mergeNow: false }, post: `held` }
  }
  return { pr: { ...pr, state: outcome, attempts, mergeNow: false }, post: outcome }
}

/** One edge of a PR's merge state. Pure: the worker persists `pr` and
 *  posts `post`. */
export function mergeStep(pr: MergePr, event: MergeEvent): MergeStepResult {
  const same = { pr, post: null }
  if (pr.state === `merged`) return same
  switch (event.type) {
    case `readiness`: {
      if (FROZEN.has(pr.state)) return same
      // A Merge now waits for nothing but a non-draft head.
      if (pr.mergeNow && event.wait !== `draft`) return { pr: { ...pr, state: `queued` }, post: null }
      const state = preTrainState(event.wait)
      const queuedSince = state === `queued` ? (pr.queuedSince ?? event.at) : pr.queuedSince
      return { pr: { ...pr, state, queuedSince }, post: null }
    }
    case `push`:
      // #EXP-1711 (Danny 2026-11-08): a new head resets the bounces; it
      // revives a bounces hold (the agent is back), never a person's or the
      // 24 h one, and the clock keeps running from the first queue.
      if (pr.state === `held` && pr.holdReason !== `bounces`) return same
      return { pr: { ...pr, state: `checks`, attempts: 0, holdReason: null }, post: null }
    case `train_start`:
      return pr.state === `queued` ? { pr: { ...pr, state: `testing` }, post: null } : same
    case `train_landed`:
      return pr.state === `testing`
        ? { pr: { ...pr, state: `merged`, mergeNow: false }, post: `merged` }
        : same
    case `train_requeued`:
      return pr.state === `testing` ? { pr: { ...pr, state: `queued` }, post: null } : same
    case `train_unlanded`: {
      // #FEED-212: a landing the forge never confirmed: back to the queue
      // (never closed, never merged), the attempt counted; the limit holds.
      if (pr.state !== `testing`) return same
      const attempts = pr.attempts + 1
      if (attempts >= MERGE_TRAIN_LIMITS.bounces) {
        return { pr: { ...pr, state: `held`, attempts, holdReason: `bounces`, mergeNow: false }, post: `held` }
      }
      return { pr: { ...pr, state: `queued`, attempts }, post: null }
    }
    case `train_conflict`:
      return pr.state === `testing` ? bounce(pr, `conflict`) : same
    case `train_failing`:
      return pr.state === `testing` ? bounce(pr, `failing`) : same
    case `tick`: {
      if (pr.state === `held` || pr.state === `testing` || pr.state === `draft`) return same
      if (pr.queuedSince === null || pr.mergeNow) return same
      const age = Date.parse(event.now) - Date.parse(pr.queuedSince)
      if (age < MERGE_TRAIN_LIMITS.hours * HOUR) return same
      return { pr: { ...pr, state: `held`, holdReason: `timeout` }, post: `held` }
    }
    case `hold`:
      if (pr.state === `draft` || pr.state === `held`) return same
      return { pr: { ...pr, state: `held`, holdReason: `person`, mergeNow: false }, post: `held` }
    case `resume`:
      if (pr.state !== `held`) return same
      return {
        pr: { ...pr, state: `checks`, attempts: 0, queuedSince: null, holdReason: null },
        post: null,
      }
    case `merge_now`:
      if (pr.state === `draft`) return same
      return {
        pr: { ...pr, state: pr.state === `testing` ? `testing` : `queued`, holdReason: null, mergeNow: true },
        post: null,
      }
  }
}

/** #FEED-212 (merge-train.json `landing`): a landing is confirmed only when
 *  the base moved off its old sha to a commit that holds the landed one
 *  (`contains` = the forge's read of the base AFTER the push: it points at
 *  or contains `landed`; null = unreadable). Anything else = unlanded. */
export function landingConfirmed(input: { from: string | null; landed: string | null; contains: boolean | null }): boolean {
  return input.landed !== null && input.landed !== input.from && input.contains === true
}

// ── BATCHING + BISECT ──────────────────────────────────────────────────────

export interface TrainMember {
  number: number
  ready: boolean
  readyAt: string | null
  /** The member it is based on (a tree-shaped stack); none = the one before. */
  parent?: number
}

export interface TrainUnit {
  /** A lone PR, or a stack bottom first. */
  members: TrainMember[]
  mergeNow?: boolean
}

function unitReadyAt(unit: TrainUnit): string {
  return unit.members.reduce((latest, m) => ((m.readyAt ?? ``) > latest ? (m.readyAt ?? ``) : latest), ``)
}

/** #EXP-1711: a stack's READY PREFIX: every ready member whose parent
 *  (`parent`; none = the member before it) is in the prefix too. A lone PR
 *  is its own prefix when ready. Members keep their bottom-first order. */
export function readyPrefix(unit: TrainUnit): TrainUnit {
  const numbers = new Set(unit.members.map((m) => m.number))
  const inPrefix = new Set<number>()
  const members: TrainMember[] = []
  unit.members.forEach((m, i) => {
    const parent = m.parent ?? (i > 0 ? unit.members[i - 1]!.number : undefined)
    const rooted = parent === undefined || !numbers.has(parent) || inPrefix.has(parent)
    if (m.ready && rooted) {
      inPrefix.add(m.number)
      members.push(m)
    }
  })
  return { ...unit, members }
}

/** The queue in train order: a unit's eligible part is its ready prefix
 *  (none = not eligible); Merge now units first; then by when the prefix
 *  became ready (its latest member), ties by the lowest PR number. */
export function trainQueue(units: readonly TrainUnit[]): TrainUnit[] {
  return units
    .map(readyPrefix)
    .filter((unit) => unit.members.length > 0)
    .map((unit) => ({ unit, at: unitReadyAt(unit), first: Math.min(...unit.members.map((m) => m.number)) }))
    .sort((a, b) => {
      const now = Number(Boolean(b.unit.mergeNow)) - Number(Boolean(a.unit.mergeNow))
      if (now !== 0) return now
      if (a.at !== b.at) return a.at < b.at ? -1 : 1
      return a.first - b.first
    })
    .map(({ unit }) => unit)
}

/** The next batch, as PR numbers per unit: whole units in queue order while
 *  the PR count stays within `batchMax`; a unit never splits (an oversized
 *  one rides alone); the first unit that does not fit ends the batch (no
 *  skipping ahead); a Merge now unit always rides alone. */
export function composeBatch(units: readonly TrainUnit[], batchMax: number = MERGE_TRAIN_LIMITS.batchMax): number[][] {
  // #EXP-1711: an instance may cap batches lower (never above the limit).
  const cap = Math.max(1, Math.min(MERGE_TRAIN_LIMITS.batchMax, Math.floor(batchMax) || MERGE_TRAIN_LIMITS.batchMax))
  const queue = trainQueue(units)
  const batch: number[][] = []
  let size = 0
  for (const unit of queue) {
    const prs = unit.members.map((m) => m.number)
    if (unit.mergeNow) return batch.length === 0 ? [prs] : batch
    if (batch.length > 0 && size + prs.length > cap) break
    batch.push(prs)
    size += prs.length
    if (size >= cap) break
  }
  return batch
}

export interface TrainRunResult {
  /** Every CI run, as the PR numbers it tested, in order. */
  runs: number[][]
  landed: number[]
  failing: number[]
}

/** One batch through the train against a red oracle: a run is red when it
 *  contains a culprit, or while `flaky` runs are left (a flake). A red batch
 *  is re-run ONCE; still red = bisect by units (left = the first
 *  ceil(n/2) units): each half is tested on the base as it is then (a
 *  landed left half moves it); a green half lands; a red half of one unit
 *  is failing (no re-run); a red half of more units bisects again. A batch
 *  of ONE unit red twice = failing. Units of a half land together. */
export function trainRun(input: { batch: number[][]; culprits: number[]; flaky?: number }): TrainRunResult {
  const runs: number[][] = []
  const landed: number[] = []
  const failing: number[] = []
  let flaky = input.flaky ?? 0
  const culprits = new Set(input.culprits)
  const green = (units: number[][]): boolean => {
    const prs = units.flat()
    runs.push(prs)
    if (flaky > 0) {
      flaky -= 1
      return false
    }
    return !prs.some((n) => culprits.has(n))
  }
  const resolve = (units: number[][]): void => {
    if (green(units)) {
      landed.push(...units.flat())
      return
    }
    if (units.length === 1) {
      failing.push(...units[0])
      return
    }
    split(units)
  }
  const split = (units: number[][]): void => {
    const half = Math.ceil(units.length / 2)
    resolve(units.slice(0, half))
    resolve(units.slice(half))
  }
  if (input.batch.length === 0) return { runs, landed, failing }
  if (green(input.batch) || green(input.batch)) {
    landed.push(...input.batch.flat())
  } else if (input.batch.length === 1) {
    failing.push(...input.batch[0])
  } else {
    split(input.batch)
  }
  return { runs, landed, failing }
}

// ── RESOLVER POLICY (generated files) ──────────────────────────────────────

export type ResolveStrategy = `prepend-union` | `renumber` | `regenerate` | `json-union` | `take-later`

export interface ResolveRule {
  name: string
  paths: string[]
  strategy: ResolveStrategy
  command?: string
}

export type ResolveVerdict =
  | { ok: true; steps: Array<{ name: string; strategy: ResolveStrategy; paths: string[]; command: string | null }> }
  | { ok: false; files: string[] }

/** A repo glob: `**` = any run of path segments (incl. none), `*` = within
 *  one segment, `?` = one character; anchored at the repo root. */
export function globMatch(glob: string, path: string): boolean {
  let re = ``
  for (let i = 0; i < glob.length; i++) {
    const c = glob[i]
    if (c === `*` && glob[i + 1] === `*`) {
      if (glob[i + 2] === `/`) {
        re += `(?:.*/)?`
        i += 2
      } else {
        re += `.*`
        i += 1
      }
    } else if (c === `*`) re += `[^/]*`
    else if (c === `?`) re += `[^/]`
    else re += c.replace(/[.+^${}()|[\]\\]/g, `\\$&`)
  }
  return new RegExp(`^${re}$`).test(path)
}

/** The conflicted paths of one unit against the train under the repo's
 *  policy: every path matched (first rule wins) = resolved, one step per
 *  rule in policy order with its paths in input order; ANY unmatched path
 *  = `conflict`, naming only the unmatched files (the unit bounces). */
export function resolveConflicts(paths: readonly string[], policy: readonly ResolveRule[]): ResolveVerdict {
  const unmatched: string[] = []
  const byRule = new Map<number, string[]>()
  for (const path of paths) {
    const index = policy.findIndex((rule) => rule.paths.some((glob) => globMatch(glob, path)))
    if (index < 0) unmatched.push(path)
    else byRule.set(index, [...(byRule.get(index) ?? []), path])
  }
  if (unmatched.length > 0) return { ok: false, files: unmatched }
  return {
    ok: true,
    steps: policy.flatMap((rule, index) =>
      byRule.has(index)
        ? [{ name: rule.name, strategy: rule.strategy, paths: byRule.get(index)!, command: rule.command ?? null }]
        : []
    ),
  }
}

// ── THE TRAIN PIPELINE (#EXP-1708, `pipeline`) ─────────────────────────────

/** The repo's `.exponential/merge-train.json`. */
export interface TrainRepoConfig {
  /** Runs once before the first resolver command (installs the tools). */
  setup?: string
  /** What the pipeline starts on the composed ref with input `base_sha`
   *  (GitHub: workflow file names). Empty = no CI. */
  ci?: { workflows: string[] }
  generated: Array<ResolveRule & { verify?: string[] }>
  /** #LOGS-2 (`repoPolicy.lockfiles`): a Merge now runs CI when a lock the
   *  unit touches composes to a blob other than its head's, or a manifest
   *  it changed sits under a paired lock (`lock` = the file name, in the
   *  manifest's dir or an ancestor) the train moved. */
  lockfiles?: { paths: string[]; manifests?: Array<{ paths: string[]; lock: string }> }
}

export const TRAIN_PIPELINE_CHECK = fixture.pipeline.check
export const trainRef = (trainId: string): string => fixture.pipeline.ref.replace(`<train_id>`, trainId)

export interface TrainPipelinePr {
  number: number
  head_sha: string
  /** The final squash title; committed once as `{title} (#{number})`. */
  title: string
  /** `Name <email>`. */
  author?: string
}

export interface TrainPipelineUnit {
  prs: TrainPipelinePr[]
  /** A commit whose REQUIRED checks were seen green (a lone unit only). */
  tested_sha?: string
}

/** The dispatch inputs; `units` travels JSON-encoded (`JSON.stringify`). */
export interface TrainPipelineInputs {
  train_id: string
  base: string
  base_sha: string
  mode: `test` | `untested`
  units: string
}

export type TrainChecksMode = `dispatched` | `proven` | `skipped` | `none`

export interface TrainUnitOutcome {
  prs: number[]
  outcome: `composed` | `conflict`
  files: string[]
  /** The resolver rule names applied to this unit. */
  resolved: string[]
  /** #EXP-1711, a conflict only: `batch` = it composes clean on the base
   *  alone, so only an earlier unit of this batch (not landed) is in the
   *  way: no bounce. Absent = `base`. */
  against?: `base` | `batch`
  /** #LOGS-2, composed only: the lockfiles that drifted from the unit's
   *  head (`repoPolicy.lockfiles`). Absent = none. */
  drifted?: string[]
}

/** The `exp-train` result's JSON. */
export interface TrainPipelineOutcome {
  train: string
  base_sha: string
  head: string | null
  checks: TrainChecksMode
  units: TrainUnitOutcome[]
  error: string | null
}

export interface TrainCheck {
  name: string
  status: `queued` | `in_progress` | `completed`
  conclusion: string | null
}

export interface TrainVerdict {
  state: `pending` | `green` | `red` | `conflict` | `error`
  head: string | null
  /** The units that ride (green: land; red: re-run, then bisect). */
  composed: number[][]
  /** The units that bounce, whatever the state. */
  conflicts: Array<{ prs: number[]; files: string[]; against?: `base` | `batch` }>
}

const PASSING = new Set([`success`, `neutral`, `skipped`])

/** COMPOSE's CI mode (`pipeline.checksModes`). A Merge now is untested
 *  only when nothing was resolved and no lockfile drifted from its unit's
 *  head: either way the tree is one nobody tested, so CI runs (#LOGS-2). */
export function trainChecksMode(input: {
  mode: `test` | `untested`
  composed: boolean
  /** Any composed unit's `resolved` non-empty. */
  resolved: boolean
  /** Any composed unit's `drifted` non-empty (`repoPolicy.lockfiles`). */
  drifted: boolean
  /** A lone unit whose composed tree is its tested_sha's. */
  proven: boolean
  workflows: readonly string[]
}): TrainChecksMode {
  if (!input.composed) return `none`
  if (input.mode === `untested` && !input.resolved && !input.drifted) return `skipped`
  if (input.proven) return `proven`
  return input.workflows.length > 0 ? `dispatched` : `none`
}


/** The pipeline's result + the forge's checks at its head (oldest first:
 *  the newest per name wins) + the base branch's required check names. */
export function trainVerdict(input: {
  outcome: TrainPipelineOutcome | null
  checks: readonly TrainCheck[]
  required: readonly string[]
}): TrainVerdict {
  const o = input.outcome
  if (!o) return { state: `pending`, head: null, composed: [], conflicts: [] }
  const composed = o.units.filter((u) => u.outcome === `composed`).map((u) => u.prs)
  const conflicts = o.units
    .filter((u) => u.outcome === `conflict`)
    .map((u) => ({ prs: u.prs, files: u.files, ...(u.against ? { against: u.against } : {}) }))
  const base = { head: o.head, composed, conflicts }
  if (o.error) return { state: `error`, head: o.head, composed: [], conflicts: [] }
  if (composed.length === 0) return { state: `conflict`, ...base }
  if (o.checks !== `dispatched` || input.required.length === 0) return { state: `green`, ...base }
  const latest = new Map<string, TrainCheck>()
  for (const c of input.checks) latest.set(c.name, c)
  let pending = false
  for (const name of input.required) {
    const c = latest.get(name)
    if (!c || c.status !== `completed`) pending = true
    else if (!PASSING.has(c.conclusion ?? ``)) return { state: `red`, ...base }
  }
  return { state: pending ? `pending` : `green`, ...base }
}

// ── pr_merge TEXTS ─────────────────────────────────────────────────────────

function list(items: readonly string[]): string {
  const shown = items.slice(0, 5).join(`, `)
  return items.length > 5 ? `${shown} +${items.length - 5}` : shown
}

function prs(numbers: readonly number[]): string {
  return numbers.map((n) => `#${n}`).join(`, `)
}

/** The merged line: the PR's session (no issues) and the Assistant's one
 *  line per landed batch (with its issues, deduplicated in PR order). */
export function mergedText(numbers: readonly number[], base: string, issues: readonly string[] = []): string {
  const unique = [...new Set(issues)]
  const tail = unique.length > 0 ? ` (issues: ${list(unique)})` : ``
  return `Merged ${prs(numbers)} into ${base}${tail}.`
}

export function conflictText(number: number, base: string, files: readonly string[], attempts: number): string {
  return `Auto-merge of #${number} hit a conflict with ${base} in ${list(files)}. Rebase onto ${base}, resolve and push: it re-queues by itself (bounce ${attempts} of ${MERGE_TRAIN_LIMITS.bounces}).`
}

export function failingText(
  number: number,
  failing: ReadonlyArray<{ name: string; detailsUrl?: string | null }>,
  attempts: number
): string {
  const named = list(failing.map((c) => (c.detailsUrl ? `${c.name} (${c.detailsUrl})` : c.name)))
  return `Auto-merge of #${number} failed in the merge train: ${named}. Fix and push: it re-queues by itself (bounce ${attempts} of ${MERGE_TRAIN_LIMITS.bounces}).`
}

export function heldText(number: number, reason: HoldReason): string {
  if (reason === `person`) return `A person put #${number} on hold. It does not merge until they resume it.`
  const why =
    reason === `bounces`
      ? ` after ${MERGE_TRAIN_LIMITS.bounces} bounces`
      : `: not merged within ${MERGE_TRAIN_LIMITS.hours} hours`
  return `Auto-merge of #${number} stopped${why}. It waits for a person (Needs you); do not retry.`
}

/** The Assistant's held line (person holds never reach it). */
export function stuckText(number: number, reason: Exclude<HoldReason, `person`>): string {
  return `Auto-merge stuck on #${number} (${reason === `bounces` ? `${MERGE_TRAIN_LIMITS.bounces} bounces` : `${MERGE_TRAIN_LIMITS.hours} hours`}).`
}

// ── ROUTING ────────────────────────────────────────────────────────────────

export interface RouteInput {
  outcome: MergeOutcome
  holdReason?: HoldReason | null
  opener: { id: string; live: boolean }
  root: { id: string; ended: boolean }
  /** The root starter's Assistant conversation (agent-settings.json role
   *  `assistant`), null when they have none (never created for this). */
  assistant: string | null
}

export interface RouteTarget {
  sessionId: string
  /** `pr` = the PR's own line; `assistant` = the batch line. */
  line: `pr` | `assistant`
  wakes: boolean
}

/** Where one PR's `pr_merge` goes: the opener while live, else the root (an
 *  ended root keeps it for display only, never woken); `merged` and a
 *  non-person `held` ALSO reach the root starter's Assistant, unless that
 *  conversation already is the target. */
export function prMergeRoute(input: RouteInput): RouteTarget[] {
  const own: RouteTarget = input.opener.live
    ? { sessionId: input.opener.id, line: `pr`, wakes: true }
    : { sessionId: input.root.id, line: `pr`, wakes: !input.root.ended }
  const tellsAssistant =
    input.outcome === `merged` || (input.outcome === `held` && input.holdReason !== `person`)
  if (!tellsAssistant || input.assistant === null || input.assistant === own.sessionId) return [own]
  return [own, { sessionId: input.assistant, line: `assistant`, wakes: true }]
}

export interface LandedPr {
  number: number
  /** The root starter's Assistant conversation, or null. */
  assistant: string | null
  issues: string[]
}

/** ONE merged line per Assistant per landed batch: its PRs in landing
 *  order, their issues deduplicated; Assistants in first-PR order. */
export function assistantBatchLines(
  landed: readonly LandedPr[],
  base: string
): Array<{ sessionId: string; text: string }> {
  const groups = new Map<string, LandedPr[]>()
  for (const pr of landed) {
    if (pr.assistant === null) continue
    groups.set(pr.assistant, [...(groups.get(pr.assistant) ?? []), pr])
  }
  return [...groups].map(([sessionId, rows]) => ({
    sessionId,
    text: mergedText(
      rows.map((r) => r.number),
      base,
      rows.flatMap((r) => r.issues)
    ),
  }))
}

// ── NEEDS YOU ──────────────────────────────────────────────────────────────

export interface StuckInput {
  number: number
  state: PrMergeState
  holdReason: HoldReason | null
  /** When the opener's device went offline (ISO), null while online. */
  deviceOfflineSince: string | null
  now: string
  owner: boolean
}

export interface OfflineSinceInput {
  /** The PR has an opener (its `sessionId` session, else its root) with a device. */
  opener: boolean
  /** The opener's device row: presence + last heartbeat; null = none synced. */
  device: { online: boolean; lastSeenAt: string | null } | null
  /** The PR's `updatedAt`: its last write, the move to conflict/failing included. */
  updatedAt: string
}

/** `stuckRow`'s `deviceOfflineSince`: null while the opener's device is
 *  online (or there is none); else its last heartbeat, and an UNKNOWN
 *  last-seen = offline since the PR went conflict/failing. */
export function offlineSince(input: OfflineSinceInput): string | null {
  if (!input.opener || input.device?.online) return null
  return input.device?.lastSeenAt ?? input.updatedAt
}

export interface StuckRow {
  title: string
  meta: string
  actions: Array<`retry` | `merge-now` | `close-pr`>
}

/** The Needs you row of a stuck PR, or null: `held` by the limits, or a
 *  `conflict`/`failing` whose opener's device is offline 2 h+. A person's
 *  Hold is a decision already made: never a row. Actions: Retry (= Resume),
 *  Merge now (owners), Close PR. */
export function stuckRow(input: StuckInput): StuckRow | null {
  const copy = MERGE_TRAIN_COPY.needsYou
  let meta: string | null = null
  if (input.state === `held` && input.holdReason === `bounces`)
    meta = copy.bounces.replace(`{count}`, String(MERGE_TRAIN_LIMITS.bounces))
  else if (input.state === `held` && input.holdReason === `timeout`) meta = copy.timeout
  else if (
    (input.state === `conflict` || input.state === `failing`) &&
    input.deviceOfflineSince !== null &&
    Date.parse(input.now) - Date.parse(input.deviceOfflineSince) >= MERGE_TRAIN_LIMITS.offlineHours * HOUR
  )
    meta = copy.offline
  if (meta === null) return null
  return {
    title: copy.title.replace(`{number}`, String(input.number)),
    meta,
    actions: input.owner ? [`retry`, `merge-now`, `close-pr`] : [`retry`, `close-pr`],
  }
}

// ── THE STATUS PILL ────────────────────────────────────────────────────────

export type PillGlyph = `spinner` | `pr-draft` | `ci-queued` | `pr-merged` | `pr-conflict` | `ci-failed` | `pr-hold`
export type PillTone = `muted` | `red`

export interface StatusPillInput {
  state: PrMergeState
  wait: MergeWait | null
  position?: number | null
  /** PRs in the testing batch, this one included. */
  batchSize?: number | null
  holdReason?: HoldReason | null
  stack: boolean
  /** The PR's check rows (a status pill with none is readonly). */
  checks: number
}

export interface StatusPillView {
  label: string
  glyph: PillGlyph
  tone: PillTone
  primary: boolean
  /** approve = the merge confirm, then the queue; checks = the Checks
   *  submenu; null = readonly. */
  tap: `approve` | `checks` | null
  confirm?: `merge` | `stack`
}

export function ordinal(n: number): string {
  const rem100 = n % 100
  if (rem100 >= 11 && rem100 <= 13) return `${n}th`
  const suffix = ({ 1: `st`, 2: `nd`, 3: `rd` } as Record<number, string>)[n % 10] ?? `th`
  return `${n}${suffix}`
}

export function statusPill(input: StatusPillInput): StatusPillView {
  const copy = MERGE_TRAIN_COPY.pill
  const status = (label: string, glyph: PillGlyph, tone: PillTone = `muted`): StatusPillView => ({
    label,
    glyph,
    tone,
    primary: false,
    tap: input.checks > 0 ? `checks` : null,
  })
  switch (input.state) {
    case `draft`:
      return status(copy.draft, `pr-draft`)
    case `checks`:
      if (input.wait === `approval`)
        return {
          label: copy.needsApproval,
          glyph: `pr-merged`,
          tone: `muted`,
          primary: true,
          tap: `approve`,
          confirm: input.stack ? `stack` : `merge`,
        }
      return status(input.wait === `session` ? copy.sessionWorking : copy.checksRunning, `spinner`)
    case `queued`:
      return status(
        input.position ? copy.queuedAt.replace(`{position}`, ordinal(input.position)) : copy.queued,
        `ci-queued`
      )
    case `testing`: {
      const others = (input.batchSize ?? 1) - 1
      return status(others > 0 ? copy.testingWith.replace(`{count}`, String(others)) : copy.testing, `spinner`)
    }
    case `merged`:
      return status(copy.merged, `pr-merged`)
    case `conflict`:
      return status(copy.conflict, `pr-conflict`, `red`)
    case `failing`:
      return status(copy.failing, `ci-failed`, `red`)
    case `held`:
      return input.holdReason === `person` ? status(copy.onHold, `pr-hold`) : status(copy.stuck, `pr-hold`, `red`)
  }
}

// ── THE `…` MENU ───────────────────────────────────────────────────────────

export type MenuEntryId = `checks` | `merge-now` | `hold` | `resume` | `close-pr`

export interface MenuInput {
  /** contract prState of the guide PR, null = none. */
  state: string | null
  member: boolean
  owner: boolean
  checks: number
  mergeState: PrMergeState | null
}

/** The header `…` (the chat menu): checks
 *  (any viewer, any PR with rows), Merge now (owners; a live PR not draft,
 *  merged or conflict), Hold | Resume auto-merge (members; a live PR not
 *  draft or merged: Resume while held), Close PR (members, live PR). */
export function mergeMenuEntries(input: MenuInput): MenuEntryId[] {
  if (input.state === null) return []
  const entries: MenuEntryId[] = []
  if (input.checks > 0) entries.push(`checks`)
  const live = input.state === `open` || input.state === `draft`
  if (!live || !input.member) return entries
  const ms = input.mergeState ?? `checks`
  if (input.owner && ms !== `draft` && ms !== `merged` && ms !== `conflict`) entries.push(`merge-now`)
  if (ms !== `draft` && ms !== `merged`) entries.push(ms === `held` ? `resume` : `hold`)
  entries.push(`close-pr`)
  return entries
}
