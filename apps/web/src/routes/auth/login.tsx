import * as React from "react"
import { createFileRoute, Link } from "@tanstack/react-router"
import { authClient } from "@/lib/auth/client"
import { type AuthConfig, getAuthConfig } from "@/lib/auth/config"
import { captureOAuthResumeUrl } from "@/lib/auth/oauth-resume"
import { withFirstTouchParams } from "@/lib/conversion/first-touch"
import { sanitizeRedirectPath } from "@/lib/auth/safe-redirect"
import { authErrorMessage } from "@/lib/auth/error-messages"
import { oauthErrorMessage } from "@/lib/deep-link"
import { useState } from "react"
import { conceptIcon } from "@/lib/icons.generated"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { AuthFormShell } from "@/components/auth-form-shell"
import { PasswordInput } from "@/components/password-input"
import {
  OAuthProviderButtons,
  useOAuthSignIn,
} from "@/components/oauth-provider-buttons"

// EXP-632: the server-fn RPC is the only thing standing between a visitor and
// the login form, and it is also the call most exposed to a broken hop — a
// proxy (or Node 26's unix-socket fetch) that strips the response headers makes
// TanStack Start throw `expected content-type header to be set` instead of
// returning, and the router renders an error boundary over the sole way into
// the app. `/api/auth-config` is a plain route handler serving the SAME
// buildAuthConfig() shape, so retrying there keeps login reachable.
async function loadAuthConfig(): Promise<AuthConfig> {
  try {
    return await getAuthConfig()
  } catch {
    const res = await fetch(`/api/auth-config`)
    if (!res.ok) {
      throw new Error(`Could not load auth config (${res.status})`)
    }
    return (await res.json()) as AuthConfig
  }
}

// The native browser handoff (EXP-857): a desktop/mobile app that cannot run
// a ceremony itself opens this page with the return route as `redirect`
// (/api/mobile-oauth-start?provider=browser sets the state cookie first), and
// the completed login deep-links back into the app from there.
const NATIVE_RETURN_PATH = `/api/mobile-oauth-return`

// Passkey ceremony outcomes the user caused on purpose — nothing to explain.
const PASSKEY_SILENT_CODES = new Set([
  `AUTH_CANCELLED`,
  `ERROR_CEREMONY_ABORTED`,
  `NotAllowedError`,
])

export const Route = createFileRoute(`/auth/login`)({
  component: LoginPage,
  ssr: false,
  loader: () => loadAuthConfig(),
  // Pass unknown params through — an in-flight OAuth authorize query
  // (client_id, redirect_uri, ...) must survive router normalization.
  validateSearch: (
    search: Record<string, unknown>
  ): { redirect?: string; error?: string } & Record<string, unknown> => ({
    ...search,
    redirect: sanitizeRedirectPath(search.redirect),
    // The native OAuth hop bounces failures here as `?error=<reason>`
    // (REV2-53) — kept as a string so the page can render it.
    error: typeof search.error === `string` ? search.error : undefined,
  }),
})

type Step = `methods` | `email` | `code`
type EmailMode = `otp` | `password`

// ONE screen for signing in AND signing up (EXP-188, reworded in EXP-857):
// every button says "Continue with …" because the same tap creates the
// account when it does not exist yet. The stack is Apple, Google, the
// configured OIDC providers, email and passkey; "Continue with email" expands
// in place into the one-time-code flow (or the password form on an instance
// without mail, with its create-account toggle).
function LoginPage() {
  const { redirect: redirectTo, error: errorParam } = Route.useSearch()
  const {
    passwordEnabled,
    signupEnabled,
    passwordResetEnabled,
    oidcProviders,
    googleLoginEnabled,
    appleLoginEnabled,
    emailOtpEnabled,
    passkeyEnabled,
  } = Route.useLoaderData()
  const [oauthResumeUrl] = useState(captureOAuthResumeUrl)
  // oauthResumeUrl is the separately-guarded MCP OAuth resume path (an
  // internally composed relative URL) — only the router-provided redirect
  // needs the same-origin-path clamp (re-applied here as sink-side defense).
  const destination = oauthResumeUrl || sanitizeRedirectPath(redirectTo)
  const nativeHandoff = destination === NATIVE_RETURN_PATH
  const emailAvailable = emailOtpEnabled || passwordEnabled
  const [step, setStep] = useState<Step>(`methods`)
  const [emailMode, setEmailMode] = useState<EmailMode>(
    emailOtpEnabled ? `otp` : `password`
  )
  const [mode, setMode] = useState<`signin` | `signup`>(`signin`)
  const isSignup =
    mode === `signup` && emailMode === `password` && passwordEnabled && signupEnabled
  const [name, setName] = useState(``)
  const [email, setEmail] = useState(``)
  const [password, setPassword] = useState(``)
  const [code, setCode] = useState(``)
  const [sentTo, setSentTo] = useState(``)
  const [isLoading, setIsLoading] = useState(false)
  const [passkeyPending, setPasskeyPending] = useState(false)
  const {
    pendingProvider,
    error,
    setError,
    signInWithOidc,
    signInWithGoogle,
    signInWithApple,
  } = useOAuthSignIn(destination)

  // A failed native OAuth hop lands back here with `?error=<reason>`
  // (REV2-53). Seed the shared error state so the page explains itself instead
  // of showing a blank form; any later sign-in attempt clears it as usual.
  React.useEffect(() => {
    if (errorParam) setError(oauthErrorMessage(errorParam))
  }, [errorParam, setError])

  const finishLogin = React.useCallback(async () => {
    await authClient.getSession()
    // Full-page navigation wipes the in-memory first-touch capture —
    // forward any ref/utm params so the next load can claim them
    // (cookieless attribution, EXP-362; no-op without params).
    window.location.href = withFirstTouchParams(destination || `/`)
  }, [destination])

  // Passkey conditional UI (EXP-857): browsers that support it list the
  // user's passkeys in the email field's autofill, so a returning user
  // never has to pick a method. The pending request is aborted by any later
  // explicit ceremony; its rejection is noise, not an error.
  React.useEffect(() => {
    if (!passkeyEnabled || typeof window === `undefined`) return
    const credential = (
      window as Window & {
        PublicKeyCredential?: {
          isConditionalMediationAvailable?: () => Promise<boolean>
        }
      }
    ).PublicKeyCredential
    if (!credential?.isConditionalMediationAvailable) return
    let cancelled = false
    void credential
      .isConditionalMediationAvailable()
      .then(async (available) => {
        if (!available || cancelled) return
        const { error: passkeyError } = await authClient.signIn.passkey({
          autoFill: true,
        })
        if (!passkeyError && !cancelled) await finishLogin()
      })
      .catch(() => {})
    return () => {
      cancelled = true
    }
  }, [passkeyEnabled, finishLogin])

  const openEmailStep = () => {
    setError(``)
    setStep(`email`)
  }

  const backToMethods = () => {
    setError(``)
    setStep(`methods`)
    setCode(``)
  }

  const toggleMode = (next: `signin` | `signup`) => {
    setMode(next)
    setError(``)
  }

  const switchEmailMode = (next: EmailMode) => {
    setEmailMode(next)
    setStep(`email`)
    setCode(``)
    setError(``)
  }

  const sendCode = async (target: string) => {
    setIsLoading(true)
    setError(``)
    try {
      const { error: sendError } = await authClient.emailOtp.sendVerificationOtp(
        { email: target, type: `sign-in` }
      )
      if (sendError) {
        setError(authErrorMessage(sendError, `Couldn't send the code. Try again.`))
        return
      }
      setSentTo(target)
      setCode(``)
      setStep(`code`)
    } catch {
      setError(`An unexpected error occurred`)
    } finally {
      setIsLoading(false)
    }
  }

  const handleEmailSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (emailMode === `otp`) {
      await sendCode(email.trim())
      return
    }
    setIsLoading(true)
    setError(``)
    try {
      const { error: authError } = isSignup
        ? await authClient.signUp.email(
            { name, email, password },
            { onSuccess: finishLogin }
          )
        : await authClient.signIn.email(
            { email, password },
            { onSuccess: finishLogin }
          )
      if (authError) {
        setError(
          authErrorMessage(
            authError,
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

  const handleCodeSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setIsLoading(true)
    setError(``)
    try {
      const { error: otpError } = await authClient.signIn.emailOtp(
        { email: sentTo, otp: code.trim() },
        { onSuccess: finishLogin }
      )
      if (otpError) {
        setError(authErrorMessage(otpError, `Couldn't check the code. Try again.`))
      }
    } catch {
      setError(`An unexpected error occurred`)
    } finally {
      setIsLoading(false)
    }
  }

  const signInWithPasskey = async () => {
    setPasskeyPending(true)
    setError(``)
    try {
      const { error: passkeyError } = await authClient.signIn.passkey()
      if (!passkeyError) {
        await finishLogin()
        return
      }
      const code = (passkeyError as { code?: string }).code
      if (code && PASSKEY_SILENT_CODES.has(code)) {
        return
      }
      setError(
        authErrorMessage(passkeyError, `Couldn't sign you in with a passkey.`)
      )
    } catch {
      setError(`An unexpected error occurred`)
    } finally {
      setPasskeyPending(false)
    }
  }

  const busy = isLoading || passkeyPending || pendingProvider !== null
  const PasskeyIcon = conceptIcon(`auth-passkey`)
  const MailIcon = conceptIcon(`ui-mail`)

  const title =
    step === `code`
      ? `Check your email`
      : isSignup
        ? `Create an account`
        : `Continue to Exponential`
  const description =
    step === `code`
      ? `We sent a 6-digit code to ${sentTo}.`
      : nativeHandoff
        ? `You'll be sent back to the app once you continue.`
        : isSignup
          ? `Enter your details to get started`
          : `Sign in or create your account`

  const footer =
    step === `email` && emailMode === `password` && passwordEnabled && signupEnabled ? (
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
              Continue with your email
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

  return (
    <AuthFormShell title={title} description={description} footer={footer}>
      <div className="space-y-4">
        {step === `methods` && (
          <>
            <OAuthProviderButtons
              oidcProviders={oidcProviders}
              googleLoginEnabled={googleLoginEnabled}
              appleLoginEnabled={appleLoginEnabled}
              verb="Continue"
              pendingProvider={pendingProvider}
              showDivider={false}
              onOidc={signInWithOidc}
              onGoogle={signInWithGoogle}
              onApple={signInWithApple}
            />

            {emailAvailable && (
              <Button
                type="button"
                variant="outline"
                className="w-full"
                disabled={busy}
                onClick={openEmailStep}
              >
                <MailIcon />
                Continue with email
              </Button>
            )}

            {passkeyEnabled && (
              <Button
                type="button"
                variant="outline"
                className="w-full"
                disabled={busy}
                onClick={signInWithPasskey}
              >
                <PasskeyIcon />
                {passkeyPending ? `Waiting for your passkey…` : `Login with passkey`}
              </Button>
            )}

            {error && <p className="text-sm text-destructive">{error}</p>}
          </>
        )}

        {step === `email` && (
          <form onSubmit={handleEmailSubmit} className="space-y-4">
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
                autoComplete={passkeyEnabled ? `username webauthn` : `email`}
                autoFocus
                required
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                placeholder="you@example.com"
              />
            </div>
            {emailMode === `password` && (
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
            )}

            {error && <p className="text-sm text-destructive">{error}</p>}

            <Button type="submit" className="w-full" disabled={busy}>
              {emailMode === `otp`
                ? isLoading
                  ? `Sending code…`
                  : `Send code`
                : isSignup
                  ? isLoading
                    ? `Creating account…`
                    : `Create account`
                  : isLoading
                    ? `Checking…`
                    : `Continue`}
            </Button>

            <div className="flex flex-wrap justify-center gap-x-4 gap-y-1 text-xs text-muted-foreground">
              <Button
                type="button"
                variant="link"
                className="h-auto p-0 text-xs text-muted-foreground underline-offset-4 hover:underline"
                onClick={backToMethods}
              >
                All options
              </Button>
              {emailOtpEnabled && passwordEnabled && (
                <Button
                  type="button"
                  variant="link"
                  className="h-auto p-0 text-xs text-muted-foreground underline-offset-4 hover:underline"
                  onClick={() =>
                    switchEmailMode(emailMode === `otp` ? `password` : `otp`)
                  }
                >
                  {emailMode === `otp` ? `Use a password instead` : `Use a code instead`}
                </Button>
              )}
            </div>
          </form>
        )}

        {step === `code` && (
          <form onSubmit={handleCodeSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="code">Code</Label>
              <Input
                id="code"
                type="text"
                inputMode="numeric"
                pattern="[0-9]*"
                autoComplete="one-time-code"
                autoFocus
                required
                maxLength={6}
                value={code}
                onChange={(e) => setCode(e.target.value.replace(/\D/g, ``))}
                placeholder="123456"
                className="text-center text-lg tracking-[0.4em]"
              />
            </div>

            {error && <p className="text-sm text-destructive">{error}</p>}

            <Button
              type="submit"
              className="w-full"
              disabled={busy || code.trim().length < 6}
            >
              {isLoading ? `Checking…` : `Continue`}
            </Button>

            <div className="flex flex-wrap justify-center gap-x-4 gap-y-1 text-xs text-muted-foreground">
              <Button
                type="button"
                variant="link"
                className="h-auto p-0 text-xs text-muted-foreground underline-offset-4 hover:underline"
                disabled={busy}
                onClick={() => void sendCode(sentTo)}
              >
                Resend code
              </Button>
              <Button
                type="button"
                variant="link"
                className="h-auto p-0 text-xs text-muted-foreground underline-offset-4 hover:underline"
                onClick={() => {
                  setError(``)
                  setCode(``)
                  setStep(`email`)
                }}
              >
                Use a different email
              </Button>
            </div>
          </form>
        )}
      </div>
    </AuthFormShell>
  )
}
