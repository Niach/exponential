import { createServerFn } from "@tanstack/react-start"
import { parseOidcProviders } from "@/lib/oidc-providers"
import { emailEnabled } from "@/lib/email-enabled"
import { isProductionBuild } from "@/lib/production-build"

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
  // RFC 8628 device-code login is available (EXP-403) — the CLI feature-detects
  // this and falls back to password login against older self-hosted instances.
  deviceFlowEnabled: boolean
  // EXP-857: passwordless "Continue with email" (a 6-digit one-time code sent
  // by mail) — needs a mail transport. Natives decode absent as false.
  emailOtpEnabled: boolean
  // EXP-857: "Login with passkey" (WebAuthn). Needs a secure context, so it is
  // off on plain-http self-hosts (localhost excepted).
  passkeyEnabled: boolean
}

// Public password sign-up: historically OFF in production (invite/OAuth
// only). AUTH_SIGNUP_ENABLED overrides in either direction so the cloud
// instance can open registration for launch. Shared by the Better Auth
// server config (emailAndPassword.disableSignUp in lib/auth/index.ts) and
// buildAuthConfig's signupEnabled.
export function isPasswordSignupDisabled(): boolean {
  return process.env.AUTH_SIGNUP_ENABLED
    ? process.env.AUTH_SIGNUP_ENABLED === `false`
    : isProductionBuild
}

// Better Auth's global rate limiter (REV-5): ON in every production build,
// off under the dev server; AUTH_RATE_LIMIT_ENABLED overrides in either
// direction (e.g. `false` behind a reverse proxy that already rate-limits,
// or load tests against a production image).
export function isAuthRateLimitEnabled(): boolean {
  return process.env.AUTH_RATE_LIMIT_ENABLED
    ? process.env.AUTH_RATE_LIMIT_ENABLED !== `false`
    : isProductionBuild
}

// EXP-857: one-time-code login rides the mail transport; AUTH_EMAIL_OTP_ENABLED
// force-disables it (an instance that wants password-only email login).
export function isEmailOtpEnabled(): boolean {
  return emailEnabled && process.env.AUTH_EMAIL_OTP_ENABLED !== `false`
}

// EXP-857: WebAuthn only runs in a secure context, and the relying-party id
// derives from BETTER_AUTH_URL, so passkeys are offered only when that base is
// https (or localhost, where browsers make an exception for development).
// AUTH_PASSKEY_ENABLED=false force-disables it.
export function isPasskeyEnabled(): boolean {
  if (process.env.AUTH_PASSKEY_ENABLED === `false`) return false
  const base = process.env.BETTER_AUTH_URL
  if (!base) return false
  try {
    const url = new URL(base)
    return url.protocol === `https:` || url.hostname === `localhost`
  } catch {
    return false
  }
}

// The WebAuthn relying-party id + rpName (EXP-857): the instance hostname.
export function passkeyRelyingParty(): { rpID: string; rpName: string } {
  let rpID = `localhost`
  try {
    rpID = new URL(process.env.BETTER_AUTH_URL ?? ``).hostname || rpID
  } catch {
    // unset or malformed base URL — the plugin falls back to localhost too
  }
  return { rpID, rpName: `Exponential` }
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
    deviceFlowEnabled: true,
    emailOtpEnabled: isEmailOtpEnabled(),
    passkeyEnabled: isPasskeyEnabled(),
  }
}

export const getAuthConfig = createServerFn({ method: `GET` }).handler(() =>
  buildAuthConfig()
)
