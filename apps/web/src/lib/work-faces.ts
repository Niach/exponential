import { isCodingSessionStale } from "@exp/db-schema/domain"
import { contract } from "@exp/domain-contract"
import { runIsStaleEnd } from "@/lib/past-runs"
import type { SessionConfigState } from "@/lib/agent-feed"
import {
  GUIDE_CHANGES_TOPIC,
  guideCoverage,
  guideSectionCaption,
  hasSessionResults,
  sessionResultPrUrls,
  type GuideCoverageFile,
  type SessionDotTone,
} from "@exp/ui"
import { formatTurnDuration } from "@/lib/working-caption"
import type { SessionStatusTone } from "@/lib/coding-session-display"

// EXP-893: the PHONE's Work screen — one screen per subject (an issue, or a
// session) with up to three FACES held as screen state, never as navigation:
// `issue`, `run` and `guide` (EXP-1251: Changes + Results merged into the
// Guide, its diff counts moved into the body). EXP-1150: the faces are TABS — a
// segmented strip under the header (the ONE segmented control every list
// strip wears) names every available face in its fixed order, and a
// horizontal swipe on the face's body moves to the neighbour. The header
// band carries the Merge PR pill beside the tabs on EVERY face while the PR
// is open (the one merge of the phone Work screen); the bottom bar keeps
// only the face's OWN controls, and Stop / Resume sit in the nav bar's
// trailing slot while the Run face shows. These are the PURE
// rules every phone client mirrors byte for byte: iOS
// `ExpCore/Domain/WorkFaces.swift`, Android `domain/WorkFaces.kt` — same
// names, same cases, same test names.

/** The three faces. EXP-1251: `guide` = the run's Guide (its published
 *  sections + pictures) over the diff (the run's live diff, else the issue's
 *  open PR), mirrored by the natives. */
export type WorkFaceKind = `issue` | `run` | `guide`

export const ISSUE_FACE_LABEL = `Issue`
export const RUN_FACE_LABEL = `Run`
/** EXP-886: the Run face's label with MORE THAN ONE own run on the issue. */
export const RUNS_FACE_LABEL = `Runs`
/** EXP-1251: the Guide face (contract `diffUi.guideFace`). */
export const GUIDE_FACE_LABEL = contract.diffUi.guideFace
/** EXP-933: the transcript card under a settled `sessions_guide` call that
 *  switches the run to its Guide face. */
export const OPEN_RESULTS_LABEL = `Open ${GUIDE_FACE_LABEL}`
/** The Start circle's label — the Run face's bar once the shown run ended
 *  for good, the Issue face's bar always. */
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
    case `guide`:
      return GUIDE_FACE_LABEL
  }
}

/** The faces a subject can show, in their fixed order. EXP-1251: the Guide
 *  shows when the run published results OR there is a diff (live, PR or
 *  branch), independent of Run: an issue with an open PR and no run of mine
 *  still has its PR files. It comes last: what the run did, then its Guide. */
export function availableFaces(input: {
  hasIssue: boolean
  hasRun: boolean
  hasResults: boolean
  hasDiff: boolean
}): WorkFaceKind[] {
  const faces: WorkFaceKind[] = []
  if (input.hasIssue) faces.push(`issue`)
  if (input.hasRun) faces.push(`run`)
  if (input.hasResults || input.hasDiff) faces.push(`guide`)
  return faces
}

// ── EXP-1251: the Guide's URL and its section pages ────────────────────────
// `?view=guide` is the Guide face on BOTH routes (the issue's, the run's);
// `&section=` opens a section's changes as a page under it: a 1-based
// section number, `lead` (the Summary's own files), `other` (the automatic
// Other changes) or `all` (Show complete diff); `&file=` = the file in focus.
// The legacy `?view=diff` (= the complete diff) and `?view=results` (= the
// Guide) normalise into it, so old links and agent messages keep landing.

export type GuideSectionKey = number | `lead` | `other` | `all`

export interface GuideSearch {
  view?: `guide`
  section?: GuideSectionKey
  file?: string
}

/** A `?section=` value, else undefined (a garbage value = the Guide). */
export function parseGuideSection(raw: unknown): GuideSectionKey | undefined {
  if (raw === `lead` || raw === `other` || raw === `all`) return raw
  const value =
    typeof raw === `number` ? raw : typeof raw === `string` && /^\d+$/.test(raw) ? Number(raw) : NaN
  return Number.isInteger(value) && value >= 1 ? value : undefined
}

/** Both routes' `validateSearch` for the face keys: `guide` stands, `results`
 *  becomes the Guide, `diff` its complete diff; a section or file without
 *  the Guide is dropped. */
export function parseGuideSearch(search: Record<string, unknown>): GuideSearch {
  const view = search.view
  if (view !== `guide` && view !== `results` && view !== `diff`) return {}
  const file =
    typeof search.file === `string` && search.file ? search.file : undefined
  const section =
    view === `diff` ? (parseGuideSection(search.section) ?? `all`) : view === `guide` ? parseGuideSection(search.section) : undefined
  return {
    view: `guide`,
    ...(section !== undefined ? { section } : {}),
    ...(section !== undefined && file ? { file } : {}),
  }
}

/** The search a navigation to the Guide (or one of its pages) writes. */
export function guideSearch(input: {
  from?: string
  section?: GuideSectionKey
  file?: string | null
}): { from?: string; view: `guide`; section?: GuideSectionKey; file?: string } {
  return {
    ...(input.from ? { from: input.from } : {}),
    view: `guide`,
    ...(input.section !== undefined ? { section: input.section } : {}),
    ...(input.section !== undefined && input.file ? { file: input.file } : {}),
  }
}

/** A section page: the back row's caption (`02 / 06`, numbered sections
 *  only), its title, the files it covers and their summed counts. */
export interface GuideSectionPage<F extends GuideCoverageFile> {
  section: GuideSectionKey
  caption: string | null
  title: string
  files: F[]
  additions: number
  deletions: number
}

/** The page `section` opens over `files` (the diff the Guide counts): the
 *  same coverage the Guide's rows read, so a page never shows a file its row
 *  did not count. Null = no such section (a stale link) or no diff loaded. */
export function guideSectionPage<
  G extends { topic: string; files?: readonly string[] | null },
  F extends GuideCoverageFile,
>(
  groups: readonly G[],
  files: readonly F[] | null | undefined,
  section: GuideSectionKey,
  /** False for the PR-body fallback (an unnumbered Guide): no `01 / 01`
   *  caption on its page, as its band carries no number. */
  numbered = true
): GuideSectionPage<F> | null {
  if (!files) return null
  const coverage = guideCoverage(groups, files)
  const page = (
    title: string,
    caption: string | null,
    set: { files: F[]; additions: number; deletions: number } | null
  ): GuideSectionPage<F> | null =>
    set
      ? { section, caption, title, files: set.files, additions: set.additions, deletions: set.deletions }
      : null
  if (section === `all`) return page(GUIDE_CHANGES_TOPIC, null, coverage.complete)
  if (section === `other`) {
    return coverage.other ? page(coverage.other.topic, null, coverage.other.changes) : null
  }
  if (section === `lead`) {
    return coverage.lead ? page(coverage.lead.group.topic, null, coverage.lead.changes) : null
  }
  const hit = coverage.sections.find((entry) => entry.index === section)
  return hit
    ? page(
        hit.group.topic,
        numbered ? guideSectionCaption(hit.index, hit.total) : null,
        hit.changes
      )
    : null
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

/**
 * EXP-933: the run whose Results an ISSUE shows. Results live on the run but
 * sync team-wide, and an agent message deep-links a teammate to the ISSUE's
 * Results — so this is not limited to my runs: `codingTarget` when that run
 * has results, else the newest run on the issue (any member) that has any,
 * else (EXP-1251) the newest run anywhere whose topics are tagged with the
 * issue's PR (a run that stacked this issue's PR on its own).
 * Fixture: `packages/domain-contract/fixtures/session-results.json` (×4).
 */
export function issueResultsRun<T extends CodingTargetRow & { results?: unknown }>(
  rows: readonly T[],
  issueId: string,
  boundId: string | null | undefined,
  me: string | undefined,
  now: Date,
  issuePrUrl?: string | null
): T | null {
  const own = codingTarget(rows, issueId, boundId, me, now)
  if (own && hasSessionResults(own.results)) return own
  const onIssue = newest(
    rows.filter((row) => row.issueId === issueId && hasSessionResults(row.results))
  )
  if (onIssue || !issuePrUrl) return onIssue
  return newest(rows.filter((row) => sessionResultPrUrls(row.results).includes(issuePrUrl)))
}

/** EXP-1251: the rows `issueResultsRun` reads on an issue page — the
 *  issue's own runs plus every team run that published results (a run on
 *  ANOTHER issue, or a batch run, may tag topics with this issue's PR). */
export function resultsCandidateRows<T extends { id: string; results?: unknown }>(
  issueRows: readonly T[],
  teamRows: readonly T[]
): T[] {
  const seen = new Set(issueRows.map((row) => row.id))
  return [
    ...issueRows,
    ...teamRows.filter((row) => !seen.has(row.id) && hasSessionResults(row.results)),
  ]
}

/** EXP-934: the header's `…` CONTEXT MENU (Share · Move to board · Unmark
 *  duplicate · Delete issue) belongs to the issue, so it shows on the ISSUE
 *  face alone. On Run and Guide the trailing slot carries the run's
 *  own verb (Stop / Resume) and nothing else — a Delete issue sitting beside a
 *  running agent acts on a subject that face is not even showing. */
export function faceShowsContextMenu(face: WorkFaceKind): boolean {
  return face === `issue`
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

/** EXP-1150: which way the finger went. `left` = the content followed the
 *  finger leftwards, so the NEXT face slides in; `right` = the previous. */
export type SwipeDirection = `left` | `right`

/** The face (or any phone tab, EXP-1190) a horizontal swipe on the body
 *  lands on: the neighbour in the strip's order, `null` at either end (or
 *  when the shown face is not in the strip at all — nothing to swipe from). */
export function swipeTarget<T>(
  faces: readonly T[],
  shown: T,
  direction: SwipeDirection
): T | null {
  const index = faces.indexOf(shown)
  if (index < 0) return null
  const next = direction === `left` ? index + 1 : index - 1
  return faces[next] ?? null
}

/** Where a face lands when it vanishes under the reader (the diff cleared
 *  and the results list was empty, the run row went): guide → run → issue.
 *  `null` = nothing left. */
export function fallbackFace(
  shown: WorkFaceKind,
  available: readonly WorkFaceKind[]
): WorkFaceKind | null {
  if (available.includes(shown)) return shown
  const order: WorkFaceKind[] =
    shown === `guide`
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

/** EXP-1162: a face tab's state dot — the session-dot tones (×4). */
export type FaceDotTone = `running` | `needs_input` | `review`

/**
 * EXP-1162: which face TABS carry a state dot (contract `detail-chrome.json`
 * `faceDots`, ×4). The header title carries none. The Run tab says its run
 * is live — amber while it waits on a person; an open pull request dots the
 * Guide tab (EXP-1251). An ended run carries no dot, and a face that is not
 * on show never does.
 */
export function faceDots(input: {
  faces: readonly WorkFaceKind[]
  runLive: boolean
  needsInput: boolean
  prOpen: boolean
}): Partial<Record<WorkFaceKind, FaceDotTone>> {
  const dots: Partial<Record<WorkFaceKind, FaceDotTone>> = {}
  if (input.runLive && input.faces.includes(`run`)) {
    dots.run = input.needsInput ? `needs_input` : `running`
  }
  if (input.prOpen && input.faces.includes(`guide`)) dots.guide = `review`
  return dots
}

// ── EXP-1175: the Run face's status row + Show work ─────────────────────────
// The Run face opens as the THREAD: one status row (the agent's run mark as
// the spinner, this caption, the last tool line muted, `Show work` on the
// right) over the run's published results in publish order (`sessionThread`,
// @exp/ui), pending plan/question cards still in place. Show work swaps the
// thread for the full transcript IN PLACE; the choice is remembered per user.
// Fixture `packages/domain-contract/fixtures/run-row.json` (×4).

/** The viewer's Show work preference before they ever touched it. */
export const SHOW_WORK_DEFAULT = false
export const SHOW_WORK_LABEL = `Show work`
export const HIDE_WORK_LABEL = `Hide work`

/** The row's trailing button: what pressing it DOES next. */
export function showWorkLabel(showWork: boolean): string {
  return showWork ? HIDE_WORK_LABEL : SHOW_WORK_LABEL
}

/** The ×4 display state (`sessionDisplayState`) plus the two the row alone
 *  tells apart: a paused (offline host) run and an ended one. */
export type RunRowState =
  | `working`
  | `needs_input`
  | `paused`
  | `review`
  | `done`
  | `ended`

/** The row's state: the VIEWER's live signals folded over the synced display
 *  state (`sessionDisplayState`), so the row never contradicts the Run tab's
 *  mark — paused first, then ended, then needs input (a pending plan/question
 *  the viewer sees, or the synced flag), then working (the viewer's working
 *  predicate, or the synced busy flag), else review | done. Fixture
 *  `run-row.json` `states` (×4). */
export function runRowState(input: {
  paused: boolean
  ended: boolean
  awaitingInput: boolean
  working: boolean
  display: `needs_input` | `working` | `review` | `done`
}): RunRowState {
  if (input.paused) return `paused`
  if (input.ended) return `ended`
  if (input.awaitingInput || input.display === `needs_input`) return `needs_input`
  if (input.working || input.display === `working`) return `working`
  return input.display
}

export interface RunRowCaptionInput {
  state: RunRowState
  /** The resolved device label (`device.label || session.deviceLabel || 'Desktop'`). */
  device: string
  startedAt: Date | string | number | null | undefined
  /** The run's `ended_at`, else its `updated_at` — an ended run's end. */
  endedAt: Date | string | number | null | undefined
  now: Date | string | number
}

function stampOrNull(value: Date | string | number | null | undefined): number | null {
  if (value === null || value === undefined) return null
  const ms =
    typeof value === `number`
      ? value
      : (typeof value === `string` ? new Date(value) : value).getTime()
  return Number.isFinite(ms) ? ms : null
}

/** The status row's first line and tone: `Building on <device> · <elapsed>`
 *  while it works (now − start), `Ended on <device> · <elapsed>` once it is
 *  over (end − start), else the session list row's words. The elapsed part
 *  is the working caption's ladder and drops when a stamp is missing. */
export function runRowCaption(
  input: RunRowCaptionInput
): { text: string; tone: SessionStatusTone } {
  const device = input.device
  switch (input.state) {
    case `paused`:
      return { text: `Paused · ${device}`, tone: `muted` }
    case `needs_input`:
      return { text: `Needs input · ${device}`, tone: `amber` }
    case `review`:
      return { text: `Ready for review · ${device}`, tone: `emerald` }
    case `done`:
      return { text: `Done · ${device}`, tone: `sky` }
    case `working`:
    case `ended`: {
      const verb = input.state === `working` ? `Building on` : `Ended on`
      const start = stampOrNull(input.startedAt)
      const end = stampOrNull(input.state === `working` ? input.now : input.endedAt)
      const elapsed =
        start !== null && end !== null ? ` · ${formatTurnDuration(end - start)}` : ``
      return { text: `${verb} ${device}${elapsed}`, tone: `muted` }
    }
  }
}

// ── EXP-1245: one status row PER TURN ───────────────────────────────────────
// The owner's thread is a conversation of turns (`sessionTurns`, @exp/ui):
// every turn draws its own status row. A settled turn reads `Done on
// <device> · <turn duration>`; the open (newest) turn reads the run's row
// (`runRowCaption`) timed from the TURN's start, not the run's; a sent
// message still waiting for its turn has no row. Fixture `run-row.json`
// `turnCaptions` (×4).

export interface TurnRowCaptionInput {
  turn: { startedAt: number | string | null; endedAt: number | string | null }
  /** The run's row state (`runRowState`), read by the open turn only. */
  state: RunRowState
  device: string
  /** The run's end (`ended_at`, else `updated_at`) for an open turn of an
   *  ended run. */
  runEndedAt: Date | string | number | null | undefined
  now: Date | string | number
  /** False = the settled turn's end is not a real observation (the first
   *  turn closed only by the next message, its own end never seen): the
   *  row drops the duration rather than count the idle gap. */
  endKnown?: boolean
}

export function turnRowCaption(
  input: TurnRowCaptionInput
): { text: string; tone: SessionStatusTone } | null {
  const start = stampOrNull(input.turn.startedAt)
  if (start === null) return null
  const end = stampOrNull(input.turn.endedAt)
  if (end !== null) {
    if (input.endKnown === false) return { text: `Done on ${input.device}`, tone: `muted` }
    return {
      text: `Done on ${input.device} · ${formatTurnDuration(end - start)}`,
      tone: `muted`,
    }
  }
  return runRowCaption({
    state: input.state,
    device: input.device,
    startedAt: start,
    endedAt: input.runEndedAt,
    now: input.now,
  })
}
