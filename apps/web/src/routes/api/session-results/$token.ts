import { TRPCError } from "@trpc/server"
import { createFileRoute } from "@tanstack/react-router"
import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { codingSessions, sessionAttachments } from "@/db/schema"
import { errorToResponse } from "@/lib/http-errors"
import { prepareSessionImage } from "@/lib/storage/session-attachment-upload"
import { verifySessionResultToken } from "@/lib/storage/session-result-token"
import { resultsSummary } from "@/lib/session-result-writes"
import { publishSessionResultPicture } from "@/lib/session-result-publish"
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
// FEED-75: the object store may throttle the put (Hetzner: `503 SlowDown`).
// `lib/storage` waits that out; past its retries the route answers 503 +
// Retry-After, and the curl line the tools hand out carries `--retry` so
// the same command lands on its own once the window moves on.
//
// The write is a jsonb read-modify-write, so it runs inside a transaction
// that takes `FOR UPDATE` on the coding_sessions row: two pictures published
// concurrently by the same run would otherwise lose one another. EXP-1172:
// that write is `publishSessionResultPicture`, shared with sessions_show.

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

  // EXP-1172: a show grant pre-allocated its attachment id, so a second curl
  // of the same line would put its object over the first one's key and then
  // fail the row insert; refuse it before any storage work.
  if (payload.a) {
    const [taken] = await db
      .select({ id: sessionAttachments.id })
      .from(sessionAttachments)
      .where(eq(sessionAttachments.id, payload.a))
      .limit(1)
    if (taken) {
      throw new TRPCError({
        code: `CONFLICT`,
        message: `This picture was already uploaded. Call exponential_sessions_show again for another one.`,
      })
    }
  }

  const scope = { teamId: run.teamId, sessionId: run.id }
  // Validates the part (images only, 10 MB), charges the team's storage
  // budget, probes the pixel size and puts the object — no row yet.
  const prepared = await prepareSessionImage(request, scope, payload.a)
  const published = await publishSessionResultPicture(
    {
      sessionId: run.id,
      teamId: run.teamId,
      topic: payload.t,
      label: payload.l || null,
      uploaderId: payload.u,
      ...(payload.i === 1 ? { inline: { caption: payload.c ?? null } } : {}),
    },
    prepared
  )

  return Response.json({
    ok: true,
    id: prepared.attachmentId,
    url: buildAttachmentUrl(prepared.attachmentId),
    width: prepared.width,
    height: prepared.height,
    topic: payload.t,
    label: published.label,
    replaced: published.replaced,
    results: resultsSummary(published.results),
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
