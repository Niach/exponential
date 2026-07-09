import crypto from "node:crypto"

// Signed single-use `state` for the GitHub App install flow.
//
// The App's Setup URL redirect (/api/integrations/github/setup) is reachable
// by anyone, and `installation_id` is a guessable sequential integer — so a
// bare session must never be enough to attribute an installation to a user
// (any signed-in attacker could otherwise claim someone else's installation
// and inherit its private repos). Attribution requires PROOF that the very
// user whose session the callback carries launched this install from inside
// the app: this token — an HMAC (BETTER_AUTH_SECRET) over the initiating user
// id + single-use nonce + expiry — is minted into the install link and echoed
// back by GitHub in `state`. Without a valid, matching, unconsumed token the
// callback still records the installation, just unattributed (the
// webhook/admin-self-heal model).

// Install flows can sit on GitHub for a while (repo selection, org 2FA), and
// the clients re-fetch the install URL on window focus, so an hour is plenty.
const STATE_TTL_MS = 60 * 60 * 1000

interface SetupStatePayload {
  u: string // initiating user id
  w?: string // workspace the claim links the installation to
  o?: boolean // OAuth-claim purpose (consumed only by the OAuth callback)
  d?: boolean // launched from an in-app dialog → self-closing landing page
  m?: boolean // launched from a native mobile client → exponential:// deep-link page
  n: string // single-use nonce
  exp: number // unix ms expiry
}

function secret(): string | null {
  return process.env.BETTER_AUTH_SECRET || null
}

function sign(body: string, key: string): string {
  return crypto.createHmac(`sha256`, key).update(body).digest(`base64url`)
}

// Consumed nonces (single-use). In-process like the other short-lived caches
// in this app; entries expire with their token so the map stays bounded.
const consumedNonces = new Map<string, number>()

function pruneConsumedNonces(now: number) {
  for (const [nonce, expiresAt] of consumedNonces) {
    if (expiresAt <= now) consumedNonces.delete(nonce)
  }
}

export function mintGithubSetupState(
  userId: string,
  opts?: {
    dialog?: boolean
    mobile?: boolean
    workspaceId?: string
    oauth?: boolean
  },
  now: number = Date.now()
): string | undefined {
  const key = secret()
  if (!key) return undefined
  const payload: SetupStatePayload = {
    u: userId,
    ...(opts?.workspaceId ? { w: opts.workspaceId } : {}),
    ...(opts?.oauth ? { o: true } : {}),
    ...(opts?.dialog ? { d: true } : {}),
    ...(opts?.mobile ? { m: true } : {}),
    n: crypto.randomBytes(16).toString(`base64url`),
    exp: now + STATE_TTL_MS,
  }
  const body = Buffer.from(JSON.stringify(payload)).toString(`base64url`)
  return `${body}.${sign(body, key)}`
}

function decodePayload(state: string): SetupStatePayload | null {
  const dot = state.lastIndexOf(`.`)
  if (dot <= 0) return null
  try {
    const parsed = JSON.parse(
      Buffer.from(state.slice(0, dot), `base64url`).toString(`utf8`)
    ) as SetupStatePayload
    if (
      typeof parsed?.u !== `string` ||
      typeof parsed?.n !== `string` ||
      typeof parsed?.exp !== `number`
    ) {
      return null
    }
    return parsed
  } catch {
    return null
  }
}

// Landing-page choice only (never security-relevant): whether the install was
// launched from an in-app dialog. Reads the payload without requiring a valid
// signature so even an expired token still lands on the right page.
export function githubSetupStateWantsDialog(state: string | null): boolean {
  if (!state) return false
  return decodePayload(state)?.d === true
}

// Same landing-page-choice contract as the dialog flag: whether the install
// was launched from a native mobile client, which wants a 200 page firing the
// exponential:// deep link instead of a web redirect. Never security-relevant —
// read
// without requiring a valid signature so even an expired token still hands
// the user back to the app.
export function githubSetupStateWantsMobile(state: string | null): boolean {
  if (!state) return false
  return decodePayload(state)?.m === true
}

// Verify + consume: returns the initiating user id (+ target workspace) only
// when the signature, expiry, and single-use nonce all hold AND the token was
// minted for `sessionUserId` (the user the callback's browser session resolves
// to). The nonce is burned only on full success, so a callback that lands
// without a session doesn't invalidate the link for a signed-in retry.
// `expectOauth` pins the token's purpose: an install-flow state can't be
// replayed against the OAuth callback (or vice versa) — the two callbacks
// create workspace links under different proofs of control.
export function consumeGithubSetupState(
  state: string | null,
  sessionUserId: string | null,
  opts?: { expectOauth?: boolean },
  now: number = Date.now()
): { userId: string; workspaceId: string | null } | null {
  if (!state || !sessionUserId) return null
  const key = secret()
  if (!key) return null
  const dot = state.lastIndexOf(`.`)
  if (dot <= 0) return null
  const body = state.slice(0, dot)
  const sig = Buffer.from(state.slice(dot + 1))
  const expected = Buffer.from(sign(body, key))
  if (sig.length !== expected.length || !crypto.timingSafeEqual(sig, expected)) {
    return null
  }
  const payload = decodePayload(state)
  if (!payload) return null
  if (payload.exp <= now) return null
  if (payload.u !== sessionUserId) return null
  if ((payload.o === true) !== (opts?.expectOauth === true)) return null
  pruneConsumedNonces(now)
  if (consumedNonces.has(payload.n)) return null
  consumedNonces.set(payload.n, payload.exp)
  return { userId: payload.u, workspaceId: payload.w ?? null }
}

// ---------------------------------------------------------------------------
// Claim ticket — the OAuth callback's hand-off to the /integrations/github/
// claim page when the user's GitHub account has SEVERAL installations to pick
// from. Same HMAC scheme as the setup state, but deliberately NOT single-use:
// linking is idempotent and the ticket is bound to user + workspace + the
// exact installation-id set the callback enumerated, so replaying it can only
// re-create the same links. Short TTL keeps the window tight.
// ---------------------------------------------------------------------------

const CLAIM_TICKET_TTL_MS = 15 * 60 * 1000

export interface GithubClaimTicket {
  u: string // claiming user id
  w: string // workspace the links target
  ids: number[] // installation ids the OAuth enumeration proved control of
  m?: boolean // mobile flow → claim page shows the exponential:// return card
  d?: boolean // dialog flow → claim page self-closes into the opener
  exp: number
}

export function mintGithubClaimTicket(
  payload: Omit<GithubClaimTicket, `exp`>,
  now: number = Date.now()
): string | undefined {
  const key = secret()
  if (!key) return undefined
  const full: GithubClaimTicket = { ...payload, exp: now + CLAIM_TICKET_TTL_MS }
  const body = Buffer.from(JSON.stringify(full)).toString(`base64url`)
  return `${body}.${sign(body, key)}`
}

// Verify a claim ticket: signature + expiry + minted-for-this-user. Returns
// null on any mismatch — the claim procedures treat that as "restart the
// connect flow".
export function readGithubClaimTicket(
  ticket: string | null,
  sessionUserId: string | null,
  now: number = Date.now()
): GithubClaimTicket | null {
  if (!ticket || !sessionUserId) return null
  const key = secret()
  if (!key) return null
  const dot = ticket.lastIndexOf(`.`)
  if (dot <= 0) return null
  const body = ticket.slice(0, dot)
  const sig = Buffer.from(ticket.slice(dot + 1))
  const expected = Buffer.from(sign(body, key))
  if (sig.length !== expected.length || !crypto.timingSafeEqual(sig, expected)) {
    return null
  }
  let payload: GithubClaimTicket
  try {
    payload = JSON.parse(
      Buffer.from(body, `base64url`).toString(`utf8`)
    ) as GithubClaimTicket
  } catch {
    return null
  }
  if (
    typeof payload?.u !== `string` ||
    typeof payload?.w !== `string` ||
    !Array.isArray(payload?.ids) ||
    payload.ids.some((id) => typeof id !== `number`) ||
    typeof payload?.exp !== `number`
  ) {
    return null
  }
  if (payload.exp <= now) return null
  if (payload.u !== sessionUserId) return null
  return payload
}
