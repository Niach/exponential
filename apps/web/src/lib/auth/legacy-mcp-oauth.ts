import { and, eq, isNull, ne } from "drizzle-orm"
import { symmetricEncrypt } from "better-auth/crypto"
import { legacyOauthApplications, oauthClients } from "@/db/auth-schema"
import type { db as Database } from "@/db/connection"

// better-auth 1.7 moved MCP OAuth to `@better-auth/oauth-provider`. Migration
// 0267 copied the 1.6 plugin's clients, consents and live tokens (hashed)
// into the new tables. What SQL cannot do is the confidential clients'
// secrets: 1.6 kept them in plaintext, oauth-provider (opaque tokens, so
// `storeClientSecret: "encrypted"`) keeps them encrypted with the auth
// secret, which only the server holds. This fills them, at boot.
//
// Idempotent and safe to re-run on every boot: it only writes a client whose
// new secret is still empty, so a secret rotated on 1.7 is never replaced,
// and it never touches tokens (a re-import would resurrect revoked ones).

type Db = typeof Database

/** Encrypts with the auth context's secret, as oauth-provider does. */
export type SecretSealer = (secret: string) => Promise<string>

export function sealerFor(secretConfig: Parameters<typeof symmetricEncrypt>[0][`key`]): SecretSealer {
  return (secret) => symmetricEncrypt({ key: secretConfig, data: secret })
}

/** Fills the encrypted secret of every 1.6 confidential client still without
 * one. Returns how many it filled. */
export async function fillLegacyMcpClientSecrets(db: Db, seal: SecretSealer): Promise<number> {
  const pending = await db
    .select({ clientId: legacyOauthApplications.clientId, secret: legacyOauthApplications.clientSecret })
    .from(legacyOauthApplications)
    .innerJoin(oauthClients, eq(oauthClients.clientId, legacyOauthApplications.clientId))
    .where(
      and(
        ne(legacyOauthApplications.type, `public`),
        ne(legacyOauthApplications.clientSecret, ``),
        isNull(oauthClients.clientSecret)
      )
    )
  let filled = 0
  for (const row of pending) {
    if (!row.secret) continue
    const sealed = await seal(row.secret)
    const updated = await db
      .update(oauthClients)
      .set({ clientSecret: sealed, updatedAt: new Date() })
      .where(and(eq(oauthClients.clientId, row.clientId), isNull(oauthClients.clientSecret)))
      .returning({ clientId: oauthClients.clientId })
    filled += updated.length
  }
  return filled
}

/** The boot hook: the auth context's secret, then the fill. Never throws: a
 * failure leaves those clients unable to authenticate until the next boot,
 * which the log says. */
export async function importLegacyMcpOAuthAtBoot(): Promise<void> {
  try {
    const [{ auth }, { db }] = await Promise.all([import(`@/lib/auth`), import(`@/db/connection`)])
    const ctx = await auth.$context
    const filled = await fillLegacyMcpClientSecrets(db, sealerFor(ctx.secretConfig))
    if (filled > 0) console.info(`[auth] sealed ${filled} legacy MCP client secret(s) for oauth-provider`)
  } catch (err) {
    console.error(`[auth] legacy MCP client secret import failed:`, err)
  }
}
