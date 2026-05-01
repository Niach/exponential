import { createServerFn } from "@tanstack/react-start"
import { parseOidcProviders } from "@/lib/auth"

export const getAuthConfig = createServerFn({ method: `GET` }).handler(() => {
  const googleClientConfigured = Boolean(
    process.env.GOOGLE_CLIENT_ID && process.env.GOOGLE_CLIENT_SECRET
  )
  return {
    passwordEnabled: process.env.AUTH_PASSWORD_ENABLED !== `false`,
    oidcProviders: parseOidcProviders().map(({ id, name }) => ({ id, name })),
    googleLoginEnabled:
      googleClientConfigured && process.env.GOOGLE_LOGIN_ENABLED === `true`,
    googleCalendarEnabled:
      googleClientConfigured && process.env.GOOGLE_CALENDAR_ENABLED === `true`,
  }
})
