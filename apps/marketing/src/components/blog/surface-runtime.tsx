/* The lazy half of LiveSurface: the Exponential UI React renderer painting
   one A2UI tree in the Exponential theme's dark mode. Loaded by the island
   after hydration, never by the prerender. */
import { useLayoutEffect, useMemo } from "react"
import { ExponentialSurface, useSurface } from "@exponential-at/ui-react"
import type { A2uiMessage, HostPlugin } from "@exponential-at/ui-react"
import { surfaceMessages } from "../../blog/a2ui"
import { BLOG_ICONS } from "./surface-icons"
import type { SurfaceRuntimeProps } from "./LiveSurface"

export function SurfaceRuntime({ id, tree }: SurfaceRuntimeProps) {
  const surface = useSurface({ surfaceId: id })
  const { apply, reset } = surface

  useLayoutEffect(() => {
    reset()
    for (const m of surfaceMessages(id, tree)) apply(m as unknown as A2uiMessage)
  }, [id, tree, apply, reset])

  const host = useMemo<HostPlugin>(
    () => ({
      icons: BLOG_ICONS,
      onAction: () => Promise.resolve(),
      onInput: () => Promise.resolve(),
      openUrl: (url) => window.open(url, `_blank`, `noopener,noreferrer`),
    }),
    []
  )

  return <ExponentialSurface id={`xui-blog-${id}`} surface={surface} host={host} theme="exponential" mode="dark" />
}
