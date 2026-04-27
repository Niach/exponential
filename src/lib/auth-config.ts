import { createServerFn } from "@tanstack/react-start"

export const getAuthConfig = createServerFn({ method: `GET` }).handler(
  () => ({
    oidcEnabled: process.env.AUTH_OIDC_ENABLED === `true`,
    passwordEnabled: process.env.AUTH_PASSWORD_ENABLED !== `false`,
    oidcProviderId: process.env.OIDC_PROVIDER_ID || `authentik`,
    googleCalendarEnabled: Boolean(
      process.env.GOOGLE_CLIENT_ID && process.env.GOOGLE_CLIENT_SECRET
    ),
  })
)
