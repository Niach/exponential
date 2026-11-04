import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import sharp from "sharp"
import { TEAM_IMAGE_UPLOAD } from "@exp/domain-contract/team-image"
import { db } from "@/db/connection"
import { attachments, teams } from "@/db/schema"
import { resolveSession } from "@/lib/auth/resolve-bearer"
import { canonicalizeContentType } from "@/lib/storage/issue-attachments"
import {
  prepareSessionImageBytes,
  rollbackSessionImage,
  sessionAttachmentValues,
} from "@/lib/storage/session-attachment-upload"
import { assertTeamOwner } from "@/lib/team-membership"

// #EXP-1645 (fixtures/team-image.json): the team image, on the session upload's
// storage half. Every accepted type (SVG too, which the byte route never serves
// inline and the phones cannot all draw) becomes ONE transparent PNG, square,
// nothing cropped, so all four clients paint the same bytes. The row stays
// owner-less: the orphan sweep keeps what a team's `icon_url` names and reclaims
// a replaced or removed image after its window.

const UUID_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

/** The image as the stored PNG; a file sharp cannot read = BAD_REQUEST. */
export async function normalizeTeamImage(body: Uint8Array): Promise<Uint8Array> {
  const side = TEAM_IMAGE_UPLOAD.side
  try {
    return await sharp(Buffer.from(body), { density: 300 })
      .resize(side, side, {
        fit: `contain`,
        background: { r: 0, g: 0, b: 0, alpha: 0 },
      })
      .png()
      .toBuffer()
  } catch {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `That image could not be read` })
  }
}

export async function handleTeamIconUpload({
  params,
  request,
}: {
  params: { teamId: string }
  request: Request
}) {
  const session = await resolveSession(request)
  if (!session?.user) {
    throw new TRPCError({ code: `UNAUTHORIZED`, message: `Unauthorized` })
  }
  if (!UUID_RE.test(params.teamId)) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Team not found` })
  }
  await assertTeamOwner(session.user.id, params.teamId)

  const file = (await request.formData()).get(`file`)
  if (!(file instanceof File)) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `Missing file` })
  }
  if (!TEAM_IMAGE_UPLOAD.accept.includes(canonicalizeContentType(file.type))) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `Team images must be PNG, JPEG, WebP, GIF or SVG` })
  }
  if (file.size === 0) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `File is empty` })
  }
  if (file.size > TEAM_IMAGE_UPLOAD.maxBytes) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Team images must be ${TEAM_IMAGE_UPLOAD.maxBytes / (1024 * 1024)} MB or smaller`,
    })
  }

  const body = await normalizeTeamImage(new Uint8Array(await file.arrayBuffer()))
  const scope = { teamId: params.teamId }
  const prepared = await prepareSessionImageBytes(
    { filename: `team-image.png`, contentType: `image/png`, body },
    scope,
    undefined,
    { imagesOnly: true }
  )
  try {
    await db.transaction(async (tx) => {
      await tx.insert(attachments).values(sessionAttachmentValues(prepared, scope, session.user.id))
      await tx.update(teams).set({ iconUrl: prepared.url }).where(eq(teams.id, params.teamId))
    })
  } catch (error) {
    await rollbackSessionImage(prepared.storageKey)
    throw error
  }
  return Response.json({ iconUrl: prepared.url })
}
