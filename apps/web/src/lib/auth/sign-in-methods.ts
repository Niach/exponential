import { and, eq } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import { APIError, createAuthMiddleware, getSessionFromCtx } from "better-auth/api"
import type { BetterAuthPlugin } from "better-auth"
import { accounts, passkeys, users } from "@/db/auth-schema"
import type { db as Database } from "@/db/connection"
import { db } from "@/db/connection"
import { buildAuthConfig, type AuthConfig } from "@/lib/auth/config"

// EXP-1126: the account's sign-in methods as ONE payload for every client
// (tRPC `users.signInMethods`), plus the invariant every removal respects:
// at least one way in must remain. A "way in" is a login the instance
// currently offers AND the account holds — an `accounts` row whose provider
// is still configured, a passkey while passkeys are on, and the one-time
// code to the primary email while a mail transport exists (that one needs
// no row: it is always there for the primary address).
//
// Better Auth's own last-account check (`/unlink-account`) only counts
// `accounts` rows — it would refuse to drop the sole Google row of an account
// that also signs in by code and passkey — so the server config sets
// `allowUnlinkingAll` and this module owns the rule instead: the tRPC
// mutations call `assertNotLastWayIn`, and the plugin below guards the two
// Better Auth endpoints a direct API caller could still reach.

export type SignInProviderKind = `apple` | `google` | `oidc` | `password`

export interface SignInProvider {
  // Better Auth provider id: `google`, `apple`, the OIDC id, or `credential`.
  id: string
  name: string
  kind: SignInProviderKind
  // The instance still offers this login (a linked-but-unconfigured provider
  // stays listed so it can be unlinked, but cannot be used or re-linked).
  available: boolean
  linked: boolean
  linkedAt: string | null
}

export interface SignInPasskey {
  id: string
  name: string | null
  createdAt: string | null
  backedUp: boolean
}

export interface SignInMethods {
  email: string
  emailVerified: boolean
  emailOtpEnabled: boolean
  passwordEnabled: boolean
  passkeyEnabled: boolean
  providers: SignInProvider[]
  passkeys: SignInPasskey[]
  waysIn: number
}

export const LAST_SIGN_IN_METHOD_CODE = `LAST_SIGN_IN_METHOD`
export const LAST_SIGN_IN_METHOD_MESSAGE = `This is your only way to sign in. Add another method before removing it.`

type ProviderConfig = Pick<
  AuthConfig,
  | `emailOtpEnabled`
  | `passwordEnabled`
  | `passkeyEnabled`
  | `googleLoginEnabled`
  | `appleLoginEnabled`
  | `oidcProviders`
>

type AccountRow = { providerId: string; createdAt?: Date | null }
type PasskeyRow = {
  id: string
  name: string | null
  createdAt: Date | null
  backedUp: boolean
}

/** The linkable providers this instance offers, in the order every client
 *  renders them: Apple, Google, then the OIDC providers as configured. */
export function configuredProviders(
  config: ProviderConfig
): Array<{ id: string; name: string; kind: SignInProviderKind }> {
  return [
    ...(config.appleLoginEnabled
      ? [{ id: `apple`, name: `Apple`, kind: `apple` as const }]
      : []),
    ...(config.googleLoginEnabled
      ? [{ id: `google`, name: `Google`, kind: `google` as const }]
      : []),
    ...config.oidcProviders.map((p) => ({
      id: p.id,
      name: p.name,
      kind: `oidc` as const,
    })),
  ]
}

function providerAvailable(providerId: string, config: ProviderConfig): boolean {
  if (providerId === `credential`) return config.passwordEnabled
  if (providerId === `google`) return config.googleLoginEnabled
  if (providerId === `apple`) return config.appleLoginEnabled
  return config.oidcProviders.some((p) => p.id === providerId)
}

/** Pure: how many ways in the account has right now. */
export function countWaysIn(input: {
  accounts: AccountRow[]
  passkeyCount: number
  config: ProviderConfig
}): number {
  let n = input.config.emailOtpEnabled ? 1 : 0
  for (const row of input.accounts) {
    if (providerAvailable(row.providerId, input.config)) n += 1
  }
  if (input.config.passkeyEnabled) n += input.passkeyCount
  return n
}

function isoOrNull(value: Date | string | null | undefined): string | null {
  if (!value) return null
  const date = value instanceof Date ? value : new Date(value)
  return Number.isNaN(date.getTime()) ? null : date.toISOString()
}

/** Pure: the payload from the rows. */
export function buildSignInMethods(input: {
  user: { email: string; emailVerified: boolean }
  accounts: AccountRow[]
  passkeys: PasskeyRow[]
  config: ProviderConfig
}): SignInMethods {
  const linkedByProvider = new Map<string, AccountRow>()
  for (const row of input.accounts) {
    if (!linkedByProvider.has(row.providerId)) linkedByProvider.set(row.providerId, row)
  }
  const providers: SignInProvider[] = configuredProviders(input.config).map((p) => {
    const linked = linkedByProvider.get(p.id)
    return {
      ...p,
      available: true,
      linked: linked !== undefined,
      linkedAt: isoOrNull(linked?.createdAt),
    }
  })
  // Rows for providers the instance no longer offers stay listed (unlinkable,
  // never usable) so a removed OIDC provider or a switched-off Google login
  // does not leave an invisible grant behind.
  for (const [providerId, row] of linkedByProvider) {
    if (providers.some((p) => p.id === providerId)) continue
    if (providerId === `credential`) continue
    providers.push({
      id: providerId,
      name: providerId,
      kind: providerId === `google` || providerId === `apple` ? providerId : `oidc`,
      available: false,
      linked: true,
      linkedAt: isoOrNull(row.createdAt),
    })
  }
  // A password shows up only while one is set: there is no "set a password"
  // here, only removing one (reset-by-mail stays the way to get one).
  const credential = linkedByProvider.get(`credential`)
  if (credential) {
    providers.push({
      id: `credential`,
      name: `Password`,
      kind: `password`,
      available: input.config.passwordEnabled,
      linked: true,
      linkedAt: isoOrNull(credential.createdAt),
    })
  }
  return {
    email: input.user.email,
    emailVerified: input.user.emailVerified,
    emailOtpEnabled: input.config.emailOtpEnabled,
    passwordEnabled: input.config.passwordEnabled,
    passkeyEnabled: input.config.passkeyEnabled,
    providers,
    passkeys: input.passkeys.map((row) => ({
      id: row.id,
      name: row.name,
      createdAt: isoOrNull(row.createdAt),
      backedUp: row.backedUp,
    })),
    waysIn: countWaysIn({
      accounts: input.accounts,
      passkeyCount: input.passkeys.length,
      config: input.config,
    }),
  }
}

type DbLike = Pick<typeof Database, `select`>

async function loadRows(dbLike: DbLike, userId: string) {
  const [[user], accountRows, passkeyRows] = await Promise.all([
    dbLike
      .select({ email: users.email, emailVerified: users.emailVerified })
      .from(users)
      .where(eq(users.id, userId))
      .limit(1),
    dbLike
      .select({ providerId: accounts.providerId, createdAt: accounts.createdAt })
      .from(accounts)
      .where(eq(accounts.userId, userId)),
    dbLike
      .select({
        id: passkeys.id,
        name: passkeys.name,
        createdAt: passkeys.createdAt,
        backedUp: passkeys.backedUp,
      })
      .from(passkeys)
      .where(eq(passkeys.userId, userId)),
  ])
  return { user, accountRows, passkeyRows }
}

export async function loadSignInMethods(
  dbLike: DbLike,
  userId: string
): Promise<SignInMethods | null> {
  const { user, accountRows, passkeyRows } = await loadRows(dbLike, userId)
  if (!user) return null
  return buildSignInMethods({
    user,
    accounts: accountRows,
    passkeys: passkeyRows,
    config: buildAuthConfig(),
  })
}

export type SignInMethodRemoval =
  | { providerId: string }
  | { passkeyId: string }

/** Pure: would removing `removal` leave the account with no way in? */
export function removalLeavesNoWayIn(input: {
  accounts: AccountRow[]
  passkeys: Array<{ id: string }>
  config: ProviderConfig
  removal: SignInMethodRemoval
}): boolean {
  const removedProvider =
    `providerId` in input.removal ? input.removal.providerId : null
  const removedPasskey =
    `passkeyId` in input.removal ? input.removal.passkeyId : null
  const accountsLeft =
    removedProvider === null
      ? input.accounts
      : input.accounts.filter((a) => a.providerId !== removedProvider)
  const passkeysLeft =
    removedPasskey === null
      ? input.passkeys
      : input.passkeys.filter((p) => p.id !== removedPasskey)
  return (
    countWaysIn({
      accounts: accountsLeft,
      passkeyCount: passkeysLeft.length,
      config: input.config,
    }) === 0
  )
}

async function wouldLeaveNoWayIn(
  dbLike: DbLike,
  userId: string,
  removal: SignInMethodRemoval
): Promise<boolean> {
  const { accountRows, passkeyRows } = await loadRows(dbLike, userId)
  return removalLeavesNoWayIn({
    accounts: accountRows,
    passkeys: passkeyRows,
    config: buildAuthConfig(),
    removal,
  })
}

/** tRPC flavour: refuses the removal that would lock the account out. */
export async function assertNotLastWayIn(
  dbLike: DbLike,
  userId: string,
  removal: SignInMethodRemoval
): Promise<void> {
  if (await wouldLeaveNoWayIn(dbLike, userId, removal)) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: LAST_SIGN_IN_METHOD_MESSAGE,
    })
  }
}

const GUARDED_PATHS = new Set([`/unlink-account`, `/passkey/delete-passkey`])

/** Better Auth flavour: the same rule in front of the two endpoints a direct
 *  API caller could still reach (the clients go through tRPC). Registered as
 *  a plugin so it composes with the config-level `hooks.before`. */
export function signInMethodsGuardPlugin(): BetterAuthPlugin {
  return {
    id: `exp-sign-in-methods-guard`,
    hooks: {
      before: [
        {
          matcher: (ctx) => GUARDED_PATHS.has(ctx.path ?? ``),
          handler: createAuthMiddleware(async (ctx) => {
            const session = await getSessionFromCtx(ctx)
            // No session: the endpoint's own middleware answers 401.
            if (!session?.user) return
            const body = (ctx.body ?? {}) as { providerId?: unknown; id?: unknown }
            const removal: SignInMethodRemoval | null =
              ctx.path === `/unlink-account`
                ? typeof body.providerId === `string`
                  ? { providerId: body.providerId }
                  : null
                : typeof body.id === `string`
                  ? { passkeyId: body.id }
                  : null
            if (!removal) return
            if (await wouldLeaveNoWayIn(db, session.user.id, removal)) {
              throw new APIError(`BAD_REQUEST`, {
                code: LAST_SIGN_IN_METHOD_CODE,
                message: LAST_SIGN_IN_METHOD_MESSAGE,
              })
            }
          }),
        },
      ],
    },
  }
}

// Drizzle helpers the tRPC mutations share.
export function accountRowFilter(userId: string, providerId: string) {
  return and(eq(accounts.userId, userId), eq(accounts.providerId, providerId))
}

export function passkeyRowFilter(userId: string, passkeyId: string) {
  return and(eq(passkeys.userId, userId), eq(passkeys.id, passkeyId))
}
