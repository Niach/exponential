import * as React from "react"
import { createFileRoute, Link } from "@tanstack/react-router"
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

export const Route = createFileRoute(`/auth/login`)({
  component: LoginPage,
  ssr: false,
  loader: () => getAuthConfig(),
  // Pass unknown params through — an in-flight OAuth authorize query
  // (client_id, redirect_uri, ...) must survive router normalization.
  validateSearch: (
    search: Record<string, unknown>
  ): { redirect?: string } & Record<string, unknown> => ({
    ...search,
    redirect: sanitizeRedirectPath(search.redirect),
  }),
})

// Signup and login are ONE merged page (EXP-188): a sign-in/create-account
// mode toggle, shown only when password auth is on AND public sign-up is
// open (signupEnabled from buildAuthConfig). /auth/register is a pure
// redirect here.
function LoginPage() {
  const { redirect: redirectTo } = Route.useSearch()
  const {
    passwordEnabled,
    signupEnabled,
    passwordResetEnabled,
    oidcProviders,
    googleLoginEnabled,
    appleLoginEnabled,
  } = Route.useLoaderData()
  const [oauthResumeUrl] = useState(captureOAuthResumeUrl)
  // oauthResumeUrl is the separately-guarded MCP OAuth resume path (an
  // internally composed relative URL) — only the router-provided redirect
  // needs the same-origin-path clamp (re-applied here as sink-side defense).
  const destination = oauthResumeUrl || sanitizeRedirectPath(redirectTo)
  const [mode, setMode] = useState<`signin` | `signup`>(`signin`)
  const isSignup = mode === `signup` && passwordEnabled && signupEnabled
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

  const toggleMode = (next: `signin` | `signup`) => {
    setMode(next)
    setError(``)
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setIsLoading(true)
    setError(``)

    try {
      const onSuccess = async () => {
        await authClient.getSession()
        window.location.href = destination || `/`
      }
      const { error } = isSignup
        ? await authClient.signUp.email({ name, email, password }, { onSuccess })
        : await authClient.signIn.email({ email, password }, { onSuccess })

      if (error) {
        setError(
          authErrorMessage(
            error,
            isSignup
              ? `Couldn't create your account. Try again.`
              : `Couldn't sign you in. Try again.`
          )
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
      title={isSignup ? `Create an account` : `Sign in`}
      description={
        isSignup
          ? `Enter your details to get started`
          : passwordEnabled
            ? `Enter your email and password to continue`
            : `Sign in with your account`
      }
      footer={
        passwordEnabled && signupEnabled ? (
          <p className="mt-4 text-center text-sm text-muted-foreground">
            {isSignup ? (
              <>
                Already have an account?{` `}
                <Button
                  type="button"
                  variant="link"
                  className="h-auto p-0 text-primary underline-offset-4 hover:underline"
                  onClick={() => toggleMode(`signin`)}
                >
                  Sign in
                </Button>
              </>
            ) : (
              <>
                Don&apos;t have an account?{` `}
                <Button
                  type="button"
                  variant="link"
                  className="h-auto p-0 text-primary underline-offset-4 hover:underline"
                  onClick={() => toggleMode(`signup`)}
                >
                  Create one
                </Button>
              </>
            )}
          </p>
        ) : null
      }
    >
      <div className="space-y-4">
        <OAuthProviderButtons
          oidcProviders={oidcProviders}
          googleLoginEnabled={googleLoginEnabled}
          appleLoginEnabled={appleLoginEnabled}
          verb={isSignup ? `Sign up` : `Sign in`}
          pendingProvider={pendingProvider}
          showDivider={passwordEnabled}
          onOidc={signInWithOidc}
          onGoogle={signInWithGoogle}
          onApple={signInWithApple}
        />

        {passwordEnabled && (
          <form onSubmit={handleSubmit} className="space-y-4">
            {isSignup && (
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
            )}
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
                {!isSignup && passwordResetEnabled && (
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
                autoComplete={isSignup ? `new-password` : `current-password`}
                required
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="Password"
              />
            </div>

            {error && <p className="text-sm text-destructive">{error}</p>}

            <Button type="submit" className="w-full" disabled={isLoading}>
              {isSignup
                ? isLoading
                  ? `Creating account...`
                  : `Create account`
                : isLoading
                  ? `Signing in...`
                  : `Sign in`}
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
