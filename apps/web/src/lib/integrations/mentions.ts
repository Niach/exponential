import { and, eq, inArray, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { issues, projects, workspaceMembers } from "@/db/schema"
import { users } from "@/db/auth-schema"
import { extractIssueRefs } from "@/lib/issue-refs"

export { extractIssueRefs }

type Tx = Parameters<Parameters<typeof db.transaction>[0]>[0]

// Mentions are written as `@<email>` in the comment/markdown source — typeable
// by hand and inserted by the editor's @-autocomplete. This is the single
// interchange form across all clients (it round-trips trivially as plain GFM
// text). The captured group is the bare email after the leading `@`.
const MENTION_RE = /@([A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,})/g

export function extractMentionEmails(text: string): string[] {
  return [
    ...new Set(
      [...text.matchAll(MENTION_RE)].map((m) => m[1].toLowerCase())
    ),
  ]
}

// Resolve `@email` mentions in a piece of text to the user ids of workspace
// members (so a mention only fires for someone who can actually see the issue).
// Case-insensitive on the email.
export async function resolveMentions(
  tx: Tx,
  text: string,
  workspaceId: string
): Promise<string[]> {
  const emails = extractMentionEmails(text)
  if (emails.length === 0) return []

  const rows = await tx
    .select({ id: users.id })
    .from(users)
    .innerJoin(workspaceMembers, eq(workspaceMembers.userId, users.id))
    .where(
      and(
        eq(workspaceMembers.workspaceId, workspaceId),
        inArray(sql`lower(${users.email})`, emails)
      )
    )

  return [...new Set(rows.map((r) => r.id))]
}

// Resolve `#IDENTIFIER` issue references (see lib/issue-refs.ts for the token
// contract) to issues in the same workspace — mirroring resolveMentions, so a
// reference only counts when the target issue is actually visible there. v1
// keeps references as plain links (no notification fan-out); this resolver is
// the anchor point for a future "referenced-in" signal.
export async function resolveIssueRefs(
  tx: Tx,
  text: string,
  workspaceId: string
): Promise<Array<{ id: string; identifier: string }>> {
  const identifiers = extractIssueRefs(text)
  if (identifiers.length === 0) return []

  const rows = await tx
    .select({ id: issues.id, identifier: issues.identifier })
    .from(issues)
    .innerJoin(projects, eq(projects.id, issues.projectId))
    .where(
      and(
        eq(projects.workspaceId, workspaceId),
        inArray(issues.identifier, identifiers)
      )
    )

  return rows
}
