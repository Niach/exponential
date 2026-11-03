// VAPP-221 (eApps R7): files on records. A FileUpload in an app uploads its
// picked bytes here (`POST /api/apps/{appId}/files`, one multipart part
// named "file"; the same caps, team storage budget and response as the
// session `/files` routes). The row is the APP's (`attachments.app_id`), no
// other owner; its `url` (`/api/attachments/{id}`) is what a record's
// `attachment` field holds, and the app hosts' media rule (`appMediaAllowed`)
// loads it. A write needs write access to the app (eApps R3). The records
// service accepts only refs to this app's and team's rows
// (`assertRecordAttachments`); the app-file sweep reclaims a row no record of
// its app references any more (a trashed record still does, until the trash
// purge), after the orphan grace window.
import { TRPCError } from "@trpc/server"
import { and, eq, inArray } from "drizzle-orm"
import { RECORD_POLICY_COPY, recordAttachmentIds, type RecordSchema } from "@exponential_app/eui/host"
import { db } from "@/db/connection"
import { attachments } from "@/db/schema"
import { resolveSession } from "@/lib/auth/resolve-bearer"
import { loadApp, appAccessLevel } from "@/lib/apps/records-service"
import { assertTeamMember } from "@/lib/auth/membership"
import {
  prepareSessionImage,
  rollbackSessionImage,
  sessionAttachmentValues,
  sessionImageResponse,
} from "@/lib/storage/session-attachment-upload"
import type { Tx } from "@/lib/sessions/tx"

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

export async function handleAppFileUpload({ params, request }: { params: { appId: string }; request: Request }) {
  const session = await resolveSession(request)
  if (!session?.user) throw new TRPCError({ code: `UNAUTHORIZED`, message: `Unauthorized` })
  if (!UUID_RE.test(params.appId)) throw new TRPCError({ code: `NOT_FOUND`, message: `App not found` })
  const app = await loadApp(db, { id: params.appId.toLowerCase() })
  await assertTeamMember(session.user.id, app.teamId)
  const access = await appAccessLevel(db, app.id, session.user.id)
  if (access === `none`) throw new TRPCError({ code: `NOT_FOUND`, message: `App not found` })
  if (access !== `write`) throw new TRPCError({ code: `FORBIDDEN`, message: RECORD_POLICY_COPY.readOnly })
  const prepared = await prepareSessionImage(request, { teamId: app.teamId })
  try {
    await db.insert(attachments).values({ ...sessionAttachmentValues(prepared, { teamId: app.teamId }, session.user.id), appId: app.id, asFile: true })
  } catch (error) {
    await rollbackSessionImage(prepared.storageKey)
    throw error
  }
  return sessionImageResponse(prepared)
}

/** The refusal for refs outside the app (one message, the first few ids). */
export const APP_FILE_REFUSAL = `attachments must be files uploaded to this app`

/** VAPP-221: every attachment a write's data references through its
 *  schema's `attachment` fields must be a row of THIS app and team (an app
 *  can never reach an issue's, a session's or another app's file). Runs
 *  before the records role (it reads `attachments`). */
export async function assertRecordAttachments(
  tx: Pick<Tx, `select`>,
  app: { id: string; teamId: string },
  writes: readonly { schema: RecordSchema; data: Record<string, unknown> }[]
): Promise<void> {
  const ids = [...new Set(writes.flatMap((w) => recordAttachmentIds(w.schema, w.data)))]
  if (ids.length === 0) return
  const rows = await tx
    .select({ id: attachments.id })
    .from(attachments)
    .where(and(inArray(attachments.id, ids), eq(attachments.teamId, app.teamId), eq(attachments.appId, app.id)))
  const found = new Set(rows.map((row) => row.id))
  const missing = ids.filter((id) => !found.has(id))
  if (missing.length > 0) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `record is invalid: ${APP_FILE_REFUSAL} (${missing.slice(0, 3).join(`, `)})` })
  }
}
