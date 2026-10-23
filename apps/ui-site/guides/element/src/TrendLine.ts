// An extension = a catalog (JSON: what the model may use) + one painter per
// NATIVE component. `Metric` is a macro: it expands into Card + Text +
// TrendLine in the reducer and needs no painter on any platform.
import { defineExtension } from "@exponential_app/ui"
import type { ExtensionDef } from "@exponential_app/ui"
import { defineElementExtension } from "@exponential_app/ui-element"
import catalog from "./trendline.extension.json"

// JSON imports widen literals, hence the cast; defineExtension checks it and throws listing every problem.
export const metricsCatalog = defineExtension(catalog as unknown as ExtensionDef)

const SVG = `http://www.w3.org/2000/svg`

// A vanilla painter: returns an Element, re-run whenever props, theme or mode change.
export const metrics = defineElementExtension({
  catalog: metricsCatalog,
  components: {
    TrendLine: (ctx) => {
      const values = Array.isArray(ctx.props.values) ? ctx.props.values.map(Number) : []
      const height = Number(ctx.props.height ?? 48)
      const min = Math.min(...values), max = Math.max(...values)
      const y = (n: number) => (max === min ? 50 : 100 - ((n - min) / (max - min)) * 100)
      const root = document.createElement(`div`)
      ctx.applyRootProps(root)
      root.style.height = `${height}px`
      root.style.width = `100%`
      const svg = document.createElementNS(SVG, `svg`)
      for (const [k, v] of Object.entries({ viewBox: `0 0 100 100`, preserveAspectRatio: `none`, width: `100%`, height: String(height), role: `img`, "aria-label": `${values.length} values` })) svg.setAttribute(k, v)
      const line = document.createElementNS(SVG, `polyline`)
      line.setAttribute(`points`, values.map((n, i) => `${(i / Math.max(values.length - 1, 1)) * 100},${y(n)}`).join(` `))
      for (const [k, v] of Object.entries({ fill: `none`, stroke: ctx.theme.modes[ctx.mode].color.primary, "stroke-width": `2`, "vector-effect": `non-scaling-stroke` })) line.setAttribute(k, v)
      svg.append(line)
      root.append(svg)
      return root
    },
  },
})

// Per surface: el.extensions = [metrics] and new ExponentialHost({extensions: [metricsCatalog]})
// (or registerExtension(metrics) once for every surface on the page).
