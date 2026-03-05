import * as React from "react"
import { createFileRoute, Link } from "@tanstack/react-router"
import { authClient } from "@/lib/auth-client"
import { useState } from "react"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { ExponentialLogo } from "@/components/exponential-logo"

export const Route = createFileRoute(`/auth/register`)({
  component: RegisterPage,
  ssr: false,
  validateSearch: (search: Record<string, unknown>) => ({
    redirect: (search.redirect as string) || undefined,
  }),
})

function RegisterPage() {
  const { redirect: redirectTo } = Route.useSearch()
  const [name, setName] = useState(``)
  const [email, setEmail] = useState(``)
  const [password, setPassword] = useState(``)
  const [isLoading, setIsLoading] = useState(false)
  const [error, setError] = useState(``)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setIsLoading(true)
    setError(``)

    try {
      const { error } = await authClient.signUp.email(
        { name, email, password },
        {
          onSuccess: () => {
            window.location.href = redirectTo || `/`
          },
        }
      )

      if (error) {
        setError(error.message || `Registration failed`)
      }
    } catch {
      setError(`An unexpected error occurred`)
    } finally {
      setIsLoading(false)
    }
  }

  return (
    <div className="min-h-screen flex items-center justify-center bg-background px-4">
      <div className="w-full max-w-sm space-y-6">
        <div className="flex items-center justify-center gap-2">
          <ExponentialLogo variant="light" size={32} />
          <span className="text-xl font-semibold">Exponential</span>
        </div>
      <Card>
        <CardHeader className="text-center">
          <CardTitle className="text-2xl">Create an account</CardTitle>
          <CardDescription>Enter your details to get started</CardDescription>
        </CardHeader>
        <CardContent>
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
              <Input
                id="password"
                type="password"
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

          <p className="mt-4 text-center text-sm text-muted-foreground">
            Already have an account?{` `}
            <Link
              to="/auth/login"
              search={{ redirect: redirectTo }}
              className="text-primary underline-offset-4 hover:underline"
            >
              Sign in
            </Link>
          </p>
        </CardContent>
      </Card>
      </div>
    </div>
  )
}
