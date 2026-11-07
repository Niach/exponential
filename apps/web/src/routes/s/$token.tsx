import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react"
import { createFileRoute } from "@tanstack/react-router"
import { ExponentialHost, type AppPackage } from "@exponential_app/eui"
import { HostSurface } from "@exponential_app/eui-react"
import { APP_SHARE_COPY } from "@exp/domain-contract/app-share"
import { surfaceState } from "@exp/domain-contract/apps"
import { Alert, AlertDescription, SkeletonRows } from "@exp/ui"
import { AppSurfaceBoundary } from "@/components/apps/app-surface-boundary"
import { exponentialUiIcons } from "@/lib/exponential-ui-icons"
import { pageTitle } from "@/lib/page-title"

// VAPP-241 (eApps P1, fixtures/app-share.json): a guest's page, outside the
// auth shell: a one-template package from `/api/share/<token>` on a plain
// host: `records.upsert` posts the submission, `records:` follows the
// server's snapshot. No session, no synced data, no `exp:` source. #VAPP-251:
// a package the host refuses or cannot paint = the dead link's page.
export const Route = createFileRoute(`/s/$token`)({
  head: () => ({ meta: [{ title: pageTitle(`Exponential`) }, { name: `robots`, content: `noindex` }, { name: `referrer`, content: `no-referrer` }] }),
  component: GuestPage,
  ssr: false,
})

interface Page {
  mode: `submit` | `view`
  title: string
  package: AppPackage
  value?: unknown
}

const SURFACE = `share`

function GuestSurface({ token, page, honeypot, onDone, onError }: { token: string; page: Page; honeypot: () => string; onDone: () => void; onError: (message: string) => void }) {
  const host = useMemo(
    () =>
      new ExponentialHost({
        packages: [page.package],
        // `exp:` = a RecordForm's member list: a guest has none (so no Author).
        sources: { records: (_source, emit) => emit(page.value ?? { rows: [], count: 0 }), exp: (_source, emit) => emit(null) },
        functions: {
          "records.upsert": async (args) => {
            const response = await fetch(`/api/share/${token}`, { method: `POST`, headers: { "content-type": `application/json` }, body: JSON.stringify({ data: args.data, website: honeypot() }) })
            const body = (await response.json().catch(() => ({}))) as { error?: string }
            if (response.ok) onDone()
            else onError(body.error ?? APP_SHARE_COPY.closed)
            return {}
          },
        },
      }),
    [token, page, honeypot, onDone, onError]
  )
  const plugin = useMemo(() => ({ icons: exponentialUiIcons }), [])
  const [appliedHost, setAppliedHost] = useState<ExponentialHost | null>(null)
  useEffect(() => {
    host.applyTemplate({ surfaceId: SURFACE, templateId: `main`, packageId: page.package.id })
    setAppliedHost(host)
    return () => host.close()
  }, [host, page.package.id])
  const hasSurface = useSyncExternalStore(useCallback((cb: () => void) => host.subscribe(cb), [host]), () => host.surface(SURFACE) !== undefined)
  if (surfaceState(true, appliedHost === host, hasSurface) === `needsUpdate`) throw new Error(`the guest page of ${page.package.id} made no surface`)
  return <HostSurface host={host} surfaceId={SURFACE} plugin={plugin} theme="exponential" mode="dark" readOnly={page.mode === `view`} />
}

const gone = (
  <Alert>
    <AlertDescription>{APP_SHARE_COPY.gone}</AlertDescription>
  </Alert>
)

function GuestPage() {
  const { token } = Route.useParams()
  const [page, setPage] = useState<Page | null | undefined>(undefined)
  const [sent, setSent] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const trap = useRef<HTMLInputElement>(null)
  useEffect(() => {
    let live = true
    void fetch(`/api/share/${token}`)
      .then((r) => (r.ok ? (r.json() as Promise<Page>) : null))
      .catch(() => null)
      .then((next) => {
        if (live) setPage(next)
      })
    return () => {
      live = false
    }
  }, [token])
  const honeypot = useMemo(() => () => trap.current?.value ?? ``, [])
  const done = useMemo(() => () => setSent(true), [])
  return (
    <main className="mx-auto flex min-h-dvh w-full max-w-2xl flex-col gap-4 p-4">
      {page === undefined ? <SkeletonRows /> : null}
      {page === null ? gone : null}
      {sent ? (
        <Alert>
          <AlertDescription>{APP_SHARE_COPY.thanks}</AlertDescription>
        </Alert>
      ) : null}
      {error ? (
        <Alert variant="destructive">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      ) : null}
      {page && !sent ? (
        <AppSurfaceBoundary resetKey={token} fallback={gone}>
          <GuestSurface token={token} page={page} honeypot={honeypot} onDone={done} onError={setError} />
        </AppSurfaceBoundary>
      ) : null}
      <input ref={trap} name="website" tabIndex={-1} autoComplete="off" aria-hidden="true" className="absolute -left-[9999px] size-px opacity-0" />
    </main>
  )
}
