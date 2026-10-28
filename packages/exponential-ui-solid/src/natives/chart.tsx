// FEED-128 (React's natives/chart.tsx): Chart, kinds bar, stackedBar, line,
// area, pie, donut and sparkline. The width is the BOX's (measured, no fixed
// viewBox, so text is never stretched); the numbers every painter shares
// come from `@exponential_app/ui` (`chartExtent`, `niceTicks`, `seriesColor`,
// `DONUT_HOLE`). Pie and donut draw one RING per series (slices =
// categories). The legend shows with 2+ series or slices; the tooltip part
// follows hover and, with the chart focused, ArrowLeft/ArrowRight between
// categories; screen readers get a summary label plus a data table. In RTL
// the cartesian kinds MIRROR; pie and donut do not.
//
// Solid: the geometry is ONE memo of plain numbers; the SVG element stays
// put (a keyboard highlight keeps the focus), and the highlight (`active`)
// only drives attribute bindings, the band and the tooltip.

import { createMemo, createSignal, createUniqueId, For, Index, Show, type Component, type JSX } from "solid-js"
import { chartExtent, chartSummaryParams, niceTicks, seriesColor, DONUT_HOLE } from "@exponential_app/ui"
import { tokenVar, visualScale } from "@exponential_app/ui/web"
import { useSurfaceContext } from "../context"
import { mergeStyle, type NativeProps } from "../node-view"
import { createElementSize } from "../platform"
import { arr, bool, css, num, objects, parts, str } from "./shared"

interface Series {
  name: string
  values: number[]
  tone?: string
}

const colorVar = (i: number, tone?: string) => tokenVar(seriesColor(i, tone)) ?? `currentColor`
const AXIS_FONT = 11

type Arc = { si: number; i: number; d: string; label: { x: number; y: number; v: number } | null }
type Bar = { si: number; i: number; x: number; y: number; w: number; h: number; rx: number; fill: string; label: { x: number; y: number; middle: boolean; v: number } | null }
type Line = { si: number; color: string; line: string; area: string | null; dots: { i: number; cx: number; cy: number }[] }

export function ChartNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const id = createUniqueId()
  const kind = () => str(props.props.kind, `bar`)
  const sparkline = () => kind() === `sparkline`
  const round = () => kind() === `pie` || kind() === `donut`
  const series = createMemo((): Series[] => objects<Series>(props.props.series).map((s) => ({ name: str(s?.name), tone: s?.tone, values: arr<unknown>(s?.values).map((v) => num(v, NaN)) })))
  const categories = createMemo(() => arr<unknown>(props.props.categories).map(String))
  const n = () => Math.max(categories().length, Math.max(0, ...series().map((s) => s.values.length)))
  // Round 2 §7: `height` = the WHOLE chart box (title, legend and axes
  // inside); the plot takes what is left of it.
  const boxHeight = () => Math.max(16, num(props.props.height, sparkline() ? 24 : 200))
  const showAxes = () => !sparkline() && !round() && props.props.showAxes !== false
  const showGrid = () => !sparkline() && !round() && props.props.showGrid !== false
  const showValues = () => bool(props.props.showValues)
  const [box, setBox] = createSignal<HTMLDivElement | null>(null)
  const size = createElementSize(box, 320)
  const fmt = (v: number) => ctx.formatter.number(v)
  const [active, setActive] = createSignal<number | null>(null)
  let plotRef: SVGSVGElement | undefined

  const geo = createMemo(() => {
    const k = kind()
    const sp = sparkline()
    const rd = round()
    const ss = series()
    const nn = n()
    const width = Math.max(40, size().width || 320)
    const height = Math.max(16, size().measured && size().height > 0 ? size().height : boxHeight())
    const ext = chartExtent(k, ss, props.props.min === undefined ? undefined : num(props.props.min), props.props.max === undefined ? undefined : num(props.props.max))
    const ticks = sp || rd ? { min: ext.min, max: ext.max, step: 0, ticks: [] as number[] } : niceTicks(ext.min, ext.max, 5)
    const lo = ticks.min
    const hi = ticks.max === ticks.min ? ticks.min + 1 : ticks.max
    const tickText = ticks.ticks.map((t) => fmt(t))
    const yLabel = str(props.props.yLabel)
    const xLabel = str(props.props.xLabel)
    const axes = showAxes()
    const sv = showValues()
    const pad = {
      l: (axes ? Math.max(...tickText.map((t) => t.length), 1) * AXIS_FONT * 0.62 + 8 : sp ? 1 : 4) + (yLabel ? 16 : 0),
      r: sp ? 1 : 8,
      t: sp ? 2 : sv ? 16 : 8,
      b: (axes ? AXIS_FONT + 10 : sp ? 2 : 4) + (xLabel ? 16 : 0),
    }
    const innerW = Math.max(1, width - pad.l - pad.r)
    const innerH = Math.max(1, height - pad.t - pad.b)
    const y = (v: number) => pad.t + innerH - ((Math.min(hi, Math.max(lo, v)) - lo) / (hi - lo)) * innerH
    const slot = innerW / Math.max(1, nn)
    const cx = (i: number) => (sp ? pad.l + (nn <= 1 ? innerW / 2 : (innerW * i) / (nn - 1)) : pad.l + slot * (i + 0.5))
    const zero = y(Math.max(lo, Math.min(hi, 0)))
    const rtl = ctx.direction === `rtl` && !rd
    const mx = (x: number) => (rtl ? width - x : x)
    const px = (i: number) => mx(cx(i))

    const arcs: Arc[] = []
    const bars: Bar[] = []
    const lines: (Line | null)[] = []
    const lineValues: { si: number; i: number; x: number; y: number; v: number }[] = []
    if (rd) {
      const R = Math.min(innerW, innerH) / 2
      const ox = pad.l + innerW / 2
      const oy = pad.t + innerH / 2
      const inner = k === `donut` ? R * DONUT_HOLE : 0
      const ring = (R - inner) / Math.max(1, ss.length)
      ss.forEach((s, si) => {
        const r0 = inner + ring * si
        const r1 = r0 + ring
        const total = s.values.reduce((a, b) => a + (Number.isFinite(b) && b > 0 ? b : 0), 0) || 1
        let a0 = -Math.PI / 2
        s.values.forEach((v, i) => {
          const share = (Number.isFinite(v) && v > 0 ? v : 0) / total
          const a1 = a0 + share * 2 * Math.PI
          if (share > 0) {
            const large = a1 - a0 > Math.PI ? 1 : 0
            const p = (r: number, a: number) => `${ox + r * Math.cos(a)},${oy + r * Math.sin(a)}`
            const full = share >= 0.9999
            const d = full
              ? `M${ox - r1},${oy} A${r1},${r1} 0 1 1 ${ox + r1},${oy} A${r1},${r1} 0 1 1 ${ox - r1},${oy} ${r0 > 0 ? `M${ox - r0},${oy} A${r0},${r0} 0 1 0 ${ox + r0},${oy} A${r0},${r0} 0 1 0 ${ox - r0},${oy}` : ``}Z`
              : r0 > 0
                ? `M${p(r1, a0)} A${r1},${r1} 0 ${large} 1 ${p(r1, a1)} L${p(r0, a1)} A${r0},${r0} 0 ${large} 0 ${p(r0, a0)} Z`
                : `M${ox},${oy} L${p(r1, a0)} A${r1},${r1} 0 ${large} 1 ${p(r1, a1)} Z`
            const mid = (a0 + a1) / 2
            const rm = (r0 + r1) / 2
            arcs.push({ si, i, d, label: sv ? { x: ox + rm * Math.cos(mid), y: oy + rm * Math.sin(mid), v } : null })
          }
          a0 = a1
        })
      })
    } else if (k === `line` || k === `area` || sp) {
      ss.forEach((s, si) => {
        const pts = s.values.map((v, i) => (Number.isFinite(v) ? [px(i), y(v)] : null)).filter((p): p is number[] => p !== null)
        if (pts.length === 0) return void lines.push(null)
        const line = `M${pts.map((p) => p.join(`,`)).join(` L`)}`
        lines.push({
          si,
          color: colorVar(si, s.tone),
          line,
          area: k === `area` ? `${line} L${pts[pts.length - 1][0]},${zero} L${pts[0][0]},${zero} Z` : null,
          dots: sp ? [] : s.values.flatMap((v, i) => (Number.isFinite(v) ? [{ i, cx: px(i), cy: y(v) }] : [])),
        })
      })
      if (sv) ss.forEach((s, si) => s.values.forEach((v, i) => (Number.isFinite(v) ? lineValues.push({ si, i, x: px(i), y: y(v) - 6, v }) : undefined)))
    } else {
      const stacked = k === `stackedBar`
      const groupW = slot * 0.72
      const bw = stacked ? groupW : groupW / Math.max(1, ss.length)
      for (let i = 0; i < nn; i++) {
        let acc = 0
        ss.forEach((s, si) => {
          const v = s.values[i]
          if (!Number.isFinite(v)) return
          const x0 = pad.l + slot * i + (slot - groupW) / 2 + (stacked ? 0 : bw * si)
          const from = stacked ? acc : 0
          const to = stacked ? acc + Math.max(0, v) : v
          if (stacked) acc = to
          const top = Math.min(y(from), y(to))
          const h = Math.abs(y(from) - y(to))
          const barW = Math.max(1, bw - (stacked ? 0 : 2))
          bars.push({ si, i, x: rtl ? width - x0 - barW : x0, y: top, w: barW, h: Math.max(0, h), rx: stacked ? 0 : 2, fill: colorVar(si, s.tone), label: sv ? { x: mx(x0 + bw / 2), y: stacked ? top + h / 2 : top - 4, middle: stacked, v } : null })
        })
      }
    }
    return { width, height, ticks, tickText, yLabel, xLabel, pad, innerW, innerH, y, slot, zero, rtl, mx, px, arcs, bars, lines, lineValues }
  })

  const legendEntries = () => (round() ? categories().map((c, i) => ({ name: c, color: colorVar(i) })) : series().map((s, i) => ({ name: s.name, color: colorVar(i, s.tone) })))
  const legend = () => props.props.showLegend !== false && !sparkline() && legendEntries().length >= 2
  const title = () => str(props.props.title)
  const summary = () => {
    const summaryParams = chartSummaryParams(series())
    const summaryText = ctx.t(`chartSummary`, { series: summaryParams.series, min: fmt(summaryParams.min), max: fmt(summaryParams.max) })
    return title() ? `${title()}: ${summaryText}` : summaryText
  }

  const onKey = (e: KeyboardEvent) => {
    const nn = n()
    if (nn === 0) return
    if (e.key === `ArrowRight` || e.key === `ArrowLeft`) {
      e.preventDefault()
      const rtl = ctx.direction === `rtl`
      const delta = (e.key === `ArrowRight`) !== rtl ? 1 : -1
      setActive((a) => (a === null ? (delta > 0 ? 0 : nn - 1) : (a + delta + nn) % nn))
    } else if (e.key === `Home`) setActive(0)
    else if (e.key === `End`) setActive(nn - 1)
    else if (e.key === `Escape`) setActive(null)
  }
  const onMove = (e: PointerEvent) => {
    const nn = n()
    if (round() || nn === 0) return
    const r = plotRef?.getBoundingClientRect()
    if (!r) return
    const g = geo()
    // NEW-82: the pointer offset is in visual px (zoom, transforms); the plot in layout px.
    const x = (g.rtl ? r.right - e.clientX : e.clientX - r.left) / visualScale(r.width, g.width) - g.pad.l
    const i = sparkline() ? Math.round((x / g.innerW) * (nn - 1)) : Math.floor(x / g.slot)
    setActive(i >= 0 && i < nn ? i : null)
  }
  const dim = (i: number, low: number) => (active() === null || active() === i ? 1 : low)
  const valueLabel = () => part(`valueLabel`)

  const body = (): JSX.Element => {
    const g = geo()
    if (round())
      return (
        <>
          {g.arcs.map((a) => (
            <path d={a.d} fill={colorVar(a.i)} fill-rule="evenodd" stroke="var(--xui-color-background)" stroke-width={1} data-category={a.i} data-active={active() === a.i ? `` : undefined} opacity={dim(a.i, 0.55)} onPointerEnter={() => setActive(a.i)} />
          ))}
          {g.arcs.map((a) =>
            a.label ? (
              <text {...valueLabel()} x={a.label.x} y={a.label.y} text-anchor="middle" dominant-baseline="middle">
                {fmt(a.label.v)}
              </text>
            ) : null
          )}
        </>
      )
    if (kind() === `line` || kind() === `area` || sparkline())
      return (
        <>
          {g.lines.map((l) =>
            l ? (
              <g data-series={l.si}>
                {l.area !== null ? <path d={l.area} fill={l.color} opacity={0.18} /> : null}
                <path d={l.line} fill="none" stroke={l.color} stroke-width={sparkline() ? 1.5 : 2} stroke-linejoin="round" stroke-linecap="round" />
                {l.dots.map((p) => (
                  <circle cx={p.cx} cy={p.cy} r={active() === p.i ? 4 : 2.5} fill={l.color} />
                ))}
              </g>
            ) : null
          )}
          {g.lineValues.map((v) => (
            <text {...valueLabel()} x={v.x} y={v.y} text-anchor="middle">
              {fmt(v.v)}
            </text>
          ))}
        </>
      )
    return (
      <>
        {g.bars.map((b) => (
          <rect x={b.x} y={b.y} width={b.w} height={b.h} rx={b.rx} fill={b.fill} opacity={dim(b.i, 0.6)} />
        ))}
        {g.bars.map((b) =>
          b.label ? (
            <text {...valueLabel()} x={b.label.x} y={b.label.y} text-anchor="middle" dominant-baseline={b.label.middle ? `middle` : undefined}>
              {fmt(b.label.v)}
            </text>
          ) : null
        )}
      </>
    )
  }

  const tooltip = (): JSX.Element => {
    const a = active()
    if (a === null || a >= n()) return null
    const g = geo()
    const tipX = round() ? g.pad.l + g.innerW / 2 : g.px(a)
    const cat = categories()[a]
    return (
      <div {...part(`tooltip`)} role="presentation" style={css({ position: `absolute`, left: Math.min(Math.max(0, tipX - 60), g.width - 120), top: g.pad.t, pointerEvents: `none` })} data-category={a}>
        {cat ? <strong>{cat}</strong> : null}
        {series().map((s, si) => (
          <span class="xui-chart-tip-row">
            <i style={css({ background: round() ? colorVar(a) : colorVar(si, s.tone) })} />
            {s.name ? `${s.name}: ` : ``}
            {Number.isFinite(s.values[a]) ? fmt(s.values[a]) : `—`}
          </span>
        ))}
      </div>
    )
  }

  return (
    <div {...props.rootProps} style={mergeStyle(props.rootProps, { height: boxHeight() })} data-kind={kind()} data-mirrored={geo().rtl ? `true` : undefined}>
      <Show when={title()}>
        <span {...part(`title`)} id={`${id}-title`}>
          {title()}
        </span>
      </Show>
      <div ref={setBox} class="xui-chart-plot">
        <svg
          ref={plotRef}
          width={geo().width}
          height={geo().height}
          viewBox={`0 0 ${geo().width} ${geo().height}`}
          role="img"
          aria-label={summary()}
          tabindex={sparkline() ? undefined : 0}
          onKeyDown={(e) => (sparkline() ? undefined : onKey(e))}
          onPointerMove={(e) => (sparkline() ? undefined : onMove(e))}
          onPointerLeave={() => setActive(null)}
          onBlur={() => setActive(null)}
          class="xui-chart-svg"
        >
          <Show when={showGrid()}>
            <g {...part(`grid`)} stroke="currentColor" stroke-width={1} shape-rendering="crispEdges">
              <For each={geo().ticks.ticks}>{(t) => <line x1={geo().mx(geo().pad.l)} x2={geo().mx(geo().width - geo().pad.r)} y1={Math.round(geo().y(t)) + 0.5} y2={Math.round(geo().y(t)) + 0.5} />}</For>
            </g>
          </Show>
          <Show when={!round() && active() !== null && !sparkline()}>
            <rect class="xui-chart-band" x={geo().rtl ? geo().width - geo().pad.l - geo().slot * (active()! + 1) : geo().pad.l + geo().slot * active()!} y={geo().pad.t} width={geo().slot} height={geo().innerH} fill="currentColor" opacity={0.06} />
          </Show>
          {body()}
          <Show when={showAxes()}>
            <g {...part(`axis`)} fill="currentColor" font-size={AXIS_FONT}>
              <line x1={geo().mx(geo().pad.l)} x2={geo().mx(geo().width - geo().pad.r)} y1={geo().zero} y2={geo().zero} stroke="currentColor" stroke-width={1} opacity={0.6} shape-rendering="crispEdges" />
              <Index each={geo().ticks.ticks}>
                {(t, i) => (
                  <text x={geo().mx(geo().pad.l - 6)} y={geo().y(t())} text-anchor={geo().rtl ? `start` : `end`} dominant-baseline="middle">
                    {geo().tickText[i]}
                  </text>
                )}
              </Index>
              <Index each={categories()}>
                {(c, i) => (
                  <text x={geo().px(i)} y={geo().pad.t + geo().innerH + AXIS_FONT + 4} text-anchor="middle">
                    {c()}
                  </text>
                )}
              </Index>
              <Show when={geo().xLabel}>
                <text x={geo().mx(geo().pad.l + geo().innerW / 2)} y={geo().height - 3} text-anchor="middle">
                  {geo().xLabel}
                </text>
              </Show>
              <Show when={geo().yLabel}>
                <text x={0} y={0} transform={`translate(${geo().rtl ? geo().width - AXIS_FONT : AXIS_FONT}, ${geo().pad.t + geo().innerH / 2}) rotate(${geo().rtl ? 90 : -90})`} text-anchor="middle">
                  {geo().yLabel}
                </text>
              </Show>
            </g>
          </Show>
        </svg>
        {tooltip()}
      </div>
      <Show when={legend()}>
        <div {...part(`legend`)}>
          <For each={legendEntries()}>
            {(e) => (
              <span>
                <i style={css({ background: e.color })} />
                {e.name}
              </span>
            )}
          </For>
        </div>
      </Show>
      <Show when={!sparkline()}>
        <table class="xui-sr-only">
          <Show when={title()}>
            <caption>{title()}</caption>
          </Show>
          <thead>
            <tr>
              <th scope="col" />
              <For each={series()}>{(s) => <th scope="col">{s.name}</th>}</For>
            </tr>
          </thead>
          <tbody>
            <For each={Array.from({ length: n() }, (_, i) => i)}>
              {(i) => (
                <tr>
                  <th scope="row">{categories()[i] ?? String(i + 1)}</th>
                  <For each={series()}>{(s) => <td>{Number.isFinite(s.values[i]) ? fmt(s.values[i]) : ``}</td>}</For>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </Show>
    </div>
  )
}

export const NATIVES_CHART: Record<string, Component<NativeProps>> = { Chart: ChartNative }
