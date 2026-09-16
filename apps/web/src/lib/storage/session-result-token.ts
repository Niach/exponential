import crypto from "node:crypto"

// EXP-879: short-lived signed UPLOAD URLs for a run's published results — the
// inverse of EXP-704's download token (attachment-token.ts).
//
// MCP `exponential_sessions_results` hands the agent
// `/api/session-results/<this>` plus a ready `curl -F file=@shot.png` line, so
// a screenshot lands with one shell command and no base64 in context. The
// token IS the authorization: it is minted only AFTER the tool checked that
// the header's run belongs to the caller and is still live, and it is bound to
// exactly one (session, topic, label, user) — a token for one picture can
// never write another, and it expires fast enough that the run ending
// mid-window is the only slack.
//
// The route deliberately does NOT re-check the run's status: a screenshot
// taken inside the window must not evaporate because the run ended while curl
// was in flight. The STATUS gate lives at mint time.
//
// Same HMAC construction as the app's other BETTER_AUTH_SECRET users, with its
// own domain-separation context (helpdesk/token.ts documents the family).
const CONTEXT = `exp-session-result:v1:`

export const SESSION_RESULT_TOKEN_TTL_MS = 10 * 60 * 1000

export interface SessionResultTokenPayload {
  s: string // coding_sessions row the picture is published on
  t: string // topic
  l: string // label
  u: string // user the MCP layer authorized at mint time (the uploader)
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

export function mintSessionResultToken(
  input: { sessionId: string; topic: string; label: string; userId: string },
  now: number = Date.now()
): { token: string; expiresAt: Date } {
  const key = secret()
  if (!key) {
    throw new Error(
      `BETTER_AUTH_SECRET is not set — cannot mint session result upload URLs`
    )
  }
  const payload: SessionResultTokenPayload = {
    s: input.sessionId,
    t: input.topic,
    l: input.label,
    u: input.userId,
    exp: now + SESSION_RESULT_TOKEN_TTL_MS,
  }
  const body = Buffer.from(JSON.stringify(payload)).toString(`base64url`)
  return {
    token: `${body}.${sign(body, key)}`,
    expiresAt: new Date(payload.exp),
  }
}

/** The (session, topic, label, user) the token authorizes, or null for
 *  anything malformed, mis-signed or expired. */
export function verifySessionResultToken(
  token: string,
  now: number = Date.now()
): SessionResultTokenPayload | null {
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
    ) as SessionResultTokenPayload
    if (
      typeof payload?.s !== `string` ||
      typeof payload?.t !== `string` ||
      typeof payload?.l !== `string` ||
      typeof payload?.u !== `string` ||
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
