import { and, eq, isNull, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { messages, pullRequests, sessions } from "@/db/schema"
import { generateTxId } from "@/lib/trpc"
import { askHidden } from "@/lib/sessions/bus"
import { insertMessage } from "@/lib/sessions/messages"
import {
  isMergeRefusal,
  MERGE_REFUSAL_COPY,
  type MergeRefusalKind,
} from "@/lib/trpc/pr-merge-error"

// EXP-1391: a refused merge goes INTO the thread. The person's merge
// (`pullRequests.merge`) and the automerge post ONE `pr_merge_refused` event
// line into the PR's root session as pending input: the root wakes, reads
// GitHub's reason and the steps below, and deals with it (session-thread.json
// EXP-1391). An agent's own `exponential_pr merge` gets the same text as its
// tool result instead (`mergeRefusalText`) and posts nothing.
//
// #EXP-1408 privacy: the line names the PR and quotes GitHub, so it lands
// where the PR's opener may be read. A PR opened by a PRIVATE session the
// root's readers may not see (`askHidden`: the bus already hid its
// `pr_opened` line there) gets the line in the OPENER's own conversation
// instead, never in the root.

const STEPS: Record<MergeRefusalKind, string> = {
  checks: `Wait for the checks to finish; if one failed, read it, fix it and push. Then merge again.`,
  conflict: `Rebase onto the base branch, resolve the conflicts, push with --force-with-lease, then merge again.`,
  other: `Handle it if you can and merge again; otherwise ask the person.`,
}

export function mergeRefusalSteps(kind: MergeRefusalKind): string {
  return STEPS[kind]
}

/** What the root session's agent reads (and an agent's tool result). */
export function mergeRefusalText(prNumber: number, reason: string, kind: MergeRefusalKind): string {
  return `The merge of PR #${prNumber} was refused: ${reason}\n\n${STEPS[kind]}`
}

/** Posts the refusal into the PR's root session (or its hidden private
 *  opener), waking it. True when a line wakes it now or already waits
 *  unread there; false when there is no thread, the target ended (the line lands pre-consumed) or the post
 *  failed. Never throws: the refusal itself still reaches the caller. */
export async function postMergeRefusal(args: {
  pullRequestId: string
  /** Who merged (the thread's starter for the automerge). */
  actorUserId: string
  error: unknown
  /** The automerge: the same refusal is posted once, read or not, so a
   *  turn ending on it never wakes the root again. */
  automerge?: boolean
}): Promise<boolean> {
  if (!isMergeRefusal(args.error)) return false
  const refusal = args.error
  try {
    return await db.transaction(async (tx) => {
      const [pr] = await tx
        .select({
          number: pullRequests.number,
          rootSessionId: pullRequests.rootSessionId,
          sessionId: pullRequests.sessionId,
        })
        .from(pullRequests)
        .where(eq(pullRequests.id, args.pullRequestId))
        .limit(1)
      if (!pr?.rootSessionId) return false
      const cols = {
        id: sessions.id,
        rootId: sessions.rootId,
        teamId: sessions.teamId,
        shared: sessions.shared,
        userId: sessions.userId,
        status: sessions.status,
      }
      const [root] = await tx.select(cols).from(sessions).where(eq(sessions.id, pr.rootSessionId)).limit(1)
      if (!root) return false
      const [opener] =
        pr.sessionId && pr.sessionId !== root.id
          ? await tx.select(cols).from(sessions).where(eq(sessions.id, pr.sessionId)).limit(1)
          : []
      const target = opener && askHidden(root, opener) ? opener : root
      const live = target.status !== `ended`
      const body = mergeRefusalText(pr.number, refusal.message, refusal.kind)
      const ofThisPr = and(
        eq(messages.sessionId, target.id),
        sql`${messages.card}->>'event' = 'pr_merge_refused'`,
        sql`${messages.card}->>'pullRequestId' = ${args.pullRequestId}`
      )
      // One unread line per PR: a repeat click or an automerge pass while
      // the last refusal still waits adds nothing.
      if (live) {
        const [waiting] = await tx
          .select({ id: messages.id })
          .from(messages)
          .where(and(ofThisPr, isNull(messages.consumedAt)))
          .limit(1)
        if (waiting) return true
      }
      // An ended target (display only) or the automerge: the same refusal is
      // already there, read or not.
      if (!live || args.automerge) {
        const [same] = await tx
          .select({ id: messages.id })
          .from(messages)
          .where(and(ofThisPr, eq(messages.body, body)))
          .limit(1)
        if (same) return false
      }
      await generateTxId(tx)
      await insertMessage(tx, target, {
        authorKind: `user`,
        authorId: args.actorUserId,
        body,
        card: {
          kind: `event`,
          event: `pr_merge_refused`,
          pullRequestId: args.pullRequestId,
        },
        consumedAt: live ? null : new Date(),
      })
      return live
    })
  } catch (err) {
    console.error(`[merge] posting the refusal of ${args.pullRequestId} failed`, err)
    return false
  }
}

/** The person's toast text: the reason, plus the handoff once the thread
 *  took it (feedback-states.json errors.mergeRefusal). */
export function withHandoff(message: string, posted: boolean): string {
  return posted ? `${message} ${MERGE_REFUSAL_COPY.handoff}` : message
}
