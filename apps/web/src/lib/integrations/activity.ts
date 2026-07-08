import { sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { issueEvents, issues } from "@/db/schema"
import type { IssueEventType } from "@/lib/domain"

type Tx = Parameters<Parameters<typeof db.transaction>[0]>[0]

// Record an issue activity-log event (D9). Called INSIDE the mutation's
// transaction so the event is atomic with the change it describes — and so
// agent/MCP-driven mutations emit events too. workspace_id is also
// trigger-denormalized from issue→project; we pass the known value to satisfy
// the NOT NULL insert. project_id is resolved via a subselect from the issue
// (also trigger-denormalized) so callers don't need to thread it through.
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
    projectId: sql`(select ${issues.projectId} from ${issues} where ${issues.id} = ${args.issueId})`,
    actorUserId: args.actorUserId,
    type: args.type,
    payload: args.payload ?? null,
  })
}
