import { isCodingSessionStale } from "@exp/db-schema/domain"
import { runIsStaleEnd } from "@/lib/past-runs"
import type { SessionConfigState } from "@/lib/agent-feed"
import { type SessionDotTone } from "@exp/ui"

// EXP-893: the PHONE's Work screen — one screen per subject (an issue, or a
// session) with up to four FACES held as screen state, never as navigation:
// `issue`, `run`, `changes` and `results`. The desktop's face toggle (EXP-877)
// becomes a floating bottom-right circle that either switches straight to the
// one other face or opens a menu above itself; Stop / Resume sit in the nav
// bar's trailing slot only while the Run face shows. These are the PURE rules
// every phone client mirrors byte for byte: iOS
// `ExpCore/Domain/WorkFaces.swift`, Android `domain/WorkFaces.kt` — same
// names, same cases, same test names.

/** The four faces. `changes` is the run's diff, else the issue's open PR;
 *  `results` (EXP-879) is the run's published screenshots, mirrored by the
 *  natives. */
export type WorkFaceKind = `issue` | `run` | `changes` | `results`

export const ISSUE_FACE_LABEL = `Issue`
export const RUN_FACE_LABEL = `Run`
/** EXP-886: the Run face's label with MORE THAN ONE own run on the issue. */
export const RUNS_FACE_LABEL = `Runs`
export const CHANGES_FACE_LABEL = `Changes`
/** EXP-879: the run's published results. */
export const RESULTS_FACE_LABEL = `Results`
/** The switcher menu's extra row once the shown run ended for good. */
export const START_CODING_LABEL = `Start coding`

/** The steer composer's placeholder, byte-identical ×4 (desktop
 *  `steer-composer.tsx` / `session_screen.rs`). */
export const STEER_COMPOSER_PLACEHOLDER = `Type / for commands`
/** The composer footer's plan-mode word, blue while plan mode is on. */
export const PLAN_MODE_LABEL = `Plan mode`

export function faceLabel(face: WorkFaceKind, multipleRuns = false): string {
  switch (face) {
    case `issue`:
      return ISSUE_FACE_LABEL
    case `run`:
      return multipleRuns ? RUNS_FACE_LABEL : RUN_FACE_LABEL
    case `changes`:
      return CHANGES_FACE_LABEL
    case `results`:
      return RESULTS_FACE_LABEL
  }
}

/** The faces a subject can show, in their fixed order. Changes is independent
 *  of Run: an issue with an open PR and no run of mine still has its PR files.
 *  Results (EXP-879) comes last — what the run PUBLISHED, after what it did. */
export function availableFaces(input: {
  hasIssue: boolean
  hasRun: boolean
  hasChanges: boolean
  hasResults: boolean
}): WorkFaceKind[] {
  const faces: WorkFaceKind[] = []
  if (input.hasIssue) faces.push(`issue`)
  if (input.hasRun) faces.push(`run`)
  if (input.hasChanges) faces.push(`changes`)
  if (input.hasResults) faces.push(`results`)
  return faces
}

interface CodingTargetRow {
  id: string
  issueId: string | null
  userId: string
  status: string
  endedBy?: string | null
  startedAt: Date | string
  updatedAt: Date | string
}

function stamp(value: Date | string): number {
  const ms = (typeof value === `string` ? new Date(value) : value).getTime()
  return Number.isNaN(ms) ? 0 : ms
}

/** Live by status AND heartbeat: a `running` row whose machine went quiet
 *  past the staleness window is neither steerable nor stoppable. */
export function isSessionLive(
  row: Pick<CodingTargetRow, `status` | `endedBy` | `updatedAt`>,
  now: Date
): boolean {
  return (
    // EXP-888: a sweep end (`ended_by = stale`) is NOT an end — the host
    // ignores the flip and heartbeats the row back to `running`.
    (row.status === `running` ||
      row.status === `in_review` ||
      runIsStaleEnd(row)) &&
    !isCodingSessionStale(new Date(stamp(row.updatedAt)), now)
  )
}

function newest<T extends { id: string; startedAt: Date | string }>(
  rows: readonly T[]
): T | null {
  let best: T | null = null
  for (const row of rows) {
    if (
      !best ||
      stamp(row.startedAt) > stamp(best.startedAt) ||
      (stamp(row.startedAt) === stamp(best.startedAt) && row.id > best.id)
    ) {
      best = row
    }
  }
  return best
}

/** The run an issue's Work screen shows — desktop `work_header.rs`
 *  `coding_target`: the bound run when it is mine and live, else my newest
 *  live run on the issue, else my newest run at all. `null` = no run of mine. */
export function codingTarget<T extends CodingTargetRow>(
  rows: readonly T[],
  issueId: string,
  boundId: string | null | undefined,
  me: string | undefined,
  now: Date
): T | null {
  if (!me) return null
  const mine = rows.filter((row) => row.issueId === issueId && row.userId === me)
  const live = mine.filter((row) => isSessionLive(row, now))
  if (boundId) {
    const bound = live.find((row) => row.id === boundId)
    if (bound) return bound
  }
  return newest(live) ?? newest(mine)
}

export type PrimaryAction = `stop` | `resume` | `start` | `none`

/** The nav bar's trailing verb on the Run face, and the bottom-right circle's
 *  start glyph: Stop wins over everything; Resume needs an ended own run a
 *  machine can take; Start only for an issue subject that can be coded on. */
export function primaryAction(input: {
  ownLive: boolean
  ownEndedResumable: boolean
  canStart: boolean
}): PrimaryAction {
  if (input.ownLive) return `stop`
  if (input.ownEndedResumable) return `resume`
  if (input.canStart) return `start`
  return `none`
}

export type SwitcherTarget =
  | { kind: `face`; face: WorkFaceKind }
  | { kind: `run`; id: string }
  | { kind: `startCoding` }

/** What the switcher circle offers from the shown face: the OTHER faces in
 *  order, the Run face expanded into one row per own run when there are two
 *  or more (EXP-886), and `Start coding` first when the shown run ended and
 *  cannot be resumed (desktop shows Start in that state; on the phone the
 *  circle is the switcher, so the menu carries it). */
export function switcherTargets(
  faces: readonly WorkFaceKind[],
  shown: WorkFaceKind,
  runIds: readonly string[],
  shownRunId: string | null,
  offerStart: boolean
): SwitcherTarget[] {
  const targets: SwitcherTarget[] = []
  if (offerStart) targets.push({ kind: `startCoding` })
  for (const face of faces) {
    if (face === `run` && runIds.length >= 2) {
      for (const id of runIds) {
        if (shown === `run` && id === shownRunId) continue
        targets.push({ kind: `run`, id })
      }
      continue
    }
    if (face === shown) continue
    targets.push({ kind: `face`, face })
  }
  return targets
}

export type SwitcherMode =
  | { kind: `hidden` }
  | { kind: `toggle`; target: SwitcherTarget }
  | { kind: `menu`; targets: SwitcherTarget[] }

/** No target = no circle; exactly one = a direct switch wearing the
 *  destination's icon; two or more = the faces glyph and a menu above. */
export function switcherMode(targets: readonly SwitcherTarget[]): SwitcherMode {
  if (targets.length === 0) return { kind: `hidden` }
  if (targets.length === 1) return { kind: `toggle`, target: targets[0] }
  return { kind: `menu`, targets: [...targets] }
}

export type SwitcherBadge = SessionDotTone | `changes` | null

/** The circle's badge dot: off the Run face the shown session's state dot
 *  (none without a session); on the Run face a green dot while changes exist,
 *  so the reader knows a diff is waiting behind the switcher. */
export function switcherBadge(
  shown: WorkFaceKind,
  sessionTone: SessionDotTone | null,
  hasChanges: boolean
): SwitcherBadge {
  if (shown === `run`) return hasChanges ? `changes` : null
  return sessionTone
}

/** Where a face lands when it vanishes under the reader (the diff cleared,
 *  the results list was empty, the run row went): changes → run → issue, and
 *  results the same way (EXP-879). `null` = nothing left. */
export function fallbackFace(
  shown: WorkFaceKind,
  available: readonly WorkFaceKind[]
): WorkFaceKind | null {
  if (available.includes(shown)) return shown
  const order: WorkFaceKind[] =
    shown === `changes` || shown === `results`
      ? [`run`, `issue`]
      : shown === `run`
        ? [`issue`]
        : []
  return order.find((face) => available.includes(face)) ?? available[0] ?? null
}

/** The composer footer's model — the `model` config option's value, null
 *  when the engine reported none or a blank (web `lib/agent-feed.ts`). */
export function sessionModel(
  config: Pick<SessionConfigState, `options`> | null | undefined
): string | null {
  const value = config?.options.find((option) => option.id === `model`)?.value
  return value === undefined || value === `` ? null : value
}

/** The state dot's tone off the viewer phase — the `PhaseDot` rule: emerald
 *  while live, amber while it waits on a human (or went quiet), muted once it
 *  is over or paused. `connecting` says whether to pulse it. */
export function phaseDotTone(input: {
  live: boolean
  connecting: boolean
  awaitingInput: boolean
  paused: boolean
  stale: boolean
}): { tone: SessionDotTone; connecting: boolean } {
  const connecting = !input.paused && input.connecting
  const tone: SessionDotTone =
    !input.paused && input.live
      ? input.awaitingInput || input.stale
        ? `needs_input`
        : `running`
      : `muted`
  return { tone, connecting }
}
