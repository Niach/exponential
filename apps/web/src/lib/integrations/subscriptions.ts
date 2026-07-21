import { sql } from "drizzle-orm"
import { db } from "@/db/connection"
import type { SubscriberSource } from "@/lib/domain"

type Tx = Parameters<Parameters<typeof db.transaction>[0]>[0]

// Auto-subscribe a user to an issue (D7). Called inside the create/assign/
// comment/mention transactions. Inserts only when no (issue,user) row exists
// yet — so a prior MANUAL unsubscribe (source='manual', unsubscribed=true) is
// preserved and auto-events do NOT resurrect the subscription. team_id is also
// trigger-denormalized from issue→board; we pass the known value to satisfy the
// NOT NULL insert.
export async function ensureSubscribed(
  tx: Tx,
  args: {
    issueId: string
    userId: string
    teamId: string
    source: SubscriberSource
  }
): Promise<void> {
  // Raw SQL: uniq_issue_subscribers_user is a PARTIAL unique index (user rows
  // only; widget-reporter rows have null userId), so the conflict target must
  // carry the index predicate — and drizzle 0.39 silently DROPS the
  // `targetWhere` option from onConflictDoNothing (verified via .toSQL()).
  await tx.execute(sql`
    insert into issue_subscribers (issue_id, user_id, team_id, source)
    values (${args.issueId}, ${args.userId}, ${args.teamId}, ${args.source})
    on conflict (issue_id, user_id) where user_id is not null do nothing
  `)
}
