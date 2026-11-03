/* The real Exponential feedback widget on the site (GA-style loader snippet,
   as on the old site's WidgetEmbed): submissions land on the Exponential
   team's feedback board. The loader comes from app.exponential.at, which the
   CSP's script-src and connect-src allow. */
const WIDGET = {
  key: `expw_ATLHZ5hFiV5CqApwbCawPh72bPkpbHUp`,
  loader: `https://app.exponential.at/widget/v1/loader.js`,
} as const

type WidgetApi = { q?: unknown[][]; init(opts: Record<string, unknown>): void; setCustomData(data: Record<string, unknown>): void }

export function initWidget(): void {
  const w = window as unknown as { ExponentialWidget?: WidgetApi }
  if (w.ExponentialWidget) return
  const q: unknown[][] = []
  const api = { q } as WidgetApi
  for (const m of [`init`, `identify`, `setCustomData`, `open`, `close`, `submit`]) {
    ;(api as unknown as Record<string, (...a: unknown[]) => void>)[m] = (...args: unknown[]) => {
      q.push([m, args])
    }
  }
  w.ExponentialWidget = api
  const s = document.createElement(`script`)
  s.async = true
  s.src = WIDGET.loader
  document.head.appendChild(s)
  api.init({ key: WIDGET.key })
  api.setCustomData({ source: `marketing`, page: window.location.pathname })
}
