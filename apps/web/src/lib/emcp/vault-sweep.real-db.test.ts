// @vitest-environment node
import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { createCipheriv, hkdfSync, randomBytes, randomUUID } from "node:crypto"
import { eq, sql } from "drizzle-orm"
import fixture from "@exp/emcp/fixtures/vault.json" with { type: "json" }
import { accounts } from "@/db/auth-schema"
import { mcpCalls, mcpCredentials, mcpOauthClients, mcpServers, teams } from "@/db/schema"
import { inRollback, probeRealDb, type RealDbTx } from "@/test/real-db"

// #EXP-1491: the vault MIGRATION against the real migrated Postgres. Rows
// from before the vault (a `v1.` credential and DCR secret keyed off
// BETTER_AUTH_SECRET, plaintext `accounts` tokens) are re-sealed by the
// sweep under the derived key, then rotated to a VAULT_KEY, and every value
// still opens; the audit sweep purges past retention only. Inside a
// rollback: the sweep must never re-seal a shared dev database's rows.

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'mcp_calls') AS ok
`)
if (!ready.ok) console.info(`[vault-sweep.real-db] skipped: ${ready.reason}`)

const { runVaultSweep } = await import(`./vault-sweep`)
const { runMcpAuditSweep } = await import(`./audit`)
const { decryptCredential } = await import(`./upstream/crypto`)
const { clientSecretSlot, legacyClientSecretAad, legacyCredentialAad, openAccountToken, openSecret } = await import(
  `./vault`
)

const SECRET = `test-secret-aaaaaaaaaaaaaaaaaaaaaaaaaaaa`
const saved: Record<string, string | undefined> = {}

beforeEach(() => {
  for (const name of [`VAULT_KEY`, `VAULT_KEYS_RETIRED`, `BETTER_AUTH_SECRET`]) saved[name] = process.env[name]
  delete process.env.VAULT_KEY
  delete process.env.VAULT_KEYS_RETIRED
  process.env.BETTER_AUTH_SECRET = SECRET
})
afterEach(() => {
  for (const [name, value] of Object.entries(saved)) {
    if (value === undefined) delete process.env[name]
    else process.env[name] = value
  }
})
function legacySeal(plaintext: string, aad: string): string {
  const key = Buffer.from(hkdfSync(`sha256`, SECRET, `exp-mcp-credentials-v1`, `mcp-credentials`, 32))
  const iv = randomBytes(12)
  const cipher = createCipheriv(`aes-256-gcm`, key, iv)
  cipher.setAAD(Buffer.from(aad, `utf8`))
  const body = Buffer.concat([cipher.update(plaintext, `utf8`), cipher.final(), cipher.getAuthTag()])
  return `v1.${iv.toString(`base64url`)}.${body.toString(`base64url`)}`
}

describe.skipIf(!ready.ok)(`the vault sweep (real DB)`, () => {
  it(`migrates legacy and plaintext values, then rotates them to VAULT_KEY`, () =>
    inRollback(async (db: RealDbTx) => {
    const userId = `vault-${randomUUID().slice(0, 8)}`
    await db.execute(sql`
      INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
      VALUES (${userId}, 'Vault', ${`${userId}@vault.test`}, true, false, now(), now())
    `)
    const [team] = await db.insert(teams).values({ name: `Vault`, slug: `vault-${randomUUID().slice(0, 8)}` }).returning({ id: teams.id })
    const [server] = await db
      .insert(mcpServers)
      .values({ teamId: team!.id, name: `m5`, url: `https://m5.test/mcp`, auth: `oauth` })
      .returning({ id: mcpServers.id })
    const credentialId = randomUUID()
    await db.insert(mcpCredentials).values({
      id: credentialId,
      serverId: server!.id,
      userId,
      teamId: team!.id,
      ciphertext: legacySeal(JSON.stringify({ accessToken: `at`, refreshToken: `rt` }), legacyCredentialAad(server!.id, userId)),
    })
    const clientId = randomUUID()
    const issuer = `https://as-${clientId}.test`
    await db.insert(mcpOauthClients).values({
      id: clientId,
      issuer,
      redirectUri: `https://app.test/cb`,
      registrationEndpoint: `${issuer}/register`,
      clientId: `dcr-1`,
      clientSecretCiphertext: legacySeal(`client-secret`, legacyClientSecretAad(issuer, `${issuer}/register`)),
    })
    const accountId = `acc${randomUUID().replaceAll(`-`, ``).slice(0, 29)}`
    await db.insert(accounts).values({
      id: accountId,
      accountId: `gh-1`,
      providerId: `github`,
      userId,
      accessToken: `gho_plain`,
      refreshToken: `ghr_plain`,
      idToken: null,
      createdAt: new Date(),
      updatedAt: new Date(),
    })

    const read = async () => {
      const [credential] = await db.select().from(mcpCredentials).where(eq(mcpCredentials.id, credentialId))
      const [client] = await db.select().from(mcpOauthClients).where(eq(mcpOauthClients.id, clientId))
      const [account] = await db.select().from(accounts).where(eq(accounts.id, accountId))
      return { credential: credential!, client: client!, account: account! }
    }
    const opensAll = async (kid: string) => {
      const { credential, client, account } = await read()
      expect(credential.ciphertext.startsWith(`ev1.${kid}.`)).toBe(true)
      expect(client.clientSecretCiphertext!.startsWith(`ev1.${kid}.`)).toBe(true)
      expect(account.accessToken!.startsWith(`ev1.${kid}.`)).toBe(true)
      expect(account.idToken).toBeNull()
      expect(decryptCredential(credential.ciphertext, credential)).toEqual({ accessToken: `at`, refreshToken: `rt` })
      expect(openSecret(client.clientSecretCiphertext, clientSecretSlot(clientId))).toBe(`client-secret`)
      expect(openAccountToken(account.accessToken, accountId, `access_token`)).toBe(`gho_plain`)
      expect(openAccountToken(account.refreshToken, accountId, `refresh_token`)).toBe(`ghr_plain`)
    }

    // No VAULT_KEY: the derived key takes everything.
    const first = await runVaultSweep(db)
    expect(first.byTable.mcp_credentials!.resealed).toBeGreaterThanOrEqual(1)
    await opensAll(`auth`)

    // A second pass has nothing of ours left to do.
    await runVaultSweep(db)
    const before = await read()
    await runVaultSweep(db)
    expect((await read()).credential.ciphertext).toBe(before.credential.ciphertext)

    // Rotation: a VAULT_KEY, and nobody reconnects.
    process.env.VAULT_KEY = fixture.keys.k1
    await runVaultSweep(db)
    await opensAll(`k1`)
    }))

  it(`purges audit rows past 90 days only`, () =>
    inRollback(async (db: RealDbTx) => {
    const userId = `audit-${randomUUID().slice(0, 8)}`
    const now = new Date(`2026-10-31T12:00:00Z`)
    const row = (createdAt: string) => ({
      userId,
      tool: `exponential_issues`,
      status: `ok`,
      latencyMs: 1,
      argsHash: `sha256:x`,
      createdAt: new Date(createdAt),
    })
    await db.insert(mcpCalls).values([row(`2026-08-02T12:00:00Z`), row(`2026-08-02T12:00:01Z`)])
    await runMcpAuditSweep(now, db)
    const left = await db.select({ createdAt: mcpCalls.createdAt }).from(mcpCalls).where(eq(mcpCalls.userId, userId))
    expect(left.map((r) => r.createdAt.toISOString())).toEqual([`2026-08-02T12:00:01.000Z`])
    }))
})
