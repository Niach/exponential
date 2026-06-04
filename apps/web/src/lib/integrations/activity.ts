import { db } from "@/db/connection"
import { issueEvents } from "@/db/schema"
import type { IssueEventType } from "@/lib/domain"

type Tx = Parameters<Parameters<typeof db.transaction>[0]>[0]

// Record an issue activity-log event (D9). Called INSIDE the mutation's
// transaction so the event is atomic with the change it describes — and so
// agent/MCP-driven mutations emit events too. workspace_id is also
// trigger-denormalized from issue→project; we pass the known value to satisfy
// the NOT NULL insert.
export async function recordIssueEvent(
  tx: Tx,
  args: {
    issueId: string
    workspaceId: string
    actorUserId: string | null
    type: IssueEventType
    payload?: Record<string, unknown> | null
  }
): Promise<void> {
  await tx.insert(issueEvents).values({
    issueId: args.issueId,
    workspaceId: args.workspaceId,
    actorUserId: args.actorUserId,
    type: args.type,
    payload: args.payload ?? null,
  })
}
