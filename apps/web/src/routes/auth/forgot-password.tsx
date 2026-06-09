import * as React from "react"
import { createFileRoute, Link } from "@tanstack/react-router"
import { useState } from "react"
import { authClient } from "@/lib/auth/client"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { AuthFormShell } from "@/components/auth-form-shell"

export const Route = createFileRoute(`/auth/forgot-password`)({
  component: ForgotPasswordPage,
  ssr: false,
})

function ForgotPasswordPage() {
  const [email, setEmail] = useState(``)
  const [isLoading, setIsLoading] = useState(false)
  const [sent, setSent] = useState(false)
  const [error, setError] = useState(``)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setIsLoading(true)
    setError(``)
    try {
      await authClient.requestPasswordReset({
        email,
        redirectTo: `${window.location.origin}/auth/reset-password`,
      })
      // Always show the same confirmation — whether the email exists is none
      // of the requester's business (no account enumeration).
      setSent(true)
    } catch {
      setError(`Couldn't send the reset email. Try again in a moment.`)
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
          a reset link is on its way. Check your inbox (and spam folder) — the
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
          {error && <p className="text-sm text-destructive">{error}</p>}
          <Button type="submit" className="w-full" disabled={isLoading}>
            {isLoading ? `Sending…` : `Send reset link`}
          </Button>
        </form>
      )}
    </AuthFormShell>
  )
}
