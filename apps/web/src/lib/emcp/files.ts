// #NEW-149 (packages/emcp fixtures/files.json, D5 of #NEW-143): the draft
// `io.modelcontextprotocol/files` extension on `/api/mcp`, BEHIND
// `MCP_FILES_EXTENSION` (off = nothing here runs, nothing is declared).
// A tool's file argument (`fileInputs`: today `exponential_attachments_write
// {op: "upload", file}`) is routed onto the upload op's existing shapes:
//  - inline (data, <= 1 MiB per call) = the inline `dataBase64` path;
//  - out of band = the SIGNED path (EXP-929): the op's own access checks
//    mint the attachment's upload token, which is re-minted for the MCP
//    staging window with the file's declared size as its cap and answered
//    as `upload_required` to `${APP_URL}/api/mcp/uploads/<token>` (the same
//    handler as /api/attachment-uploads/$token), with an MRTR requestState
//    (mrtr.ts: sealed to the member, credential, tool and exact arguments,
//    spent once by the server's verify hook) carrying the staged map;
//  - the retry = the size, digest and type re-checked against what landed,
//    then the op's finalize (attachmentId alone, EXP-955's HEAD check);
//    a staged upload nobody finalized is swept (`mcp_file_stagings`).
// Attachment reads add a DownloadTarget for a client that declared
// `download` (`downloadMetaFor`).
import { ProtocolError } from "@modelcontextprotocol/server"
import { eq, inArray, lte } from "drizzle-orm"
import {
  downloadMeta,
  fileDigest,
  fileErrorResult,
  fileInputSchema,
  fileInputSpec,
  filesDeclared,
  planFiles,
  STAGING_TTL_MS,
  stagedVerdict,
  uploadCapabilityError,
  uploadRequired,
  type DownloadableAttachment,
  type FileInput,
  type StagedObject,
} from "@exp/emcp/files"
import { z } from "zod"
import { db } from "@/db/connection"
import { attachments, mcpFileStagings } from "@/db/schema"
import { deleteObject, getObject } from "@/lib/storage"
import { mintAttachmentUploadToken, verifyAttachmentUploadToken } from "@/lib/storage/attachment-upload-token"
import { canonicalizeContentType } from "@/lib/storage/issue-attachments"
import { mcpFilesEnabled } from "./extensions"
import { escapeAsRawResult, escapeAsRpcError, mintState, redeemInput, RequestStateError } from "./mrtr"
import type { EmcpToolContext } from "./request-context"

type ToolResult = { content: unknown[]; isError?: boolean; structuredContent?: unknown }

/** The sealed `data` of an `upload_required` state: file id -> attachment id. */
interface UploadData {
  kind: `upload`
  staged: Record<string, string>
}

/** The upload path a token names (`/api/attachment-uploads/<token>`). */
const SIGNED_UPLOAD_PATH = `/api/attachment-uploads/`

/** What the noun router does with a call that carries a file argument. */
export type FileRoute =
  | { kind: `result`; result: unknown }
  | { kind: `args`; args: Record<string, unknown>; after?: (handled: ToolResult) => Promise<unknown> }

const textError = (text: string): ToolResult => ({ content: [{ type: `text`, text }], isError: true })

/** The noun half's extra `file` property (JSON schema with `x-mcp-file`), flag on only.
 *  Any value passes the SDK: the router answers a bad FileInput as the fixture's isError. */
export function fileArgumentShape(tool: string, ops: readonly string[]): Record<string, z.ZodType> {
  if (!mcpFilesEnabled()) return {}
  for (const op of ops) {
    const spec = fileInputSpec(tool, op)
    const schema = fileInputSchema(tool, op)
    if (spec && schema) return { [spec.arg]: z.unknown().meta(schema).optional() }
  }
  return {}
}

/** Did this request's client declare `download` (flag on)? */
export function downloadDeclared(ctx: Partial<Pick<EmcpToolContext, `client`>> | undefined): boolean {
  return mcpFilesEnabled() && filesDeclared(ctx?.client?.extensions).download
}

/** An attachment read's `_meta` for a client that declared `download`, else null. */
export function downloadMetaFor(ctx: Partial<Pick<EmcpToolContext, `client`>> | undefined, attachment: DownloadableAttachment): Record<string, unknown> | null {
  return downloadMeta(attachment, downloadDeclared(ctx))
}

async function landedObject(attachmentId: string, userId: string, file: FileInput) {
  const [row] = await db
    .select({ id: attachments.id, uploaderId: attachments.uploaderId, sizeBytes: attachments.sizeBytes, contentType: attachments.contentType, storageKey: attachments.storageKey })
    .from(attachments)
    .where(eq(attachments.id, attachmentId))
    .limit(1)
  if (!row || row.uploaderId !== userId) return null
  let digest = ``
  if (file.digest && row.sizeBytes === file.size) {
    const object = await getObject(row.storageKey)
    const bytes = object?.Body ? await object.Body.transformToByteArray() : null
    if (!bytes) return null
    digest = fileDigest(Buffer.from(bytes))
  }
  return { row, object: { size: row.sizeBytes, digest } satisfies StagedObject }
}

/** Bytes that landed but failed a check are garbage: the row and its object go. */
async function discard(row: { id: string; storageKey: string }) {
  await db.delete(attachments).where(eq(attachments.id, row.id))
  await db.delete(mcpFileStagings).where(eq(mcpFileStagings.attachmentId, row.id))
  await deleteObject(row.storageKey).catch((error: unknown) => console.error(`MCP files: could not delete a refused upload`, error))
}

/** The watchdog (files.json `staging.ttlMs`): a staged upload nobody
 *  finalized goes, its attachment row and object with it. */
export async function sweepExpiredFileStagings(now: Date = new Date()): Promise<number> {
  const due = await db.select({ attachmentId: mcpFileStagings.attachmentId }).from(mcpFileStagings).where(lte(mcpFileStagings.expiresAt, now)).limit(500)
  if (due.length === 0) return 0
  const ids = due.map((row) => row.attachmentId)
  const rows = await db.delete(attachments).where(inArray(attachments.id, ids)).returning({ storageKey: attachments.storageKey })
  await db.delete(mcpFileStagings).where(inArray(mcpFileStagings.attachmentId, ids))
  for (const row of rows) {
    await deleteObject(row.storageKey).catch((error: unknown) => console.error(`MCP files: could not delete an expired staged upload`, error))
  }
  return rows.length
}

/**
 * Route a call carrying a file argument onto the op's own shapes, or null
 * (no file argument, the flag off, a tool without one).
 */
export async function routeFileArgument(
  ctx: EmcpToolContext,
  input: { tool: string; op: string; args: Record<string, unknown>; userId: string; origin: string }
): Promise<FileRoute | null> {
  const spec = fileInputSpec(input.tool, input.op)
  if (!mcpFilesEnabled() || !spec || !(spec.arg in input.args)) return null
  const { [spec.arg]: raw, ...rest } = input.args
  const taken = [`dataBase64`, `filename`, `contentType`, `attachmentId`].filter((key) => key in rest)
  if (taken.length > 0) return { kind: `result`, result: textError(`Pass ${spec.arg} without ${taken.join(`, `)}: the file carries its name, type and bytes.`) }

  let redeemed: ReturnType<typeof redeemInput<UploadData>>
  try {
    redeemed = redeemInput<UploadData>(ctx)
  } catch (error) {
    if (error instanceof ProtocolError) return { kind: `result`, result: escapeAsRpcError(ctx, error) }
    throw error
  }

  // RETRY of an `upload_required`: the state (already opened, bound to this
  // member, credential, tool and exact arguments, and spent) carries the map.
  if (redeemed?.data?.kind === `upload`) {
    const file = raw as FileInput
    const staged = redeemed.data.staged ?? {}
    const attachmentId = staged[file.id]
    const landed = attachmentId ? await landedObject(attachmentId, input.userId, file) : null
    const verdict = stagedVerdict(staged, [file], () => landed?.object ?? null)
    if (verdict.kind === `refused`) return { kind: `result`, result: escapeAsRpcError(ctx, new RequestStateError(`args`)) }
    if (verdict.kind === `error`) {
      // Short or wrong bytes are garbage: the caller starts a new call.
      if (landed) await discard(landed.row)
      return { kind: `result`, result: fileErrorResult(verdict.error, verdict.fileId) }
    }
    if (file.size === 0 || typeof file.data === `string`) return { kind: `result`, result: fileErrorResult(`invalid`, file.id) }
    // The type re-check: the row's is the one the token bound at mint time.
    if (!landed || landed.row.contentType !== canonicalizeContentType(file.mimeType)) {
      if (landed) await discard(landed.row)
      return { kind: `result`, result: fileErrorResult(`invalid`, file.id) }
    }
    // Verified: the op's finalize makes it the caller's; no longer staged.
    await db.delete(mcpFileStagings).where(eq(mcpFileStagings.attachmentId, attachmentId!))
    return { kind: `args`, args: { attachmentId } }
  }

  // FIRST CALL.
  const plan = planFiles([raw], spec.maxSize)
  if (plan.kind === `error`) return { kind: `result`, result: fileErrorResult(plan.error, plan.fileId) }
  const file = raw as FileInput
  const base = { ...rest, filename: file.name || file.id, contentType: file.mimeType }
  if (plan.kind === `run`) {
    const bytes = plan.inline[0]?.bytes
    if (!bytes || bytes.length === 0) return { kind: `result`, result: textError(`File ${file.id} is empty.`) }
    return { kind: `args`, args: { ...base, dataBase64: bytes.toString(`base64`) } }
  }
  const missing = uploadCapabilityError([file], filesDeclared(ctx.client.extensions).upload)
  if (missing) return { kind: `result`, result: escapeAsRpcError(ctx, new ProtocolError(missing.code, missing.message, missing.data)) }
  // No dataBase64 = the op's signed mint, after its own access checks.
  return {
    kind: `args`,
    args: base,
    after: async (handled) => {
      if (handled.isError) return handled
      const text = (handled.content?.[0] as { text?: string } | undefined)?.text ?? ``
      const minted = JSON.parse(text) as { attachmentId?: string; uploadUrl?: string }
      const at = minted.uploadUrl?.indexOf(SIGNED_UPLOAD_PATH) ?? -1
      const payload = at >= 0 ? verifyAttachmentUploadToken(minted.uploadUrl!.slice(at + SIGNED_UPLOAD_PATH.length)) : null
      if (!minted.attachmentId || !payload) throw new Error(`MCP files: the upload op did not mint a signed upload`)
      const staging = mintAttachmentUploadToken({
        attachmentId: payload.a,
        issueId: payload.i,
        boardId: payload.b,
        teamId: payload.w,
        userId: payload.u,
        filename: payload.f,
        contentType: payload.c,
        commentId: payload.m,
        maxBytes: file.size,
        ttlMs: STAGING_TTL_MS,
      })
      await db.insert(mcpFileStagings).values({ attachmentId: minted.attachmentId, userId: input.userId, expiresAt: staging.expiresAt })
      const data: UploadData = { kind: `upload`, staged: { [file.id]: minted.attachmentId } }
      // Not a CallToolResult: past the SDK's result validation as is (mrtr.ts).
      return escapeAsRawResult(ctx, uploadRequired([file], { [file.id]: staging.token }, input.origin, staging.expiresAt.toISOString(), mintState(ctx, data, Date.now(), STAGING_TTL_MS)))
    },
  }
}
