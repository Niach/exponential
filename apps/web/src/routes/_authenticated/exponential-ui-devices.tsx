import { useEffect, useMemo } from "react"
import { createFileRoute, useRouter } from "@tanstack/react-router"
import { BUILTIN_THEME_IDS, DEFAULT_THEME_ID } from "@exponential-at/ui"
import { HostSurface } from "@exponential-at/ui-react"
import { appReactExtension } from "@exp/ui"
import { ConsentCard, HostBanner, createAppHost, createConsentGate } from "@/lib/exponential-ui-host"
import { exponentialUiIcons } from "@/lib/exponential-ui-icons"
import { pageTitle } from "@/lib/page-title"

// VAPP-91: the Devices template (the app's declarative package, VAPP-83's
// screen) rendered through the SAME public host API a third party uses: the
// app host (`createAppHost`: functions, the `exp:` bindings over Electric,
// policy + consent, the app extension) applies the template and
// `<HostSurface>` paints it. `?theme=` picks a built-in, `?mode=light|dark`.
// VAPP-83 puts it beside the native Devices page and on the other clients.
type DevicesSearch = { theme?: string; mode?: `light` | `dark` }

const EXTENSIONS = [appReactExtension]

export const Route = createFileRoute(`/_authenticated/exponential-ui-devices`)({
  head: () => ({ meta: [{ title: pageTitle(`Devices · Exponential UI`) }] }),
  ssr: false,
  validateSearch: (search: Record<string, unknown>): DevicesSearch => ({
    theme: typeof search.theme === `string` && BUILTIN_THEME_IDS.includes(search.theme) ? search.theme : undefined,
    mode: search.mode === `light` ? `light` : undefined,
  }),
  component: DevicesTemplatePage,
})

function DevicesTemplatePage() {
  const { theme, mode } = Route.useSearch()
  const router = useRouter()
  const consent = useMemo(createConsentGate, [])
  const host = useMemo(() => createAppHost({ consent, navigate: (href) => router.history.push(href) }), [consent, router])
  const plugin = useMemo(() => ({ icons: exponentialUiIcons }), [])
  useEffect(() => {
    host.receive({ version: `v0.9`, applyTemplate: { surfaceId: `devices`, templateId: `devices` } })
    return () => host.close()
  }, [host])
  return (
    <div className="h-full min-h-dvh w-full overflow-y-auto bg-background" data-testid="exponential-ui-devices">
      <div className="mx-auto flex w-full max-w-3xl flex-col gap-3 px-4 py-4">
        <HostBanner host={host} />
        <HostSurface host={host} surfaceId="devices" plugin={plugin} extensions={EXTENSIONS} theme={theme ?? DEFAULT_THEME_ID} mode={mode ?? `dark`} />
      </div>
      <ConsentCard gate={consent} />
    </div>
  )
}
