import { useEffect, useState } from "react"
import {
  createFileRoute,
  Link,
  useRouter,
} from "@tanstack/react-router"
import { ArrowLeft, Calendar, ExternalLink } from "lucide-react"
import { authClient } from "@/lib/auth-client"
import { trpc } from "@/lib/trpc-client"
import { getAuthConfig } from "@/lib/auth-config"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"

export const Route = createFileRoute(`/_authenticated/account/integrations`)({
  loader: async () => {
    const [authConfig, status] = await Promise.all([
      getAuthConfig(),
      trpc.integrations.google.status.query(),
    ])
    return { authConfig, status }
  },
  component: AccountIntegrations,
})

function AccountIntegrations() {
  const router = useRouter()
  const { authConfig, status } = Route.useLoaderData()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const [backfillStatus, setBackfillStatus] = useState<string | null>(null)

  useEffect(() => {
    const params = new URLSearchParams(window.location.search)
    const errorParam = params.get(`error`)
    const backfillParam = params.get(`backfill`)
    let cleanup = false

    if (errorParam) {
      setError(`Connection failed: ${errorParam}`)
      params.delete(`error`)
      cleanup = true
    }

    if (backfillParam === `1` && !errorParam) {
      params.delete(`backfill`)
      cleanup = true
      setBackfillStatus(`Syncing existing issues to your calendar…`)
      void trpc.integrations.google.backfill
        .mutate()
        .then((res) => {
          setBackfillStatus(
            res.scheduled > 0
              ? `Synced ${res.scheduled} existing issue${res.scheduled === 1 ? `` : `s`} to your calendar.`
              : `No existing issues with due dates to sync.`
          )
          // Backfill marks calendar.events scope as granted; re-fetch loader
          // data so the status flips from not-connected to connected.
          void router.invalidate()
        })
        .catch((err) => {
          setBackfillStatus(
            `Backfill failed: ${err instanceof Error ? err.message : String(err)}`
          )
        })
    }

    if (cleanup) {
      const newSearch = params.toString()
      window.history.replaceState(
        {},
        ``,
        window.location.pathname + (newSearch ? `?${newSearch}` : ``)
      )
    }
  }, [status.connected])

  const handleConnect = async () => {
    setBusy(true)
    setError(null)
    try {
      await authClient.linkSocial({
        provider: `google`,
        callbackURL: `/account/integrations?backfill=1`,
        scopes: [`https://www.googleapis.com/auth/calendar.events`],
      })
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
      setBusy(false)
    }
  }

  const handleDisconnect = async () => {
    setBusy(true)
    setError(null)
    try {
      await trpc.integrations.google.disconnect.mutate()
      await router.invalidate()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="mx-auto max-w-2xl space-y-6 p-6">
      <div>
        <Button asChild variant="ghost" size="sm" className="-ml-2 mb-2">
          <Link to="/">
            <ArrowLeft className="h-4 w-4" />
            Back
          </Link>
        </Button>
        <h1 className="text-2xl font-bold">Integrations</h1>
        <p className="text-sm text-muted-foreground">
          Connect external services to your account.
        </p>
      </div>

      <Card>
        <CardHeader>
          <div className="flex items-start gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-md border bg-muted">
              <Calendar className="h-5 w-5" />
            </div>
            <div className="flex-1">
              <CardTitle>Google Calendar</CardTitle>
              <CardDescription>
                Sync issue due dates as events in your primary Google Calendar.
                Events appear when an issue has a due date and disappear when
                it&apos;s done, cancelled, or deleted.
              </CardDescription>
            </div>
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          {!authConfig.googleCalendarEnabled ? (
            <div className="text-sm text-muted-foreground">
              Google Calendar is not configured on this server. Set
              <code className="mx-1 rounded bg-muted px-1 py-0.5 text-xs">
                GOOGLE_CLIENT_ID
              </code>
              and
              <code className="mx-1 rounded bg-muted px-1 py-0.5 text-xs">
                GOOGLE_CLIENT_SECRET
              </code>
              to enable it.
            </div>
          ) : status.connected ? (
            <>
              <div className="flex items-center gap-2 text-sm">
                <div className="h-2 w-2 rounded-full bg-green-500" />
                <span>
                  Connected
                  {status.connectedAt
                    ? ` · since ${new Date(status.connectedAt).toLocaleDateString()}`
                    : ``}
                </span>
              </div>
              <Button
                variant="outline"
                onClick={handleDisconnect}
                disabled={busy}
              >
                Disconnect
              </Button>
            </>
          ) : (
            <Button onClick={handleConnect} disabled={busy}>
              <ExternalLink className="h-4 w-4" />
              Connect Google Calendar
            </Button>
          )}

          {error && (
            <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
              {error}
            </div>
          )}
          {backfillStatus && (
            <div className="rounded-md border border-border bg-muted/40 px-3 py-2 text-sm text-muted-foreground">
              {backfillStatus}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  )
}
