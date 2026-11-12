// EXP-1709 (merge-train.json `texts`, `routing`, `assistantBatch`): the ONE
// waking `pr_merge` line per PR outcome (opener while live, else the root;
// an ended root keeps it pre-consumed for display), plus the starter's
// Assistant: ONE merged line per landed batch, and the stuck line of a PR
// held by the limits. Agents never poll: this is how they hear the train.
import { and, eq, isNull } from "drizzle-orm"
import {
  assistantBatchLines,
  prMergeRoute,
  stuckText,
  type HoldReason,
  type MergeOutcome,
} from "@exp/domain-contract/merge-train"
import { agents, issues, pullRequestIssues, pullRequests, sessions } from "@/db/schema"
import { insertMessage } from "@/lib/sessions/messages"
import type { Tx } from "@/lib/sessions/tx"
import { generateTxId } from "@/lib/trpc"

type PrRow = Pick<typeof pullRequests.$inferSelect, `id` | `teamId` | `sessionId` | `rootSessionId`>

const sessionCols = {
  id: sessions.id,
  rootId: sessions.rootId,
  teamId: sessions.teamId,
  shared: sessions.shared,
  userId: sessions.userId,
  status: sessions.status,
}

async function sessionRow(tx: Tx, id: string | null) {
  if (!id) return null
  const [row] = await tx.select(sessionCols).from(sessions).where(eq(sessions.id, id)).limit(1)
  return row ?? null
}

/** The root starter's Assistant conversation in the PR's team (the
 *  `assistant` agent's root session of its owner), null when none: never
 *  created for this. */
export async function assistantConversation(tx: Tx, teamId: string, ownerId: string): Promise<string | null> {
  const [row] = await tx
    .select({ id: sessions.id })
    .from(sessions)
    .innerJoin(agents, eq(agents.id, sessions.agentId))
    .where(
      and(
        eq(agents.teamId, teamId),
        eq(agents.ownerId, ownerId),
        eq(agents.role, `assistant`),
        eq(sessions.userId, ownerId),
        isNull(sessions.parentId)
      )
    )
    .limit(1)
  return row?.id ?? null
}

interface Routed {
  /** Where the PR's own line went (null = nobody: no opener, no root). */
  own: string | null
  /** The starter's Assistant conversation, when the route reaches it. */
  assistant: string | null
}

/** Post ONE PR's `pr_merge` line (`text`) and resolve the Assistant the
 *  route reaches (its line is the caller's: one per batch for merged). */
export async function postPrMerge(
  tx: Tx,
  pr: PrRow,
  outcome: MergeOutcome,
  text: string,
  holdReason: HoldReason | null = null
): Promise<Routed> {
  const opener = await sessionRow(tx, pr.sessionId)
  const root = await sessionRow(tx, pr.rootSessionId)
  const ownTarget = opener && opener.status !== `ended` ? opener : root
  if (!ownTarget) return { own: null, assistant: null }
  const assistant = root ? await assistantConversation(tx, pr.teamId, root.userId) : null
  const route = prMergeRoute({
    outcome,
    holdReason,
    opener: { id: opener?.id ?? ``, live: !!opener && opener.status !== `ended` },
    root: { id: ownTarget.id, ended: ownTarget.status === `ended` },
    assistant,
  })
  await generateTxId(tx)
  const card = { kind: `event` as const, event: `pr_merge` as const, pullRequestId: pr.id, mergeOutcome: outcome }
  let routedAssistant: string | null = null
  for (const target of route) {
    if (target.line === `assistant`) {
      routedAssistant = target.sessionId
      continue
    }
    const row = target.sessionId === ownTarget.id ? ownTarget : await sessionRow(tx, target.sessionId)
    if (!row) continue
    await insertMessage(tx, row, {
      authorKind: `user`,
      authorId: row.userId,
      body: text,
      card,
      // An ended root keeps it for display only: never woken.
      consumedAt: target.wakes ? null : new Date(),
    })
  }
  return { own: ownTarget.id, assistant: routedAssistant }
}

/** The Assistant's stuck line for a PR held by the limits. */
export async function postAssistantStuck(
  tx: Tx,
  assistantId: string,
  pr: { id: string; number: number },
  reason: Exclude<HoldReason, `person`>
): Promise<void> {
  const row = await sessionRow(tx, assistantId)
  if (!row) return
  await insertMessage(tx, row, {
    authorKind: `user`,
    authorId: row.userId,
    body: stuckText(pr.number, reason),
    consumedAt: row.status === `ended` ? new Date() : null,
    card: { kind: `event`, event: `pr_merge`, pullRequestId: pr.id, mergeOutcome: `held` },
  })
}

/** The issue identifiers a PR closes (the Assistant's batch line). */
export async function closedIssueIdentifiers(tx: Tx, pullRequestId: string): Promise<string[]> {
  const rows = await tx
    .select({ identifier: issues.identifier })
    .from(pullRequestIssues)
    .innerJoin(issues, eq(issues.id, pullRequestIssues.issueId))
    .where(and(eq(pullRequestIssues.pullRequestId, pullRequestId), eq(pullRequestIssues.closes, true), isNull(issues.deletedAt)))
  return rows.map((r) => r.identifier)
}

/** ONE merged line per Assistant per landed batch (landing order). */
export async function postAssistantBatch(
  tx: Tx,
  landed: Array<{ id: string; number: number; assistant: string | null; issues: string[] }>,
  base: string
): Promise<void> {
  const lines = assistantBatchLines(
    landed.map((pr) => ({ number: pr.number, assistant: pr.assistant, issues: pr.issues })),
    base
  )
  for (const line of lines) {
    const row = await sessionRow(tx, line.sessionId)
    if (!row) continue
    const first = landed.find((pr) => pr.assistant === line.sessionId)
    await insertMessage(tx, row, {
      authorKind: `user`,
      authorId: row.userId,
      body: line.text,
      // An ended conversation keeps it for display, like the bus does.
      consumedAt: row.status === `ended` ? new Date() : null,
      card: { kind: `event`, event: `pr_merge`, pullRequestId: first?.id ?? null, mergeOutcome: `merged` },
    })
  }
}
