import { TRPCError } from "@trpc/server"
import { createFileRoute } from "@tanstack/react-router"
import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { codingSessions, sessionAttachments } from "@/db/schema"
import { errorToResponse } from "@/lib/http-errors"
import { deleteObject } from "@/lib/storage"
import {
  prepareSessionImage,
  rollbackSessionImage,
  sessionAttachmentValues,
} from "@/lib/storage/session-attachment-upload"
import { verifySessionResultToken } from "@/lib/storage/session-result-token"
import {
  exceedsSessionResultsCap,
  resultsSummary,
  upsertSessionResult,
} from "@/lib/session-result-writes"
import { buildAttachmentUrl } from "@/lib/storage/issue-attachments"

// EXP-879: the run's screenshot upload. One of the app's very few anonymous
// endpoints, and deliberately so: the agent that publishes a picture holds a
// signed token, not a session — `curl -F file=@shot.png <uploadUrl>` is the
// whole client. The HMAC token IS the authorization (see
// `lib/storage/session-result-token.ts`): MCP
// `exponential_sessions_results` mints it only after checking that the
// header's run belongs to the caller AND is still live, and it is bound to
// exactly one (session, topic, label, user) for ten minutes.
//
// THE STATUS GATE LIVES AT MINT TIME. This route only requires the row to
// still EXIST: a run may end while curl is in flight (or while the agent is
// taking the shot), and a picture that was authorized must not evaporate
// because of that race. Membership is not re-checked either — the same
// reasoning as the EXP-704 download token.
//
// The write is a jsonb read-modify-write, so it runs inside a transaction
// that takes `FOR UPDATE` on the coding_sessions row: two pictures published
// concurrently by the same run would otherwise lose one another.

async function uploadSessionResult({
  params,
  request,
}: {
  params: { token: string }
  request: Request
}) {
  // 401 before ANY database work: a bad or expired token is anonymous, and an
  // anonymous caller learns nothing about which runs exist.
  const payload = verifySessionResultToken(params.token)
  if (!payload) {
    throw new TRPCError({
      code: `UNAUTHORIZED`,
      message: `Unauthorized`,
    })
  }

  // Unlocked read first, only for the team the object is billed and keyed to
  // — the locked re-read inside the transaction is what the update trusts.
  const [run] = await db
    .select({ id: codingSessions.id, teamId: codingSessions.teamId })
    .from(codingSessions)
    .where(eq(codingSessions.id, payload.s))
    .limit(1)

  if (!run?.teamId) {
    throw new TRPCError({
      code: `NOT_FOUND`,
      message: `Session not found`,
    })
  }

  const scope = { teamId: run.teamId, sessionId: run.id }
  // Validates the part (images only, 10 MB), charges the team's storage
  // budget, probes the pixel size and puts the object — no row yet.
  const prepared = await prepareSessionImage(request, scope)

  let displacedAttachmentId: string | null = null
  let displacedStorageKey: string | null = null
  let results: Awaited<ReturnType<typeof upsertSessionResult>>[`results`] = []

  try {
    await db.transaction(async (tx) => {
      const [locked] = await tx
        .select({
          id: codingSessions.id,
          results: codingSessions.results,
        })
        .from(codingSessions)
        .where(eq(codingSessions.id, payload.s))
        .limit(1)
        .for(`update`)

      if (!locked) {
        throw new TRPCError({
          code: `NOT_FOUND`,
          message: `Session not found`,
        })
      }

      const upsert = upsertSessionResult(locked.results, {
        topic: payload.t,
        label: payload.l,
        attachmentId: prepared.attachmentId,
        width: prepared.width,
        height: prepared.height,
      })
      if (exceedsSessionResultsCap(upsert.results)) {
        throw new TRPCError({
          code: `CONFLICT`,
          message: `This run already published the maximum number of results. Remove one first (exponential_sessions_results with remove).`,
        })
      }
      results = upsert.results
      displacedAttachmentId = upsert.displacedAttachmentId

      await tx
        .insert(sessionAttachments)
        .values(sessionAttachmentValues(prepared, scope, payload.u))

      if (upsert.displacedAttachmentId) {
        const [gone] = await tx
          .delete(sessionAttachments)
          .where(eq(sessionAttachments.id, upsert.displacedAttachmentId))
          .returning({ storageKey: sessionAttachments.storageKey })
        displacedStorageKey = gone?.storageKey ?? null
      }

      await tx
        .update(codingSessions)
        .set({ results: upsert.results, updatedAt: new Date() })
        .where(eq(codingSessions.id, payload.s))
    })
  } catch (error) {
    // The row never landed, so the object it points at is garbage.
    await rollbackSessionImage(prepared.storageKey)
    throw error
  }

  // Only once the swap is durable: the replaced picture's bytes go.
  if (displacedStorageKey) {
    try {
      await deleteObject(displacedStorageKey)
    } catch (deleteError) {
      console.error(
        `Failed to delete the replaced session result object`,
        deleteError
      )
    }
  }

  return Response.json({
    ok: true,
    id: prepared.attachmentId,
    url: buildAttachmentUrl(prepared.attachmentId),
    width: prepared.width,
    height: prepared.height,
    topic: payload.t,
    label: payload.l,
    replaced: displacedAttachmentId !== null,
    results: resultsSummary(results),
  })
}

export const Route = createFileRoute(`/api/session-results/$token`)({
  server: {
    handlers: {
      POST: async (context) => {
        try {
          return await uploadSessionResult(context)
        } catch (error) {
          return errorToResponse(error)
        }
      },
    },
  },
})
