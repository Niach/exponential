import crypto from "node:crypto"

// #EXP-1616: the short-lived, repo-scoped credential of eCode's own git.
// `repositories.credentials` hands one to a member's device (git-remote-exp
// sends it as the bearer of `/api/ecode/git/*`); a device job carries one
// minted with scope `job` (the merge, backup, repack and restore jobs: the
// only callers that may record a bundle, swap packs or replace every ref).
// The token IS the authorization at the routes, re-checked against the
// member's CURRENT membership on every call (`git-routes.ts`), so a token
// outliving a removal is refused. Same HMAC family as the app's other
// BETTER_AUTH_SECRET tokens, with its own domain-separation context.
const CONTEXT = `exp-ecode-git:v1:`

export const REPO_TOKEN_TTL_MS = 60 * 60 * 1000
export const JOB_TOKEN_TTL_MS = 20 * 60 * 1000

export type RepoTokenScope = `member` | `job`

export interface RepoTokenPayload {
  /** The repository. */
  r: string
  /** The member it was minted for (a job: the device's owner). */
  u: string
  s: RepoTokenScope
  /** Unix ms expiry. */
  exp: number
}

function secret(): string {
  const key = process.env.BETTER_AUTH_SECRET
  if (!key) throw new Error(`BETTER_AUTH_SECRET is not set`)
  return key
}

function sign(body: string, key: string): string {
  return crypto.createHmac(`sha256`, key).update(CONTEXT + body).digest(`base64url`)
}

/** `expg_<payload>.<mac>`: the prefix lets log redaction and the helper spot it. */
export function mintRepoToken(input: {
  repositoryId: string
  userId: string
  scope?: RepoTokenScope
  now?: number
}): { token: string; expiresAt: Date } {
  const scope = input.scope ?? `member`
  const exp = (input.now ?? Date.now()) + (scope === `job` ? JOB_TOKEN_TTL_MS : REPO_TOKEN_TTL_MS)
  const payload: RepoTokenPayload = { r: input.repositoryId, u: input.userId, s: scope, exp }
  const body = Buffer.from(JSON.stringify(payload)).toString(`base64url`)
  return { token: `expg_${body}.${sign(body, secret())}`, expiresAt: new Date(exp) }
}

/** The payload of a valid, unexpired token; null otherwise (never throws). */
export function verifyRepoToken(token: string, now = Date.now()): RepoTokenPayload | null {
  const key = process.env.BETTER_AUTH_SECRET
  if (!key || !token.startsWith(`expg_`)) return null
  const [body, mac] = token.slice(5).split(`.`)
  if (!body || !mac) return null
  const expected = Buffer.from(sign(body, key))
  const given = Buffer.from(mac)
  if (expected.length !== given.length || !crypto.timingSafeEqual(expected, given)) return null
  try {
    const payload = JSON.parse(Buffer.from(body, `base64url`).toString(`utf8`)) as RepoTokenPayload
    if (typeof payload.r !== `string` || typeof payload.u !== `string` || typeof payload.exp !== `number`) return null
    if (payload.s !== `member` && payload.s !== `job`) return null
    return payload.exp > now ? payload : null
  } catch {
    return null
  }
}
