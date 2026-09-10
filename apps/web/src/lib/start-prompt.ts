// EXP-825: the free text a start carries beside its subject — the chat
// prompt (required for the Chat and Create-action builtins), or additional
// instructions for an issue / batch / action run. It rides
// `steer.startSession` as `prompt` in the steer-image-message shape
// (`lib/steer-image-message.ts`: prose, then trailing
// `![image](/api/attachments/<id>)` embed lines), so the device localizes
// the images with the machinery it already has for steered messages.
//
// The images were uploaded BEFORE any session existed
// (`POST /api/teams/$teamId/session-files` — `session_attachments` rows with
// a NULL `session_id`), so this is where the server checks that every embed
// is one of the caller's own pending uploads for the subject's team. Pure:
// the attachment lookup is injected, like `action-inputs.ts`.

import { MAX_START_PROMPT, MAX_START_PROMPT_IMAGES } from "@exp/db-schema/domain"
import { parseSteerMessage } from "@/lib/steer-image-message"

export interface StartPromptAttachment {
  id: string
  teamId: string
  uploaderId: string | null
  /** Bound session's owner, null while the row is still pending. */
  sessionUserId: string | null
}

export interface StartPromptLookups {
  attachments(ids: string[]): Promise<StartPromptAttachment[]>
}

export type StartPromptResult =
  | { ok: true; prompt: string | undefined }
  | {
      ok: false
      code: `BAD_REQUEST` | `PRECONDITION_FAILED`
      message: string
    }

/** Whitespace-only text with no embeds is ABSENT — never forwarded. */
export function normalizeStartPrompt(
  prompt: string | undefined
): string | undefined {
  if (prompt === undefined) return undefined
  const trimmed = prompt.trim()
  return trimmed.length === 0 ? undefined : prompt
}

export async function resolveStartPrompt(
  prompt: string | undefined,
  teamId: string,
  userId: string,
  lookups: StartPromptLookups
): Promise<StartPromptResult> {
  const normalized = normalizeStartPrompt(prompt)
  if (normalized === undefined) return { ok: true, prompt: undefined }
  if (normalized.length > MAX_START_PROMPT) {
    return {
      ok: false,
      code: `BAD_REQUEST`,
      message: `The prompt is too long (max ${MAX_START_PROMPT} chars)`,
    }
  }
  const { attachmentIds } = parseSteerMessage(normalized)
  if (attachmentIds.length === 0) return { ok: true, prompt: normalized }
  if (attachmentIds.length > MAX_START_PROMPT_IMAGES) {
    return {
      ok: false,
      code: `BAD_REQUEST`,
      message: `Up to ${MAX_START_PROMPT_IMAGES} images per start`,
    }
  }
  if (new Set(attachmentIds).size !== attachmentIds.length) {
    return {
      ok: false,
      code: `BAD_REQUEST`,
      message: `An image is attached twice`,
    }
  }
  const rows = await lookups.attachments(attachmentIds)
  const byId = new Map(rows.map((row) => [row.id, row]))
  for (const id of attachmentIds) {
    const row = byId.get(id)
    const mine =
      row !== undefined &&
      row.teamId === teamId &&
      row.uploaderId === userId &&
      (row.sessionUserId === null || row.sessionUserId === userId)
    if (!mine) {
      return {
        ok: false,
        code: `PRECONDITION_FAILED`,
        message: `One of the attached images is not yours or has expired`,
      }
    }
  }
  return { ok: true, prompt: normalized }
}
