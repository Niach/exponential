import { TRPCError } from "@trpc/server"
import { createFileRoute } from "@tanstack/react-router"
import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { attachments, sessions } from "@/db/schema"
import { errorToResponse } from "@/lib/http-errors"
import { prepareSessionImage } from "@/lib/storage/session-attachment-upload"
import { verifyGuideUploadToken } from "@/lib/storage/guide-upload-token"
import { publishGuidePicture } from "@/lib/sessions/guide"
import { assertCardPictureOpen, publishCardPicture } from "@/lib/sessions/plan"
import { buildAttachmentUrl } from "@/lib/storage/issue-attachments"

// EXP-879/1291: a Guide picture's upload. One of the app's few anonymous
// endpoints, deliberately: the agent publishing a screenshot holds a signed
// token, not a session — `curl -F file=@shot.png <uploadUrl>` is the whole
// client. The HMAC token IS the authorization (lib/storage/guide-upload-
// token.ts): minted only after the caller was checked against a LIVE
// session, bound to one (session, topic, label, user) for ten minutes.
// The status gate lives at mint time: this route only needs the session to
// still EXIST (a session may end while curl is in flight). The write is the
// root-locked Guide read-modify-write (`publishGuidePicture`).
// EXP-1280: the same route files a plan/question card's picture (a token
// with a CARD target, `m`): the attachment is owned by the card's message
// and the card's label resolves in one transaction (`publishCardPicture`).
// FEED-75: a throttled object store answers 503 + Retry-After past the
// storage retries; the handed-out curl carries `--retry`.

async function uploadGuidePicture({
  params,
  request,
}: {
  params: { token: string }
  request: Request
}) {
  // 401 before ANY database work: a bad token learns nothing.
  const payload = verifyGuideUploadToken(params.token)
  if (!payload) {
    throw new TRPCError({ code: `UNAUTHORIZED`, message: `Unauthorized` })
  }

  const [session] = await db
    .select({ id: sessions.id, teamId: sessions.teamId })
    .from(sessions)
    .where(eq(sessions.id, payload.s))
    .limit(1)
  if (!session) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })
  }

  if (payload.m) {
    // Refused before any storage work: an unknown label or a done picture.
    await assertCardPictureOpen(payload.m, session.id, payload.l)
    const prepared = await prepareSessionImage(request, { teamId: session.teamId }, undefined, {
      imagesOnly: true,
    })
    await publishCardPicture(
      {
        messageId: payload.m,
        sessionId: session.id,
        teamId: session.teamId,
        label: payload.l,
        uploaderId: payload.u,
      },
      prepared
    )
    return Response.json({
      ok: true,
      id: prepared.attachmentId,
      url: buildAttachmentUrl(prepared.attachmentId),
      width: prepared.width,
      height: prepared.height,
      label: payload.l,
    })
  }

  // A show grant pre-allocated its attachment id: a second curl of the same
  // line is refused before any storage work.
  if (payload.a) {
    const [taken] = await db
      .select({ id: attachments.id })
      .from(attachments)
      .where(eq(attachments.id, payload.a))
      .limit(1)
    if (taken) {
      throw new TRPCError({
        code: `CONFLICT`,
        message: `This picture was already uploaded. Ask for a new upload line.`,
      })
    }
  }

  const prepared = await prepareSessionImage(request, { teamId: session.teamId }, payload.a, {
    imagesOnly: true,
  })
  const published = await publishGuidePicture(
    {
      sessionId: session.id,
      teamId: session.teamId,
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
    results: published.results,
  })
}

export const Route = createFileRoute(`/api/guide-uploads/$token`)({
  server: {
    handlers: {
      POST: async (context) => {
        try {
          return await uploadGuidePicture(context)
        } catch (error) {
          return errorToResponse(error)
        }
      },
    },
  },
})
