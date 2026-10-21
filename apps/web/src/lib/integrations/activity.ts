import { sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { issueEvents, issues } from "@/db/schema"
import type { IssueEventType } from "@/lib/domain"
import { emitIssueEventInTx } from "@/lib/events"
import type { IssueEventOrigin } from "@/lib/integrations/event-origin"

type Tx = Parameters<Parameters<typeof db.transaction>[0]>[0]

// SERVER-ONLY (NEW-2: it emits the team events): a client module must never
// import it; the pure relation helpers live in `lib/issue-relations-pure.ts`.
//
// Record an issue activity-log event (D9). Called INSIDE the mutation's
// transaction so the event is atomic with the change it describes — and so
// agent/MCP-driven mutations emit events too. team_id is also
// trigger-denormalized from issue→board; we pass the known value to satisfy
// the NOT NULL insert. board_id is resolved via a subselect from the issue
// (also trigger-denormalized) so callers don't need to thread it through.
export async function recordIssueEvent(
  tx: Tx,
  args: {
    issueId: string
    teamId: string
    actorUserId: string | null
    type: IssueEventType
    payload?: Record<string, unknown> | null
    origin?: IssueEventOrigin
  }
): Promise<void> {
  await tx.insert(issueEvents).values({
    issueId: args.issueId,
    teamId: args.teamId,
    boardId: sql`(select ${issues.boardId} from ${issues} where ${issues.id} = ${args.issueId})`,
    actorUserId: args.actorUserId,
    type: args.type,
    payload: args.payload ?? null,
    viaMcp: args.origin?.viaMcp ?? false,
    sessionId: args.origin?.sessionId ?? null,
  })
  // NEW-2: the builtin team events `issue.created` / `issue.status_changed`
  // ride the same write (every path that records the activity emits them).
  if (args.type !== `created` && args.type !== `status_changed`) return
  await emitIssueEventInTx(tx, {
    issueId: args.issueId,
    teamId: args.teamId,
    name: args.type === `created` ? `issue.created` : `issue.status_changed`,
    payload: args.type === `status_changed` ? (args.payload ?? {}) : {},
    emittedBy: args.origin?.sessionId
      ? { sessionId: args.origin.sessionId }
      : args.actorUserId
        ? { userId: args.actorUserId }
        : { server: true },
  })
}
