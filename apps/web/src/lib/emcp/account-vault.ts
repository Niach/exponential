// #EXP-1491: the better-auth `accounts` tokens (`access_token`,
// `refresh_token`, `id_token`) sealed at rest by the vault, bound to the
// row id. Done at the adapter, so every better-auth path (sign-in, linking,
// `getAccessToken` and its refresh, the after hooks) reads and writes
// plaintext as before and nothing in better-auth has to know. Our own direct
// reads of those columns go through `openAccountToken` (vault.ts). A
// plaintext value from before EXP-1491 reads as itself until the vault
// sweep seals it.
import { randomBytes } from "node:crypto"
import {
  openAccountToken,
  sealAccountToken,
  type AccountTokenColumn,
} from "@/lib/emcp/vault"

const MODEL = `account`
const FIELDS: ReadonlyArray<[field: string, column: AccountTokenColumn]> = [
  [`accessToken`, `access_token`],
  [`refreshToken`, `refresh_token`],
  [`idToken`, `id_token`],
]

type Row = Record<string, unknown>
type Where = { field: string; value?: unknown; operator?: string; connector?: string }
type Adapter = Record<string, unknown>

const ALPHABET = `ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789`

/** better-auth's id shape (32 alphanumerics), minted here because the seal
 * needs the id before the row exists. */
export function newAccountId(): string {
  const bytes = randomBytes(32)
  let out = ``
  for (const byte of bytes) out += ALPHABET[byte % ALPHABET.length]
  return out
}

function hasTokens(data: Row): boolean {
  return FIELDS.some(([field]) => typeof data[field] === `string` && data[field] !== ``)
}

export function sealAccountRow<T extends Row>(data: T, id: string): T {
  const out: Row = { ...data }
  for (const [field, column] of FIELDS) {
    if (typeof out[field] === `string`) out[field] = sealAccountToken(out[field] as string, id, column)
  }
  return out as T
}

export function openAccountRow<T>(row: T): T {
  if (!row || typeof row !== `object`) return row
  const r = row as Row
  const id = typeof r.id === `string` ? r.id : null
  if (!id) return row
  const out: Row = { ...r }
  for (const [field, column] of FIELDS) {
    if (typeof out[field] === `string`) out[field] = openAccountToken(out[field] as string, id, column)
  }
  return out as T
}

/** A row of any model whose join may carry `account` rows. */
function openOutput(model: string, value: unknown): unknown {
  if (Array.isArray(value)) return value.map((v) => openOutput(model, v))
  if (!value || typeof value !== `object`) return value
  if (model === MODEL) return openAccountRow(value)
  const row = value as Row
  if (!(`account` in row)) return value
  const joined = row.account
  return {
    ...row,
    account: Array.isArray(joined) ? joined.map((a) => openAccountRow(a)) : openAccountRow(joined),
  }
}

function idOf(where: Where[] | undefined): string | null {
  const hit = where?.find(
    (w) => w.field === `id` && (w.operator === undefined || w.operator === `eq`) && typeof w.value === `string`
  )
  return (hit?.value as string | undefined) ?? null
}

type AnyFn = (...args: never[]) => Promise<unknown>

function wrapAdapter<A extends Adapter>(adapter: A): A {
  const call = (name: string) => adapter[name] as unknown as (arg: Row) => Promise<unknown>
  const overrides: Record<string, unknown> = {
    create: async (arg: { model: string; data: Row; forceAllowId?: boolean }) => {
      if (arg.model !== MODEL || !hasTokens(arg.data)) {
        return openOutput(arg.model, await call(`create`).call(adapter, arg))
      }
      const id = typeof arg.data.id === `string` && arg.forceAllowId ? arg.data.id : newAccountId()
      const created = await call(`create`).call(adapter, {
        ...arg,
        data: sealAccountRow({ ...arg.data, id }, id),
        forceAllowId: true,
      })
      return openOutput(arg.model, created)
    },
    update: async (arg: { model: string; where: Where[]; update: Row }) => {
      if (arg.model !== MODEL || !hasTokens(arg.update)) {
        return openOutput(arg.model, await call(`update`).call(adapter, arg))
      }
      let id = idOf(arg.where)
      if (!id) {
        const found = (await call(`findOne`).call(adapter, { model: MODEL, where: arg.where })) as Row | null
        id = typeof found?.id === `string` ? found.id : null
        if (!id) return null
      }
      const updated = await call(`update`).call(adapter, {
        ...arg,
        where: [{ field: `id`, value: id }],
        update: sealAccountRow(arg.update, id),
      })
      return openOutput(arg.model, updated)
    },
    updateMany: async (arg: { model: string; where: Where[]; update: Row }) => {
      if (arg.model !== MODEL || !hasTokens(arg.update)) return call(`updateMany`).call(adapter, arg)
      const rows = (await call(`findMany`).call(adapter, { model: MODEL, where: arg.where })) as Row[]
      let count = 0
      for (const row of rows) {
        const id = row.id as string
        const done = await call(`update`).call(adapter, {
          model: MODEL,
          where: [{ field: `id`, value: id }],
          update: sealAccountRow(arg.update, id),
        })
        if (done) count++
      }
      return count
    },
    findOne: async (arg: { model: string }) => openOutput(arg.model, await call(`findOne`).call(adapter, arg)),
    findMany: async (arg: { model: string }) => openOutput(arg.model, await call(`findMany`).call(adapter, arg)),
  }
  if (typeof adapter.transaction === `function`) {
    const transaction = adapter.transaction as (cb: (tx: Adapter) => Promise<unknown>) => Promise<unknown>
    overrides.transaction = (cb: (tx: Adapter) => Promise<unknown>) =>
      transaction.call(adapter, (tx: Adapter) => cb(wrapAdapter(tx)))
  }
  return new Proxy(adapter, {
    get(target, prop, receiver) {
      if (typeof prop === `string` && prop in overrides && typeof Reflect.get(target, prop) === `function`) {
        return overrides[prop] as AnyFn
      }
      return Reflect.get(target, prop, receiver)
    },
  })
}

/** Wrap a better-auth adapter factory (`drizzleAdapter(db, …)`). */
export function withAccountTokenVault<F extends (...args: never[]) => object>(adapterFactory: F): F {
  return ((...args: Parameters<F>) => wrapAdapter(adapterFactory(...args) as Adapter)) as F
}
