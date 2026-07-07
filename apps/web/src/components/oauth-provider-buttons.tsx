import { useState } from "react"
import { authClient } from "@/lib/auth/client"
import { Button } from "@/components/ui/button"

export const GOOGLE_PROVIDER_KEY = `__google__`
export const APPLE_PROVIDER_KEY = `__apple__`

// Official multi-color Google "G" mark, inlined as SVG — the CSP forbids
// external asset hosts, and the brand colors are fixed (never themed).
function GoogleIcon() {
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
function AppleIcon() {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M12.152 6.896c-.948 0-2.415-1.078-3.96-1.04-2.04.027-3.91 1.183-4.961 3.014-2.117 3.675-.546 9.103 1.519 12.09 1.013 1.454 2.208 3.09 3.792 3.039 1.52-.065 2.09-.987 3.935-.987 1.831 0 2.35.987 3.96.948 1.637-.026 2.676-1.48 3.676-2.948 1.156-1.688 1.636-3.325 1.662-3.415-.039-.013-3.182-1.221-3.22-4.857-.026-3.04 2.48-4.494 2.597-4.559-1.429-2.09-3.623-2.324-4.39-2.376-2-.156-3.675 1.09-4.61 1.09ZM15.53 3.83c.843-1.012 1.4-2.427 1.245-3.83-1.207.052-2.662.805-3.532 1.818-.78.896-1.454 2.338-1.273 3.714 1.338.104 2.715-.688 3.56-1.702Z" />
    </svg>
  )
}

export interface OidcProviderOption {
  id: string
  name: string
}

/**
 * Shared OAuth sign-in state/handlers for the login and register pages.
 * `error` is shared with the page's password form so both surface one message.
 */
export function useOAuthSignIn(redirectTo: string | undefined) {
  const [pendingProvider, setPendingProvider] = useState<string | null>(null)
  const [error, setError] = useState(``)

  const signInWithOidc = async (providerId: string) => {
    setPendingProvider(providerId)
    setError(``)
    try {
      await authClient.signIn.oauth2({ providerId, callbackURL: redirectTo || `/` })
    } catch {
      setError(`An unexpected error occurred`)
      setPendingProvider(null)
    }
  }

  const signInWithGoogle = async () => {
    setPendingProvider(GOOGLE_PROVIDER_KEY)
    setError(``)
    try {
      await authClient.signIn.social({
        provider: `google`,
        callbackURL: redirectTo || `/`,
      })
    } catch {
      setError(`An unexpected error occurred`)
      setPendingProvider(null)
    }
  }

  const signInWithApple = async () => {
    setPendingProvider(APPLE_PROVIDER_KEY)
    setError(``)
    try {
      await authClient.signIn.social({
        provider: `apple`,
        callbackURL: redirectTo || `/`,
      })
    } catch {
      setError(`An unexpected error occurred`)
      setPendingProvider(null)
    }
  }

  return {
    pendingProvider,
    error,
    setError,
    signInWithOidc,
    signInWithGoogle,
    signInWithApple,
  }
}

interface OAuthProviderButtonsProps {
  oidcProviders: OidcProviderOption[]
  googleLoginEnabled: boolean
  appleLoginEnabled: boolean
  /** Action verb shown on the buttons, e.g. "Sign in" or "Sign up". */
  verb: string
  pendingProvider: string | null
  /** Render the "or" divider below the buttons (only when a form follows). */
  showDivider: boolean
  onOidc: (providerId: string) => void
  onGoogle: () => void
  onApple: () => void
}

export function OAuthProviderButtons({
  oidcProviders,
  googleLoginEnabled,
  appleLoginEnabled,
  verb,
  pendingProvider,
  showDivider,
  onOidc,
  onGoogle,
  onApple,
}: OAuthProviderButtonsProps) {
  if (oidcProviders.length === 0 && !googleLoginEnabled && !appleLoginEnabled)
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
