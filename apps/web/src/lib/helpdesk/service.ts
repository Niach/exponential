import { and, desc, eq, inArray, isNull } from "drizzle-orm"
import { db } from "@/db/connection"
import { supportMessages, supportThreads } from "@/db/schema"
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

// Create the conversation for a (freshly created) ticket issue: the thread
// row + the reporter's opening inbound message. Returns the minted magic-link
// token so the caller can embed it in the confirmation email. The token is
// deterministic (HMAC over the thread id — see lib/helpdesk/token.ts), so it
// is STABLE for the thread's whole life — every email carries the same link —
// and nothing secret is stored on the row. Callers are responsible for the
// Pro gate (assertCanUseHelpdesk) and for checking projects.helpdesk_enabled.
export async function createSupportThreadInTx(
  tx: Tx,
  args: {
    issueId: string
    projectId: string
    reporterEmail: string
    reporterName?: string | null
    body: string
  }
): Promise<{ threadId: string; token: string }> {
  const [thread] = await tx
    .insert(supportThreads)
    .values({
      issueId: args.issueId,
      projectId: args.projectId,
      reporterEmail: args.reporterEmail,
      reporterName: args.reporterName ?? null,
    })
    .returning({ id: supportThreads.id })

  await tx.insert(supportMessages).values({
    threadId: thread.id,
    issueId: args.issueId,
    authorUserId: null,
    direction: `inbound`,
    visibility: `public`,
    body: args.body,
  })

  return { threadId: thread.id, token: mintSupportToken(thread.id) }
}

// Resolve a magic-link token to its thread: verify the HMAC by recompute
// (rejecting garbage before any DB work), then load the thread it names.
// Returns null for anything that doesn't resolve — callers answer 404 without
// distinguishing why.
export async function findThreadByToken(
  token: string
): Promise<SupportThread | null> {
  const threadId = verifySupportToken(token)
  if (!threadId) return null
  const [thread] = await db
    .select()
    .from(supportThreads)
    .where(eq(supportThreads.id, threadId))
    .limit(1)
  return thread ?? null
}

// Close-time revocation: the transcript stays readable through the link
// (losing it would read as data loss), but replies are rejected.
export async function revokeThreadToken(
  tx: Tx,
  threadId: string
): Promise<void> {
  // Explicit updatedAt: the support tables have no update_updated_at trigger,
  // and the member inbox sorts by it — close/reopen must reorder the list.
  await tx
    .update(supportThreads)
    .set({ tokenRevokedAt: new Date(), updatedAt: new Date() })
    .where(
      and(
        eq(supportThreads.id, threadId),
        isNull(supportThreads.tokenRevokedAt)
      )
    )
}

// Reopen: replies are accepted again through the SAME link (the token never
// changes; revocation is the only lever).
export async function reinstateThreadToken(
  tx: Tx,
  threadId: string
): Promise<void> {
  await tx
    .update(supportThreads)
    .set({ tokenRevokedAt: null, updatedAt: new Date() })
    .where(eq(supportThreads.id, threadId))
}

// The newest message of each given thread (snippet + unread source for the
// inbox list). One query, newest-first, first-per-thread picked in JS.
export async function latestMessagesByThread(
  threadIds: string[]
): Promise<
  Map<string, { body: string; direction: string; createdAt: Date }>
> {
  if (threadIds.length === 0) return new Map()
  const rows = await db
    .select({
      threadId: supportMessages.threadId,
      body: supportMessages.body,
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
    .orderBy(desc(supportMessages.createdAt))
  const latest = new Map<
    string,
    { body: string; direction: string; createdAt: Date }
  >()
  for (const row of rows) {
    if (!latest.has(row.threadId)) {
      latest.set(row.threadId, {
        body: row.body,
        direction: row.direction,
        createdAt: row.createdAt,
      })
    }
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
  return { readLimiter, replyIpLimiter, replyThreadLimiter }
}
