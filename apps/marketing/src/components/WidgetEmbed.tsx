import { useEffect } from "react"
import { WIDGET } from "../lib/links"

/* Embeds the REAL Exponential feedback widget on the marketing site using the
   GA-style loader snippet (mirrors apps/web widget-section.tsx and the widget
   demo page). The loader is served from app.exponential.at, so the widget's
   API origin resolves to the cloud automatically and submissions land on the
   Exponential team's feedback board. Renders nothing — it only wires the runtime.
   Mounted once per page via SiteHeader; the `ExponentialWidget` guard below
   makes any accidental second mount a no-op. */
export function WidgetEmbed() {
  useEffect(() => {
    type WidgetApi = {
      q?: unknown[][]
      init(opts: Record<string, unknown>): void
      setCustomData(data: Record<string, unknown>): void
    }
    const w = window as unknown as { ExponentialWidget?: WidgetApi }
    if (w.ExponentialWidget) return

    const q: unknown[][] = []
    const api = { q } as WidgetApi
    for (const m of [
      `init`,
      `identify`,
      `setCustomData`,
      `open`,
      `close`,
      `submit`,
    ]) {
      ;(api as unknown as Record<string, (...a: unknown[]) => void>)[m] = (
        ...args: unknown[]
      ) => {
        q.push([m, args])
      }
    }
    w.ExponentialWidget = api

    const s = document.createElement(`script`)
    s.async = true
    s.src = WIDGET.loader
    document.head.appendChild(s)

    // Desktop pinned to the bottom-right fab; mobile left to the remote
    // config/default (edge tab, middle right — EXP-569). The legacy
    // `position` stays as the cache-skew bridge for pre-EXP-569 loaders.
    api.init({
      key: WIDGET.key,
      launcher: { desktop: { mode: `fab`, position: `bottom-right` } },
      position: `bottom-right`,
    })
    api.setCustomData({ source: `marketing`, page: window.location.pathname })
  }, [])

  return null
}
