import * as React from "react"
import { createFileRoute, Link } from "@tanstack/react-router"
import { useState } from "react"
import { authClient } from "@/lib/auth/client"
import { authErrorMessage } from "@/lib/auth/error-messages"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { AuthFormShell } from "@/components/auth-form-shell"
import { PasswordInput } from "@/components/password-input"

export const Route = createFileRoute(`/auth/reset-password`)({
  component: ResetPasswordPage,
  ssr: false,
  // Better Auth appends ?token=… to the redirectTo link in the reset email
  // (?error=… when the token is invalid/expired).
  validateSearch: (search: Record<string, unknown>) => ({
    token: (search.token as string) || undefined,
    error: (search.error as string) || undefined,
  }),
})

function ResetPasswordPage() {
  const { token, error: tokenError } = Route.useSearch()
  const [password, setPassword] = useState(``)
  const [confirm, setConfirm] = useState(``)
  const [isLoading, setIsLoading] = useState(false)
  const [done, setDone] = useState(false)
  const [error, setError] = useState(``)

  const invalidLink = !token || Boolean(tokenError)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (password !== confirm) {
      setError(`Passwords don't match.`)
      return
    }
    setIsLoading(true)
    setError(``)
    try {
      const { error } = await authClient.resetPassword({
        newPassword: password,
        token: token!,
      })
      if (error) {
        setError(authErrorMessage(error, `Couldn't reset the password.`))
      } else {
        setDone(true)
      }
    } catch {
      setError(`An unexpected error occurred`)
    } finally {
      setIsLoading(false)
    }
  }

  return (
    <AuthFormShell
      title="Choose a new password"
      description={
        invalidLink
          ? `This reset link is invalid or has expired`
          : `Set a new password for your account`
      }
      footer={
        <p className="mt-4 text-center text-sm text-muted-foreground">
          <Link
            to="/auth/login"
            search={{ redirect: undefined }}
            className="text-primary underline-offset-4 hover:underline"
          >
            Back to sign in
          </Link>
        </p>
      }
    >
      {invalidLink ? (
        <p className="text-sm text-muted-foreground">
          Request a fresh link from the{` `}
          <Link
            to="/auth/forgot-password"
            className="text-primary underline-offset-4 hover:underline"
          >
            forgot password
          </Link>
          {` `}page.
        </p>
      ) : done ? (
        <p className="text-sm text-muted-foreground">
          Your password has been updated. You can sign in with it now.
        </p>
      ) : (
        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="password">New password</Label>
            <PasswordInput
              id="password"
              autoComplete="new-password"
              required
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder="At least 8 characters"
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="confirm">Confirm password</Label>
            <PasswordInput
              id="confirm"
              autoComplete="new-password"
              required
              value={confirm}
              onChange={(e) => setConfirm(e.target.value)}
              placeholder="Repeat the password"
            />
          </div>
          {error && <p className="text-sm text-destructive">{error}</p>}
          <Button type="submit" className="w-full" disabled={isLoading}>
            {isLoading ? `Saving…` : `Set new password`}
          </Button>
        </form>
      )}
    </AuthFormShell>
  )
}
