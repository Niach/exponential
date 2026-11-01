import { and, eq, gte, inArray, isNotNull, isNull, lt, or, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { repositories, sessions } from "@/db/schema"
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
 *  head: the session credited with the latest push of that branch, else the
 *  one lineage sitting on it (`soleHeadSession`), else nobody. */
export function headSession(
  tree: readonly (GuideTreeRow & { branch?: string | null; pushedBranch?: string | null; pushedAt?: Date | null })[],
  head: string
): string | null {
  const name = head.trim()
  let pusher: { id: string; at: number } | null = null
  for (const row of tree) {
    if (!row.pushedAt || row.pushedBranch?.trim() !== name) continue
    const at = row.pushedAt.getTime()
    if (!pusher || at > pusher.at) pusher = { id: row.id, at }
  }
  return pusher?.id ?? soleHeadSession(tree, name)
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
        : and(name, or(isNull(repositories.installationId), eq(repositories.installationId, args.installationId)))
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

/** #EXP-1540: credits a GitHub `push` of `branch` in `fullName` to its
 *  session (`pushAuthor`): `pushed_branch` + `pushed_at` (the push's own
 *  time `at`; only ever moves forward, so a redelivery or an out-of-order
 *  delivery never overwrites a newer credit), server-only. A push credited
 *  to nobody withdraws every older credit for that branch: the head's
 *  latest push is then nobody's, and `headSession` falls back. Returns the
 *  credited session, if any. */
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
  const repos = pushedRepositoryIds(args)
  const candidates = await db
    .select({
      id: sessions.id,
      status: sessions.status,
      busy: sessions.busy,
      background: sessions.background,
      endedAt: sessions.endedAt,
    })
    .from(sessions)
    .where(
      and(
        eq(sessions.branch, args.branch),
        inArray(sessions.repositoryId, repos),
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
  // Never the session's `updated_at` (the stuck clock, sync traffic):
  // `update_updated_at` honours the transaction-local GUC (brief.ts).
  return db.transaction(async (tx) => {
    await tx.execute(sql`SELECT set_config('exponential.preserve_timestamps', 'on', true)`)
    if (!id) {
      await tx
        .update(sessions)
        .set({ pushedBranch: null, pushedAt: null })
        .where(
          and(
            eq(sessions.pushedBranch, args.branch),
            inArray(sessions.repositoryId, pushedRepositoryIds(args)),
            lt(sessions.pushedAt, at)
          )
        )
      return null
    }
    const credited = await tx
      .update(sessions)
      .set({ pushedBranch: args.branch, pushedAt: at })
      .where(and(eq(sessions.id, id), or(isNull(sessions.pushedAt), lt(sessions.pushedAt, at))))
      .returning({ id: sessions.id })
    return credited.length > 0 ? id : null
  })
}
