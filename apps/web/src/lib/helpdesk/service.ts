import { and, desc, eq, inArray, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { supportMessages, supportThreads, teams } from "@/db/schema"
import type { SupportThread } from "@/db/schema"
import { mintSupportToken, verifySupportToken } from "@/lib/helpdesk/token"
import { appBaseUrl } from "@/lib/notification-email-policy"
import { TokenBucketLimiter, envInt } from "@/lib/widget/rate-limit"

type Tx = Parameters<Parameters<typeof db.transaction>[0]>[0]

// Reporter messages are plain text rendered small — a generous cap that still
// shuts the door on megabyte bodies.
export const MAX_SUPPORT_MESSAGE_CHARS = 10_000

export function supportThreadUrl(token: string): string {
  return `${appBaseUrl()}/support/${token}`
}

// The ticket title shown in the inbox: the first line of the reporter's
// opening message, truncated.
export function supportTicketTitle(message: string): string {
  const firstLine = (message.split(`\n`, 1)[0] ?? ``).trim()
  if (!firstLine) return `Support request`
  return firstLine.length > 120
    ? `${firstLine.slice(0, 119).trimEnd()}…`
    : firstLine
}

// Create a standalone ticket: the thread row + the reporter's opening inbound
// message. Returns the minted magic-link token so the caller can embed it in
// the confirmation email. The token is deterministic (HMAC over the thread id
// — see lib/helpdesk/token.ts), so it is STABLE for the thread's whole life —
// every email carries the same link — and nothing secret is stored on the
// row. Callers are responsible for the Pro gate (assertCanUseHelpdesk) and
// for checking teams.helpdesk_enabled.
export async function createSupportThreadInTx(
  tx: Tx,
  args: {
    teamId: string
    title: string
    reporterEmail: string
    reporterName?: string | null
    body: string
  }
): Promise<{ threadId: string; token: string }> {
  const [thread] = await tx
    .insert(supportThreads)
    .values({
      teamId: args.teamId,
      title: args.title,
      reporterEmail: args.reporterEmail,
      reporterName: args.reporterName ?? null,
    })
    .returning({ id: supportThreads.id })

  await tx.insert(supportMessages).values({
    threadId: thread.id,
    authorUserId: null,
    direction: `inbound`,
    visibility: `public`,
    body: args.body,
  })

  return { threadId: thread.id, token: mintSupportToken(thread.id) }
}

// What the anonymous reporter endpoints need about a thread: the row itself
// plus its team's display name and live helpdesk switch — one join instead of
// a follow-up query on every 5s poll.
export interface ResolvedSupportThread {
  thread: SupportThread
  teamName: string | null
  // REV2-23: turning the team helpdesk OFF freezes its open threads. Reads
  // (and the transcript) survive — losing them would read as data loss — but
  // replies are refused like a closed thread and the member fan-out never
  // fires. Re-enabling thaws every thread; nothing is auto-closed.
  helpdeskEnabled: boolean
}

// Resolve a magic-link token to its thread: verify the HMAC by recompute
// (rejecting garbage before any DB work), then load the thread it names.
// Returns null for anything that doesn't resolve — callers answer 404 without
// distinguishing why.
export async function findThreadByToken(
  token: string
): Promise<ResolvedSupportThread | null> {
  const threadId = verifySupportToken(token)
  if (!threadId) return null
  const [row] = await db
    .select({
      thread: supportThreads,
      teamName: teams.name,
      helpdeskEnabled: teams.helpdeskEnabled,
    })
    .from(supportThreads)
    .leftJoin(teams, eq(teams.id, supportThreads.teamId))
    .where(eq(supportThreads.id, threadId))
    .limit(1)
  if (!row) return null
  return {
    thread: row.thread,
    teamName: row.teamName,
    helpdeskEnabled: row.helpdeskEnabled === true,
  }
}

// The reporter-facing "this conversation takes no more replies" state: closed
// by a member (token revoked) OR frozen by the team's helpdesk switch.
export function isSupportThreadFrozen(resolved: ResolvedSupportThread): boolean {
  return (
    resolved.thread.tokenRevokedAt !== null || !resolved.helpdeskEnabled
  )
}

// Close: resolve the ticket and revoke the magic link in one write — the
// transcript stays readable through the link (losing it would read as data
// loss), but replies are rejected.
export async function closeThreadInTx(tx: Tx, threadId: string): Promise<void> {
  // Explicit updatedAt: the support tables have no update_updated_at trigger,
  // and the member inbox sorts by it — close/reopen must reorder the list.
  await tx
    .update(supportThreads)
    .set({
      status: `resolved`,
      tokenRevokedAt: new Date(),
      updatedAt: new Date(),
    })
    .where(
      and(eq(supportThreads.id, threadId), eq(supportThreads.status, `open`))
    )
}

// Reopen: replies are accepted again through the SAME link (the token never
// changes; revocation is the only lever).
export async function reopenThreadInTx(
  tx: Tx,
  threadId: string
): Promise<void> {
  await tx
    .update(supportThreads)
    .set({ status: `open`, tokenRevokedAt: null, updatedAt: new Date() })
    .where(eq(supportThreads.id, threadId))
}

// How much of the newest message the inbox list carries. The list renders it
// as a one-line truncated snippet, so shipping the full body (up to
// MAX_SUPPORT_MESSAGE_CHARS each) was pure waste.
export const SUPPORT_SNIPPET_CHARS = 200

// The newest message of each given thread (snippet + unread source for the
// inbox list). ONE row per thread via DISTINCT ON, snippet truncated in
// Postgres (REV2-40) — this used to fetch every public message body of every
// listed thread and pick first-per-thread in JS, on a 30s poll.
export async function latestMessagesByThread(
  threadIds: string[]
): Promise<
  Map<string, { body: string; direction: string; createdAt: Date }>
> {
  if (threadIds.length === 0) return new Map()
  const rows = await db
    .selectDistinctOn([supportMessages.threadId], {
      threadId: supportMessages.threadId,
      body: sql<string>`left(${supportMessages.body}, ${SUPPORT_SNIPPET_CHARS})`,
      direction: supportMessages.direction,
      createdAt: supportMessages.createdAt,
    })
    .from(supportMessages)
    .where(
      and(
        inArray(supportMessages.threadId, threadIds),
        eq(supportMessages.visibility, `public`)
      )
    )
    // DISTINCT ON demands its expressions lead the ORDER BY; the newest
    // message per thread is the row Postgres then keeps.
    .orderBy(supportMessages.threadId, desc(supportMessages.createdAt))
  const latest = new Map<
    string,
    { body: string; direction: string; createdAt: Date }
  >()
  for (const row of rows) {
    latest.set(row.threadId, {
      body: row.body,
      direction: row.direction,
      createdAt: row.createdAt,
    })
  }
  return latest
}

// ---------------------------------------------------------------------------
// Anonymous-endpoint rate limiting (same in-process token buckets as the
// widget; per-replica by design — see lib/widget/rate-limit.ts).
// ---------------------------------------------------------------------------

let readLimiter: TokenBucketLimiter | null = null
let replyIpLimiter: TokenBucketLimiter | null = null
let replyThreadLimiter: TokenBucketLimiter | null = null
let pollLimiter: TokenBucketLimiter | null = null

export function getSupportRateLimiters() {
  // Reads happen on every page load — generous. Replies are strict per IP
  // AND per thread (a stolen token must not turn a thread into a spam pipe).
  readLimiter ??= new TokenBucketLimiter({
    capacity: envInt(`SUPPORT_RATE_LIMIT_READ_BURST`, 30),
    refillPerHour: envInt(`SUPPORT_RATE_LIMIT_READ_HOURLY`, 300),
  })
  replyIpLimiter ??= new TokenBucketLimiter({
    capacity: envInt(`SUPPORT_RATE_LIMIT_REPLY_IP_BURST`, 5),
    refillPerHour: envInt(`SUPPORT_RATE_LIMIT_REPLY_IP_HOURLY`, 30),
  })
  replyThreadLimiter ??= new TokenBucketLimiter({
    capacity: envInt(`SUPPORT_RATE_LIMIT_REPLY_THREAD_BURST`, 5),
    refillPerHour: envInt(`SUPPORT_RATE_LIMIT_REPLY_THREAD_HOURLY`, 30),
  })
  // The live-chat poll runs every ~5s per visible tab (~720/h) — sized with
  // 2× headroom for a second tab; per-IP only, since the HMAC token is
  // verified before any DB work and the query is one cheap indexed read.
  pollLimiter ??= new TokenBucketLimiter({
    capacity: envInt(`SUPPORT_RATE_LIMIT_POLL_BURST`, 90),
    refillPerHour: envInt(`SUPPORT_RATE_LIMIT_POLL_HOURLY`, 1440),
  })
  return { readLimiter, replyIpLimiter, replyThreadLimiter, pollLimiter }
}
