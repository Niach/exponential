import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { issueDrafts } from "@/db/schema"
import { resolveSession } from "@/lib/auth/resolve-bearer"
import { storeAttachmentUpload } from "@/lib/storage/issue-attachment-upload"
import { assertTeamMember } from "@/lib/team-membership"

export interface DraftAttachmentUploadContext {
  params: { draftId: string }
  request: Request
}

const UUID_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

/**
 * EXP-878: upload into an issue DRAFT. The create dialog uploads EAGERLY —
 * a pasted image is stored the moment it lands, so the description only ever
 * carries final `/api/attachments/{id}` URLs and `issues.create({ draftId })`
 * just reparents the rows.
 *
 * Same request/response contract and the same caps as the issue `/files`
 * route (`storeAttachmentUpload` is literally shared), but the auth is
 * OWNER-ONLY: a draft is private to the person composing it — teammates never
 * see the row and can never put an embed of its bytes on the wire. A draft of
 * somebody else is a 404, not a 403, so the route is no existence oracle.
 */
export async function handleDraftAttachmentUpload({
  params,
  request,
}: DraftAttachmentUploadContext) {
  // Same credential surface as the issue upload route: session cookie,
  // bearer and expu_ api keys, with auth-plugin throws downgraded to a
  // clean 401.
  const session = await resolveSession(request)

  if (!session?.user) {
    throw new TRPCError({
      code: `UNAUTHORIZED`,
      message: `Unauthorized`,
    })
  }

  // The id column is uuid — reject garbage before Postgres turns it into a
  // 22P02-shaped 500.
  if (!UUID_RE.test(params.draftId)) {
    throw new TRPCError({
      code: `NOT_FOUND`,
      message: `Draft not found`,
    })
  }

  const [draft] = await db
    .select({
      id: issueDrafts.id,
      userId: issueDrafts.userId,
      teamId: issueDrafts.teamId,
    })
    .from(issueDrafts)
    .where(eq(issueDrafts.id, params.draftId))
    .limit(1)

  if (!draft || draft.userId !== session.user.id) {
    throw new TRPCError({
      code: `NOT_FOUND`,
      message: `Draft not found`,
    })
  }

  // Ownership is not enough on its own: a draft survives its author leaving
  // the team, and its bytes count against THAT team's storage budget.
  await assertTeamMember(session.user.id, draft.teamId)

  return storeAttachmentUpload(request, session.user.id, {
    kind: `draft`,
    draftId: draft.id,
    teamId: draft.teamId,
  })
}
