import * as React from "react"
import { createFileRoute, Link, redirect } from "@tanstack/react-router"
import { authClient } from "@/lib/auth/client"
import { getAuthConfig } from "@/lib/auth/config"
import { captureOAuthResumeUrl } from "@/lib/auth/oauth-resume"
import { sanitizeRedirectPath } from "@/lib/auth/safe-redirect"
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

export const Route = createFileRoute(`/auth/register`)({
  component: RegisterPage,
  ssr: false,
  loader: async ({ location }) => {
    const config = await getAuthConfig()
    if (!config.passwordEnabled) {
      // Forward the full search — an in-flight OAuth authorize query
      // (client_id, redirect_uri, ...) must survive the hop to login.
      throw redirect({
        to: `/auth/login`,
        search: location.search as Record<string, unknown>,
      })
    }
    return config
  },
  // Pass unknown params through — an in-flight OAuth authorize query
  // (client_id, redirect_uri, ...) must survive router normalization.
  validateSearch: (
    search: Record<string, unknown>
  ): { redirect?: string } & Record<string, unknown> => ({
    ...search,
    redirect: sanitizeRedirectPath(search.redirect),
  }),
})

function RegisterPage() {
  const { redirect: redirectTo } = Route.useSearch()
  const { oidcProviders, googleLoginEnabled, appleLoginEnabled } =
    Route.useLoaderData()
  const [oauthResumeUrl] = useState(captureOAuthResumeUrl)
  // oauthResumeUrl is the separately-guarded MCP OAuth resume path (an
  // internally composed relative URL) — only the router-provided redirect
  // needs the same-origin-path clamp (re-applied here as sink-side defense).
  const destination = oauthResumeUrl || sanitizeRedirectPath(redirectTo)
  const [name, setName] = useState(``)
  const [email, setEmail] = useState(``)
  const [password, setPassword] = useState(``)
  const [isLoading, setIsLoading] = useState(false)
  const {
    pendingProvider,
    error,
    setError,
    signInWithOidc,
    signInWithGoogle,
    signInWithApple,
  } = useOAuthSignIn(destination)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setIsLoading(true)
    setError(``)

    try {
      const { error } = await authClient.signUp.email(
        { name, email, password },
        {
          onSuccess: async () => {
            await authClient.getSession()
            window.location.href = destination || `/`
          },
        }
      )

      if (error) {
        setError(
          authErrorMessage(error, `Couldn't create your account. Try again.`)
        )
      }
    } catch {
      setError(`An unexpected error occurred`)
    } finally {
      setIsLoading(false)
    }
  }

  return (
    <AuthFormShell
      title="Create an account"
      description="Enter your details to get started"
      footer={
        <p className="mt-4 text-center text-sm text-muted-foreground">
          Already have an account?{` `}
          <Link
            to="/auth/login"
            search={(current) => current}
            className="text-primary underline-offset-4 hover:underline"
          >
            Sign in
          </Link>
        </p>
      }
    >
      <div className="space-y-4">
        <OAuthProviderButtons
          oidcProviders={oidcProviders}
          googleLoginEnabled={googleLoginEnabled}
          appleLoginEnabled={appleLoginEnabled}
          verb="Sign up"
          pendingProvider={pendingProvider}
          showDivider
          onOidc={signInWithOidc}
          onGoogle={signInWithGoogle}
          onApple={signInWithApple}
        />

        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="name">Name</Label>
            <Input
              id="name"
              type="text"
              autoComplete="name"
              required
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="Your name"
            />
          </div>
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
            <PasswordInput
              id="password"
              autoComplete="new-password"
              required
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder="Password"
            />
          </div>

          {error && <p className="text-sm text-destructive">{error}</p>}

          <Button type="submit" className="w-full" disabled={isLoading}>
            {isLoading ? `Creating account...` : `Create account`}
          </Button>
        </form>
      </div>
    </AuthFormShell>
  )
}
