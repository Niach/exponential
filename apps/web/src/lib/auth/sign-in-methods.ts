import { and, eq } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import { APIError, createAuthMiddleware, getSessionFromCtx } from "better-auth/api"
import type { BetterAuthPlugin } from "better-auth"
import { accounts, apikeys, passkeys, users } from "@/db/auth-schema"
import type { db as Database } from "@/db/connection"
import { db } from "@/db/connection"
import { buildAuthConfig, type AuthConfig } from "@/lib/auth/config"
import {
  API_KEY_IDENTITY_CODE,
  API_KEY_IDENTITY_MESSAGE,
} from "@/lib/auth/api-key-kind"

// EXP-1126: the account's sign-in methods as ONE payload for every client
// (tRPC `users.signInMethods`), plus the invariant every removal respects:
// at least one way in must remain. A "way in" is a login the instance
// currently offers AND the account holds — an `accounts` row whose provider
// is still configured (the password only while password login is on) and a
// passkey while passkeys are on. The one-time code to the primary email is
// NOT one (EXP-1209): it is a convenience, not a method the person holds, so
// the last real method stays put; to leave entirely, delete the account.
//
// Better Auth's own last-account check (`/unlink-account`) only counts
// `accounts` rows — it would refuse to drop the sole Google row of an account
// that also signs in by passkey — so the server config sets
// `allowUnlinkingAll` and this module owns the rule instead: the tRPC
// mutations call `assertNotLastWayIn`, and the plugin below guards the two
// Better Auth endpoints a direct API caller could still reach.

export type SignInProviderKind =
  | `apple`
  | `google`
  | `github`
  | `oidc`
  | `password`

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
export const LAST_SIGN_IN_METHOD_MESSAGE = `This is your only way to sign in. Add another method first, or delete your account.`

type ProviderConfig = Pick<
  AuthConfig,
  | `emailOtpEnabled`
  | `passwordEnabled`
  | `passkeyEnabled`
  | `googleLoginEnabled`
  | `appleLoginEnabled`
  | `githubLoginEnabled`
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
 *  renders them: Apple, Google, GitHub (SLOP-7, only with GitHub LOGIN on —
 *  the repositories connection lives under Settings → Repositories and is
 *  never a sign-in method on its own), then the OIDC providers. */
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
    ...(config.githubLoginEnabled
      ? [{ id: `github`, name: `GitHub`, kind: `github` as const }]
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
  if (providerId === `github`) return config.githubLoginEnabled
  return config.oidcProviders.some((p) => p.id === providerId)
}

/** Pure: how many ways in the account has right now — the methods the
 *  person HOLDS: each usable provider row (the password only while password
 *  login is on) and each passkey (while passkeys are on). The one-time email
 *  code is a convenience, never a way in (EXP-1209): counting it let the last
 *  real method go and locked the account out. */
export function countWaysIn(input: {
  accounts: AccountRow[]
  passkeyCount: number
  config: ProviderConfig
}): number {
  let n = 0
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
    // SLOP-7: a GitHub row on an instance without GitHub LOGIN is the
    // repositories connection, managed under Settings → Repositories — it is
    // not a sign-in method and must not read as a leftover grant here.
    if (providerId === `github`) continue
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

/** Pure: would removing `removal` take away the account's LAST way in?
 *  Only a removal that drops a way in can be refused: a row that is no way
 *  in (a provider no longer offered, the password while password login is
 *  off) always goes, even when nothing else counts. */
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
  const before = countWaysIn({
    accounts: input.accounts,
    passkeyCount: input.passkeys.length,
    config: input.config,
  })
  const after = countWaysIn({
    accounts: accountsLeft,
    passkeyCount: passkeysLeft.length,
    config: input.config,
  })
  return before > 0 && after === 0
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

// The Better Auth endpoints that change WHO can sign in as this account: the
// primary email, the linked providers and the passkeys. Every one of them
// needs a real browser/native session; an `expu_` key (a person's or the
// launcher's hidden agent key, lib/auth/api-key-kind.ts) is refused, because
// the api-key plugin mocks a session for every endpoint and a leaked or
// prompt-injected key could otherwise re-home the whole account.
const IDENTITY_PATHS = new Set([
  `/email-otp/request-email-change`,
  `/email-otp/change-email`,
  `/link-social`,
  `/oauth2/link`,
  `/unlink-account`,
])

// Better Auth endpoints that hand a linked provider's tokens to the caller.
// SLOP-7 keeps the member's GitHub App user token (Contents write on every
// installed repo) on the `accounts` row, and the api-key plugin mocks a
// session for every endpoint, so an `expu_` key could read it over HTTP. No
// client calls them; `disabledPaths` 404s the HTTP route while the server-side
// `auth.api.getAccessToken` (lib/integrations/github-user.ts) keeps working.
export const DISABLED_AUTH_PATHS = [`/get-access-token`, `/refresh-token`]

export const PROVIDER_LOGIN_DISABLED_CODE = `PROVIDER_LOGIN_DISABLED`

/** Pure: is this `/sign-in/social` body a GitHub sign-in the instance does
 * not offer? The `github` provider is registered whenever the App's OAuth
 * client exists (linking needs it), so `GITHUB_LOGIN_ENABLED` must be
 * enforced here, not only by hiding the button. `/link-social` stays open. */
export function isRefusedSocialSignIn(
  body: unknown,
  config: Pick<AuthConfig, `githubLoginEnabled`>
): boolean {
  const provider = (body as { provider?: unknown } | null | undefined)?.provider
  return provider === `github` && !config.githubLoginEnabled
}

/** Pure: is `path` an identity-changing endpoint (`/passkey/*` included)? */
export function isIdentityPath(path: string | undefined): boolean {
  if (!path) return false
  return IDENTITY_PATHS.has(path) || path.startsWith(`/passkey/`)
}

/** Pure: the `expu_` credential a request carries, in either header form
 * the api-key plugin's `customAPIKeyGetter` accepts, else `null`. Sniffed
 * from the headers because this guard runs BEFORE the api-key plugin's own
 * hook mocks the session (plugin order), so the resolved session alone
 * cannot tell a key request apart in time. */
export function apiKeyCredentialFromHeaders(
  headers: Headers | undefined | null
): string | null {
  if (!headers) return null
  const direct = headers.get(`x-api-key`)
  if (direct) return direct
  const authz = headers.get(`authorization`)
  if (!authz) return null
  const match = authz.match(/^Bearer\s+(expu_[^\s]+)$/i)
  return match ? match[1]! : null
}

async function isApiKeyRowId(dbLike: DbLike, sessionId: string): Promise<boolean> {
  const [row] = await dbLike
    .select({ id: apikeys.id })
    .from(apikeys)
    .where(eq(apikeys.id, sessionId))
    .limit(1)
  return row !== undefined
}

export const PLACEHOLDER_EMAIL_CHANGE_MESSAGE = `That address is already known to a team you are in; ask the owner to invite you instead.`

/** Pure: the notice the OLD address gets when a `users` update swaps the
 * primary email. `data` is the update payload, `session` the requester's
 * (Better Auth's `change-email` runs under the account being changed, so its
 * session user still carries the OLD address). `null` = not an email change. */
export function emailChangeNotice(
  data: { email?: unknown },
  session: { user?: { email?: string | null } } | null | undefined
): { to: string; newEmail: string } | null {
  if (typeof data.email !== `string`) return null
  const newEmail = data.email.trim().toLowerCase()
  const current = session?.user?.email?.trim().toLowerCase() ?? ``
  if (!newEmail || !current || newEmail === current) return null
  return { to: current, newEmail }
}

/** Better Auth flavour: the same rules in front of the endpoints a direct
 *  API caller could still reach (the clients go through tRPC): no GitHub
 *  sign-in while that login is off, no identity
 *  change on an API key, the placeholder refusal on a requested email change,
 *  and the last-way-in rule on the two removals. Registered as a plugin so
 *  it composes with the config-level `hooks.before`. */
export function signInMethodsGuardPlugin(): BetterAuthPlugin {
  return {
    id: `exp-sign-in-methods-guard`,
    hooks: {
      before: [
        {
          matcher: (ctx) => ctx.path === `/sign-in/social`,
          handler: createAuthMiddleware(async (ctx) => {
            if (isRefusedSocialSignIn(ctx.body, buildAuthConfig())) {
              throw new APIError(`FORBIDDEN`, {
                code: PROVIDER_LOGIN_DISABLED_CODE,
                message: `Sign-in with GitHub is not enabled on this instance.`,
              })
            }
          }),
        },
        {
          matcher: (ctx) => isIdentityPath(ctx.path),
          handler: createAuthMiddleware(async (ctx) => {
            const refuse = () => {
              throw new APIError(`UNAUTHORIZED`, {
                code: API_KEY_IDENTITY_CODE,
                message: API_KEY_IDENTITY_MESSAGE,
              })
            }
            if (apiKeyCredentialFromHeaders(ctx.headers)) refuse()
            const session = await getSessionFromCtx(ctx)
            // No session: the endpoint's own middleware answers 401.
            if (!session?.user) return
            // Belt and braces: a session whose id IS an api-key row (the
            // plugin's mock) is a key request whatever header carried it.
            if (session.session?.id && (await isApiKeyRowId(db, session.session.id))) {
              refuse()
            }
            if (ctx.path === `/email-otp/request-email-change`) {
              // A placeholder member (EXP-630) holds the address without an
              // account: Better Auth would answer success and mail nothing.
              const body = (ctx.body ?? {}) as { newEmail?: unknown }
              if (typeof body.newEmail === `string`) {
                const [existing] = await db
                  .select({ placeholderAt: users.placeholderAt })
                  .from(users)
                  .where(eq(users.email, body.newEmail.trim().toLowerCase()))
                  .limit(1)
                if (existing?.placeholderAt) {
                  throw new APIError(`BAD_REQUEST`, {
                    code: `EMAIL_HELD_BY_PLACEHOLDER`,
                    message: PLACEHOLDER_EMAIL_CHANGE_MESSAGE,
                  })
                }
              }
            }
            if (!GUARDED_PATHS.has(ctx.path ?? ``)) return
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
