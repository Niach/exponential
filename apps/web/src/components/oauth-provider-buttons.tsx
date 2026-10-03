import { useState } from "react"
import { authClient } from "@/lib/auth/client"
import { authErrorMessage } from "@/lib/auth/error-messages"
import { withFirstTouchParams } from "@/lib/conversion/first-touch"
import { Button } from "@exp/ui"

export const GOOGLE_PROVIDER_KEY = `__google__`
export const APPLE_PROVIDER_KEY = `__apple__`
export const GITHUB_PROVIDER_KEY = `__github__`

// Official multi-color Google "G" mark, inlined as SVG — the CSP forbids
// external asset hosts, and the brand colors are fixed (never themed).
// Exported for the Account page's sign-in methods rows (EXP-1126).
export function GoogleIcon() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path
        fill="#4285F4"
        d="M23.52 12.273c0-.851-.076-1.67-.218-2.455H12v4.642h6.458c-.278 1.5-1.124 2.771-2.395 3.622v3.011h3.878c2.269-2.089 3.579-5.165 3.579-8.82Z"
      />
      <path
        fill="#34A853"
        d="M12 24c3.24 0 5.956-1.075 7.941-2.907l-3.878-3.011c-1.074.72-2.449 1.145-4.063 1.145-3.125 0-5.771-2.111-6.715-4.948H1.276v3.111C3.251 21.207 7.309 24 12 24Z"
      />
      <path
        fill="#FBBC05"
        d="M5.285 14.279A7.213 7.213 0 0 1 4.909 12c0-.79.136-1.559.376-2.279V6.611H1.276A11.995 11.995 0 0 0 0 12c0 1.936.464 3.769 1.276 5.389l4.009-3.11Z"
      />
      <path
        fill="#EA4335"
        d="M12 4.773c1.762 0 3.344.605 4.587 1.794l3.442-3.441C17.94 1.19 15.24 0 12 0 7.309 0 3.251 2.793 1.276 6.611l4.009 3.11C6.229 6.885 8.875 4.773 12 4.773Z"
      />
    </svg>
  )
}

// Solid Apple glyph in `currentColor`, so it tracks the button's foreground
// on the dark theme.
export function AppleIcon() {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M12.152 6.896c-.948 0-2.415-1.078-3.96-1.04-2.04.027-3.91 1.183-4.961 3.014-2.117 3.675-.546 9.103 1.519 12.09 1.013 1.454 2.208 3.09 3.792 3.039 1.52-.065 2.09-.987 3.935-.987 1.831 0 2.35.987 3.96.948 1.637-.026 2.676-1.48 3.676-2.948 1.156-1.688 1.636-3.325 1.662-3.415-.039-.013-3.182-1.221-3.22-4.857-.026-3.04 2.48-4.494 2.597-4.559-1.429-2.09-3.623-2.324-4.39-2.376-2-.156-3.675 1.09-4.61 1.09ZM15.53 3.83c.843-1.012 1.4-2.427 1.245-3.83-1.207.052-2.662.805-3.532 1.818-.78.896-1.454 2.338-1.273 3.714 1.338.104 2.715-.688 3.56-1.702Z" />
    </svg>
  )
}

// SLOP-7: the GitHub mark, `currentColor` like Apple's, for "Continue with
// GitHub" (GITHUB_LOGIN_ENABLED) and the Account page's sign-in rows.
export function GithubIcon() {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M12 .5C5.65.5.5 5.65.5 12c0 5.08 3.29 9.39 7.86 10.91.58.1.79-.25.79-.56v-2.17c-3.2.7-3.87-1.37-3.87-1.37-.52-1.33-1.28-1.68-1.28-1.68-1.04-.71.08-.7.08-.7 1.15.08 1.76 1.19 1.76 1.19 1.03 1.76 2.69 1.25 3.35.96.1-.75.4-1.25.73-1.54-2.55-.29-5.24-1.28-5.24-5.68 0-1.26.45-2.28 1.19-3.09-.12-.29-.52-1.46.11-3.04 0 0 .97-.31 3.18 1.18a11 11 0 0 1 5.78 0c2.21-1.49 3.18-1.18 3.18-1.18.63 1.58.23 2.75.11 3.04.74.81 1.19 1.83 1.19 3.09 0 4.41-2.69 5.38-5.25 5.67.41.36.78 1.06.78 2.14v3.17c0 .31.21.67.8.56A11.5 11.5 0 0 0 23.5 12C23.5 5.65 18.35.5 12 .5Z" />
    </svg>
  )
}

export interface OidcProviderOption {
  id: string
  name: string
}

type OAuthStartError = { code?: string; message?: string } | null

/**
 * EXP-1132: where a failed provider round-trip lands — back on login WITH the
 * destination, so an invite link survives a cancelled or refused sign-in
 * (Better Auth appends `error=<reason>`, the page explains it). Without it the
 * failure went to Better Auth's error route, which in production bounces to
 * `/` and drops the redirect.
 */
export function oauthErrorCallbackURL(redirectTo: string | undefined): string {
  return redirectTo
    ? `/auth/login?redirect=${encodeURIComponent(redirectTo)}`
    : `/auth/login`
}

/**
 * Shared OAuth sign-in state/handlers for the login and register pages.
 * `error` is shared with the page's password form so both surface one message.
 */
export function useOAuthSignIn(redirectTo: string | undefined) {
  const [pendingProvider, setPendingProvider] = useState<string | null>(null)
  const [error, setError] = useState(``)

  // Better Auth resolves to `{ data, error }` instead of throwing, so a failed
  // OAuth start (misconfigured provider, 500, 429) never redirects and must
  // clear the pending state — otherwise every button stays disabled on
  // "Redirecting..." with no message and only a reload recovers.
  const startOAuth = async (
    providerKey: string,
    start: () => Promise<{ error?: OAuthStartError }>
  ) => {
    setPendingProvider(providerKey)
    setError(``)
    try {
      const { error: startError } = await start()
      if (startError) {
        setError(
          authErrorMessage(startError, `Couldn't sign you in. Try again.`)
        )
        setPendingProvider(null)
      }
    } catch {
      setError(`An unexpected error occurred`)
      setPendingProvider(null)
    }
  }

  // withFirstTouchParams threads any ref/utm params the login page arrived
  // with through the provider round-trip (cookieless attribution, EXP-362) —
  // the post-OAuth load re-captures them and claims them for fresh accounts.
  const signInWithOidc = (providerId: string) =>
    startOAuth(providerId, () =>
      authClient.signIn.oauth2({
        providerId,
        callbackURL: withFirstTouchParams(redirectTo || `/`),
        errorCallbackURL: oauthErrorCallbackURL(redirectTo),
      })
    )

  const signInWithGoogle = () =>
    startOAuth(GOOGLE_PROVIDER_KEY, () =>
      authClient.signIn.social({
        provider: `google`,
        callbackURL: withFirstTouchParams(redirectTo || `/`),
        errorCallbackURL: oauthErrorCallbackURL(redirectTo),
      })
    )

  const signInWithGithub = () =>
    startOAuth(GITHUB_PROVIDER_KEY, () =>
      authClient.signIn.social({
        provider: `github`,
        callbackURL: withFirstTouchParams(redirectTo || `/`),
        errorCallbackURL: oauthErrorCallbackURL(redirectTo),
      })
    )

  const signInWithApple = () =>
    startOAuth(APPLE_PROVIDER_KEY, () =>
      authClient.signIn.social({
        provider: `apple`,
        callbackURL: withFirstTouchParams(redirectTo || `/`),
        errorCallbackURL: oauthErrorCallbackURL(redirectTo),
      })
    )

  return {
    pendingProvider,
    error,
    setError,
    signInWithOidc,
    signInWithGoogle,
    signInWithApple,
    signInWithGithub,
  }
}

interface OAuthProviderButtonsProps {
  oidcProviders: OidcProviderOption[]
  googleLoginEnabled: boolean
  appleLoginEnabled: boolean
  /** SLOP-7: "Continue with GitHub" (GITHUB_LOGIN_ENABLED). Optional so the
   * natives' older config payloads and existing callers read as off. */
  githubLoginEnabled?: boolean
  /** Action verb shown on the buttons, e.g. "Sign in" or "Sign up". */
  verb: string
  pendingProvider: string | null
  /** Render the "or" divider below the buttons (only when a form follows). */
  showDivider: boolean
  onOidc: (providerId: string) => void
  onGoogle: () => void
  onApple: () => void
  onGithub?: () => void
}

export function OAuthProviderButtons({
  oidcProviders,
  googleLoginEnabled,
  appleLoginEnabled,
  githubLoginEnabled = false,
  verb,
  pendingProvider,
  showDivider,
  onOidc,
  onGoogle,
  onApple,
  onGithub,
}: OAuthProviderButtonsProps) {
  if (
    oidcProviders.length === 0 &&
    !googleLoginEnabled &&
    !appleLoginEnabled &&
    !githubLoginEnabled
  )
    return null

  return (
    <>
      {appleLoginEnabled && (
        <Button
          type="button"
          variant="outline"
          className="w-full"
          disabled={pendingProvider !== null}
          onClick={onApple}
        >
          <AppleIcon />
          {pendingProvider === APPLE_PROVIDER_KEY
            ? `Redirecting...`
            : `${verb} with Apple`}
        </Button>
      )}

      {oidcProviders.map((provider) => (
        <Button
          key={provider.id}
          type="button"
          variant="outline"
          className="w-full"
          disabled={pendingProvider !== null}
          onClick={() => onOidc(provider.id)}
        >
          {pendingProvider === provider.id
            ? `Redirecting...`
            : `${verb} with ${provider.name}`}
        </Button>
      ))}

      {googleLoginEnabled && (
        <Button
          type="button"
          variant="outline"
          className="w-full"
          disabled={pendingProvider !== null}
          onClick={onGoogle}
        >
          <GoogleIcon />
          {pendingProvider === GOOGLE_PROVIDER_KEY
            ? `Redirecting...`
            : `${verb} with Google`}
        </Button>
      )}

      {githubLoginEnabled && (
        <Button
          type="button"
          variant="outline"
          className="w-full"
          disabled={pendingProvider !== null}
          onClick={onGithub}
        >
          <GithubIcon />
          {pendingProvider === GITHUB_PROVIDER_KEY
            ? `Redirecting...`
            : `${verb} with GitHub`}
        </Button>
      )}

      {showDivider && (
        <div className="relative">
          <div className="absolute inset-0 flex items-center">
            <span className="w-full border-t" />
          </div>
          <div className="relative flex justify-center text-xs uppercase">
            <span className="bg-card px-2 text-muted-foreground">or</span>
          </div>
        </div>
      )}
    </>
  )
}
