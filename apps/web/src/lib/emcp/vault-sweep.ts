// #EXP-1491: the vault sweep, the MIGRATION and the ROTATION in one pass.
// It re-seals every secret column eMCP owns under the current key:
// - legacy `v1.` values (keyed off BETTER_AUTH_SECRET, per-table AAD),
// - the plaintext better-auth `accounts` tokens from before EXP-1491,
// - values sealed with a retired kid (a rotated `VAULT_KEY`, or the derived
//   `auth` key once `VAULT_KEY` is set).
// Every write is a compare-and-swap on the value it read, so a concurrent
// reconnect or token refresh wins. A value that does not open is left alone
// (it reads as "not connected"; a reconnect replaces it) and counted. Runs
// at boot and daily (server-bun.ts); idempotent, multi-replica safe. Once a
// pass reports `remaining: 0` the retired keys can be dropped.
import { and, eq, isNotNull, or, sql, type SQL } from "drizzle-orm"
import type { PgColumn } from "drizzle-orm/pg-core"
import { db } from "@/db/connection"
import { accounts } from "@/db/auth-schema"
import { mcpCredentials, mcpOauthClients, mcpOauthFlows } from "@/db/schema"
import { reportSchedulerRun } from "@/lib/metrics/registry"
import {
  accountSlot,
  clientSecretSlot,
  credentialSlot,
  flowVerifierSlot,
  isSealed,
  legacyClientSecretAad,
  legacyCredentialAad,
  legacyFlowVerifierAad,
  needsReseal,
  openSecret,
  sealSecret,
  vaultKeyring,
  type AccountTokenColumn,
  type VaultSlot,
} from "@/lib/emcp/vault"

/** The pool or a transaction (the real-DB test runs inside a rollback). */
type Db = typeof db | Parameters<Parameters<typeof db.transaction>[0]>[0]

const BATCH = 500

export interface VaultSweepResult {
  resealed: number
  /** Values that need a re-seal but do not open (left as they are). */
  unopenable: number
}

/** Rows whose value is not sealed under the current kid. */
function staleCondition(column: PgColumn): SQL {
  const prefix = `ev1.${vaultKeyring().current.kid}.%`
  return and(isNotNull(column), sql`${column} <> ''`, sql`${column} NOT LIKE ${prefix}`)!
}

/** One re-seal: the plaintext under the current key, or null (leave it). */
function reseal(value: string, slot: VaultSlot, legacyAad: string | null): string | null {
  if (!needsReseal(value)) return null
  const plain = legacyAad === null && !isSealed(value) ? value : openSecret(value, slot, legacyAad ?? undefined)
  return plain === null ? null : sealSecret(plain, slot)
}

async function sweepTable<R extends { id: string }>(
  read: (after: string) => Promise<R[]>,
  each: (row: R) => Promise<{ resealed: number; unopenable: number }>
): Promise<VaultSweepResult> {
  const total: VaultSweepResult = { resealed: 0, unopenable: 0 }
  let after = ``
  for (;;) {
    const rows = await read(after)
    for (const row of rows) {
      const r = await each(row)
      total.resealed += r.resealed
      total.unopenable += r.unopenable
    }
    if (rows.length < BATCH) return total
    after = rows[rows.length - 1]!.id
  }
}

function tally(next: string | null): { resealed: number; unopenable: number } {
  return next === null ? { resealed: 0, unopenable: 1 } : { resealed: 1, unopenable: 0 }
}

export async function sweepMcpCredentials(database: Db = db): Promise<VaultSweepResult> {
  return sweepTable(
    (after) =>
      database
        .select({ id: mcpCredentials.id, serverId: mcpCredentials.serverId, userId: mcpCredentials.userId, ciphertext: mcpCredentials.ciphertext })
        .from(mcpCredentials)
        .where(and(staleCondition(mcpCredentials.ciphertext), sql`${mcpCredentials.id}::text > ${after}`))
        .orderBy(sql`${mcpCredentials.id}::text`)
        .limit(BATCH),
    async (row) => {
      const next = reseal(row.ciphertext, credentialSlot(row.id), legacyCredentialAad(row.serverId, row.userId))
      if (next !== null) {
        await database
          .update(mcpCredentials)
          .set({ ciphertext: next })
          .where(and(eq(mcpCredentials.id, row.id), eq(mcpCredentials.ciphertext, row.ciphertext)))
      }
      return tally(next)
    }
  )
}

export async function sweepMcpOauthFlows(database: Db = db): Promise<VaultSweepResult> {
  return sweepTable(
    (after) =>
      database
        .select({ id: mcpOauthFlows.id, state: mcpOauthFlows.state, value: mcpOauthFlows.codeVerifierCiphertext })
        .from(mcpOauthFlows)
        .where(
          and(
            eq(mcpOauthFlows.status, `pending`),
            staleCondition(mcpOauthFlows.codeVerifierCiphertext),
            sql`${mcpOauthFlows.id}::text > ${after}`
          )
        )
        .orderBy(sql`${mcpOauthFlows.id}::text`)
        .limit(BATCH),
    async (row) => {
      const next = reseal(row.value, flowVerifierSlot(row.id), legacyFlowVerifierAad(row.state))
      if (next !== null) {
        await database
          .update(mcpOauthFlows)
          .set({ codeVerifierCiphertext: next })
          .where(and(eq(mcpOauthFlows.id, row.id), eq(mcpOauthFlows.codeVerifierCiphertext, row.value)))
      }
      return tally(next)
    }
  )
}

export async function sweepMcpOauthClients(database: Db = db): Promise<VaultSweepResult> {
  return sweepTable(
    (after) =>
      database
        .select({
          id: mcpOauthClients.id,
          issuer: mcpOauthClients.issuer,
          registrationEndpoint: mcpOauthClients.registrationEndpoint,
          value: mcpOauthClients.clientSecretCiphertext,
        })
        .from(mcpOauthClients)
        .where(and(staleCondition(mcpOauthClients.clientSecretCiphertext), sql`${mcpOauthClients.id}::text > ${after}`))
        .orderBy(sql`${mcpOauthClients.id}::text`)
        .limit(BATCH),
    async (row) => {
      const value = row.value!
      const next = reseal(value, clientSecretSlot(row.id), legacyClientSecretAad(row.issuer, row.registrationEndpoint))
      if (next !== null) {
        await database
          .update(mcpOauthClients)
          .set({ clientSecretCiphertext: next })
          .where(and(eq(mcpOauthClients.id, row.id), eq(mcpOauthClients.clientSecretCiphertext, value)))
      }
      return tally(next)
    }
  )
}

const ACCOUNT_COLUMNS: ReadonlyArray<[AccountTokenColumn, PgColumn, `accessToken` | `refreshToken` | `idToken`]> = [
  [`access_token`, accounts.accessToken, `accessToken`],
  [`refresh_token`, accounts.refreshToken, `refreshToken`],
  [`id_token`, accounts.idToken, `idToken`],
]

export async function sweepAccountTokens(database: Db = db): Promise<VaultSweepResult> {
  return sweepTable(
    (after) =>
      database
        .select({
          id: accounts.id,
          accessToken: accounts.accessToken,
          refreshToken: accounts.refreshToken,
          idToken: accounts.idToken,
        })
        .from(accounts)
        .where(
          and(
            or(...ACCOUNT_COLUMNS.map(([, column]) => staleCondition(column))),
            sql`${accounts.id} > ${after}`
          )
        )
        .orderBy(accounts.id)
        .limit(BATCH),
    async (row) => {
      const result = { resealed: 0, unopenable: 0 }
      for (const [column, pg, field] of ACCOUNT_COLUMNS) {
        const value = row[field]
        if (!value || !needsReseal(value)) continue
        // A plaintext token (never sealed) is its own plaintext.
        const next = reseal(value, accountSlot(row.id, column), null)
        if (next !== null) {
          await database
            .update(accounts)
            .set({ [field]: next })
            .where(and(eq(accounts.id, row.id), eq(pg, value)))
        }
        const t = tally(next)
        result.resealed += t.resealed
        result.unopenable += t.unopenable
      }
      return result
    }
  )
}

/** One full pass over every vault column. */
export async function runVaultSweep(database: Db = db): Promise<VaultSweepResult & { byTable: Record<string, VaultSweepResult> }> {
  const byTable: Record<string, VaultSweepResult> = {
    accounts: await sweepAccountTokens(database),
    mcp_credentials: await sweepMcpCredentials(database),
    mcp_oauth_clients: await sweepMcpOauthClients(database),
    mcp_oauth_flows: await sweepMcpOauthFlows(database),
  }
  const values = Object.values(byTable)
  return {
    resealed: values.reduce((n, r) => n + r.resealed, 0),
    unopenable: values.reduce((n, r) => n + r.unopenable, 0),
    byTable,
  }
}

let running = false

export async function sweepVault(): Promise<void> {
  if (running) return
  running = true
  const startMs = performance.now()
  try {
    const result = await runVaultSweep()
    reportSchedulerRun(`emcp-vault-sweep`, {
      ok: true,
      durationMs: performance.now() - startMs,
      detail: `${result.resealed} resealed, ${result.unopenable} unopenable`,
    })
    if (result.resealed > 0 || result.unopenable > 0) {
      console.log(
        `[emcp-vault] resealed ${result.resealed} value(s) under kid ${vaultKeyring().current.kid}; ${result.unopenable} do not open`
      )
    }
  } catch (err) {
    reportSchedulerRun(`emcp-vault-sweep`, {
      ok: false,
      durationMs: performance.now() - startMs,
      error: String(err),
    })
    console.error(`[emcp-vault] sweep failed:`, err)
  } finally {
    running = false
  }
}
