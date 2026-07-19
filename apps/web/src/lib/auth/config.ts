import { createServerFn } from "@tanstack/react-start"
import { parseOidcProviders } from "@/lib/oidc-providers"
import { emailEnabled } from "@/lib/email"

export type AuthConfig = {
  passwordEnabled: boolean
  // Password sign-up is open — gates the "Create account" mode on the merged
  // /auth/login page.
  signupEnabled: boolean
  // Email sending is configured (AWS_SES_REGION) — gates "Forgot password?".
  passwordResetEnabled: boolean
  oidcProviders: Array<{ id: string; name: string }>
  googleLoginEnabled: boolean
  appleLoginEnabled: boolean
  githubEnabled: boolean
}

// Public password sign-up: historically OFF in production (invite/OAuth
// only). AUTH_SIGNUP_ENABLED overrides in either direction so the cloud
// instance can open registration for launch. Shared by the Better Auth
// server config (emailAndPassword.disableSignUp in lib/auth/index.ts) and
// buildAuthConfig's signupEnabled.
export function isPasswordSignupDisabled(): boolean {
  return process.env.AUTH_SIGNUP_ENABLED
    ? process.env.AUTH_SIGNUP_ENABLED === `false`
    : process.env.NODE_ENV === `production`
}

export function buildAuthConfig(): AuthConfig {
  const googleClientConfigured = Boolean(
    process.env.GOOGLE_CLIENT_ID && process.env.GOOGLE_CLIENT_SECRET
  )
  const passwordEnabled = process.env.AUTH_PASSWORD_ENABLED !== `false`
  return {
    passwordEnabled,
    signupEnabled: passwordEnabled && !isPasswordSignupDisabled(),
    passwordResetEnabled: passwordEnabled && emailEnabled,
    oidcProviders: parseOidcProviders().map(({ id, name }) => ({ id, name })),
    googleLoginEnabled:
      googleClientConfigured && process.env.GOOGLE_LOGIN_ENABLED === `true`,
    appleLoginEnabled:
      Boolean(
        process.env.APPLE_CLIENT_ID &&
          (process.env.APPLE_CLIENT_SECRET ||
            (process.env.APPLE_PRIVATE_KEY &&
              process.env.APPLE_KEY_ID &&
              process.env.APPLE_TEAM_ID))
      ) && process.env.APPLE_LOGIN_ENABLED === `true`,
    githubEnabled: Boolean(
      process.env.GITHUB_APP_ID && process.env.GITHUB_APP_PRIVATE_KEY
    ),
  }
}

export const getAuthConfig = createServerFn({ method: `GET` }).handler(() =>
  buildAuthConfig()
)
