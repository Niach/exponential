// An extension = a catalog (JSON: what the model may use) + one painter per
// NATIVE component. `Metric` is a macro: it expands into Card + Text +
// TrendLine in the reducer and needs no painter on any platform.
import { defineExtension } from "@exponential-at/ui"
import type { ExtensionDef } from "@exponential-at/ui"
import { defineReactExtension } from "@exponential-at/ui-react"
import type { ExtensionComponentProps } from "@exponential-at/ui-react"
import catalog from "./trendline.extension.json"

// JSON imports widen literals, hence the cast; defineExtension checks it and throws listing every problem.
export const metricsCatalog = defineExtension(catalog as unknown as ExtensionDef)

function TrendLine({ props, rootProps, theme, mode }: ExtensionComponentProps) {
  const values = Array.isArray(props.values) ? props.values.map(Number) : []
  const height = Number(props.height ?? 48)
  const min = Math.min(...values), max = Math.max(...values)
  const y = (v: number) => (max === min ? 50 : 100 - ((v - min) / (max - min)) * 100)
  const points = values.map((v, i) => `${(i / Math.max(values.length - 1, 1)) * 100},${y(v)}`).join(` `)
  return (
    <div {...rootProps} style={{ height, width: `100%` }}>
      <svg viewBox="0 0 100 100" preserveAspectRatio="none" width="100%" height={height} role="img" aria-label={`${values.length} values`}>
        <polyline points={points} fill="none" stroke={theme.modes[mode].color.primary} strokeWidth="2" vectorEffect="non-scaling-stroke" />
      </svg>
    </div>
  )
}

// Per surface: <HostSurface … extensions={[metrics]} /> and new ExponentialHost({extensions: [metricsCatalog]})
// (or registerExtension({catalog, components}) once for every surface on the page).
export const metrics = defineReactExtension({ catalog: metricsCatalog, components: { TrendLine } })
