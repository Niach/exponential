import { useState, useEffect } from "react"
import { createFileRoute, Link, useNavigate } from "@tanstack/react-router"
import { authClient } from "@/lib/auth-client"
import { trpc } from "@/lib/trpc-client"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Users, Loader2, AlertCircle, CheckCircle2 } from "lucide-react"

export const Route = createFileRoute(`/invite/$token`)({
  component: InviteAcceptPage,
  ssr: false,
})

function InviteAcceptPage() {
  const { token } = Route.useParams()
  const navigate = useNavigate()
  const { data: session } = authClient.useSession()
  const [accepting, setAccepting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState(false)

  const [invite, setInvite] = useState<{
    workspaceName: string
    role: string
    acceptedAt: Date | null
    expiresAt: Date
  } | null>(null)
  const [loading, setLoading] = useState(true)

  // Fetch invite details on mount
  useEffect(() => {
    trpc.workspaceInvites.getByToken
      .query({ token })
      .then(({ invite }) => {
        setInvite({
          ...invite,
          acceptedAt: invite.acceptedAt ? new Date(invite.acceptedAt) : null,
          expiresAt: new Date(invite.expiresAt),
        })
        setLoading(false)
      })
      .catch((err) => {
        setError(err.message || `Invalid or expired invite link`)
        setLoading(false)
      })
  }, [token])

  const handleAccept = async () => {
    setAccepting(true)
    setError(null)
    try {
      const { workspace } = await trpc.workspaceInvites.accept.mutate({
        token,
      })
      setSuccess(true)
      setTimeout(() => {
        navigate({
          to: `/w/$workspaceSlug`,
          params: { workspaceSlug: workspace.slug },
        })
      }, 1500)
    } catch (err: unknown) {
      const message =
        err instanceof Error ? err.message : `Failed to accept invite`
      setError(message)
      setAccepting(false)
    }
  }

  if (loading) {
    return (
      <div className="flex min-h-screen items-center justify-center">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    )
  }

  const isExpired = invite && invite.expiresAt < new Date()
  const isUsed = invite && invite.acceptedAt
  const isLoggedIn = !!session?.user

  return (
    <div className="flex min-h-screen items-center justify-center p-4">
      <Card className="w-full max-w-md">
        <CardHeader className="text-center">
          <div className="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-full bg-primary/10">
            <Users className="h-6 w-6 text-primary" />
          </div>
          <CardTitle>
            {success
              ? `Welcome!`
              : error && !invite
                ? `Invalid Invite`
                : `Workspace Invite`}
          </CardTitle>
          <CardDescription>
            {success
              ? `You've joined the workspace. Redirecting...`
              : error && !invite
                ? error
                : invite
                  ? `You've been invited to join`
                  : ``}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          {success && (
            <div className="flex items-center justify-center gap-2 text-sm text-green-500">
              <CheckCircle2 className="h-4 w-4" />
              Successfully joined workspace
            </div>
          )}

          {invite && !success && (
            <>
              <div className="rounded-lg border p-4 text-center">
                <div className="text-lg font-semibold">
                  {invite.workspaceName}
                </div>
                <div className="mt-1 text-sm text-muted-foreground">
                  Role: {invite.role}
                </div>
              </div>

              {isExpired ? (
                <div className="flex items-center gap-2 text-sm text-destructive">
                  <AlertCircle className="h-4 w-4" />
                  This invite has expired
                </div>
              ) : isUsed ? (
                <div className="flex items-center gap-2 text-sm text-muted-foreground">
                  <AlertCircle className="h-4 w-4" />
                  This invite has already been used
                </div>
              ) : isLoggedIn ? (
                <>
                  {error && (
                    <div className="flex items-center gap-2 text-sm text-destructive">
                      <AlertCircle className="h-4 w-4" />
                      {error}
                    </div>
                  )}
                  <Button
                    className="w-full"
                    onClick={handleAccept}
                    disabled={accepting}
                  >
                    {accepting && (
                      <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                    )}
                    Accept Invite
                  </Button>
                </>
              ) : (
                <div className="space-y-2">
                  <Button className="w-full" asChild>
                    <Link
                      to="/auth/login"
                      search={{ redirect: `/invite/${token}` }}
                    >
                      Sign in to accept
                    </Link>
                  </Button>
                  <Button variant="outline" className="w-full" asChild>
                    <Link
                      to="/auth/register"
                      search={{ redirect: `/invite/${token}` }}
                    >
                      Create account
                    </Link>
                  </Button>
                </div>
              )}
            </>
          )}

          {!invite && error && (
            <Button
              variant="outline"
              className="w-full"
              onClick={() =>
                navigate({
                  to: `/w/$workspaceSlug`,
                  params: { workspaceSlug: `default` },
                })
              }
            >
              Go to your workspace
            </Button>
          )}
        </CardContent>
      </Card>
    </div>
  )
}
