// EXP-1111: one-time CLI install tokens. The Add device dialog mints one
// (`devices.createInstallToken`) and bakes it into the install one-liner as
// `EXP_INSTALL_TOKEN`, so a fresh `exponential` daemon signs in without the
// device-code round trip: its installer POSTs the token ONCE to the anonymous
// `/api/cli/install-token/redeem` and gets back a regular Better Auth session
// token — the same credential `/api/auth/device/token` hands out on approval.
//
// The token is 32 random bytes (base64url) behind an `expi_` prefix; only its
// sha256 hex is stored (`cli_install_tokens.token_hash`). Single use = ONE
// conditional UPDATE claiming `used_at`, so two racing redeems can never both
// win. 15-minute TTL; the dialog remints on expiry, and a mint revokes the
// user's earlier tokens (one live token per user).
import { createHash, randomBytes } from "node:crypto"
import { and, eq, gt, isNull } from "drizzle-orm"
import type { db as Db } from "@/db/connection"
import { cliInstallTokens } from "@/db/schema"
import { TokenBucketLimiter } from "@/lib/widget/rate-limit"

export const INSTALL_TOKEN_PREFIX = `expi_`
export const INSTALL_TOKEN_TTL_MS = 15 * 60 * 1000

type Database = typeof Db

/** `expi_` + base64url of at least 32 random bytes (43 chars). */
const TOKEN_SHAPE = /^expi_[A-Za-z0-9_-]{43,128}$/

export function isInstallTokenShape(token: unknown): token is string {
  return typeof token === `string` && TOKEN_SHAPE.test(token)
}

export function hashInstallToken(token: string): string {
  return createHash(`sha256`).update(token).digest(`hex`)
}

export function generateInstallToken(): { token: string; tokenHash: string } {
  const token = `${INSTALL_TOKEN_PREFIX}${randomBytes(32).toString(`base64url`)}`
  return { token, tokenHash: hashInstallToken(token) }
}

// "Rate-limit a little": a person opens the dialog, maybe remints a few
// times as tokens expire. Per replica, like every in-process bucket here.
export const installTokenMintLimiter = new TokenBucketLimiter({
  capacity: 10,
  refillPerHour: 30,
})
// The redeem is anonymous: 256-bit tokens make guessing hopeless, but an
// open endpoint still gets a per-IP ceiling.
export const installTokenRedeemLimiter = new TokenBucketLimiter({
  capacity: 20,
  refillPerHour: 60,
})

export async function createInstallToken(
  database: Database,
  userId: string,
  now = new Date()
): Promise<{ token: string; expiresAt: Date }> {
  const { token, tokenHash } = generateInstallToken()
  const expiresAt = new Date(now.getTime() + INSTALL_TOKEN_TTL_MS)
  // ONE live token per user: a remint revokes every earlier one still
  // unused, and the spent and expired rows go with them.
  await database
    .delete(cliInstallTokens)
    .where(eq(cliInstallTokens.userId, userId))
  await database.insert(cliInstallTokens).values({ userId, tokenHash, expiresAt })
  return { token, expiresAt }
}

/** Claims the token (atomic `used_at` stamp) and returns its user, or null
 *  for a malformed, unknown, expired or already-used token. */
export async function claimInstallToken(
  database: Database,
  token: unknown,
  now = new Date()
): Promise<string | null> {
  if (!isInstallTokenShape(token)) return null
  const [row] = await database
    .update(cliInstallTokens)
    .set({ usedAt: now })
    .where(
      and(
        eq(cliInstallTokens.tokenHash, hashInstallToken(token)),
        isNull(cliInstallTokens.usedAt),
        gt(cliInstallTokens.expiresAt, now)
      )
    )
    .returning({ userId: cliInstallTokens.userId })
  return row?.userId ?? null
}
