import { useEffect, useMemo, useRef, useState } from "react"
import { createFileRoute } from "@tanstack/react-router"
import { ExponentialHost, type AppPackage } from "@exponential_app/eui"
import { HostSurface } from "@exponential_app/eui-react"
import { APP_SHARE_COPY } from "@exp/domain-contract/app-share"
import { Alert, SkeletonRows } from "@exp/ui"
import { exponentialUiIcons } from "@/lib/exponential-ui-icons"
import { pageTitle } from "@/lib/page-title"

// VAPP-241 (eApps P1, fixtures/app-share.json): a guest's page, outside the
// auth shell: a one-template package from `/api/share/<token>` on a plain
// host: `records.upsert` posts the submission, `records:` follows the
// server's snapshot. No session, no synced data, no `exp:` source.
export const Route = createFileRoute(`/s/$token`)({
  head: () => ({ meta: [{ title: pageTitle(`Exponential`) }, { name: `robots`, content: `noindex` }] }),
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
        sources: { records: (_source, emit) => emit(page.value ?? { rows: [], count: 0 }) },
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
  useEffect(() => {
    host.receive({ version: `v0.9`, applyTemplate: { surfaceId: SURFACE, templateId: `main`, packageId: page.package.id } })
    return () => host.close()
  }, [host, page.package.id])
  return <HostSurface host={host} surfaceId={SURFACE} plugin={plugin} theme="exponential" mode="dark" readOnly={page.mode === `view`} />
}

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
      {page === null ? <Alert>{APP_SHARE_COPY.gone}</Alert> : null}
      {sent ? <Alert>{APP_SHARE_COPY.thanks}</Alert> : null}
      {error ? <Alert variant="destructive">{error}</Alert> : null}
      {page && !sent ? <GuestSurface token={token} page={page} honeypot={honeypot} onDone={done} onError={setError} /> : null}
      <input ref={trap} name="website" tabIndex={-1} autoComplete="off" aria-hidden="true" className="absolute -left-[9999px] size-px opacity-0" />
    </main>
  )
}
