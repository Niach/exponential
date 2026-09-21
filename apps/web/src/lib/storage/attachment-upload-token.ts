import crypto from "node:crypto"

// EXP-929: short-lived signed UPLOAD URLs for issue attachments — the
// sessions_results flow (session-result-token.ts) applied to
// `exponential_attachments_upload`.
//
// Without `dataBase64` the MCP tool hands the agent
// `/api/attachment-uploads/<this>` plus a ready `curl -T <file>` line, so a
// 33 KB PNG never has to be retyped as base64 inside a tool call (the
// corruption EXP-916 hit). The token IS the authorization: it is minted only
// AFTER the tool resolved the issue and checked grant + membership, and it
// binds everything the upload route needs to write the row — the
// pre-allocated attachment id, the issue/board/team, the uploader, the
// sanitized filename, the canonical content type and the optional comment.
// Nothing is stored at mint time: an abandoned mint leaves no phantom row, the
// token just expires.
//
// Same HMAC construction as the app's other BETTER_AUTH_SECRET users, with its
// own domain-separation context (helpdesk/token.ts documents the family).
const CONTEXT = `exp-attachment-upload:v1:`

export const ATTACHMENT_UPLOAD_TOKEN_TTL_MS = 10 * 60 * 1000

export interface AttachmentUploadTokenPayload {
  a: string // the attachments row id the upload will create
  i: string // issue id
  b: string // board id
  w: string // team id (storage budget + row scope)
  u: string // uploader — the user the MCP layer authorized at mint time
  f: string // sanitized filename
  c: string // canonical content type
  m?: string // optional comment id the row attaches to
  exp: number // unix ms expiry
}

export interface AttachmentUploadTokenInput {
  attachmentId: string
  issueId: string
  boardId: string
  teamId: string
  userId: string
  filename: string
  contentType: string
  commentId?: string
}

function secret(): string | null {
  return process.env.BETTER_AUTH_SECRET || null
}

function sign(body: string, key: string): string {
  return crypto
    .createHmac(`sha256`, key)
    .update(CONTEXT + body)
    .digest(`base64url`)
}

export function mintAttachmentUploadToken(
  input: AttachmentUploadTokenInput,
  now: number = Date.now()
): { token: string; expiresAt: Date } {
  const key = secret()
  if (!key) {
    throw new Error(
      `BETTER_AUTH_SECRET is not set — cannot mint attachment upload URLs`
    )
  }
  const payload: AttachmentUploadTokenPayload = {
    a: input.attachmentId,
    i: input.issueId,
    b: input.boardId,
    w: input.teamId,
    u: input.userId,
    f: input.filename,
    c: input.contentType,
    ...(input.commentId ? { m: input.commentId } : {}),
    exp: now + ATTACHMENT_UPLOAD_TOKEN_TTL_MS,
  }
  const body = Buffer.from(JSON.stringify(payload)).toString(`base64url`)
  return {
    token: `${body}.${sign(body, key)}`,
    expiresAt: new Date(payload.exp),
  }
}

/** The upload the token authorizes, or null for anything malformed,
 *  mis-signed or expired. */
export function verifyAttachmentUploadToken(
  token: string,
  now: number = Date.now()
): AttachmentUploadTokenPayload | null {
  const key = secret()
  if (!key) return null
  const dot = token.lastIndexOf(`.`)
  if (dot <= 0) return null
  const body = token.slice(0, dot)
  const sig = Buffer.from(token.slice(dot + 1))
  const expected = Buffer.from(sign(body, key))
  if (sig.length !== expected.length || !crypto.timingSafeEqual(sig, expected)) {
    return null
  }
  try {
    const payload = JSON.parse(
      Buffer.from(body, `base64url`).toString(`utf8`)
    ) as AttachmentUploadTokenPayload
    if (
      typeof payload?.a !== `string` ||
      typeof payload?.i !== `string` ||
      typeof payload?.b !== `string` ||
      typeof payload?.w !== `string` ||
      typeof payload?.u !== `string` ||
      typeof payload?.f !== `string` ||
      typeof payload?.c !== `string` ||
      (payload.m !== undefined && typeof payload.m !== `string`) ||
      typeof payload?.exp !== `number`
    ) {
      return null
    }
    if (payload.exp <= now) return null
    return payload
  } catch {
    return null
  }
}
