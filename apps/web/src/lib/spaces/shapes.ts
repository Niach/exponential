// #EXP-2161 (member-space.json `shapes`): what a member space syncs and
// reads over REST. Everything not named here is refused.
import { eq } from "drizzle-orm"
import { MEMBER_SPACE } from "@exp/domain-contract/member-space"
import { attachments, messages } from "@/db/schema"
import { andClauses, buildWhereClause } from "@/lib/auth/shape-where"
import type { SpaceGrant } from "@/lib/spaces/context"
import { rootInSpace, spaceRootIds, spaceSessionIds, type SpaceReader } from "@/lib/spaces/grant"

export const spaceMayReadShape = (table: string): boolean => (MEMBER_SPACE.shapes as readonly string[]).includes(table)

/** A shape's where clause (already over the space's teams), confined to the
 *  space's own sessions where the table holds them. */
export async function spaceShapeWhere(space: SpaceGrant, table: string, where: string | null | undefined): Promise<string | null | undefined> {
  const { db } = await import(`@/db/connection`)
  const and = (own: string) => (where ? andClauses(where, own) : own)
  switch (table) {
    case `sessions`:
    case `messages`:
      return and(buildWhereClause(`root_id`, await spaceRootIds(db, space)))
    case `session_issues`:
      return and(buildWhereClause(`session_id`, await spaceSessionIds(db, space)))
    default:
      return where
  }
}

/** Whether the space may read this attachment: an issue's (its team is
 *  checked), a message's in its own trees, an upload of the member's nothing carries yet; never
 *  a draft's or an app record's. */
export async function attachmentInSpace(
  exec: SpaceReader,
  space: SpaceGrant,
  attachment: { attachmentId?: string; issueId?: string | null; draftId?: string | null; messageId?: string | null; appId?: string | null }
): Promise<boolean> {
  if (attachment.draftId || attachment.appId) return false
  if (attachment.issueId) return true
  if (!attachment.messageId) {
    // An upload nothing carries yet: its member's own.
    if (!attachment.attachmentId) return false
    const [row] = await exec.select({ uploaderId: attachments.uploaderId }).from(attachments).where(eq(attachments.id, attachment.attachmentId)).limit(1)
    return row?.uploaderId === space.userId
  }
  const [message] = await exec.select({ rootId: messages.rootId }).from(messages).where(eq(messages.id, attachment.messageId)).limit(1)
  return !!message?.rootId && rootInSpace(exec, space, message.rootId, `own`)
}
