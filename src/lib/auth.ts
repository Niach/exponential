import { betterAuth } from "better-auth"
import { drizzleAdapter } from "better-auth/adapters/drizzle"
import { genericOAuth } from "better-auth/plugins"
import { tanstackStartCookies } from "better-auth/tanstack-start"
import { db } from "@/db/connection"
import * as schema from "@/db/auth-schema"

const oidcEnabled = process.env.AUTH_OIDC_ENABLED === `true`
const googleCalendarEnabled = Boolean(
  process.env.GOOGLE_CLIENT_ID && process.env.GOOGLE_CLIENT_SECRET
)

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
  socialProviders: googleCalendarEnabled
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
  // user that signed in via genericOAuth (Authentik) — the OAuth flow
  // completes on Google's side but the accounts row never lands.
  account: {
    accountLinking: {
      enabled: true,
      trustedProviders: [`google`, `authentik`],
    },
  },
  logger: {
    level: `debug`,
    log: (level, message, ...args) => {
      console.log(`[better-auth][${level}] ${message}`, ...args)
    },
  },
  plugins: [
    tanstackStartCookies(),
    ...(oidcEnabled
      ? [
          genericOAuth({
            config: [
              {
                providerId: process.env.OIDC_PROVIDER_ID || `authentik`,
                clientId: process.env.OIDC_CLIENT_ID!,
                clientSecret: process.env.OIDC_CLIENT_SECRET!,
                discoveryUrl: process.env.OIDC_DISCOVERY_URL!,
                scopes: [`openid`, `profile`, `email`],
                mapProfileToUser: (profile) => ({
                  name:
                    profile.name ||
                    profile.preferred_username ||
                    profile.nickname ||
                    profile.email,
                }),
              },
            ],
          }),
        ]
      : []),
  ],
})
