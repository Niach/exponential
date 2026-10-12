import * as React from "react"
import { createFileRoute, Link } from "@tanstack/react-router"
import { useState } from "react"
import { authClient } from "@/lib/auth/client"
import { Button, FEEDBACK_WORDS, Input, Label, AuthFormShell } from "@exp/ui"
import { failedAction } from "@/lib/action-error"
import { pageTitle } from "@/lib/page-title"

export const Route = createFileRoute(`/auth/forgot-password`)({
  head: () => ({ meta: [{ title: pageTitle(`Reset your password`) }] }),
  component: ForgotPasswordPage,
  ssr: false,
})

function ForgotPasswordPage() {
  const [email, setEmail] = useState(``)
  const [isLoading, setIsLoading] = useState(false)
  const [sent, setSent] = useState(false)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setIsLoading(true)
    try {
      // The auth client reports failures via the returned `error`, not by
      // throwing. The server answers unknown emails with the same success as
      // known ones (no account enumeration), so an error here means the
      // request itself failed (rate limit, outage): a toast, not the field.
      const { error: requestError } = await authClient.requestPasswordReset({
        email,
        redirectTo: `${window.location.origin}/auth/reset-password`,
      })
      if (requestError) {
        failedAction(`send`, `the reset email`, requestError, {
          description: `Try again in a moment.`,
        })
        return
      }
      setSent(true)
    } catch (err) {
      failedAction(`send`, `the reset email`, err)
    } finally {
      setIsLoading(false)
    }
  }

  return (
    <AuthFormShell
      title="Reset your password"
      description="Enter your account's email and we'll send you a reset link"
      footer={
        <p className="mt-4 text-center text-sm text-muted-foreground">
          Remembered it?{` `}
          <Link
            to="/auth/login"
            search={{ redirect: undefined }}
            className="text-primary underline-offset-4 hover:underline"
          >
            Sign in
          </Link>
        </p>
      }
    >
      {sent ? (
        <p className="text-sm text-muted-foreground">
          If an account exists for <span className="text-foreground">{email}</span>,
          a reset link is on its way. Check your inbox (and spam folder). The
          link expires in one hour.
        </p>
      ) : (
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
          <Button
            type="submit"
            className="w-full"
            pending={isLoading}
            pendingLabel={FEEDBACK_WORDS.pending.send}
          >
            Send reset link
          </Button>
        </form>
      )}
    </AuthFormShell>
  )
}
