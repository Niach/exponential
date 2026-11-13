import { useEffect, useMemo, useRef } from "react"
import { CORE_CATALOG_ID, DEFAULT_THEME_ID, type NestedNode } from "@exponential_app/eui"
import { ExponentialSurface, useSurface, type HostPlugin, type SurfaceActionEvent, type SurfaceState } from "@exponential_app/eui-react"
import { exponentialUiIcons } from "@/lib/exponential-ui-icons"

// #EXP-1804: a genUI tree the app rebuilds from live data (the CI run page,
// the CI runner section), painted by the core catalog like every other
// surface ×4. `tree` replaces the painted tree whenever it changes; `data`
// seeds the data model (bound fields); `onAction` gets every event.

export function CiSurface({
  surfaceId,
  tree,
  data,
  onAction,
  surfaceRef,
}: {
  surfaceId: string
  tree: NestedNode
  data?: Record<string, unknown>
  onAction?: (event: SurfaceActionEvent) => void
  /** The surface state, for a host writing back into the data model. */
  surfaceRef?: { current: SurfaceState | null }
}) {
  const surface = useSurface({ surfaceId, catalogId: CORE_CATALOG_ID, initial: tree, data })
  if (surfaceRef) surfaceRef.current = surface
  // `useSurface` hands back a fresh object per render; its setters are
  // stable. The first tree is the `initial` one.
  const { setNested } = surface
  const painted = useRef(tree)
  useEffect(() => {
    if (painted.current === tree) return
    painted.current = tree
    setNested(tree)
  }, [setNested, tree])
  const actionRef = useRef(onAction)
  actionRef.current = onAction
  const host = useMemo<HostPlugin>(
    () => ({
      icons: exponentialUiIcons,
      onAction: (event) => {
        actionRef.current?.(event)
      },
    }),
    []
  )
  return <ExponentialSurface surface={surface} host={host} theme={DEFAULT_THEME_ID} mode="dark" />
}
