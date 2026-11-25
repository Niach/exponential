import { and, asc, eq, inArray, isNull, lte, ne, or, sql, type SQL } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import { checksWakeText } from "@exp/domain-contract/pr-checks"
import { parseSessionWatchSpec, UUID_RE, type SessionWatchSpec } from "@exp/db-schema/domain"
import { db } from "@/db/connection"
import {
  issues,
  pullRequests,
  repositories,
  sessionIssues,
  sessions,
  sessionWatches,
  type Session,
  type SessionWatchingJson,
  type SessionWatchKind,
} from "@/db/schema"
import { LIVE_PR_STATES } from "@/lib/pr-merge-guard"
import { insertMessage } from "@/lib/sessions/messages"
import { deriveSessionTitles } from "@/lib/sessions/title"
import { ownAssistantTeam, treeTeams } from "@/lib/sessions/tree-teams"
import type { Tx } from "@/lib/sessions/tx"

// #NEW-49 (packages/eagent fixtures/wakeups.json WATCH EVENTS, the server
// half of `send.watch` / `send.unwatch` / `watch.fire` and the watchdog's
// time watches): what a session WAITS ON, one `session_watches` row each.
// A `session` watch hears every transition of that session until it ends
// (the bus, `busListeners`); an EVENT watch (a PR's CI or close, an issue's
// move, a time) fires ONCE: one pending, user-authored line into its live
// watcher (card event `watch_fired`), then the row is gone. A gone subject
// fires its watches too. Nothing lives on the device: a watch survives an
// app restart and no agent polls. `sessions.watching` is the synced
// projection every client reads ("Waiting on …"), rewritten with every
// change (`syncWatching`).

/** eagent rules.watch.maxPerSession (drift-gated by eagent-spec.test.ts). */
export const SESSION_WATCH_MAX = 20

/** eagent rules.copy (drift-gated). */
export const WATCH_COPY = {
  invalid: `A watch names its pull request or issue, or a time (at).`,
  tooMany: (max: number) =>
    `A session waits on at most ${max} things at once; cancel one first (exponential_sessions_send {wakeOn: ["-<id>"]}).`,
  at: (label: string) => `It is ${label}, the time you asked to be woken at.`,
  prMerged: (number: number) => `#${number} was merged.`,
  prClosed: (number: number) => `#${number} was closed without merging.`,
  issueMoved: (identifier: string, status: string) => `${identifier} moved to ${status}.`,
  issueGone: (identifier: string) => `${identifier} was deleted; its watches are gone.`,
} as const

const LABEL_MAX = 32

/** Rows of one insert keep the order they were named in (`now()` is one instant per transaction). */
const ORDERED_NOW = sql<Date>`clock_timestamp()`

/** The label of a private session watched from a session others read (#EXP-1408). */
export const PRIVATE_WATCH_LABEL = `a private chat`

/** bus.ts `askHidden`: a reader of `listener` may not see `origin`. */
const askHidden = (listener: { shared: boolean; userId: string }, origin: { shared: boolean; userId: string }) =>
  !origin.shared && (listener.shared || listener.userId !== origin.userId)

function clip(text: string, max = LABEL_MAX): string {
  const trimmed = text.trim().replace(/\s+/g, ` `)
  return trimmed.length > max ? `${trimmed.slice(0, max - 1).trimEnd()}…` : trimmed
}

/** A time watch's label: `14:30 UTC` today (UTC), else `Nov 17, 14:30 UTC`. Clients render `at` in local time. */
export function timeWatchLabel(at: Date, now: Date = new Date()): string {
  const hhmm = at.toISOString().slice(11, 16)
  if (at.toISOString().slice(0, 10) === now.toISOString().slice(0, 10)) return `${hhmm} UTC`
  const day = at.toLocaleDateString(`en-US`, { month: `short`, day: `numeric`, timeZone: `UTC` })
  return `${day}, ${hhmm} UTC`
}

/** A watched session's label: its first linked issue's identifier, else its display title (clipped). */
export async function sessionWatchLabels(tx: Pick<Tx, `select`>, ids: readonly string[]): Promise<Map<string, string>> {
  const labels = new Map<string, string>()
  if (ids.length === 0) return labels
  const links = await tx
    .select({ sessionId: sessionIssues.sessionId, identifier: issues.identifier })
    .from(sessionIssues)
    .innerJoin(issues, eq(issues.id, sessionIssues.issueId))
    .where(and(inArray(sessionIssues.sessionId, [...ids]), isNull(issues.deletedAt)))
    .orderBy(asc(sessionIssues.sortOrder), asc(sessionIssues.id))
  for (const link of links) if (!labels.has(link.sessionId)) labels.set(link.sessionId, link.identifier)
  const rest = ids.filter((id) => !labels.has(id))
  if (rest.length > 0) {
    const rows = await tx
      .select({ id: sessions.id, rootId: sessions.rootId, title: sessions.title })
      .from(sessions)
      .where(inArray(sessions.id, rest))
    const titles = await deriveSessionTitles(rows, tx)
    for (const row of rows) labels.set(row.id, clip(titles.get(row.id) ?? row.title ?? `a session`))
  }
  return labels
}

/** Rewrites `sessions.watching` (the synced projection, oldest first) of each id from its rows. */
export async function syncWatching(tx: Tx, ids: Iterable<string>): Promise<void> {
  const unique = [...new Set(ids)]
  if (unique.length === 0) return
  const rows = await tx
    .select()
    .from(sessionWatches)
    .where(inArray(sessionWatches.sessionId, unique))
    .orderBy(asc(sessionWatches.createdAt), asc(sessionWatches.id))
  const bySession = new Map<string, SessionWatchingJson[]>(unique.map((id) => [id, []]))
  for (const row of rows) {
    bySession.get(row.sessionId)!.push({
      id: row.id,
      kind: row.kind,
      subjectId: row.subjectId,
      label: row.label,
      ...(row.kind === `at` && row.at ? { at: row.at.toISOString() } : {}),
    })
  }
  for (const [id, watching] of bySession) {
    // A watch change never bumps `updated_at` (no list reorder).
    await tx
      .update(sessions)
      .set({ watching: watching.length > 0 ? watching : null })
      .where(eq(sessions.id, id))
  }
}

/** The sessions watching `subjectId` (live ones only) as an SQL condition on `sessions`. */
export function watchesSession(subjectId: string): SQL {
  return sql`EXISTS (SELECT 1 FROM ${sessionWatches} sw WHERE sw.session_id = ${sessions.id} AND sw.kind = 'session' AND sw.subject_id = ${subjectId})`
}

async function countWatches(tx: Pick<Tx, `select`>, sessionId: string): Promise<number> {
  const [row] = await tx
    .select({ n: sql<number>`count(*)::int` })
    .from(sessionWatches)
    .where(eq(sessionWatches.sessionId, sessionId))
  return Number(row?.n ?? 0)
}

/** `watcher` watches `ids`: LIVE sessions of its tree's teams its account
 *  may see (shared, or the watcher's user started them), never itself;
 *  others drop silently; idempotent (#EXP-1399, #EXP-1720). #EXP-2107: a
 *  plain chat also watches its member's own Assistant conversation. */
export async function watchSessions(tx: Tx, watcherId: string, ids: readonly string[]): Promise<string[]> {
  const wanted = [...new Set(ids)].filter((id) => id !== watcherId)
  if (wanted.length === 0) return []
  const [row] = await tx
    .select({ rootId: sessions.rootId, userId: sessions.userId, shared: sessions.shared })
    .from(sessions)
    .where(eq(sessions.id, watcherId))
    .limit(1)
    .for(`update`)
  if (!row) return []
  const own = await ownAssistantTeam(row, row.userId, tx)
  const teams = [...(await treeTeams(row.rootId, tx)), ...(own ? [own] : [])]
  if (teams.length === 0) return []
  const live = await tx
    .select({ id: sessions.id, shared: sessions.shared, userId: sessions.userId })
    .from(sessions)
    .where(
      and(
        inArray(sessions.id, wanted),
        ne(sessions.status, `ended`),
        inArray(sessions.teamId, teams),
        or(eq(sessions.shared, true), eq(sessions.userId, row.userId))
      )
    )
  const liveIds = live.map((entry) => entry.id).sort()
  if (liveIds.length === 0) return []
  const labels = await sessionWatchLabels(tx, liveIds)
  // #EXP-1408: a private session watched from where others read never shows its name.
  const hidden = new Set(live.filter((entry) => askHidden(row, entry)).map((entry) => entry.id))
  await tx
    .insert(sessionWatches)
    .values(
      liveIds.map((id) => ({
        sessionId: watcherId,
        kind: `session` as const,
        subjectId: id,
        label: hidden.has(id) ? PRIVATE_WATCH_LABEL : (labels.get(id) ?? `a session`),
        createdAt: ORDERED_NOW,
      }))
    )
    .onConflictDoNothing()
  await syncWatching(tx, [watcherId])
  return liveIds
}

/** The ended sessions leave every watch on them (their watchers keep their other watches). */
export async function forgetWatched(tx: Tx, endedIds: readonly string[]): Promise<void> {
  if (endedIds.length === 0) return
  const gone = await tx
    .delete(sessionWatches)
    .where(and(eq(sessionWatches.kind, `session`), inArray(sessionWatches.subjectId, [...endedIds])))
    .returning({ sessionId: sessionWatches.sessionId })
  await syncWatching(tx, gone.map((row) => row.sessionId))
}

/** An ended session waits on no EVENT any more (eagent: End clears its watches); its watched
 *  sessions stay for a continuation (`continueSession` hands them over). */
export async function dropEventWatchesOf(tx: Tx, ids: readonly string[]): Promise<void> {
  if (ids.length === 0) return
  const gone = await tx
    .delete(sessionWatches)
    .where(and(inArray(sessionWatches.sessionId, [...ids]), ne(sessionWatches.kind, `session`)))
    .returning({ sessionId: sessionWatches.sessionId })
  await syncWatching(tx, gone.map((row) => row.sessionId))
}

/** #EXP-1402: a continuation takes over what `ended` waited on, and every
 *  watch ON `ended` now watches the continuation. */
export async function handOverWatches(tx: Tx, ended: string, next: string): Promise<void> {
  const own = await tx.select().from(sessionWatches).where(eq(sessionWatches.sessionId, ended))
  if (own.length > 0) {
    await tx
      .insert(sessionWatches)
      .values(own.map(({ id: _id, sessionId: _s, createdAt: _c, ...rest }) => ({ ...rest, sessionId: next, createdAt: ORDERED_NOW })))
      .onConflictDoNothing()
  }
  const watchers = await tx
    .select({ id: sessionWatches.id, sessionId: sessionWatches.sessionId, label: sessionWatches.label })
    .from(sessionWatches)
    .where(and(eq(sessionWatches.kind, `session`), eq(sessionWatches.subjectId, ended)))
  for (const watch of watchers) {
    await tx.delete(sessionWatches).where(eq(sessionWatches.id, watch.id))
    if (watch.sessionId === next) continue
    // The continuation is the same work: it keeps the label its watcher reads (a private one stays private).
    await tx
      .insert(sessionWatches)
      .values({ sessionId: watch.sessionId, kind: `session`, subjectId: next, label: watch.label })
      .onConflictDoNothing()
  }
  await syncWatching(tx, [next, ...watchers.map((watch) => watch.sessionId)])
}

export interface ResolvedWatch {
  kind: Exclude<SessionWatchKind, `session`>
  subjectId: string | null
  status: string | null
  at: Date | null
  label: string
}

const notFound = (what: string) => new TRPCError({ code: `NOT_FOUND`, message: `${what} not found` })

/** The PR an agent names: an id, `owner/repo#N`, or a number (`#N`): its
 *  own repository's first, else its tree teams' only PR with it. */
async function resolvePr(tx: Pick<Tx, `select`>, watcher: Session, teams: string[], ref: string) {
  const cols = {
    id: pullRequests.id,
    number: pullRequests.number,
    state: pullRequests.state,
    checksState: pullRequests.checksState,
    repositoryId: pullRequests.repositoryId,
  }
  const inTeams = inArray(pullRequests.teamId, teams)
  if (UUID_RE.test(ref)) {
    const [row] = await tx.select(cols).from(pullRequests).where(and(eq(pullRequests.id, ref), inTeams)).limit(1)
    if (!row) throw notFound(`Pull request ${ref}`)
    return row
  }
  const text = ref.trim()
  const match = /^(?:([\w.-]+\/[\w.-]+))?#?(\d+)$/.exec(text)
  if (!match) throw new TRPCError({ code: `BAD_REQUEST`, message: `A pr is a number, owner/repo#N or an id` })
  const number = Number(match[2])
  const scope: SQL[] = [inTeams, eq(pullRequests.number, number)]
  if (match[1]) {
    scope.push(
      sql`${pullRequests.repositoryId} IN (SELECT ${repositories.id} FROM ${repositories} WHERE lower(${repositories.fullName}) = lower(${match[1]}))`
    )
  }
  const rows = await tx.select(cols).from(pullRequests).where(and(...scope)).limit(20)
  const own = watcher.repositoryId ? rows.filter((row) => row.repositoryId === watcher.repositoryId) : []
  const pick = own.length === 1 ? own : rows
  if (pick.length === 0) throw notFound(`Pull request #${number}`)
  if (pick.length > 1) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `Several pull requests are #${number}: name it owner/repo#${number}` })
  }
  return pick[0]!
}

async function resolveIssue(tx: Pick<Tx, `select`>, teams: string[], ref: string) {
  const cols = { id: issues.id, identifier: issues.identifier }
  const scope = and(inArray(issues.teamId, teams), isNull(issues.deletedAt))
  const where = UUID_RE.test(ref) ? eq(issues.id, ref) : eq(issues.identifier, ref.toUpperCase())
  const [row] = await tx.select(cols).from(issues).where(and(scope, where)).orderBy(sql`${issues.createdAt} DESC`).limit(1)
  if (!row) throw notFound(`Issue ${ref}`)
  return row
}

/** What fires at once (the subject already did what the watch waits for). */
interface Immediate {
  watch: ResolvedWatch
  text: string
}

/** The EVENT watches of parsed `wakeOn` entries resolved against the watcher's tree teams (the session ids stay
 *  for `watchSessions`, the cancels for `unwatch`). A PR already merged or closed, or whose CI already settled,
 *  fires at once. */
export async function resolveWatchSpecs(
  tx: Tx,
  watcher: Session,
  specs: readonly SessionWatchSpec[],
  now: Date = new Date()
): Promise<{ sessionIds: string[]; cancels: string[]; watches: ResolvedWatch[]; immediate: Immediate[] }> {
  const sessionIds: string[] = []
  const cancels: string[] = []
  const watches: ResolvedWatch[] = []
  const immediate: Immediate[] = []
  const teams = specs.some((spec) => spec.kind === `pr` || spec.kind === `issue`) ? await treeTeams(watcher.rootId, tx) : []
  for (const spec of specs) {
    if (spec.kind === `session`) {
      sessionIds.push(spec.id)
    } else if (spec.kind === `cancel`) {
      cancels.push(spec.id)
    } else if (spec.kind === `pr`) {
      const pr = await resolvePr(tx, watcher, teams, spec.ref)
      const kind = spec.on === `closed` ? (`pr_closed` as const) : (`pr_checks` as const)
      const watch: ResolvedWatch = {
        kind,
        subjectId: pr.id,
        status: null,
        at: null,
        label: kind === `pr_checks` ? `CI #${pr.number}` : `#${pr.number}`,
      }
      watches.push(watch)
      if (!(LIVE_PR_STATES as readonly string[]).includes(pr.state)) {
        immediate.push({ watch, text: pr.state === `merged` ? WATCH_COPY.prMerged(pr.number) : WATCH_COPY.prClosed(pr.number) })
      } else if (kind === `pr_checks` && (pr.checksState === `success` || pr.checksState === `failure`)) {
        immediate.push({ watch, text: checksWakeText(pr.number, pr.checksState, []) })
      }
    } else if (spec.kind === `issue`) {
      const issue = await resolveIssue(tx, teams, spec.ref)
      watches.push({
        kind: `issue_status`,
        subjectId: issue.id,
        status: spec.status ? spec.status.toLowerCase() : null,
        at: null,
        label: issue.identifier,
      })
    } else {
      const at = new Date(spec.at)
      watches.push({ kind: `at`, subjectId: null, status: null, at, label: timeWatchLabel(at, now) })
    }
  }
  return { sessionIds, cancels, watches, immediate }
}

/** `sessions.send({wakeOn})` (eagent `send.watch` + `send.unwatch`): cancels
 *  first, then event watches, then the sessions; both count against
 *  SESSION_WATCH_MAX together (`limit`, nothing changes). Watches that
 *  already fired at the watch fire now. */
export async function watch(tx: Tx, watcher: Session, entries: readonly string[]): Promise<void> {
  const specs = entries.map((entry) => {
    const spec = parseSessionWatchSpec(entry)
    if (!spec) throw new TRPCError({ code: `BAD_REQUEST`, message: WATCH_COPY.invalid })
    return spec
  })
  const { sessionIds, cancels, watches, immediate } = await resolveWatchSpecs(tx, watcher, specs)
  if (cancels.length > 0) await unwatch(tx, watcher.id, cancels)
  const before = await countWatches(tx, watcher.id)
  if (watches.length > 0) {
    await tx
      .insert(sessionWatches)
      .values(watches.map((entry) => ({ ...entry, sessionId: watcher.id, createdAt: ORDERED_NOW })))
      .onConflictDoNothing()
  }
  await watchSessions(tx, watcher.id, sessionIds)
  const after = await countWatches(tx, watcher.id)
  if (after > SESSION_WATCH_MAX && after > before) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: WATCH_COPY.tooMany(SESSION_WATCH_MAX) })
  }
  await syncWatching(tx, [watcher.id])
  for (const fired of immediate) {
    await fireWatches(tx, {
      kind: fired.watch.kind === `pr_closed` ? `pr_closed` : `pr_checks`,
      subjectId: fired.watch.subjectId!,
      text: fired.text,
      only: watcher.id,
    })
  }
}

/** `sessions.send({unwatch})` / `sessions.unwatch` (eagent `send.unwatch`): by watch id or a watched session's id. */
export async function unwatch(tx: Tx, sessionId: string, ids: readonly string[]): Promise<void> {
  if (ids.length === 0) return
  await tx
    .delete(sessionWatches)
    .where(
      and(
        eq(sessionWatches.sessionId, sessionId),
        or(
          inArray(sessionWatches.id, [...ids]),
          and(eq(sessionWatches.kind, `session`), inArray(sessionWatches.subjectId, [...ids]))
        )
      )
    )
  await syncWatching(tx, [sessionId])
}

export type FireKind = `pr_checks` | `pr_closed` | `issue_status` | `gone`

/** eagent `fires`: which watch kinds an event on a subject fires. */
function firedKinds(kind: FireKind): SessionWatchKind[] {
  if (kind === `gone`) return [`pr_checks`, `pr_closed`, `issue_status`]
  if (kind === `pr_closed`) return [`pr_checks`, `pr_closed`]
  return [kind]
}

/** eagent `watch.fire`: per live watcher (by id) ONE pending line with
 *  `text` (card event `watch_fired`, watchId = the first watch it fired),
 *  every watch it fired gone. `status` = an issue's new status (its anchor
 *  and its name both match a watch's target). Returns the woken sessions. */
export async function fireWatches(
  tx: Tx,
  args: { kind: FireKind; subjectId: string; text: string; status?: readonly string[]; only?: string }
): Promise<string[]> {
  const statuses = (args.status ?? []).map((status) => status.toLowerCase())
  const rows = await tx
    .select({ watch: sessionWatches, session: sessions })
    .from(sessionWatches)
    .innerJoin(sessions, eq(sessions.id, sessionWatches.sessionId))
    .where(
      and(
        eq(sessionWatches.subjectId, args.subjectId),
        inArray(sessionWatches.kind, firedKinds(args.kind)),
        ne(sessions.status, `ended`),
        args.only ? eq(sessionWatches.sessionId, args.only) : undefined,
        args.kind === `issue_status`
          ? or(isNull(sessionWatches.status), statuses.length > 0 ? inArray(sessionWatches.status, statuses) : sql`false`)
          : undefined
      )
    )
    .orderBy(asc(sessionWatches.sessionId), asc(sessionWatches.createdAt), asc(sessionWatches.id))
  return postFired(tx, rows, () => args.text)
}

async function postFired(
  tx: Tx,
  rows: Array<{ watch: typeof sessionWatches.$inferSelect; session: Session }>,
  text: (first: typeof sessionWatches.$inferSelect) => string
): Promise<string[]> {
  const bySession = new Map<string, { session: Session; watches: Array<typeof sessionWatches.$inferSelect> }>()
  for (const row of rows) {
    const entry = bySession.get(row.session.id) ?? { session: row.session, watches: [] }
    entry.watches.push(row.watch)
    bySession.set(row.session.id, entry)
  }
  const woken: string[] = []
  for (const [id, { session, watches }] of [...bySession].sort(([a], [b]) => (a < b ? -1 : 1))) {
    await tx.delete(sessionWatches).where(inArray(sessionWatches.id, watches.map((entry) => entry.id)))
    // Pending input: the session wakes and reads the line (the device delivers it like the CI line).
    await insertMessage(tx, session, {
      authorKind: `user`,
      authorId: session.userId,
      body: text(watches[0]!),
      card: { kind: `event`, event: `watch_fired`, sessionId: id, watchId: watches[0]!.id },
    })
    woken.push(id)
  }
  await syncWatching(tx, woken)
  return woken
}

/** The watchdog (every minute): each live session's time watches that came due fire, one line per watcher. */
export async function fireDueTimeWatches(now: Date = new Date()): Promise<number> {
  return db.transaction(async (tx) => {
    const rows = await tx
      .select({ watch: sessionWatches, session: sessions })
      .from(sessionWatches)
      .innerJoin(sessions, eq(sessions.id, sessionWatches.sessionId))
      .where(and(eq(sessionWatches.kind, `at`), lte(sessionWatches.at, now), ne(sessions.status, `ended`)))
      .orderBy(asc(sessionWatches.sessionId), asc(sessionWatches.createdAt), asc(sessionWatches.id))
      .limit(500)
      .for(`update`, { of: sessionWatches, skipLocked: true })
    if (rows.length === 0) return 0
    return (await postFired(tx, rows, (first) => WATCH_COPY.at(first.label))).length
  })
}

/** `sessions get` (MCP): what a session waits on, ids first (`unwatch` takes them). */
export async function watchesOf(
  tx: Pick<Tx, `select`>,
  sessionId: string
): Promise<Array<{ id: string; kind: SessionWatchKind; subjectId: string | null; status: string | null; at: string | null; label: string }>> {
  const rows = await tx
    .select()
    .from(sessionWatches)
    .where(eq(sessionWatches.sessionId, sessionId))
    .orderBy(asc(sessionWatches.createdAt), asc(sessionWatches.id))
  return rows.map((row) => ({
    id: row.id,
    kind: row.kind,
    subjectId: row.subjectId,
    status: row.status,
    at: row.at?.toISOString() ?? null,
    label: row.label,
  }))
}

/** The session ids `sessionId` watches (its `session` watches), oldest first. */
export async function watchedSessionIds(sessionId: string, exec: Pick<Tx, `select`> = db): Promise<string[]> {
  const rows = await exec
    .select({ subjectId: sessionWatches.subjectId })
    .from(sessionWatches)
    .where(and(eq(sessionWatches.sessionId, sessionId), eq(sessionWatches.kind, `session`)))
    .orderBy(asc(sessionWatches.createdAt), asc(sessionWatches.id))
  return rows.map((row) => row.subjectId!)
}

/** Fixtures (tests, seeds): `watcherIds` watch `subjectIds` as raw rows, no visibility rule. */
export async function insertSessionWatchRows(
  exec: Pick<Tx, `insert`>,
  watcherIds: readonly string[],
  subjectIds: readonly string[]
): Promise<void> {
  const values = watcherIds.flatMap((sessionId) =>
    subjectIds.map((subjectId) => ({ sessionId, kind: `session` as const, subjectId, label: `a session` }))
  )
  if (values.length > 0) await exec.insert(sessionWatches).values(values).onConflictDoNothing()
}
