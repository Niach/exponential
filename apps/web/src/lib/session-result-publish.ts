import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { codingSessions, sessionAttachments } from "@/db/schema"
import type { CodingSessionResult } from "@exp/db-schema/domain"
import { deleteObject } from "@/lib/storage"
import {
  rollbackSessionImage,
  sessionAttachmentValues,
  type PreparedSessionImage,
} from "@/lib/storage/session-attachment-upload"
import {
  exceedsSessionResultsCap,
  nextShowLabel,
  upsertSessionResult,
} from "@/lib/session-result-writes"

// EXP-879/EXP-1172: the ONE write behind every published picture — the
// token-gated upload route (`sessions_results` and `sessions_show` grants)
// and `exponential_sessions_show`'s `dataBase64`. The object is already put
// (`prepareSessionImage*`); this lands the `session_attachments` row and the
// `coding_sessions.results` entry inside a transaction holding `FOR UPDATE`
// on the run (a jsonb read-modify-write: two pictures in flight would
// otherwise lose one another), then reclaims a replaced picture's bytes.

export interface SessionResultPicture {
  sessionId: string
  teamId: string
  topic: string
  /** null = a show without a label: the topic's next `Shot N`. */
  label: string | null
  uploaderId: string
  /** EXP-1172: a `sessions_show` picture + its caption. */
  inline?: { caption: string | null }
}

export interface PublishedSessionResult {
  label: string
  results: CodingSessionResult[]
  replaced: boolean
}

export async function publishSessionResultPicture(
  input: SessionResultPicture,
  prepared: PreparedSessionImage
): Promise<PublishedSessionResult> {
  const scope = { teamId: input.teamId, sessionId: input.sessionId }
  let displacedAttachmentId: string | null = null
  let displacedStorageKey: string | null = null
  let results: CodingSessionResult[] = []
  let label = input.label ?? ``

  try {
    await db.transaction(async (tx) => {
      const [locked] = await tx
        .select({
          id: codingSessions.id,
          results: codingSessions.results,
        })
        .from(codingSessions)
        .where(eq(codingSessions.id, input.sessionId))
        .limit(1)
        .for(`update`)

      if (!locked) {
        throw new TRPCError({
          code: `NOT_FOUND`,
          message: `Session not found`,
        })
      }

      label = input.label || nextShowLabel(locked.results, input.topic)
      const upsert = upsertSessionResult(locked.results, {
        topic: input.topic,
        label,
        attachmentId: prepared.attachmentId,
        width: prepared.width,
        height: prepared.height,
        // Only a show's entry carries the keys: a results picture stays
        // byte-identical to what every reader parsed before EXP-1172.
        ...(input.inline
          ? {
              inline: true,
              ...(input.inline.caption ? { caption: input.inline.caption } : {}),
            }
          : {}),
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
        .values(sessionAttachmentValues(prepared, scope, input.uploaderId))

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
        .where(eq(codingSessions.id, input.sessionId))
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

  return { label, results, replaced: displacedAttachmentId !== null }
}
