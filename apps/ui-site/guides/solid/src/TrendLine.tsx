// An extension = a catalog (JSON: what the model may use) + one painter per
// NATIVE component. `Metric` is a macro: it expands into Card + Text +
// TrendLine in the reducer and needs no painter on any platform.
import { defineExtension } from "@exponential-at/ui"
import type { ExtensionDef } from "@exponential-at/ui"
import { defineSolidExtension } from "@exponential-at/ui-solid"
import type { ExtensionComponentProps } from "@exponential-at/ui-solid"
import catalog from "./trendline.extension.json"

// JSON imports widen literals, hence the cast; defineExtension checks it and throws listing every problem.
export const metricsCatalog = defineExtension(catalog as unknown as ExtensionDef)

// Solid props are live getters: read them inside JSX/functions, never destructure.
function TrendLine(props: ExtensionComponentProps) {
  const values = () => (Array.isArray(props.props.values) ? props.props.values.map(Number) : [])
  const height = () => Number(props.props.height ?? 48)
  const points = () => {
    const v = values()
    const min = Math.min(...v), max = Math.max(...v)
    const y = (n: number) => (max === min ? 50 : 100 - ((n - min) / (max - min)) * 100)
    return v.map((n, i) => `${(i / Math.max(v.length - 1, 1)) * 100},${y(n)}`).join(` `)
  }
  return (
    <div {...props.rootProps} style={{ height: `${height()}px`, width: `100%` }}>
      <svg viewBox="0 0 100 100" preserveAspectRatio="none" width="100%" height={height()} role="img" aria-label={`${values().length} values`}>
        <polyline points={points()} fill="none" stroke={props.theme.modes[props.mode].color.primary} stroke-width="2" vector-effect="non-scaling-stroke" />
      </svg>
    </div>
  )
}

// Per surface: <ExponentialSurface … extensions={[metrics]} /> and new ExponentialHost({extensions: [metricsCatalog]})
// (or registerExtension({catalog, components}) once for every surface on the page).
export const metrics = defineSolidExtension({ catalog: metricsCatalog, components: { TrendLine } })
