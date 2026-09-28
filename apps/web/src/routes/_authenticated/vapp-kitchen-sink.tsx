import { createFileRoute } from "@tanstack/react-router"
import { kitchenSink } from "@/components/vapp-spike/fixture"
import { VappSurface } from "@/components/vapp-spike/render"

// VAPP-4 spike (throwaway): the real-CSS reference rendering of the vApp
// kitchen sink. `?geometry=1` = fixed fake measure (diffable against the
// taffy core's frames), `?width=390` forces the surface width, `?rtl=1`
// flips the surface direction.
type VappSearch = { geometry?: boolean; width?: number; rtl?: boolean }

const flag = (v: unknown) => v === 1 || v === `1` || v === true || v === `true`

export const Route = createFileRoute(`/_authenticated/vapp-kitchen-sink`)({
  ssr: false,
  validateSearch: (search: Record<string, unknown>): VappSearch => {
    const width = Number(search.width)
    return {
      geometry: flag(search.geometry) || undefined,
      width: Number.isFinite(width) && width > 0 ? width : undefined,
      rtl: flag(search.rtl) || undefined,
    }
  },
  component: VappKitchenSinkPage,
})

function VappKitchenSinkPage() {
  const { geometry, width, rtl } = Route.useSearch()
  return (
    <div className="min-h-dvh w-full overflow-y-auto bg-background">
      <VappSurface tree={kitchenSink} geometry={geometry} width={width} rtl={rtl} />
    </div>
  )
}
