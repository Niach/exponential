import { betterAuth } from "better-auth"
import { drizzleAdapter } from "better-auth/adapters/drizzle"
import { genericOAuth } from "better-auth/plugins"
import { tanstackStartCookies } from "better-auth/tanstack-start"
import { db } from "@/db/connection"
import * as schema from "@/db/auth-schema"

export type OidcProviderConfig = {
  id: string
  name: string
  clientId: string
  clientSecret: string
  discoveryUrl: string
  scopes?: string[]
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
  plugins: [
    tanstackStartCookies(),
    ...(oidcProviders.length > 0
      ? [
          genericOAuth({
            config: oidcProviders.map((p) => ({
              providerId: p.id,
              clientId: p.clientId,
              clientSecret: p.clientSecret,
              discoveryUrl: p.discoveryUrl,
              scopes: p.scopes ?? [`openid`, `profile`, `email`],
              mapProfileToUser: (profile) => ({
                name:
                  profile.name ||
                  profile.preferred_username ||
                  profile.nickname ||
                  profile.email,
              }),
            })),
          }),
        ]
      : []),
  ],
})
