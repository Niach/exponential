import * as React from "react"
import { createFileRoute, Link } from "@tanstack/react-router"
import { authClient } from "@/lib/auth/client"
import { getAuthConfig } from "@/lib/auth/config"
import { authErrorMessage } from "@/lib/auth/error-messages"
import { useState } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { AuthFormShell } from "@/components/auth-form-shell"
import { PasswordInput } from "@/components/password-input"
import {
  OAuthProviderButtons,
  useOAuthSignIn,
} from "@/components/oauth-provider-buttons"

export const Route = createFileRoute(`/auth/login`)({
  component: LoginPage,
  ssr: false,
  loader: () => getAuthConfig(),
  validateSearch: (search: Record<string, unknown>) => ({
    redirect: (search.redirect as string) || undefined,
  }),
})

function LoginPage() {
  const { redirect: redirectTo } = Route.useSearch()
  const {
    passwordEnabled,
    passwordResetEnabled,
    oidcProviders,
    googleLoginEnabled,
  } = Route.useLoaderData()
  const [email, setEmail] = useState(``)
  const [password, setPassword] = useState(``)
  const [isLoading, setIsLoading] = useState(false)
  const { pendingProvider, error, setError, signInWithOidc, signInWithGoogle } =
    useOAuthSignIn(redirectTo)

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
        setError(authErrorMessage(error, `Couldn't sign you in. Try again.`))
      }
    } catch {
      setError(`An unexpected error occurred`)
    } finally {
      setIsLoading(false)
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
        <OAuthProviderButtons
          oidcProviders={oidcProviders}
          googleLoginEnabled={googleLoginEnabled}
          verb="Sign in"
          pendingProvider={pendingProvider}
          showDivider={passwordEnabled}
          onOidc={signInWithOidc}
          onGoogle={signInWithGoogle}
        />

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
              <div className="flex items-center justify-between">
                <Label htmlFor="password">Password</Label>
                {passwordResetEnabled && (
                  <Link
                    to="/auth/forgot-password"
                    className="text-xs text-muted-foreground underline-offset-4 hover:text-foreground hover:underline"
                  >
                    Forgot password?
                  </Link>
                )}
              </div>
              <PasswordInput
                id="password"
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
