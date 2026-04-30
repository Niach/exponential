import { createServerFn } from "@tanstack/react-start"
import { parseOidcProviders } from "@/lib/auth"

export const getAuthConfig = createServerFn({ method: `GET` }).handler(
  () => ({
    passwordEnabled: process.env.AUTH_PASSWORD_ENABLED !== `false`,
    oidcProviders: parseOidcProviders().map(({ id, name }) => ({ id, name })),
    googleLoginEnabled: Boolean(
      process.env.GOOGLE_CLIENT_ID && process.env.GOOGLE_CLIENT_SECRET
    ),
    googleCalendarEnabled: Boolean(
      process.env.GOOGLE_CLIENT_ID && process.env.GOOGLE_CLIENT_SECRET
    ),
  })
)
