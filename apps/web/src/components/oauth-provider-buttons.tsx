import { useState } from "react"
import { authClient } from "@/lib/auth/client"
import { Button } from "@/components/ui/button"

export const GOOGLE_PROVIDER_KEY = `__google__`

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

  return { pendingProvider, error, setError, signInWithOidc, signInWithGoogle }
}

interface OAuthProviderButtonsProps {
  oidcProviders: OidcProviderOption[]
  googleLoginEnabled: boolean
  /** Action verb shown on the buttons, e.g. "Sign in" or "Sign up". */
  verb: string
  pendingProvider: string | null
  /** Render the "or" divider below the buttons (only when a form follows). */
  showDivider: boolean
  onOidc: (providerId: string) => void
  onGoogle: () => void
}

export function OAuthProviderButtons({
  oidcProviders,
  googleLoginEnabled,
  verb,
  pendingProvider,
  showDivider,
  onOidc,
  onGoogle,
}: OAuthProviderButtonsProps) {
  if (oidcProviders.length === 0 && !googleLoginEnabled) return null

  return (
    <>
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
