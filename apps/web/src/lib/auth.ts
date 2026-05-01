import { betterAuth } from "better-auth"
import { drizzleAdapter } from "better-auth/adapters/drizzle"
import { bearer, genericOAuth } from "better-auth/plugins"
import { createAuthMiddleware } from "better-auth/api"
import { tanstackStartCookies } from "better-auth/tanstack-start"
import { eq, and } from "drizzle-orm"
import { db } from "@/db/connection"
import * as schema from "@/db/auth-schema"

export type OidcProviderConfig = {
  id: string
  name: string
  clientId: string
  clientSecret: string
  discoveryUrl: string
  scopes?: string[]
  adminGroups?: string[]
  groupsClaim?: string
}

function extractGroups(profile: unknown, claimPath: string): string[] {
  if (!profile || typeof profile !== `object`) return []
  // Support dotted paths like `realm_access.roles`
  let value: unknown = profile
  for (const segment of claimPath.split(`.`)) {
    if (value && typeof value === `object` && segment in value) {
      value = (value as Record<string, unknown>)[segment]
    } else {
      return []
    }
  }
  if (Array.isArray(value)) {
    return value.filter((g): g is string => typeof g === `string`)
  }
  if (typeof value === `string`) {
    return [value]
  }
  return []
}

function isAdminFromProfile(
  profile: unknown,
  provider: OidcProviderConfig
): boolean {
  if (!provider.adminGroups || provider.adminGroups.length === 0) return false
  const groups = extractGroups(profile, provider.groupsClaim ?? `groups`)
  return provider.adminGroups.some((g) => groups.includes(g))
}

function decodeJwtPayload(token: string): unknown {
  const parts = token.split(`.`)
  if (parts.length !== 3) return null
  try {
    // base64url decode the payload (parts[1])
    const payload = parts[1].replace(/-/g, `+`).replace(/_/g, `/`)
    const padded = payload + `=`.repeat((4 - (payload.length % 4)) % 4)
    return JSON.parse(Buffer.from(padded, `base64`).toString(`utf-8`))
  } catch {
    return null
  }
}

export function parseOidcProviders(): OidcProviderConfig[] {
  const raw = process.env.OIDC_PROVIDERS
  if (raw && raw.trim()) {
    try {
      const parsed = JSON.parse(raw) as Array<Partial<OidcProviderConfig>>
      if (!Array.isArray(parsed)) {
        console.error(`OIDC_PROVIDERS must be a JSON array`)
        return []
      }
      return parsed
        .filter((p) => p.id && p.clientId && p.clientSecret && p.discoveryUrl)
        .map((p) => ({
          id: p.id!,
          name: p.name || p.id!,
          clientId: p.clientId!,
          clientSecret: p.clientSecret!,
          discoveryUrl: p.discoveryUrl!,
          scopes: p.scopes,
          adminGroups: Array.isArray(p.adminGroups)
            ? p.adminGroups.filter((g): g is string => typeof g === `string`)
            : undefined,
          groupsClaim: typeof p.groupsClaim === `string` ? p.groupsClaim : undefined,
        }))
    } catch (err) {
      console.error(`Failed to parse OIDC_PROVIDERS env var:`, err)
      return []
    }
  }

  if (
    process.env.AUTH_OIDC_ENABLED === `true` &&
    process.env.OIDC_CLIENT_ID &&
    process.env.OIDC_CLIENT_SECRET &&
    process.env.OIDC_DISCOVERY_URL
  ) {
    const id = process.env.OIDC_PROVIDER_ID || `authentik`
    return [
      {
        id,
        name: id.charAt(0).toUpperCase() + id.slice(1),
        clientId: process.env.OIDC_CLIENT_ID,
        clientSecret: process.env.OIDC_CLIENT_SECRET,
        discoveryUrl: process.env.OIDC_DISCOVERY_URL,
      },
    ]
  }
  return []
}

const oidcProviders = parseOidcProviders()
const googleClientConfigured = Boolean(
  process.env.GOOGLE_CLIENT_ID && process.env.GOOGLE_CLIENT_SECRET
)
const googleLoginEnabled =
  googleClientConfigured && process.env.GOOGLE_LOGIN_ENABLED === `true`
const googleCalendarEnabled =
  googleClientConfigured && process.env.GOOGLE_CALENDAR_ENABLED === `true`
const googleSocialEnabled = googleLoginEnabled || googleCalendarEnabled

export const auth = betterAuth({
  database: drizzleAdapter(db, {
    provider: `pg`,
    usePlural: true,
    schema,
  }),
  emailAndPassword: {
    enabled: process.env.AUTH_PASSWORD_ENABLED !== `false`,
    disableSignUp: process.env.NODE_ENV === `production`,
    minPasswordLength: process.env.NODE_ENV === `production` ? 8 : 1,
  },
  user: {
    additionalFields: {
      isAdmin: {
        type: `boolean`,
        defaultValue: false,
        // Block clients from setting this via the public auth API.
        input: false,
      },
    },
  },
  trustedOrigins: (process.env.BETTER_AUTH_TRUSTED_ORIGINS || ``)
    .split(`,`)
    .filter(Boolean),
  socialProviders: googleSocialEnabled
    ? {
        google: {
          clientId: process.env.GOOGLE_CLIENT_ID!,
          clientSecret: process.env.GOOGLE_CLIENT_SECRET!,
          accessType: `offline`,
          prompt: `select_account consent`,
        },
      }
    : undefined,
  // Without this, Better Auth refuses to attach a Google account to a
  // user that signed in via genericOAuth — the OAuth flow completes on
  // Google's side but the accounts row never lands.
  account: {
    accountLinking: {
      enabled: true,
      trustedProviders: [
        ...oidcProviders.map((p) => p.id),
        ...(googleSocialEnabled ? [`google`] : []),
      ],
      // Logged-in user's email (from an OIDC provider) likely differs from
      // their Google account email — without this, Better Auth refuses to link.
      allowDifferentEmails: true,
    },
  },
  logger: {
    level: `debug`,
    log: (level, message, ...args) => {
      // stderr is unbuffered in Bun; console.log goes to stdout which can
      // get held in a buffer when not attached to a TTY (e.g. inside docker).
      const payload =
        args.length > 0
          ? `[better-auth][${level}] ${message} ${JSON.stringify(args)}\n`
          : `[better-auth][${level}] ${message}\n`
      process.stderr.write(payload)
    },
  },
  hooks: {
    after: createAuthMiddleware(async (ctx) => {
      // Re-evaluate admin status after every OIDC sign-in, so that group
      // changes upstream (added or revoked) take effect on next login.
      const newSession = ctx.context.newSession
      if (!newSession?.user) return

      // Match the genericOAuth callback path. better-auth mounts the plugin
      // under /oauth2/callback/:providerId.
      const path = ctx.path
      if (!path?.startsWith(`/oauth2/callback/`)) return

      const providerId = path.split(`/`).pop()
      if (!providerId) return

      const provider = oidcProviders.find((p) => p.id === providerId)
      if (!provider || !provider.adminGroups?.length) return

      try {
        const [account] = await db
          .select({ idToken: schema.accounts.idToken })
          .from(schema.accounts)
          .where(
            and(
              eq(schema.accounts.userId, newSession.user.id),
              eq(schema.accounts.providerId, providerId)
            )
          )
          .limit(1)

        if (!account?.idToken) return

        const claims = decodeJwtPayload(account.idToken)
        const shouldBeAdmin = isAdminFromProfile(claims, provider)
        const currentIsAdmin = (newSession.user as { isAdmin?: boolean })
          .isAdmin

        if (shouldBeAdmin !== currentIsAdmin) {
          await db
            .update(schema.users)
            .set({ isAdmin: shouldBeAdmin, updatedAt: new Date() })
            .where(eq(schema.users.id, newSession.user.id))
        }
      } catch (err) {
        console.error(`[auth] failed to re-evaluate admin status:`, err)
      }
    }),
  },
  plugins: [
    tanstackStartCookies(),
    bearer(),
    ...(oidcProviders.length > 0
      ? [
          genericOAuth({
            config: oidcProviders.map((p) => ({
              providerId: p.id,
              clientId: p.clientId,
              clientSecret: p.clientSecret,
              discoveryUrl: p.discoveryUrl,
              scopes:
                p.scopes ??
                (p.adminGroups?.length
                  ? [`openid`, `profile`, `email`, `groups`]
                  : [`openid`, `profile`, `email`]),
              mapProfileToUser: (profile) => ({
                name:
                  profile.name ||
                  profile.preferred_username ||
                  profile.nickname ||
                  profile.email,
                isAdmin: isAdminFromProfile(profile, p),
              }),
            })),
          }),
        ]
      : []),
  ],
})
