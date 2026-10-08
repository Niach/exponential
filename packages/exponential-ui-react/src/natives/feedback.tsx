// VAPP-87: Ring, Spinner, Skeleton, Chart, TreeGuides.

import { resolveRecipe, nativeRecipeProps } from "@exponential-at/ui"
import { useSurfaceContext } from "../context"
import { CHROME } from "../icons"
import type { NativeProps } from "../node-view"
import { arr, bool, num, str, useParts } from "./shared"

function sizeOf(ctx: ReturnType<typeof useSurfaceContext>, component: string, part: string, props: Record<string, unknown>, fallback: number): number {
  const style = resolveRecipe(ctx.theme, { component, part, props: nativeRecipeProps(component, props, ctx.extensionDefs) }, ctx.mode)
  return typeof style.width === `number` ? style.width : fallback
}

export function RingNative({ node, props, rootProps }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const size = sizeOf(ctx, `Ring`, `root`, props, 32)
  const value = Math.max(0, Math.min(1, num(props.value, 0)))
  const trackStyle = resolveRecipe(ctx.theme, { component: `Ring`, part: `track`, props: nativeRecipeProps(`Ring`, props) }, ctx.mode)
  const fillStyle = resolveRecipe(ctx.theme, { component: `Ring`, part: `fill`, props: nativeRecipeProps(`Ring`, props) }, ctx.mode)
  const stroke = typeof trackStyle.borderWidth === `number` ? trackStyle.borderWidth : 2
  const r = size / 2 - stroke / 2
  const c = 2 * Math.PI * r
  const label = str(props.label)
  return (
    <span {...(rootProps as Record<string, unknown>)} role="progressbar" aria-valuemin={0} aria-valuemax={1} aria-valuenow={value} aria-label={label || undefined} style={{ width: size, height: size }}>
      <svg viewBox={`0 0 ${size} ${size}`} width={size} height={size}>
        <circle {...(part(`track`) as Record<string, string>)} cx={size / 2} cy={size / 2} r={r} fill="none" stroke="currentColor" strokeWidth={stroke} style={{ color: String(trackStyle.color ?? `currentColor`) }} />
        <circle {...(part(`fill`) as Record<string, string>)} cx={size / 2} cy={size / 2} r={r} fill="none" stroke="currentColor" strokeWidth={typeof fillStyle.borderWidth === `number` ? fillStyle.borderWidth : stroke} strokeDasharray={`${c * value} ${c}`} strokeLinecap="round" transform={`rotate(-90 ${size / 2} ${size / 2})`} style={{ color: String(fillStyle.color ?? `currentColor`) }} />
      </svg>
      {label ? <span {...(part(`label`) as Record<string, string>)}>{label}</span> : null}
    </span>
  )
}

export function SpinnerNative({ props, rootProps }: NativeProps) {
  const ctx = useSurfaceContext()
  const size = sizeOf(ctx, `Spinner`, `root`, props, 20)
  const Icon = CHROME.spinner
  const label = str(props.label, `Loading`)
  return (
    <span {...(rootProps as Record<string, unknown>)} role="status" aria-label={label} style={{ width: size, height: size }}>
      <Icon aria-hidden="true" />
    </span>
  )
}

export function SkeletonNative({ props, rootProps }: NativeProps) {
  const width = str(props.width, `100%`)
  const height = str(props.height, `16px`)
  return <div {...(rootProps as Record<string, unknown>)} aria-hidden="true" style={{ width, height, borderRadius: bool(props.rounded) ? 9999 : undefined }} />
}

interface Series {
  name: string
  values: number[]
  tone?: string
}

const TONE_VAR: Record<string, string> = {
  primary: `var(--xui-color-primary)`,
  success: `var(--xui-color-success)`,
  warning: `var(--xui-color-warning)`,
  danger: `var(--xui-color-destructive)`,
  info: `var(--xui-color-info)`,
  neutral: `var(--xui-color-mutedForeground)`,
}

export function ChartNative({ node, props, rootProps }: NativeProps) {
  const part = useParts(node, props)
  const kind = str(props.kind, `bar`)
  const categories = arr<string>(props.categories).map(String)
  const series = arr<Series>(props.series)
  const height = num(props.height, 200)
  const width = 320
  const pad = { l: 8, r: 8, t: 8, b: 20 }
  const max = Math.max(1, ...series.flatMap((s) => arr<number>(s.values).map((v) => num(v))))
  const color = (s: Series, i: number) => (s.tone && TONE_VAR[s.tone]) || `var(--xui-color-chart${(i % 5) + 1})`
  const innerW = width - pad.l - pad.r
  const innerH = height - pad.t - pad.b
  const x = (i: number) => pad.l + (innerW * (i + 0.5)) / Math.max(1, categories.length)
  const y = (v: number) => pad.t + innerH - (innerH * v) / max
  let body: React.ReactNode = null
  if (kind === `pie`) {
    const values = arr<number>(series[0]?.values).map((v) => num(v))
    const total = values.reduce((a, b) => a + b, 0) || 1
    let angle = -Math.PI / 2
    const cx = width / 2
    const cy = height / 2
    const r = Math.min(innerW, innerH) / 2
    body = values.map((v, i) => {
      const a0 = angle
      const a1 = angle + (2 * Math.PI * v) / total
      angle = a1
      const large = a1 - a0 > Math.PI ? 1 : 0
      const d = `M${cx},${cy} L${cx + r * Math.cos(a0)},${cy + r * Math.sin(a0)} A${r},${r} 0 ${large} 1 ${cx + r * Math.cos(a1)},${cy + r * Math.sin(a1)} Z`
      return <path key={i} d={d} fill={`var(--xui-color-chart${(i % 5) + 1})`} />
    })
  } else if (kind === `line` || kind === `area`) {
    body = series.map((s, si) => {
      const pts = arr<number>(s.values).map((v, i) => `${x(i)},${y(num(v))}`)
      const line = `M${pts.join(` L`)}`
      return (
        <g key={si}>
          {kind === `area` && pts.length ? <path d={`${line} L${x(pts.length - 1)},${y(0)} L${x(0)},${y(0)} Z`} fill={color(s, si)} opacity={0.2} /> : null}
          <path d={line} fill="none" stroke={color(s, si)} strokeWidth={2} strokeLinejoin="round" />
        </g>
      )
    })
  } else {
    const groups = Math.max(1, categories.length)
    const slot = innerW / groups
    const bw = (slot * 0.7) / Math.max(1, series.length)
    body = series.map((s, si) =>
      arr<number>(s.values).map((v, i) => {
        const h = (innerH * num(v)) / max
        return <rect key={`${si}-${i}`} x={pad.l + slot * i + slot * 0.15 + bw * si} y={pad.t + innerH - h} width={bw} height={h} fill={color(s, si)} rx={2} />
      })
    )
  }
  const title = str(props.title)
  return (
    <div {...(rootProps as Record<string, unknown>)} role="img" aria-label={title || `${kind} chart`}>
      {title ? <span {...(part(`root`) as Record<string, string>)} className="xui-Chart-title">{title}</span> : null}
      <svg viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none" style={{ height }}>
        {kind !== `pie` ? (
          <g {...(part(`grid`) as Record<string, string>)} stroke="var(--xui-color-border)" strokeWidth={1}>
            {[0, 0.25, 0.5, 0.75, 1].map((t) => (
              <line key={t} x1={pad.l} x2={width - pad.r} y1={y(max * t)} y2={y(max * t)} />
            ))}
          </g>
        ) : null}
        {body}
        {kind !== `pie` ? (
          <g {...(part(`axis`) as Record<string, string>)} fill="var(--xui-color-mutedForeground)" fontSize={10} textAnchor="middle">
            {categories.map((c, i) => (
              <text key={i} x={x(i)} y={height - 6}>
                {c}
              </text>
            ))}
          </g>
        ) : null}
      </svg>
      {series.length > 1 || kind === `pie` ? (
        <div {...(part(`legend`) as Record<string, string>)}>
          {(kind === `pie` ? categories.map((c, i) => ({ name: c, tone: undefined, i })) : series.map((s, i) => ({ name: s.name, tone: s.tone, i }))).map((e) => (
            <span key={e.i}>
              <i style={{ background: (e.tone && TONE_VAR[e.tone]) || `var(--xui-color-chart${(e.i % 5) + 1})` }} />
              {str(e.name)}
            </span>
          ))}
        </div>
      ) : null}
    </div>
  )
}

export function TreeGuidesNative({ node, props, rootProps }: NativeProps) {
  const part = useParts(node, props)
  const depth = Math.max(0, num(props.depth, 0))
  const elbowAt = props.elbowAt === undefined ? depth - 1 : num(props.elbowAt)
  const tee = bool(props.tee)
  const pass = new Set(arr<number>(props.passThrough).map((v) => num(v)))
  const cols = []
  for (let i = 0; i < depth; i++) {
    const elbow = i === elbowAt
    cols.push(
      <span key={i} {...(part(`line`) as Record<string, string>)} data-col={i}>
        {pass.has(i) ? <i style={{ left: 7, top: 0, bottom: 0, width: 1 }} /> : null}
        {elbow ? <i style={{ left: 7, top: 0, height: tee ? `100%` : `50%`, width: 1 }} /> : null}
        {elbow ? <i style={{ left: 7, top: `50%`, width: 9, height: 1 }} /> : null}
      </span>
    )
  }
  return (
    <span {...(rootProps as Record<string, unknown>)} aria-hidden="true" data-tee={tee ? `true` : undefined}>
      {cols}
    </span>
  )
}
