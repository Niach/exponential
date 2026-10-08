import { useMemo, useState } from "react"
import { createFileRoute } from "@tanstack/react-router"
import { CORE_CATALOG_ID, BUILTIN_THEME_IDS, DEFAULT_THEME_ID } from "@exponential-at/ui"
import type { NestedNode } from "@exponential-at/ui"
import kitchenSink from "@exponential-at/ui/fixtures/kitchen-sink.json"
import { ExponentialSurface, useSurface, type HostPlugin } from "@exponential-at/ui-react"
import { exponentialUiIcons } from "@/lib/exponential-ui-icons"
import { pageTitle } from "@/lib/page-title"

// VAPP-87: the Exponential UI kitchen sink rendered by the React renderer
// inside the app (view `exponential-ui-kitchen-sink`, the web reference shot
// every painter is compared against). `?theme=` picks a built-in,
// `?mode=light|dark`, `?width=N` forces the surface width, `?rtl=1` flips
// it. The host stub echoes every input edit back after 150 ms and logs
// actions to the console — the app's real host plugin is VAPP-91.
type SinkSearch = { theme?: string; mode?: `light` | `dark`; width?: number; rtl?: boolean }

const flag = (v: unknown) => v === 1 || v === `1` || v === true || v === `true`

export const Route = createFileRoute(`/_authenticated/exponential-ui-kitchen-sink`)({
  head: () => ({ meta: [{ title: pageTitle(`Exponential UI kitchen sink`) }] }),
  ssr: false,
  validateSearch: (search: Record<string, unknown>): SinkSearch => {
    const width = Number(search.width)
    return {
      theme: typeof search.theme === `string` && BUILTIN_THEME_IDS.includes(search.theme) ? search.theme : undefined,
      mode: search.mode === `light` ? `light` : undefined,
      width: Number.isFinite(width) && width > 0 ? width : undefined,
      rtl: flag(search.rtl) || undefined,
    }
  },
  component: KitchenSinkPage,
})

function KitchenSinkPage() {
  const { theme, mode, width, rtl } = Route.useSearch()
  const surface = useSurface({ surfaceId: `kitchen-sink`, catalogId: CORE_CATALOG_ID, initial: kitchenSink as unknown as NestedNode, data: { draft: { title: `` } } })
  const [echo, setEcho] = useState(``)
  const host = useMemo<HostPlugin>(
    () => ({
      icons: exponentialUiIcons,
      onAction: (e) => {
        console.info(`[exponential-ui] action`, e.name, e.componentId, e.context, e.payload)
        return new Promise((r) => setTimeout(r, 150))
      },
      onInput: (e) =>
        new Promise<void>((resolve) =>
          setTimeout(() => {
            if (e.path) surface.setData(e.path, e.value)
            setEcho(String(e.value ?? ``))
            resolve()
          }, 150)
        ),
    }),
    [surface]
  )
  return (
    <div className="min-h-dvh w-full overflow-y-auto bg-background" data-testid="exponential-ui-kitchen-sink">
      <ExponentialSurface surface={surface} host={host} theme={theme ?? DEFAULT_THEME_ID} mode={mode ?? `dark`} direction={rtl ? `rtl` : `ltr`} width={width} />
      <p className="px-4 py-2 font-mono text-xs text-muted-foreground" data-testid="host-echo">
        host: {echo}
      </p>
    </div>
  )
}
