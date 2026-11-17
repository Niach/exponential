import { TRPCError } from "@trpc/server"
import { eq, sql } from "drizzle-orm"
import { AGENT_VOICE } from "@exp/domain-contract/agent-settings"
import { db } from "@/db/connection"
import { agents, attachments } from "@/db/schema"
import { resolveSession } from "@/lib/auth/resolve-bearer"
import { canonicalizeContentType } from "@/lib/storage/issue-attachments"
import {
  prepareSessionImageBytes,
  rollbackSessionImage,
  sessionAttachmentValues,
} from "@/lib/storage/session-attachment-upload"
import { loadOwnedAgent } from "@/lib/trpc/agents"

// #EXP-1847 (agent-settings.json `voice.clipUpload`): an agent's voice clip,
// on the session upload's storage half. Its editors only (a team agent's
// owners, the Assistant's member). The row stays owner-less: the orphan sweep
// keeps what an agent's `voice.clipId` names and reclaims a replaced or
// removed clip after its window.

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

/** A WAV by its bytes (`RIFF….WAVE`): pickers on the phones send no or a
 *  generic type; a declared type must be one of `clipUpload.accept`. */
export function isWav(type: string, body: Uint8Array): boolean {
  const declared = canonicalizeContentType(type)
  if (declared && declared !== `application/octet-stream` && !AGENT_VOICE.clipUpload.accept.includes(declared)) return false
  const tag = (from: number) => String.fromCharCode(...body.subarray(from, from + 4))
  return body.byteLength >= 12 && tag(0) === `RIFF` && tag(8) === `WAVE`
}

async function editableAgent(request: Request, agentId: string) {
  const session = await resolveSession(request)
  if (!session?.user) throw new TRPCError({ code: `UNAUTHORIZED`, message: `Unauthorized` })
  if (!UUID_RE.test(agentId)) throw new TRPCError({ code: `NOT_FOUND`, message: `Agent not found` })
  const agent = await loadOwnedAgent(session.user.id, agentId)
  return { userId: session.user.id, agent }
}

/** POST: one multipart part `file` (a WAV) → {clipId, clipName}. */
export async function handleAgentVoiceClipUpload({ params, request }: { params: { agentId: string }; request: Request }) {
  const { userId, agent } = await editableAgent(request, params.agentId)
  const file = (await request.formData()).get(`file`)
  if (!(file instanceof File)) throw new TRPCError({ code: `BAD_REQUEST`, message: `Missing file` })
  if (file.size === 0) throw new TRPCError({ code: `BAD_REQUEST`, message: `File is empty` })
  if (file.size > AGENT_VOICE.clipUpload.maxBytes) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: AGENT_VOICE.copy.errors.clipSize })
  }
  const body = new Uint8Array(await file.arrayBuffer())
  if (!isWav(file.type, body)) throw new TRPCError({ code: `BAD_REQUEST`, message: AGENT_VOICE.copy.errors.clipType })
  const scope = { teamId: agent.teamId }
  const prepared = await prepareSessionImageBytes(
    { filename: file.name || `voice.wav`, contentType: `audio/wav`, body },
    scope
  )
  // #EXP-1935: merged in SQL, never written whole from the row read before
  // the body arrived: a transcript saved meanwhile stays.
  const clip = JSON.stringify({ clipId: prepared.attachmentId, clipName: prepared.filename })
  const voice = sql`coalesce(${agents.voice}, '{}'::jsonb) || ${clip}::jsonb`
  try {
    await db.transaction(async (tx) => {
      await tx.insert(attachments).values(sessionAttachmentValues(prepared, scope, userId))
      await tx.update(agents).set({ voice, updatedAt: new Date() }).where(eq(agents.id, agent.id))
    })
  } catch (error) {
    await rollbackSessionImage(prepared.storageKey)
    throw error
  }
  return Response.json({ clipId: prepared.attachmentId, clipName: prepared.filename })
}

/** DELETE: the agent speaks with the built-in (or the member's) voice again. */
export async function handleAgentVoiceClipDelete({ params, request }: { params: { agentId: string }; request: Request }) {
  const { agent } = await editableAgent(request, params.agentId)
  const voice = sql`coalesce(${agents.voice}, '{}'::jsonb) - 'clipId' - 'clipName'`
  await db.update(agents).set({ voice, updatedAt: new Date() }).where(eq(agents.id, agent.id))
  return Response.json({ ok: true })
}
