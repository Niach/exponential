// Round 1 (contract §3 Chart, a11y.json Chart): kinds bar, stackedBar,
// line, area, pie, donut and sparkline. The width is the BOX's (measured,
// no fixed viewBox, so text is never stretched); the numbers every painter
// shares come from `@exponential-at/ui` (`chartExtent`, `niceTicks` for the
// axis ticks and grid lines, `seriesColor` = the tone else
// `$color.chart1..8` wrapping, `DONUT_HOLE`). Pie and donut draw one RING
// per series (slices = categories). The legend shows with 2+ series or
// slices; the tooltip part follows hover and, with the chart focused,
// ArrowLeft/ArrowRight between categories; screen readers get a summary
// label plus a data table. In RTL the cartesian kinds MIRROR (the first
// category on the right, the value axis on the right), so ArrowLeft/Right
// keep moving the highlight the way the arrow points; pie and donut do not
// mirror.

import { useId, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react"
import { chartExtent, niceTicks, seriesColor, DONUT_HOLE } from "@exponential-at/ui"
import { useSurfaceContext } from "../context"
import type { NativeProps } from "../node-view"
import { useElementSize } from "../platform"
import { tokenVar } from "../theme-css"
import { arr, bool, num, str, useParts } from "./shared"

interface Series {
  name: string
  values: number[]
  tone?: string
}

const colorVar = (i: number, tone?: string) => tokenVar(seriesColor(i, tone)) ?? `currentColor`
const AXIS_FONT = 11

export function ChartNative({ node, props, rootProps }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const id = useId()
  const kind = str(props.kind, `bar`)
  const sparkline = kind === `sparkline`
  const round = kind === `pie` || kind === `donut`
  const series = arr<Series>(props.series).map((s) => ({ name: str(s?.name), tone: s?.tone, values: arr<unknown>(s?.values).map((v) => num(v, NaN)) }))
  const longest = Math.max(0, ...series.map((s) => s.values.length))
  const categories = arr<unknown>(props.categories).map(String)
  const n = Math.max(categories.length, longest)
  const height = Math.max(16, num(props.height, sparkline ? 24 : 200))
  const showAxes = !sparkline && !round && props.showAxes !== false
  const showGrid = !sparkline && !round && props.showGrid !== false
  const showValues = bool(props.showValues)
  const [box, setBox] = useState<HTMLDivElement | null>(null)
  const size = useElementSize(box, 320)
  const width = Math.max(40, size.width || 320)
  const fmt = useMemo(() => new Intl.NumberFormat(ctx.locale, { maximumFractionDigits: 2 }), [ctx.locale])
  const [active, setActive] = useState<number | null>(null)
  const plotRef = useRef<SVGSVGElement>(null)

  const ext = chartExtent(kind, series, props.min === undefined ? undefined : num(props.min), props.max === undefined ? undefined : num(props.max))
  const ticks = sparkline || round ? { min: ext.min, max: ext.max, step: 0, ticks: [] as number[] } : niceTicks(ext.min, ext.max, 5)
  const lo = ticks.min
  const hi = ticks.max === ticks.min ? ticks.min + 1 : ticks.max
  const tickText = ticks.ticks.map((t) => fmt.format(t))
  const yLabel = str(props.yLabel)
  const xLabel = str(props.xLabel)
  const pad = {
    l: (showAxes ? Math.max(...tickText.map((t) => t.length), 1) * AXIS_FONT * 0.62 + 8 : sparkline ? 1 : 4) + (yLabel ? 16 : 0),
    r: sparkline ? 1 : 8,
    t: sparkline ? 2 : showValues ? 16 : 8,
    b: (showAxes ? AXIS_FONT + 10 : sparkline ? 2 : 4) + (xLabel ? 16 : 0),
  }
  const innerW = Math.max(1, width - pad.l - pad.r)
  const innerH = Math.max(1, height - pad.t - pad.b)
  const y = (v: number) => pad.t + innerH - ((Math.min(hi, Math.max(lo, v)) - lo) / (hi - lo)) * innerH
  const slot = innerW / Math.max(1, n)
  const cx = (i: number) => (sparkline ? pad.l + (n <= 1 ? innerW / 2 : (innerW * i) / (n - 1)) : pad.l + slot * (i + 0.5))
  const zero = y(Math.max(lo, Math.min(hi, 0)))
  // The x mirror of the cartesian kinds in RTL (geometry is laid out LTR,
  // then every x goes through `mx`).
  const rtl = ctx.direction === `rtl` && !round
  const mx = (x: number) => (rtl ? width - x : x)
  const px = (i: number) => mx(cx(i))

  let body: ReactNode = null
  let values: ReactNode = null
  if (round) {
    const R = Math.min(innerW, innerH) / 2
    const ox = pad.l + innerW / 2
    const oy = pad.t + innerH / 2
    const inner = kind === `donut` ? R * DONUT_HOLE : 0
    const ring = (R - inner) / Math.max(1, series.length)
    const arcs: ReactNode[] = []
    const labels: ReactNode[] = []
    series.forEach((s, si) => {
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
          arcs.push(<path key={`${si}-${i}`} d={d} fill={colorVar(i)} fillRule="evenodd" stroke="var(--xui-color-background)" strokeWidth={1} data-category={i} data-active={active === i ? `` : undefined} opacity={active === null || active === i ? 1 : 0.55} onPointerEnter={() => setActive(i)} />)
          if (showValues) {
            const mid = (a0 + a1) / 2
            const rm = (r0 + r1) / 2
            labels.push(
              <text key={`v${si}-${i}`} {...(part(`valueLabel`) as Record<string, string>)} x={ox + rm * Math.cos(mid)} y={oy + rm * Math.sin(mid)} textAnchor="middle" dominantBaseline="middle">
                {fmt.format(v)}
              </text>
            )
          }
        }
        a0 = a1
      })
    })
    body = arcs
    values = labels
  } else if (kind === `line` || kind === `area` || sparkline) {
    body = series.map((s, si) => {
      const pts = s.values.map((v, i) => (Number.isFinite(v) ? [px(i), y(v)] : null)).filter((p): p is number[] => p !== null)
      if (pts.length === 0) return null
      const line = `M${pts.map((p) => p.join(`,`)).join(` L`)}`
      const color = colorVar(si, s.tone)
      return (
        <g key={si} data-series={si}>
          {kind === `area` ? <path d={`${line} L${pts[pts.length - 1][0]},${zero} L${pts[0][0]},${zero} Z`} fill={color} opacity={0.18} /> : null}
          <path d={line} fill="none" stroke={color} strokeWidth={sparkline ? 1.5 : 2} strokeLinejoin="round" strokeLinecap="round" />
          {!sparkline ? s.values.map((v, i) => (Number.isFinite(v) ? <circle key={i} cx={px(i)} cy={y(v)} r={active === i ? 4 : 2.5} fill={color} /> : null)) : null}
        </g>
      )
    })
    if (showValues)
      values = series.flatMap((s, si) =>
        s.values.map((v, i) =>
          Number.isFinite(v) ? (
            <text key={`${si}-${i}`} {...(part(`valueLabel`) as Record<string, string>)} x={px(i)} y={y(v) - 6} textAnchor="middle">
              {fmt.format(v)}
            </text>
          ) : null
        )
      )
  } else {
    const stacked = kind === `stackedBar`
    const groupW = slot * 0.72
    const bw = stacked ? groupW : groupW / Math.max(1, series.length)
    const bars: ReactNode[] = []
    const labels: ReactNode[] = []
    for (let i = 0; i < n; i++) {
      let acc = 0
      series.forEach((s, si) => {
        const v = s.values[i]
        if (!Number.isFinite(v)) return
        const x0 = pad.l + slot * i + (slot - groupW) / 2 + (stacked ? 0 : bw * si)
        const from = stacked ? acc : 0
        const to = stacked ? acc + Math.max(0, v) : v
        if (stacked) acc = to
        const top = Math.min(y(from), y(to))
        const h = Math.abs(y(from) - y(to))
        const barW = Math.max(1, bw - (stacked ? 0 : 2))
        bars.push(<rect key={`${si}-${i}`} x={rtl ? width - x0 - barW : x0} y={top} width={barW} height={Math.max(0, h)} rx={stacked ? 0 : 2} fill={colorVar(si, s.tone)} opacity={active === null || active === i ? 1 : 0.6} />)
        if (showValues)
          labels.push(
            <text key={`${si}-${i}`} {...(part(`valueLabel`) as Record<string, string>)} x={mx(x0 + bw / 2)} y={stacked ? top + h / 2 : top - 4} textAnchor="middle" dominantBaseline={stacked ? `middle` : undefined}>
              {fmt.format(v)}
            </text>
          )
      })
    }
    body = bars
    values = labels
  }

  const legendEntries = round ? categories.map((c, i) => ({ name: c, color: colorVar(i) })) : series.map((s, i) => ({ name: s.name, color: colorVar(i, s.tone) }))
  const legend = props.showLegend !== false && !sparkline && legendEntries.length >= 2
  const title = str(props.title)
  const summary = `${title ? `${title}: ` : ``}${kind} chart, ${series.map((s) => s.name).filter(Boolean).join(`, `) || `${series.length} series`}${round ? `` : `, ${fmt.format(ext.min)} to ${fmt.format(ext.max)}`}`

  const onKey = (e: KeyboardEvent) => {
    if (n === 0) return
    if (e.key === `ArrowRight` || e.key === `ArrowLeft`) {
      e.preventDefault()
      const rtl = ctx.direction === `rtl`
      const delta = (e.key === `ArrowRight`) !== rtl ? 1 : -1
      setActive((a) => (a === null ? (delta > 0 ? 0 : n - 1) : (a + delta + n) % n))
    } else if (e.key === `Home`) setActive(0)
    else if (e.key === `End`) setActive(n - 1)
    else if (e.key === `Escape`) setActive(null)
  }
  const onMove = (e: React.PointerEvent<SVGSVGElement>) => {
    if (round || n === 0) return
    const r = plotRef.current?.getBoundingClientRect()
    if (!r) return
    const x = (rtl ? r.right - e.clientX : e.clientX - r.left) - pad.l
    const i = sparkline ? Math.round((x / innerW) * (n - 1)) : Math.floor(x / slot)
    setActive(i >= 0 && i < n ? i : null)
  }
  const tipX = active === null ? 0 : round ? pad.l + innerW / 2 : px(active)
  const tooltip =
    active !== null && active < n ? (
      <div {...(part(`tooltip`) as Record<string, string>)} role="presentation" style={{ position: `absolute`, left: Math.min(Math.max(0, tipX - 60), width - 120), top: pad.t, pointerEvents: `none` }} data-category={active}>
        {categories[active] ? <strong>{categories[active]}</strong> : null}
        {series.map((s, si) => (
          <span key={si} className="xui-chart-tip-row">
            <i style={{ background: round ? colorVar(active) : colorVar(si, s.tone) }} />
            {s.name ? `${s.name}: ` : ``}
            {Number.isFinite(s.values[active]) ? fmt.format(s.values[active]) : `—`}
          </span>
        ))}
      </div>
    ) : null

  return (
    <div {...(rootProps as Record<string, unknown>)} data-kind={kind} data-mirrored={rtl ? `true` : undefined}>
      {title ? (
        <span {...(part(`title`) as Record<string, string>)} id={`${id}-title`}>
          {title}
        </span>
      ) : null}
      <div ref={setBox} className="xui-chart-plot" style={{ height }}>
        <svg
          ref={plotRef}
          width={width}
          height={height}
          viewBox={`0 0 ${width} ${height}`}
          role="img"
          aria-label={summary}
          tabIndex={sparkline ? undefined : 0}
          onKeyDown={sparkline ? undefined : onKey}
          onPointerMove={sparkline ? undefined : onMove}
          onPointerLeave={() => setActive(null)}
          onBlur={() => setActive(null)}
          className="xui-chart-svg"
        >
          {showGrid ? (
            <g {...(part(`grid`) as Record<string, string>)} stroke="currentColor" strokeWidth={1} shapeRendering="crispEdges">
              {ticks.ticks.map((t) => (
                <line key={t} x1={mx(pad.l)} x2={mx(width - pad.r)} y1={Math.round(y(t)) + 0.5} y2={Math.round(y(t)) + 0.5} />
              ))}
            </g>
          ) : null}
          {!round && active !== null && !sparkline ? <rect className="xui-chart-band" x={rtl ? width - pad.l - slot * (active + 1) : pad.l + slot * active} y={pad.t} width={slot} height={innerH} fill="currentColor" opacity={0.06} /> : null}
          {body}
          {values}
          {showAxes ? (
            <g {...(part(`axis`) as Record<string, string>)} fill="currentColor" fontSize={AXIS_FONT}>
              <line x1={mx(pad.l)} x2={mx(width - pad.r)} y1={zero} y2={zero} stroke="currentColor" strokeWidth={1} opacity={0.6} shapeRendering="crispEdges" />
              {ticks.ticks.map((t, i) => (
                <text key={t} x={mx(pad.l - 6)} y={y(t)} textAnchor={rtl ? `start` : `end`} dominantBaseline="middle">
                  {tickText[i]}
                </text>
              ))}
              {categories.map((c, i) => (
                <text key={i} x={px(i)} y={pad.t + innerH + AXIS_FONT + 4} textAnchor="middle">
                  {c}
                </text>
              ))}
              {xLabel ? (
                <text x={mx(pad.l + innerW / 2)} y={height - 3} textAnchor="middle">
                  {xLabel}
                </text>
              ) : null}
              {yLabel ? (
                <text x={0} y={0} transform={`translate(${rtl ? width - AXIS_FONT : AXIS_FONT}, ${pad.t + innerH / 2}) rotate(${rtl ? 90 : -90})`} textAnchor="middle">
                  {yLabel}
                </text>
              ) : null}
            </g>
          ) : null}
        </svg>
        {tooltip}
      </div>
      {legend ? (
        <div {...(part(`legend`) as Record<string, string>)}>
          {legendEntries.map((e, i) => (
            <span key={i}>
              <i style={{ background: e.color }} />
              {e.name}
            </span>
          ))}
        </div>
      ) : null}
      {sparkline ? null : (
        <table className="xui-sr-only">
          {title ? <caption>{title}</caption> : null}
          <thead>
            <tr>
              <th scope="col" />
              {series.map((s, si) => (
                <th key={si} scope="col">
                  {s.name}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {Array.from({ length: n }, (_, i) => (
              <tr key={i}>
                <th scope="row">{categories[i] ?? String(i + 1)}</th>
                {series.map((s, si) => (
                  <td key={si}>{Number.isFinite(s.values[i]) ? fmt.format(s.values[i]) : ``}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  )
}
