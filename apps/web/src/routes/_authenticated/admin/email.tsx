import { useState } from "react"
import { createFileRoute, useRouter } from "@tanstack/react-router"
import { ShieldBan, ShieldCheck } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { formatRelative } from "./-shared"

type BounceRow = Awaited<
  ReturnType<typeof trpc.admin.listEmailBounces.query>
>[number]

// Mirror of sendEmail's send-time suppression predicate (isEmailSuppressed
// in lib/email.ts): complaints and Permanent bounces never get another send.
function isAutoBlocked(row: BounceRow): boolean {
  return row.kind === `complaint` || row.bounceType === `Permanent`
}

export const Route = createFileRoute(`/_authenticated/admin/email`)({
  loader: async () => {
    const bounces = await trpc.admin.listEmailBounces.query()
    return { bounces }
  },
  component: AdminEmail,
})

// Bounced/complaining recipient addresses reported by SES (via the SNS
// feedback webhook). Complaints and Permanent bounces are refused
// automatically at send time (sendEmail's suppression check) — the
// "Auto-blocked" badge mirrors exactly that predicate. The button is the
// manual escalation on top: push the address onto the SES ACCOUNT-LEVEL
// suppression list (e.g. a Transient-bouncing address you want blocked
// anyway).
function AdminEmail() {
  const router = useRouter()
  const { bounces } = Route.useLoaderData()
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState<string | null>(null)

  const handleSuppress = async (row: BounceRow) => {
    setError(null)
    setBusy(row.id)
    try {
      await trpc.admin.suppressEmailBounce.mutate({ bounceId: row.id })
      await router.invalidate()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(null)
    }
  }

  return (
    <div className="mx-auto max-w-5xl space-y-4 p-4 md:p-6">
      <div>
        <h1 className="text-2xl font-bold">Email health</h1>
        <p className="text-sm text-muted-foreground">
          Addresses that bounced or complained, reported by SES. Hard bounces
          and complaints are blocked automatically at send time; suppressing
          additionally puts the address on the SES account-level suppression
          list.
        </p>
      </div>

      {error && (
        <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {error}
        </div>
      )}

      {bounces.length === 0 ? (
        <div className="rounded-md border px-4 py-8 text-center text-sm text-muted-foreground">
          No bounces or complaints reported. (Requires the SES feedback
          webhook — an SNS topic subscribed to{` `}
          <code className="text-xs">/api/webhooks/ses</code>.)
        </div>
      ) : (
        <div className="rounded-md border">
          <div className="hidden md:grid grid-cols-[1fr_110px_150px_60px_100px_150px] items-center gap-3 border-b px-4 py-2 text-xs font-medium text-muted-foreground">
            <div>Address</div>
            <div>Kind</div>
            <div>Type</div>
            <div>Events</div>
            <div>Last event</div>
            <div />
          </div>
          {bounces.map((row: BounceRow) => (
            <div
              key={row.id}
              className="flex flex-col md:grid md:grid-cols-[1fr_110px_150px_60px_100px_150px] md:items-center gap-2 md:gap-3 border-b px-4 py-3 last:border-b-0"
            >
              <div className="min-w-0">
                <div className="text-sm font-medium truncate">{row.email}</div>
                {row.diagnostic && (
                  <div
                    className="text-xs text-muted-foreground truncate"
                    title={row.diagnostic}
                  >
                    {row.diagnostic}
                  </div>
                )}
              </div>
              <div>
                <Badge variant="destructive" className="text-xs">
                  {row.kind}
                </Badge>
              </div>
              <div className="text-xs text-muted-foreground truncate">
                {[row.bounceType, row.bounceSubType]
                  .filter(Boolean)
                  .join(` / `) || `—`}
              </div>
              <div className="text-sm tabular-nums">{row.eventCount}</div>
              <div
                className="text-xs text-muted-foreground"
                title={new Date(row.lastEventAt).toLocaleString()}
              >
                {formatRelative(row.lastEventAt)}
              </div>
              <div className="flex flex-col items-start gap-1 md:items-end md:justify-self-end">
                {isAutoBlocked(row) && (
                  <Badge variant="secondary" className="text-xs">
                    <ShieldCheck className="h-3 w-3" />
                    Auto-blocked
                  </Badge>
                )}
                {row.suppressedAt ? (
                  <Badge variant="secondary" className="text-xs">
                    <ShieldCheck className="h-3 w-3" />
                    Suppressed in SES
                  </Badge>
                ) : (
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={busy !== null}
                    onClick={() => handleSuppress(row)}
                  >
                    <ShieldBan className="h-4 w-4" />
                    Suppress in SES
                  </Button>
                )}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
