// FEED-130 sample: the web sample (../web) as a plain Vite page with NO
// framework: one <exponential-surface> streamed from the local A2UI JSONL
// server (samples/exponential-ui/server), with the server's custom theme and
// ONE custom extension component (TrendLine, a vanilla SVG painter). No
// Exponential account, no backend of ours: the element's public API only
// (`src` + `post` build the host and its JSONL transport).
import "@exponential_app/eui-element/define"
import { defineExtension } from "@exponential_app/eui"
import { defineElementExtension } from "@exponential_app/eui-element"

const params = new URLSearchParams(location.search)
const SERVER = params.get(`server`) ?? `http://localhost:4190`
const MODE = params.get(`mode`) === `dark` ? `dark` : `light`
const SVG = `http://www.w3.org/2000/svg`

/** The extension's painter: an SVG polyline of the bound readings, built
 *  again whenever its props, the theme or the mode change. */
function TrendLine(ctx) {
  const values = Array.isArray(ctx.props.values) ? ctx.props.values.map(Number) : []
  const height = Number(ctx.props.height ?? 48)
  const color = ctx.props.color ?? ctx.theme.modes[ctx.mode].color.primary
  const min = Math.min(...values), max = Math.max(...values)
  const points = values.map((n, i) => `${(i / Math.max(values.length - 1, 1)) * 100},${max === min ? 50 : 100 - ((n - min) / (max - min)) * 100}`).join(` `)
  const root = document.createElement(`div`)
  ctx.applyRootProps(root)
  Object.assign(root.style, { height: `${height}px`, width: `100%` })
  const svg = document.createElementNS(SVG, `svg`)
  for (const [k, v] of Object.entries({ viewBox: `0 0 100 100`, preserveAspectRatio: `none`, width: `100%`, height: String(height), role: `img`, "aria-label": `${values.length} readings` })) svg.setAttribute(k, v)
  const line = document.createElementNS(SVG, `polyline`)
  for (const [k, v] of Object.entries({ points, fill: `none`, stroke: color, "stroke-width": `3`, "vector-effect": `non-scaling-stroke`, "stroke-linejoin": `round` })) line.setAttribute(k, v)
  svg.append(line)
  root.append(svg)
  return root
}

const root = document.getElementById(`root`)
root.style.background = MODE === `dark` ? `#0b1220` : `#f8fafc`
const status = document.getElementById(`status`)

async function main() {
  const [extension, theme] = await Promise.all([fetch(`${SERVER}/extension.json`).then((r) => r.json()), fetch(`${SERVER}/theme.json`).then((r) => r.json())])
  const catalog = defineExtension(extension)

  // Configure first, attach last: the element connects its host on attach.
  const el = document.createElement(`exponential-surface`)
  el.surfaceId = `greenhouse`
  el.mode = MODE
  el.theme = theme
  el.extensions = [defineElementExtension({ catalog, components: { TrendLine } })]
  el.post = `${SERVER}/action`
  el.src = `${SERVER}/a2ui.jsonl${params.get(`once`) ? `?once=1` : ``}`
  el.addEventListener(`xui-status`, () => (status.textContent = `transport: ${el.status}`))
  document.getElementById(`loading`).remove()
  root.prepend(el)
}

void main()
