import crypto from "node:crypto"

// EXP-704: short-lived signed download URLs for attachments.
//
// MCP's exponential_attachments_get hands an agent metadata plus
// `/api/attachments/{id}?token=<this>` so any content type (xlsx, PDF, CSV)
// is retrievable with curl/fetch into the agent's cwd — no base64 blob in
// context, no size cap, and it works for external MCP clients that never
// hold a session cookie or expu_ key. The token IS the authorization: it is
// minted only AFTER the MCP layer ran its OAuth board-grant confinement and
// the membership check, it is bound to exactly one attachment id, and it
// expires fast enough that a member losing access mid-window is acceptable.
// It stays a tool-RESPONSE value only — never persisted into markdown (the
// canonicalizer strips unknown query params anyway).
//
// Same HMAC construction as the app's other BETTER_AUTH_SECRET users, with
// its own domain-separation context (helpdesk/token.ts documents the family).
const CONTEXT = `exp-attachment:v1:`

export const ATTACHMENT_TOKEN_TTL_MS = 10 * 60 * 1000

interface AttachmentTokenPayload {
  a: string // attachment id the token is scoped to
  u: string // user the MCP layer authorized at mint time (audit only)
  exp: number // unix ms expiry
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

export function mintAttachmentToken(
  attachmentId: string,
  userId: string,
  now: number = Date.now()
): { token: string; expiresAt: Date } {
  const key = secret()
  if (!key) {
    throw new Error(
      `BETTER_AUTH_SECRET is not set — cannot mint attachment download URLs`
    )
  }
  const payload: AttachmentTokenPayload = {
    a: attachmentId,
    u: userId,
    exp: now + ATTACHMENT_TOKEN_TTL_MS,
  }
  const body = Buffer.from(JSON.stringify(payload)).toString(`base64url`)
  return {
    token: `${body}.${sign(body, key)}`,
    expiresAt: new Date(payload.exp),
  }
}

// Returns the attachment id the token authorizes, or null for anything
// malformed, mis-signed, or expired. The caller must still compare the id
// against the requested attachment — a valid token for A grants nothing on B.
export function verifyAttachmentToken(
  token: string,
  now: number = Date.now()
): { attachmentId: string } | null {
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
    ) as AttachmentTokenPayload
    if (
      typeof payload?.a !== `string` ||
      typeof payload?.u !== `string` ||
      typeof payload?.exp !== `number`
    ) {
      return null
    }
    if (payload.exp <= now) return null
    return { attachmentId: payload.a }
  } catch {
    return null
  }
}
