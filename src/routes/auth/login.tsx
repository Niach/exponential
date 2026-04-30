import * as React from "react"
import { createFileRoute, Link } from "@tanstack/react-router"
import { authClient } from "@/lib/auth-client"
import { getAuthConfig } from "@/lib/auth-config"
import { useState } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { AuthFormShell } from "@/components/auth-form-shell"

export const Route = createFileRoute(`/auth/login`)({
  component: LoginPage,
  ssr: false,
  loader: () => getAuthConfig(),
  validateSearch: (search: Record<string, unknown>) => ({
    redirect: (search.redirect as string) || undefined,
  }),
})

const GOOGLE_PROVIDER_KEY = `__google__`

function LoginPage() {
  const { redirect: redirectTo } = Route.useSearch()
  const { passwordEnabled, oidcProviders, googleLoginEnabled } =
    Route.useLoaderData()
  const [email, setEmail] = useState(``)
  const [password, setPassword] = useState(``)
  const [isLoading, setIsLoading] = useState(false)
  const [pendingProvider, setPendingProvider] = useState<string | null>(null)
  const [error, setError] = useState(``)

  const hasOauthOptions = oidcProviders.length > 0 || googleLoginEnabled

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setIsLoading(true)
    setError(``)

    try {
      const { error } = await authClient.signIn.email(
        { email, password },
        {
          onSuccess: async () => {
            await authClient.getSession()
            window.location.href = redirectTo || `/`
          },
        }
      )

      if (error) {
        setError(error.message || `Authentication failed`)
      }
    } catch {
      setError(`An unexpected error occurred`)
    } finally {
      setIsLoading(false)
    }
  }

  const handleOidcSignIn = async (providerId: string) => {
    setPendingProvider(providerId)
    setError(``)
    try {
      await authClient.signIn.oauth2({
        providerId,
        callbackURL: redirectTo || `/`,
      })
    } catch {
      setError(`An unexpected error occurred`)
      setPendingProvider(null)
    }
  }

  const handleGoogleSignIn = async () => {
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

  return (
    <AuthFormShell
      title="Sign in"
      description={
        passwordEnabled
          ? `Enter your email and password to continue`
          : `Sign in with your account`
      }
      footer={
        passwordEnabled ? (
          <p className="mt-4 text-center text-sm text-muted-foreground">
            Don&apos;t have an account?{` `}
            <Link
              to="/auth/register"
              search={{ redirect: redirectTo }}
              className="text-primary underline-offset-4 hover:underline"
            >
              Register
            </Link>
          </p>
        ) : null
      }
    >
      <div className="space-y-4">
        {oidcProviders.map((provider) => (
          <Button
            key={provider.id}
            type="button"
            variant="outline"
            className="w-full"
            disabled={pendingProvider !== null}
            onClick={() => handleOidcSignIn(provider.id)}
          >
            {pendingProvider === provider.id
              ? `Redirecting...`
              : `Sign in with ${provider.name}`}
          </Button>
        ))}

        {googleLoginEnabled && (
          <Button
            type="button"
            variant="outline"
            className="w-full"
            disabled={pendingProvider !== null}
            onClick={handleGoogleSignIn}
          >
            {pendingProvider === GOOGLE_PROVIDER_KEY
              ? `Redirecting...`
              : `Sign in with Google`}
          </Button>
        )}

        {hasOauthOptions && passwordEnabled && (
          <div className="relative">
            <div className="absolute inset-0 flex items-center">
              <span className="w-full border-t" />
            </div>
            <div className="relative flex justify-center text-xs uppercase">
              <span className="bg-card px-2 text-muted-foreground">or</span>
            </div>
          </div>
        )}

        {passwordEnabled && (
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="email">Email</Label>
              <Input
                id="email"
                type="email"
                autoComplete="email"
                required
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                placeholder="you@example.com"
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="password">Password</Label>
              <Input
                id="password"
                type="password"
                autoComplete="current-password"
                required
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="Password"
              />
            </div>

            {error && <p className="text-sm text-destructive">{error}</p>}

            <Button type="submit" className="w-full" disabled={isLoading}>
              {isLoading ? `Signing in...` : `Sign in`}
            </Button>
          </form>
        )}

        {!passwordEnabled && error && (
          <p className="text-sm text-destructive">{error}</p>
        )}
      </div>
    </AuthFormShell>
  )
}
