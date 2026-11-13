import { repoInstallationId } from "@/lib/ecode/provider-config"
import { and, eq, gte, inArray, isNotNull, isNull, lt, or, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { branchPushes, repositories, sessions } from "@/db/schema"
import { soleHeadSession, type GuideTreeRow } from "@/lib/sessions/guide-entries"

/** #EXP-1540: how long after it ended a session may still be credited with
 *  a push (it pushed, then ended before GitHub delivered the webhook). */
export const PUSH_GRACE_MS = 2 * 60 * 1000

/** A session on the pushed branch, in the pushed repository. */
export interface PushCandidate {
  id: string
  status: string
  busy: boolean
  background: boolean
  endedAt: Date | null
}

/** #EXP-1540: the session a branch push is credited to. The device pushes
 *  with the clone's shared App token, so the delivery names no session: the
 *  pusher is the one session working on that branch at the time. One
 *  candidate = it; several = the only one mid-turn (busy, background work,
 *  or just ended); still ambiguous = nobody (never "the newest"). */
export function pushAuthor(candidates: readonly PushCandidate[], now: Date): string | null {
  const live = candidates.filter((row) => row.status !== `ended`)
  const fresh = (row: PushCandidate) =>
    row.status === `ended` && !!row.endedAt && now.getTime() - row.endedAt.getTime() <= PUSH_GRACE_MS
  const working = candidates.filter((row) => (row.status !== `ended` && (row.busy || row.background)) || fresh(row))
  if (working.length === 1) return working[0]!.id
  if (working.length === 0 && live.length === 1) return live[0]!.id
  return null
}

/** #EXP-1540: who a PR with no recorded session resolves to through its
 *  head: `pusher` (the session credited with the head's latest push,
 *  `headPusher`) while it is in this tree and still on that head, else the
 *  one lineage sitting on it (`soleHeadSession`), else nobody. */
export function headSession(
  tree: readonly (GuideTreeRow & { branch?: string | null })[],
  head: string,
  pusher?: string | null
): string | null {
  const name = head.trim()
  if (pusher && tree.some((row) => row.id === pusher && row.branch?.trim() === name)) return pusher
  return soleHeadSession(tree, name)
}

/** #EXP-1540: the session credited with the latest push of `branch` in the
 *  repository (`branch_pushes`); null when that push was nobody's, or the
 *  branch was never pushed. */
export async function headPusher(args: { repositoryId?: string | null; branch: string }): Promise<string | null> {
  if (!args.repositoryId) return null
  const [row] = await db
    .select({ sessionId: branchPushes.sessionId })
    .from(branchPushes)
    .where(and(eq(branchPushes.repositoryId, args.repositoryId), eq(branchPushes.branch, args.branch.trim())))
  return row?.sessionId ?? null
}

/** #EXP-1540: the repositories a `push` delivery is about: its full name,
 *  bound to the delivering installation or still unbound (the
 *  `installation_repositories` heal's rule). The same full name may be
 *  registered by several teams; another installation's row never counts. */
export function pushedRepositoryIds(args: { fullName: string; installationId?: number | null }) {
  const name = eq(repositories.fullName, args.fullName)
  return db
    .select({ id: repositories.id })
    .from(repositories)
    .where(
      args.installationId == null
        ? name
        : and(name, or(isNull(repoInstallationId()), eq(repoInstallationId(), args.installationId)))
    )
}

/** #EXP-1540: when a push happened, from its own payload (a redelivered
 *  old push is never "now"): the head commit's timestamp, else the
 *  repository's `pushed_at` (unix seconds), else undefined. Never later
 *  than `now` (a skewed clock must not pin the credit). */
export function pushTime(
  payload: { head_commit?: { timestamp?: string } | null; repository?: { pushed_at?: number | string } },
  now: Date = new Date()
): Date | undefined {
  const parse = (value: unknown): Date | null => {
    if (typeof value === `number`) return new Date(value * 1000)
    if (typeof value === `string` && value) return new Date(value)
    return null
  }
  for (const value of [payload.head_commit?.timestamp, payload.repository?.pushed_at]) {
    const at = parse(value)
    if (at && !Number.isNaN(at.getTime())) return at > now ? now : at
  }
  return undefined
}

/** #EXP-1540: records a GitHub `push` of `branch` in `fullName` as that
 *  branch's latest push (`branch_pushes`, per repository), credited to its
 *  session (`pushAuthor`) or to nobody. A row only ever moves forward (the
 *  push's own time `at`): a redelivered or late-delivered older push changes
 *  nothing and credits nobody, so an uncredited newer push is never undone
 *  by an older credited one. Returns the credited session, if any. */
export async function attributePush(args: {
  fullName: string
  branch: string
  installationId?: number | null
  /** When the push happened (`pushTime`); defaults to `now`. */
  at?: Date
  /** When the delivery arrived: the ended grace window's clock. */
  now?: Date
}): Promise<string | null> {
  const now = args.now ?? new Date()
  const at = args.at ?? now
  const repoIds = (await pushedRepositoryIds(args)).map((row) => row.id)
  if (repoIds.length === 0) return null
  const candidates = await db
    .select({
      id: sessions.id,
      repositoryId: sessions.repositoryId,
      status: sessions.status,
      busy: sessions.busy,
      background: sessions.background,
      endedAt: sessions.endedAt,
    })
    .from(sessions)
    .where(
      and(
        eq(sessions.branch, args.branch),
        inArray(sessions.repositoryId, repoIds),
        or(
          inArray(sessions.status, [`running`, `idle`]),
          and(
            eq(sessions.status, `ended`),
            isNotNull(sessions.endedAt),
            gte(sessions.endedAt, new Date(now.getTime() - PUSH_GRACE_MS))
          )
        )
      )
    )
  const id = pushAuthor(candidates, now)
  const author = candidates.find((row) => row.id === id)
  const latest = await db
    .insert(branchPushes)
    .values(
      repoIds.map((repositoryId) => ({
        repositoryId,
        branch: args.branch,
        pushedAt: at,
        sessionId: author?.repositoryId === repositoryId ? author.id : null,
      }))
    )
    .onConflictDoUpdate({
      target: [branchPushes.repositoryId, branchPushes.branch],
      set: { pushedAt: sql`excluded.pushed_at`, sessionId: sql`excluded.session_id` },
      setWhere: lt(branchPushes.pushedAt, sql`excluded.pushed_at`),
    })
    .returning({ sessionId: branchPushes.sessionId })
  return id && latest.some((row) => row.sessionId === id) ? id : null
}
