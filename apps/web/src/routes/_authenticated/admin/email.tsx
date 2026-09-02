import { useEffect, useState } from "react"
import {
  createFileRoute,
  useRouter,
  type ErrorComponentProps,
} from "@tanstack/react-router"
import { ShieldBan, ShieldCheck } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { Pill } from "@/components/ui/pill"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { EmailDeliveriesTable, formatRelative } from "./-shared"

type BounceRow = Awaited<
  ReturnType<typeof trpc.admin.listEmailBounces.query>
>[number]
type DeliveryRow = Awaited<
  ReturnType<typeof trpc.admin.listEmailDeliveries.query>
>[number]

// Client-side mirrors of admin.ts EMAIL_DELIVERY_KINDS / _STATUSES — the
// router module is server-only (it imports lib/email, which statically
// imports the db), so the literals can't be imported here.
const DELIVERY_KINDS = [
  `digest`,
  `team_invite`,
  `support_reply`,
  `support_confirmation`,
  `widget_resolution`,
  `password_reset`,
  `email_verification`,
  `contact`,
  `notification`,
] as const
const DELIVERY_STATUSES = [
  `queued`,
  `sent`,
  `failed`,
  `suppressed`,
  `bounced`,
  `complained`,
] as const

type DeliveryKind = (typeof DELIVERY_KINDS)[number]
type DeliveryStatus = (typeof DELIVERY_STATUSES)[number]

const PAGE_SIZE = 50

// Mirror of sendEmail's send-time suppression predicate (isEmailSuppressed
// in lib/email.ts): complaints and Permanent bounces never get another send.
function isAutoBlocked(row: BounceRow): boolean {
  return row.kind === `complaint` || row.bounceType === `Permanent`
}

export const Route = createFileRoute(`/_authenticated/admin/email`)({
  // Filters stay OPTIONAL in the search schema so plain links to /admin/email
  // (the admin nav) don't have to carry them; invalid values just drop. The
  // active tab lives in the URL too (?tab=bounces; absent = sent), matching
  // the inbox ?tab= pattern.
  validateSearch: (
    search: Record<string, unknown>
  ): {
    tab?: `bounces`
    kind?: DeliveryKind
    status?: DeliveryStatus
    q?: string
  } => ({
    tab: search.tab === `bounces` ? `bounces` : undefined,
    kind: DELIVERY_KINDS.includes(search.kind as DeliveryKind)
      ? (search.kind as DeliveryKind)
      : undefined,
    status: DELIVERY_STATUSES.includes(search.status as DeliveryStatus)
      ? (search.status as DeliveryStatus)
      : undefined,
    q:
      typeof search.q === `string` && search.q.trim()
        ? search.q.trim()
        : undefined,
  }),
  loaderDeps: ({ search }) => ({
    kind: search.kind,
    status: search.status,
    q: search.q,
  }),
  loader: async ({ deps }) => {
    const [bounces, deliveries] = await Promise.all([
      trpc.admin.listEmailBounces.query(),
      trpc.admin.listEmailDeliveries.query({ ...deps, limit: PAGE_SIZE }),
    ])
    return { bounces, deliveries }
  },
  component: AdminEmail,
  // Without a route-level boundary a failing loader escapes to the router's
  // global fallback, which replaces the ENTIRE app — on the forced dark theme
  // that reads as a black screen with no way back (EXP-373). Keep the failure
  // inside the admin shell so the nav survives and the reason is readable.
  errorComponent: EmailError,
})

function EmailError({ error }: ErrorComponentProps) {
  const router = useRouter()
  return (
    <div className="mx-auto max-w-5xl space-y-4 p-4 md:p-6">
      <h1 className="text-2xl font-bold">Email health</h1>
      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Couldn’t load email data</CardTitle>
          <CardDescription className="text-xs">
            {error instanceof Error ? error.message : String(error)}
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Button
            size="sm"
            variant="secondary"
            onClick={() => void router.invalidate()}
          >
            Retry
          </Button>
        </CardContent>
      </Card>
    </div>
  )
}

// All outbound email (the email_deliveries ledger) + bounce/complaint
// feedback. The sent list is the "what is going out to whom" overview
// (EXP-461); the bounce section is the suppression worklist (EXP-227).
function AdminEmail() {
  const router = useRouter()
  const { bounces, deliveries } = Route.useLoaderData()
  const search = Route.useSearch()
  const navigate = Route.useNavigate()
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState<string | null>(null)

  // Cursor pages fetched past the loader's first page. Reset whenever the
  // loader re-runs (filter change / invalidate) — `deliveries` is a fresh
  // reference each load.
  const [extraPages, setExtraPages] = useState<DeliveryRow[][]>([])
  const [loadingMore, setLoadingMore] = useState(false)
  useEffect(() => {
    setExtraPages([])
  }, [deliveries])

  // Local draft of the search box, committed to the URL on Enter/blur so the
  // loader doesn't re-run per keystroke.
  const [qDraft, setQDraft] = useState(search.q ?? ``)
  useEffect(() => {
    setQDraft(search.q ?? ``)
  }, [search.q])

  const allRows = [...deliveries, ...extraPages.flat()]
  const lastPage = extraPages.length
    ? extraPages[extraPages.length - 1]
    : deliveries
  const hasMore = lastPage.length === PAGE_SIZE

  const commitSearch = (value: string) => {
    const q = value.trim() || undefined
    if (q === search.q) return
    void navigate({ search: (prev) => ({ ...prev, q }) })
  }

  const loadMore = async () => {
    const last = allRows[allRows.length - 1]
    if (!last) return
    setError(null)
    setLoadingMore(true)
    try {
      const page = await trpc.admin.listEmailDeliveries.query({
        kind: search.kind,
        status: search.status,
        q: search.q,
        limit: PAGE_SIZE,
        // The µs-exact (created_at, id) cursor — round-tripped verbatim so
        // rows sharing a millisecond across a page boundary aren't skipped.
        cursor: { createdAt: last.cursorCreatedAt, id: last.id },
      })
      setExtraPages((pages) => [...pages, page])
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoadingMore(false)
    }
  }

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
          Every outbound email the instance sends, plus the addresses that
          bounced or complained.
        </p>
      </div>

      {error && (
        <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {error}
        </div>
      )}

      <Tabs
        value={search.tab ?? `sent`}
        onValueChange={(value) =>
          void navigate({
            search: (prev) => ({
              ...prev,
              tab: value === `bounces` ? `bounces` : undefined,
            }),
          })
        }
      >
        <TabsList>
          <TabsTrigger value="sent">Sent emails</TabsTrigger>
          <TabsTrigger value="bounces">
            Bounces &amp; complaints
            {bounces.length > 0 && (
              <Pill>{bounces.length}</Pill>
            )}
          </TabsTrigger>
        </TabsList>

        <TabsContent value="sent" className="mt-3 space-y-3">
          <p className="text-sm text-muted-foreground">
            The outbound-email ledger, newest first — what is going out to whom.
          </p>
          <div className="flex flex-wrap items-center gap-2">
            <Select
              value={search.kind ?? `all`}
              onValueChange={(value) =>
                void navigate({
                  search: (prev) => ({
                    ...prev,
                    kind: value === `all` ? undefined : (value as DeliveryKind),
                  }),
                })
              }
            >
              <SelectTrigger size="sm" className="w-[190px]">
                <SelectValue placeholder="Kind" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">All kinds</SelectItem>
                {DELIVERY_KINDS.map((kind) => (
                  <SelectItem key={kind} value={kind}>
                    {kind}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Select
              value={search.status ?? `all`}
              onValueChange={(value) =>
                void navigate({
                  search: (prev) => ({
                    ...prev,
                    status:
                      value === `all` ? undefined : (value as DeliveryStatus),
                  }),
                })
              }
            >
              <SelectTrigger size="sm" className="w-[150px]">
                <SelectValue placeholder="Status" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">All statuses</SelectItem>
                {DELIVERY_STATUSES.map((status) => (
                  <SelectItem key={status} value={status}>
                    {status}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Input
              value={qDraft}
              onChange={(e) => setQDraft(e.target.value)}
              onBlur={() => commitSearch(qDraft)}
              onKeyDown={(e) => {
                if (e.key === `Enter`) commitSearch(qDraft)
              }}
              placeholder="Search recipient…"
              className="h-8 max-w-xs"
            />
          </div>

          {allRows.length === 0 ? (
            <div className="rounded-md border px-4 py-8 text-center text-sm text-muted-foreground">
              No emails match.
            </div>
          ) : (
            <>
              <EmailDeliveriesTable rows={allRows} showSubject />
              {hasMore && (
                <Button
                  variant="outline"
                  size="sm"
                  disabled={loadingMore}
                  onClick={() => void loadMore()}
                >
                  {loadingMore ? `Loading…` : `Load more`}
                </Button>
              )}
            </>
          )}
        </TabsContent>

        {/* Bounced/complaining recipient addresses reported by SES (via the
            SNS feedback webhook). Complaints and Permanent bounces are refused
            automatically at send time (sendEmail's suppression check — the
            "Auto-blocked" badge mirrors exactly that predicate) AND auto-added
            to the SES account-level suppression list by the webhook. The
            button covers only what automation deliberately leaves alone:
            Transient (soft) bounces an operator wants blocked anyway. */}
        <TabsContent value="bounces" className="mt-3 space-y-3">
          <p className="text-sm text-muted-foreground">
            Addresses that bounced or complained, reported by SES. Hard bounces
            and complaints are blocked automatically, at send time and via the
            SES account-level suppression list. The button suppresses a soft
            (Transient) bounce manually.
          </p>

          {bounces.length === 0 ? (
            <div className="rounded-md border px-4 py-8 text-center text-sm text-muted-foreground">
              No bounces or complaints reported. (Requires the SES feedback
              webhook: an SNS topic subscribed to{` `}
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
                    <div className="text-sm font-medium truncate">
                      {row.email}
                    </div>
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
                    <Pill className="text-destructive">{row.kind}</Pill>
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
                      <Pill>
                        <ShieldCheck className="h-3 w-3" />
                        Auto-blocked
                      </Pill>
                    )}
                    {row.suppressedAt ? (
                      <Pill>
                        <ShieldCheck className="h-3 w-3" />
                        Suppressed in SES
                      </Pill>
                    ) : (
                      // Auto-blocked rows need no operator action (send-time
                      // check + webhook auto-suppression); the button covers
                      // only soft bounces.
                      !isAutoBlocked(row) && (
                        <Button
                          variant="outline"
                          size="sm"
                          disabled={busy !== null}
                          onClick={() => handleSuppress(row)}
                        >
                          <ShieldBan className="h-4 w-4" />
                          Suppress in SES
                        </Button>
                      )
                    )}
                  </div>
                </div>
              ))}
            </div>
          )}
        </TabsContent>
      </Tabs>
    </div>
  )
}
