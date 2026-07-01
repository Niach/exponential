import { createServerFn } from "@tanstack/react-start"
import { parseOidcProviders } from "@/lib/oidc-providers"
import { emailEnabled } from "@/lib/email"

export type AuthConfig = {
  passwordEnabled: boolean
  // Email sending is configured (RESEND_API_KEY) — gates "Forgot password?".
  passwordResetEnabled: boolean
  oidcProviders: Array<{ id: string; name: string }>
  googleLoginEnabled: boolean
  githubEnabled: boolean
}

export function buildAuthConfig(): AuthConfig {
  const googleClientConfigured = Boolean(
    process.env.GOOGLE_CLIENT_ID && process.env.GOOGLE_CLIENT_SECRET
  )
  const passwordEnabled = process.env.AUTH_PASSWORD_ENABLED !== `false`
  return {
    passwordEnabled,
    passwordResetEnabled: passwordEnabled && emailEnabled,
    oidcProviders: parseOidcProviders().map(({ id, name }) => ({ id, name })),
    googleLoginEnabled:
      googleClientConfigured && process.env.GOOGLE_LOGIN_ENABLED === `true`,
    githubEnabled: Boolean(
      process.env.GITHUB_APP_ID && process.env.GITHUB_APP_PRIVATE_KEY
    ),
  }
}

export const getAuthConfig = createServerFn({ method: `GET` }).handler(() =>
  buildAuthConfig()
)
