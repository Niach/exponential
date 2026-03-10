import { betterAuth } from "better-auth"
import { drizzleAdapter } from "better-auth/adapters/drizzle"
import { genericOAuth } from "better-auth/plugins"
import { tanstackStartCookies } from "better-auth/tanstack-start"
import { db } from "@/db/connection"
import * as schema from "@/db/auth-schema"

const oidcEnabled = process.env.AUTH_OIDC_ENABLED === `true`

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
