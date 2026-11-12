// NEW-1 (plan 3.7): the EXACT issue lifecycle a session drives, ONE rule
// ×4. fixtures/issue-lifecycle.json holds the table + cases; the server
// (`lib/sessions/issue-lifecycle.ts`) is the only writer, the natives
// mirror the table only where they render it.

import fixture from "../fixtures/issue-lifecycle.json" with { type: "json" }

export type IssueLifecycleEvent = `taken` | `pr_opened` | `pr_merged` | `unlinked` | `filed`

export type IssueLifecycleTarget = `backlog` | `in_progress` | `in_review` | `done`

export interface IssueLifecycleState {
  /** The issue's anchor status (contract issueStatus). */
  status: string
  /** A live (open or draft) PR links it. */
  openPr: boolean
  /** Live sessions with a repository that hold it AFTER the event. */
  workers: number
}

/** A session that takes issues: it has a repository (its branch comes at
 *  claim). The fixture names the field. */
export const ISSUE_LIFECYCLE_WORKER_FIELD = fixture.worker

interface Rule {
  event: string
  from: string[]
  to: string
  when?: string
}

const RULES = fixture.rules as Rule[]

/** The anchor status the issue moves to on `event`, or null. */
export function issueLifecycleTarget(
  event: IssueLifecycleEvent,
  state: IssueLifecycleState
): IssueLifecycleTarget | null {
  // `filed` (a session created the issue) holds without taking: never a move.
  if (event === `filed`) return null
  const rule = RULES.find((entry) => entry.event === event)
  if (!rule || !rule.from.includes(state.status)) return null
  if (event === `unlinked` && (state.openPr || state.workers > 0)) return null
  return rule.to as IssueLifecycleTarget
}

export const ISSUE_LIFECYCLE_CASES = fixture.cases as {
  name: string
  input: { event: IssueLifecycleEvent } & IssueLifecycleState
  expected: IssueLifecycleTarget | null
}[]

/** EXP-1706: the watchdog's live-PR reconciler (fixture `reconcile`). */
export const PR_RECONCILE = {
  staleMs: fixture.reconcile.staleMinutes * 60_000,
  batch: fixture.reconcile.batch,
} as const

// ── EXP-1706: the issues a PR links (fixture `links`) ─────────────────────

const IDENT = `#?([A-Za-z][A-Za-z0-9]*-\\d+)(?![\\w-])`
const IDENT_STRICT = `#?([A-Z][A-Z0-9]*-\\d+)(?![\\w-])`
const SEP = `\\s*(?:,|&|\\+|/|\\band\\b)?\\s*`
/** One ident prefix: `EDE-13, EDE-14:`. */
const TITLE_PREFIX = new RegExp(`^\\s*((?:${IDENT_STRICT}${SEP})+?)\\s*:\\s*`)
const CLOSING_LINE = new RegExp(
  `^\\s*(?:[-*+]\\s+)?(?:fix|fixes|fixed|close|closes|closed|resolve|resolves|resolved)\\b\\s*:?\\s+(.*)$`,
  `i`
)

function identsIn(text: string, source: string, flags = `g`): string[] {
  return [...text.matchAll(new RegExp(source, flags))].map((match) => match[1]!.toUpperCase())
}

/** The head branch's trailing ident (`exp/EXP-12`, `feature/MET-3`). */
export function branchIdentifier(branch: string): string | null {
  const match = branch.match(/(?:^|\/)([A-Z][A-Z0-9]*-\d+)$/)
  return match ? match[1]! : null
}

/** The title's ident prefix (every leading `IDENT[, IDENT]:` run, each
 *  ident once) and the rest. */
export function splitTitlePrefix(title: string): { idents: string[]; rest: string } {
  const idents: string[] = []
  let rest = title
  for (let match = rest.match(TITLE_PREFIX); match; match = rest.match(TITLE_PREFIX)) {
    for (const ident of identsIn(match[1]!, IDENT_STRICT)) if (!idents.includes(ident)) idents.push(ident)
    rest = rest.slice(match[0].length)
  }
  return { idents, rest }
}

/** The idents of the body's closing lines (`Fixes #EXP-12`). */
export function closingIdentifiers(body: string): string[] {
  return body.split(/\r?\n/).flatMap((line) => {
    const match = line.match(CLOSING_LINE)
    return match ? identsIn(match[1]!, IDENT) : []
  })
}

export interface PrLinksInput {
  /** The opener's explicit ids (as identifiers), null = omitted. */
  issueIds: readonly string[] | null
  branch: string
  title: string
  body: string
  /** The session's share; joins only when `issueIds` is omitted. */
  share: readonly string[]
  /** Linked idents that have sub-issues. */
  parents: readonly string[]
}

/** EXP-1706: the identifiers a PR links, and the ones its own events close. */
export function prLinks(input: PrLinksInput): { link: string[]; close: string[] } {
  const branch = branchIdentifier(input.branch)
  const link = [
    ...new Set(
      [
        ...(input.issueIds ?? []),
        ...(branch ? [branch] : []),
        ...splitTitlePrefix(input.title).idents,
        ...closingIdentifiers(input.body),
        ...(input.issueIds ? [] : input.share),
      ].map((ident) => ident.toUpperCase())
    ),
  ]
  const parents = new Set(input.parents.map((ident) => ident.toUpperCase()))
  return { link, close: link.filter((ident) => !parents.has(ident)) }
}

/** EXP-1706: the squash commit's title (fixture `squashTitle`). */
export function squashTitle(identifier: string | null, title: string, number: number): string {
  const { idents, rest } = splitTitlePrefix(title)
  if (idents.length > 0) return `${idents.join(`, `)}: ${rest} (#${number})`
  return identifier ? `${identifier}: ${title} (#${number})` : `${title} (#${number})`
}

export const PR_LINK_CASES = fixture.links.cases as {
  name: string
  input: PrLinksInput
  expected: { link: string[]; close: string[] }
}[]

export const SQUASH_TITLE_CASES = fixture.squashTitle.cases as {
  name: string
  input: { identifier: string | null; title: string; number: number }
  expected: string
}[]

// ── #EXP-1797: the issue trash (fixture `trash`) ──────────────────────────

export const ISSUE_TRASH = fixture.trash

/** How long a deleted issue stays restorable before the sweep purges it. */
export const ISSUE_TRASH_RETENTION_MS = fixture.trash.retentionDays * 24 * 60 * 60 * 1000

/** The trash's team-stream event names. */
export const ISSUE_TRASH_EVENTS = fixture.trash.events

/** The trash band's copy ×4. */
export const ISSUE_TRASH_COPY = fixture.trash.copy

const HOUR_MS = 60 * 60 * 1000

/** A trashed issue's row caption: the purge countdown, in hours inside the
 *  last two days, else in whole days (both rounded up). */
export function issueTrashCaption(purgeAt: Date | string | null, now: Date = new Date()): string {
  if (purgeAt == null) return `Purges soon`
  const ms = new Date(purgeAt).getTime() - now.getTime()
  if (ms <= 0) return `Purging soon`
  if (ms <= 48 * HOUR_MS) return `Purges in ~${Math.ceil(ms / HOUR_MS)}h`
  return `Purges in ~${Math.ceil(ms / (24 * HOUR_MS))}d`
}

/** The trash row's pills for a member of `role`, display order. */
export function issueTrashActions(role: string): (`restore` | `purge`)[] {
  return role === fixture.trash.access.purge ? [`restore`, `purge`] : [`restore`]
}

/** The row text: `{identifier} {title}`. */
export function issueTrashRowText(identifier: string, title: string): string {
  return fixture.trash.copy.row.replace(`{identifier}`, identifier).replace(`{title}`, title)
}

/** The sweep's rule: due once deletedAt + retention has passed. */
export function isIssuePurgeDue(deletedAt: Date | string | null, now: Date = new Date()): boolean {
  if (deletedAt == null) return false
  return new Date(deletedAt).getTime() + ISSUE_TRASH_RETENTION_MS <= now.getTime()
}

export const ISSUE_TRASH_CAPTION_CASES = fixture.trash.captionCases as {
  name: string
  input: { purgeAt: string | null; now: string }
  expected: string
}[]
export const ISSUE_TRASH_ACTION_CASES = fixture.trash.actionCases as {
  name: string
  input: { role: string }
  expected: string[]
}[]
export const ISSUE_PURGE_DUE_CASES = fixture.trash.purgeDueCases as {
  name: string
  input: { deletedAt: string | null; now: string }
  expected: boolean
}[]
