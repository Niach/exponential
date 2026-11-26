// #NEW-145 / #NEW-143 D5: file uploads over MCP (`io.modelcontextprotocol/files`,
// draft), behind MCP_FILES_EXTENSION: inline payloads up to 1 MiB, an
// `upload_required` round to staging above that, and the retry's checks.
// Pure (node:crypto) and locked by `fixtures/files.json` (its `$comment`
// is the spec); the requestState is request-state.ts's.

import { createHash } from "node:crypto"

import fixture from "../fixtures/files.json" with { type: "json" }
import { verifyRequestState, type RequestStateError, type VerifyContext } from "./request-state"

export const FILES_EXTENSION: string = fixture.extension
export const MAX_INLINE_BYTES: number = fixture.capability.maxInlineBytes

/** Is the extension on in this deployment? */
export function filesEnabled(env: Record<string, string | undefined>): boolean {
  return fixture.flag.on.includes((env[fixture.flag.env] ?? ``).trim().toLowerCase())
}

/** The tool's `x-mcp-file` input, or null. */
export function fileInputSpec(tool: string, op: unknown): { arg: string; accept: string[]; maxSize: number } | null {
  const spec = (fixture.fileInputs as Record<string, { op: string; arg: string; accept: string[]; maxSize: number }>)[tool]
  return spec && spec.op === op ? { arg: spec.arg, accept: spec.accept, maxSize: spec.maxSize } : null
}

export interface FileInput {
  id: string
  name?: string
  mimeType: string
  size: number
  digest?: { algorithm: string; value: string }
  data?: string | null
}

export type FileError = keyof typeof fixture.errors

const BASE64 = /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/

/** sha-256 of the bytes, base64url without padding (the spec's digest value). */
export const fileDigest = (bytes: Buffer) => createHash(`sha256`).update(bytes).digest(`base64url`)

function shapeError(input: unknown): FileError | null {
  if (!input || typeof input !== `object` || Array.isArray(input)) return `invalid`
  const f = input as Record<string, unknown>
  if (typeof f.id !== `string` || f.id.length < 1 || f.id.length > fixture.idMax) return `invalid`
  if (typeof f.mimeType !== `string` || !f.mimeType) return `invalid`
  if (typeof f.size !== `number` || !Number.isInteger(f.size) || f.size < 0) return `invalid`
  if (f.name !== undefined && typeof f.name !== `string`) return `invalid`
  if (`data` in f && (f.data === null || typeof f.data !== `string`)) return `invalid`
  if (f.size === 0 && `data` in f) return `invalid`
  if (f.digest !== undefined) {
    const d = f.digest as Record<string, unknown> | null
    if (!d || d.algorithm !== fixture.digestAlgorithm || typeof d.value !== `string`) return `invalid`
  }
  return null
}

export type FilePlan =
  | { kind: `run`; inline: { id: string; bytes: Buffer }[] }
  | { kind: `error`; error: FileError; fileId: string | null }
  | { kind: `upload`; fileIds: string[] }

/** What a first call with these file inputs does: run, refuse (an isError result, the tool not run) or ask for uploads. */
export function planFiles(inputs: readonly unknown[], maxSize: number, maxInlineBytes = MAX_INLINE_BYTES): FilePlan {
  const ids = new Set<string>()
  const inline: { id: string; bytes: Buffer }[] = []
  const outOfBand: string[] = []
  let inlineTotal = 0
  for (const raw of inputs) {
    const shape = shapeError(raw)
    const id = (raw as { id?: unknown } | null)?.id
    if (shape) return { kind: `error`, error: shape, fileId: typeof id === `string` ? id : null }
    const f = raw as FileInput
    if (ids.has(f.id)) return { kind: `error`, error: `duplicateId`, fileId: f.id }
    ids.add(f.id)
    if (f.size > maxSize) return { kind: `error`, error: `oversize`, fileId: f.id }
    if (typeof f.data === `string`) {
      if (!BASE64.test(f.data)) return { kind: `error`, error: `undecodable`, fileId: f.id }
      const bytes = Buffer.from(f.data, `base64`)
      if (bytes.length !== f.size) return { kind: `error`, error: `sizeMismatch`, fileId: f.id }
      if (f.digest && fileDigest(bytes) !== f.digest.value) return { kind: `error`, error: `digestMismatch`, fileId: f.id }
      inlineTotal += bytes.length
      if (inlineTotal > maxInlineBytes) return { kind: `error`, error: `inlineTooLarge`, fileId: f.id }
      inline.push({ id: f.id, bytes })
    } else if (f.size > 0) outOfBand.push(f.id)
  }
  return outOfBand.length ? { kind: `upload`, fileIds: outOfBand } : { kind: `run`, inline }
}

/** The staging URL of one upload: `${APP_URL}/api/mcp/uploads/<stagingId>`. */
export const stagingUrl = (appUrl: string, stagingId: string) => `${appUrl.replace(/\/+$/, ``)}${fixture.staging.path}${stagingId}`

/** The `upload_required` result (the state's keys are `<fileId>:<stagingId>`). */
export function uploadRequired(
  files: readonly FileInput[],
  staged: Readonly<Record<string, string>>,
  appUrl: string,
  expiresAt: string,
  requestState: string
) {
  return {
    resultType: `upload_required` as const,
    uploads: files.map((f) => ({
      fileId: f.id,
      url: stagingUrl(appUrl, staged[f.id]!),
      method: fixture.staging.method,
      headers: { "Content-Type": f.mimeType },
      credentials: fixture.staging.credentials,
      expiresAt,
    })),
    requestState,
  }
}

/** The state keys of a staged set. */
export const stagingKeys = (staged: Readonly<Record<string, string>>) => Object.entries(staged).map(([fileId, stagingId]) => `${fileId}:${stagingId}`)

export interface StagedObject {
  size: number
  /** sha-256 base64url of what arrived. */
  digest: string
}

export type RetryVerdict =
  | { kind: `run`; staged: Record<string, string> }
  | { kind: `refused`; error: RequestStateError }
  | { kind: `error`; error: FileError; fileId: string }

/** A retry with `requestState`: the state's checks (-32602), then the bytes (isError, never a second upload_required). */
export function retryVerdict(
  requestState: unknown,
  ctx: VerifyContext,
  inputs: readonly FileInput[],
  lookup: (stagingId: string) => StagedObject | null
): RetryVerdict {
  const state = verifyRequestState(requestState, ctx)
  if (!state.ok) return { kind: `refused`, error: state.error }
  if (state.payload.kind !== `upload`) return { kind: `refused`, error: `mismatch` }
  const staged: Record<string, string> = {}
  for (const key of state.payload.keys) {
    const at = key.indexOf(`:`)
    staged[key.slice(0, at)] = key.slice(at + 1)
  }
  for (const f of inputs) {
    if (typeof f.data === `string` || f.size === 0) continue
    const stagingId = staged[f.id]
    if (!stagingId) return { kind: `refused`, error: `mismatch` }
    const object = lookup(stagingId)
    if (!object || object.size !== f.size) return { kind: `error`, error: `missingBytes`, fileId: f.id }
    if (f.digest && object.digest !== f.digest.value) return { kind: `error`, error: `digestMismatch`, fileId: f.id }
  }
  return { kind: `run`, staged }
}

/** The isError tool result of a refused file. */
export function fileErrorResult(error: FileError, fileId: string | null) {
  return { content: [{ type: `text`, text: fixture.errors[error].replace(`{id}`, fileId ?? `?`) }], isError: true }
}

/** -32021 when a file must go out of band and the client did not declare `upload`; else null. */
export function uploadCapabilityError(inputs: readonly FileInput[], declaredUpload: boolean) {
  if (declaredUpload || !inputs.some((f) => typeof f.data !== `string` && f.size > 0)) return null
  return { code: fixture.missingCapability.code, message: fixture.missingCapability.message, data: { requiredCapabilities: { extensions: { [FILES_EXTENSION]: { upload: {} } } } } }
}
