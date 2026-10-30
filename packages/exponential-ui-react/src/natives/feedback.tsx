// VAPP-87: Ring, Spinner, Skeleton, TreeGuides, VoiceWave (Chart: chart.tsx).

import { useEffect, useLayoutEffect, useRef } from "react"
import { resolveRecipe, nativeRecipeProps, voiceWave, voiceWaveBars, voiceWaveChildBox, voiceWavePath, voiceWaveVariant, TREE_GUIDE_BRIDGE, TREE_GUIDE_COLUMN, TREE_GUIDE_RADIUS, VOICE_WAVE_ACTIVITIES } from "@exponential_app/eui"
import type { VoiceWaveActivity, VoiceWaveSizeName } from "@exponential_app/eui"
import { voiceWaveAnimates, voiceWaveClock, voiceWaveFitChild, voiceWaveName } from "@exponential_app/eui/web"
import { useSurfaceContext } from "../context"
import { CHROME } from "../icons"
import type { NativeProps } from "../node-view"
import { mergeStyle } from "../node-view"
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
    <span {...(rootProps as Record<string, unknown>)} role="progressbar" aria-valuemin={0} aria-valuemax={1} aria-valuenow={value} aria-label={label || undefined} style={mergeStyle(rootProps, { width: size, height: size })}>
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
  const label = str(props.label) || ctx.t(`loading`)
  return (
    <span {...(rootProps as Record<string, unknown>)} role="status" aria-label={label} style={mergeStyle(rootProps, { width: size, height: size })}>
      <Icon aria-hidden="true" />
    </span>
  )
}

/** The bars' widths per placeholder row (lead, title): the core's
 *  `SKELETON_BARS`, so a stack never reads as one ruled block. */
const SKELETON_BARS: readonly [number, number][] = [
  [56, 168],
  [56, 120],
  [56, 192],
]

/** Skeleton: one pulsing block, or (EXP-1259, feedback-states.json
 *  `placeholder`) `rows` placeholder rows in the row geometry: each a `dot`
 *  (unless `dot: false`) and two `bar`s, hairline-divided; the root paints
 *  nothing itself, the pieces pulse (base CSS). */
export function SkeletonNative({ node, props, rootProps }: NativeProps) {
  const part = useParts(node, props)
  const rows = Math.min(50, Math.floor(num(props.rows, 0)))
  if (rows >= 1) {
    const dot = props.dot === undefined ? true : bool(props.dot)
    const height = props.rowHeight === undefined ? `var(--xui-control-row)` : `${num(props.rowHeight)}px`
    return (
      <div {...(rootProps as Record<string, unknown>)} data-rows="" aria-hidden="true">
        {Array.from({ length: rows }, (_, r) => {
          const [lead, title] = SKELETON_BARS[r % SKELETON_BARS.length]
          return (
            <div key={r} {...(part.at(`row`, r) as Record<string, string>)} style={{ height }}>
              {dot ? <span {...(part.at(`dot`, r) as Record<string, string>)} /> : null}
              <span {...(part.at(`bar`, `${r}.0`) as Record<string, string>)} style={{ width: lead }} />
              <span {...(part.at(`bar`, `${r}.1`) as Record<string, string>)} data-title="" style={{ width: title }} />
            </div>
          )
        })}
      </div>
    )
  }
  const width = str(props.width, `100%`)
  const height = str(props.height, `16px`)
  return <div {...(rootProps as Record<string, unknown>)} aria-hidden="true" style={mergeStyle(rootProps, { width, height, borderRadius: bool(props.rounded) ? 9999 : undefined })} />
}

/** TreeGuides (round 3, a Row's `guides` part the CORE fills: `elbowAt`,
 *  `tee`, `passThrough` arrive computed): `depth` columns of
 *  `TREE_GUIDE_COLUMN` (14) px; column i's line sits at its centre
 *  (x = i·14 + 7). The elbow = ONE element whose left + bottom borders are
 *  the vertical (from `TREE_GUIDE_BRIDGE` px above the row's top to its
 *  centre) and the stub (to the column's right edge), the corner rounded by
 *  `TREE_GUIDE_RADIUS`; `tee` and pass-through columns = full-height
 *  verticals from the same bridge (paint only: the overshoot covers a
 *  Section's hairline divider, never layout). Stroke width + colour = the
 *  `TreeGuides/line` recipe. Physical x (the gutter is not mirrored). */
export function TreeGuidesNative({ node, props, rootProps }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const depth = Math.max(0, Math.floor(num(props.depth, 0)))
  // The core fills `elbowAt`; a row it never saw (a lone root, a template item) draws no elbow.
  const elbowAt = props.elbowAt === undefined || props.elbowAt === null ? -1 : num(props.elbowAt)
  const tee = bool(props.tee)
  const pass = new Set(arr<number>(props.passThrough).map((v) => num(v)))
  const stroke = sizeOf(ctx, `TreeGuides`, `line`, props, 1)
  const line = part(`line`) as Record<string, string>
  // The line's LEFT edge sits at i·14 + 7 (the reading of the three native painters).
  const x = TREE_GUIDE_COLUMN / 2
  const vertical = (key: string) => <i key={key} {...line} data-vertical="" style={{ left: x, width: stroke, top: -TREE_GUIDE_BRIDGE, bottom: 0 }} />
  const cols = []
  for (let i = 0; i < depth; i++) {
    const elbow = i === elbowAt
    cols.push(
      <span key={i} className="xui-tree-col" data-col={i} style={{ width: TREE_GUIDE_COLUMN }}>
        {pass.has(i) && !elbow ? vertical(`p`) : null}
        {elbow && tee ? vertical(`t`) : null}
        {elbow ? (
          <i
            key="e"
            {...line}
            data-elbow=""
            style={{
              left: x,
              top: -TREE_GUIDE_BRIDGE,
              width: TREE_GUIDE_COLUMN - x,
              height: `calc(50% + ${TREE_GUIDE_BRIDGE + stroke / 2}px)`,
              borderLeftWidth: stroke,
              borderBottomWidth: stroke,
              borderBottomLeftRadius: TREE_GUIDE_RADIUS,
            }}
          />
        ) : null}
      </span>
    )
  }
  return (
    <span {...(rootProps as Record<string, unknown>)} aria-hidden="true" data-tee={tee ? `true` : undefined}>
      {cols}
    </span>
  )
}

/** NEW-77: VoiceWave (catalog/style.json `voiceWave`): one SVG of three
 *  closed waves, the core's `voiceWave` per frame, each ring filled and
 *  stroked in its part's recipe colour at the ring's alphas; the root
 *  recipe sizes the box and sets the opacity (CSS); the ONE child sits
 *  centred at its own size, drawn scaled about the centre by
 *  `voiceWaveChildBox` (never up; at `sm` kept but unpainted). While
 *  listening, thinking or speaking (and not under reduced motion) the
 *  frames write the paths' `d` directly, never a re-render; the elapsed
 *  seconds count from the moment the wave started moving. */
export function VoiceWaveNative(p: NativeProps) {
  return voiceWaveVariant(p.props.variant) === `bars` ? <VoiceWaveBarsNative {...p} /> : <VoiceWaveRingNative {...p} />
}

/** VAPP-162 (round-5 contract §5): `variant: bars`, the horizontal level
 *  meter: the core's `voiceWaveBars` per frame as rounded rects in the
 *  `bar` part colour; the root recipe sizes the box; a child stays in the
 *  tree, never painted. While moving, the frames write each rect's `y` and
 *  `height` directly, never a re-render. */
function VoiceWaveBarsNative({ node, props, rootProps, children }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const activity: VoiceWaveActivity = (VOICE_WAVE_ACTIVITIES as readonly string[]).includes(str(props.activity)) ? (str(props.activity) as VoiceWaveActivity) : `idle`
  const input = { size: str(props.size, `md`) as VoiceWaveSizeName, activity, level: num(props.level, 0), bands: arr<unknown>(props.bands).map((b) => num(b, 0)), reducedMotion: ctx.reducedMotion }
  const latest = useRef(input)
  latest.current = input
  const elapsed = useRef(0)
  const rects = useRef<(SVGRectElement | null)[]>([])
  const animates = voiceWaveAnimates(activity) && !ctx.reducedMotion
  useEffect(() => {
    if (!animates) {
      elapsed.current = 0
      return
    }
    return voiceWaveClock((t) => {
      elapsed.current = t
      voiceWaveBars({ ...latest.current, elapsed: t }).bars.forEach((bar, i) => {
        const el = rects.current[i]
        if (!el) return
        el.setAttribute(`y`, String(bar.y))
        el.setAttribute(`height`, String(bar.height))
      })
    })
  }, [animates])
  const meter = voiceWaveBars({ ...input, elapsed: elapsed.current })
  const colour = String(resolveRecipe(ctx.theme, { component: `VoiceWave`, part: `bar`, props: nativeRecipeProps(`VoiceWave`, props) }, ctx.mode).color ?? `currentColor`)
  const name = voiceWaveName(str(props.label) || undefined, activity)
  return (
    <span {...(rootProps as Record<string, unknown>)} role={name ? `img` : undefined} aria-label={name} aria-hidden={name ? undefined : `true`} data-variant="bars" style={mergeStyle(rootProps, { width: meter.width, height: meter.height })}>
      <svg viewBox={`0 0 ${meter.width} ${meter.height}`} width={meter.width} height={meter.height} aria-hidden="true" style={{ color: colour }}>
        {meter.bars.map((bar, i) => (
          <rect
            key={i}
            ref={(el) => {
              rects.current[i] = el
            }}
            {...(part(`bar`) as Record<string, string>)}
            x={bar.x}
            y={bar.y}
            width={bar.width}
            height={bar.height}
            rx={bar.radius}
            fill="currentColor"
          />
        ))}
      </svg>
      {node.children.length > 0 ? (
        <span {...(part(`child`) as Record<string, string>)} data-hidden="true" aria-hidden="true">
          {children}
        </span>
      ) : null}
    </span>
  )
}

function VoiceWaveRingNative({ node, props, rootProps, children }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const size = sizeOf(ctx, `VoiceWave`, `root`, props, 56)
  const activity: VoiceWaveActivity = (VOICE_WAVE_ACTIVITIES as readonly string[]).includes(str(props.activity)) ? (str(props.activity) as VoiceWaveActivity) : `idle`
  const input = { size, activity, level: num(props.level, 0), bands: arr<unknown>(props.bands).map((b) => num(b, 0)), reducedMotion: ctx.reducedMotion }
  const latest = useRef(input)
  latest.current = input
  const elapsed = useRef(0)
  const paths = useRef<(SVGPathElement | null)[]>([])
  const animates = voiceWaveAnimates(activity) && !ctx.reducedMotion
  useEffect(() => {
    if (!animates) {
      elapsed.current = 0
      return
    }
    return voiceWaveClock((t) => {
      elapsed.current = t
      voiceWave({ ...latest.current, elapsed: t }).rings.forEach((ring, k) => paths.current[k]?.setAttribute(`d`, voiceWavePath(ring)))
    })
  }, [animates])
  const childEl = useRef<HTMLSpanElement | null>(null)
  const hasChild = node.children.length > 0
  useLayoutEffect(() => (hasChild && childEl.current ? voiceWaveFitChild(childEl.current, (c) => voiceWaveChildBox(size, c).scale) : undefined), [hasChild, size])
  const wave = voiceWave({ ...input, elapsed: elapsed.current })
  const recipeProps = nativeRecipeProps(`VoiceWave`, props)
  const colour = (name: string) => String(resolveRecipe(ctx.theme, { component: `VoiceWave`, part: name, props: recipeProps }, ctx.mode).color ?? `currentColor`)
  const name = voiceWaveName(str(props.label) || undefined, activity)
  return (
    <span {...(rootProps as Record<string, unknown>)} role={name ? `img` : undefined} aria-label={name} aria-hidden={name ? undefined : `true`} style={mergeStyle(rootProps, { width: size, height: size })}>
      <svg viewBox={`0 0 ${size} ${size}`} width={size} height={size} aria-hidden="true">
        {wave.rings.map((ring, k) => (
          <path
            key={ring.part}
            ref={(el) => {
              paths.current[k] = el
            }}
            {...(part(ring.part) as Record<string, string>)}
            d={voiceWavePath(ring)}
            fill="currentColor"
            fillOpacity={ring.fillAlpha}
            stroke="currentColor"
            strokeOpacity={ring.strokeAlpha}
            strokeWidth={ring.strokeWidth}
            strokeLinejoin="round"
            style={{ color: colour(ring.part) }}
          />
        ))}
      </svg>
      {hasChild ? (
        <span ref={childEl} {...(part(`child`) as Record<string, string>)} data-hidden={wave.childSize > 0 ? undefined : `true`} aria-hidden={wave.childSize > 0 ? undefined : `true`} style={{ color: colour(`child`) }}>
          {children}
        </span>
      ) : null}
    </span>
  )
}
