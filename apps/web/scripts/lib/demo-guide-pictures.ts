// EXP-1283: the Guide thread's pictures. The Guide picture viewer (tile →
// full-size, `index / count`) needs a REAL picture to photograph, so the
// batch-labels thread's topic carries two: the board's committed web and iOS
// shots. Stored exactly like a `/api/guide-uploads/$token` upload — the same
// validate-probe-put half (`prepareSessionImageBytes`) and the same owner-less
// team `attachments` row (`sessionAttachmentValues`); the Guide entry is what
// references them (the orphan sweep skips Guide-named rows).
import { readFile } from "node:fs/promises"
import { fileURLToPath } from "node:url"
import { db } from "@/db/connection"
import { attachments } from "@/db/schema"
import type { GuideEntry } from "@exp/db-schema/domain"
import {
  prepareSessionImageBytes,
  rollbackSessionImage,
  sessionAttachmentValues,
} from "@/lib/storage/session-attachment-upload"

export const DEMO_GUIDE_PICTURE_TOPIC = `Label picker on the selection bar`

/** label → the committed shot it stores (repo-root `shots/`). */
const PICTURES = [
  { label: `web`, file: `board/web.webp` },
  { label: `ios`, file: `board/ios.webp` },
] as const

const SHOTS_DIR = fileURLToPath(new URL(`../../../../shots/`, import.meta.url))

/** Stores the pictures + their rows; returns the Guide entries to append. */
export async function seedDemoGuidePictures(args: {
  teamId: string
  sessionId: string
  uploaderId: string
  at: Date
}): Promise<GuideEntry[]> {
  const entries: GuideEntry[] = []
  for (const picture of PICTURES) {
    const body = new Uint8Array(await readFile(`${SHOTS_DIR}${picture.file}`))
    const prepared = await prepareSessionImageBytes(
      { filename: `board-${picture.label}.webp`, contentType: `image/webp`, body },
      { teamId: args.teamId }
    )
    try {
      await db
        .insert(attachments)
        .values({
          ...sessionAttachmentValues(prepared, { teamId: args.teamId }, args.uploaderId),
          createdAt: args.at,
        })
    } catch (error) {
      await rollbackSessionImage(prepared.storageKey)
      throw error
    }
    entries.push({
      topic: DEMO_GUIDE_PICTURE_TOPIC,
      label: picture.label,
      attachmentId: prepared.attachmentId,
      width: prepared.width,
      height: prepared.height,
      at: args.at.getTime(),
      sessionId: args.sessionId,
    })
  }
  return entries
}
