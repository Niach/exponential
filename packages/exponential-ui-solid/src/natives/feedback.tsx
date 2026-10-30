// FEED-128 (React's natives/feedback.tsx): Ring, Spinner, Skeleton,
// TreeGuides, VoiceWave (Chart: chart.tsx). Sizes and strokes come from the theme's
// recipes (`resolveRecipe`), exactly as React reads them.

import { For, Index, Show, createEffect, createMemo, createSignal, onCleanup, type JSX } from "solid-js"
import { resolveRecipe, nativeRecipeProps, voiceWave, voiceWaveBars, voiceWaveChildBox, voiceWavePath, voiceWaveVariant, TREE_GUIDE_BRIDGE, TREE_GUIDE_COLUMN, TREE_GUIDE_RADIUS, VOICE_WAVE_ACTIVITIES } from "@exponential_app/eui"
import type { VoiceWaveActivity, VoiceWaveSizeName } from "@exponential_app/eui"
import { voiceWaveAnimates, voiceWaveClock, voiceWaveFitChild, voiceWaveName } from "@exponential_app/eui/web"
import type { Component } from "solid-js"
import { useSurfaceContext, type SurfaceContextValue } from "../context"
import { CHROME, PaintIcon } from "../icons"
import { mergeStyle, type NativeProps } from "../node-view"
import { arr, bool, css, num, parts, str } from "./shared"

function sizeOf(ctx: SurfaceContextValue, component: string, part: string, props: Record<string, unknown>, fallback: number): number {
  const style = resolveRecipe(ctx.theme, { component, part, props: nativeRecipeProps(component, props, ctx.extensionDefs) }, ctx.mode)
  return typeof style.width === `number` ? style.width : fallback
}

export function RingNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const geo = createMemo(() => {
    const size = sizeOf(ctx, `Ring`, `root`, props.props, 32)
    const value = Math.max(0, Math.min(1, num(props.props.value, 0)))
    const trackStyle = resolveRecipe(ctx.theme, { component: `Ring`, part: `track`, props: nativeRecipeProps(`Ring`, props.props) }, ctx.mode)
    const fillStyle = resolveRecipe(ctx.theme, { component: `Ring`, part: `fill`, props: nativeRecipeProps(`Ring`, props.props) }, ctx.mode)
    const stroke = typeof trackStyle.borderWidth === `number` ? trackStyle.borderWidth : 2
    const r = size / 2 - stroke / 2
    const c = 2 * Math.PI * r
    return { size, value, trackStyle, fillStyle, stroke, r, c, fillStroke: typeof fillStyle.borderWidth === `number` ? fillStyle.borderWidth : stroke }
  })
  const label = () => str(props.props.label)
  return (
    <span {...props.rootProps} role="progressbar" aria-valuemin={0} aria-valuemax={1} aria-valuenow={geo().value} aria-label={label() || undefined} style={mergeStyle(props.rootProps, { width: geo().size, height: geo().size })}>
      <svg viewBox={`0 0 ${geo().size} ${geo().size}`} width={geo().size} height={geo().size}>
        <circle {...part(`track`)} cx={geo().size / 2} cy={geo().size / 2} r={geo().r} fill="none" stroke="currentColor" stroke-width={geo().stroke} style={css({ color: String(geo().trackStyle.color ?? `currentColor`) })} />
        <circle
          {...part(`fill`)}
          cx={geo().size / 2}
          cy={geo().size / 2}
          r={geo().r}
          fill="none"
          stroke="currentColor"
          stroke-width={geo().fillStroke}
          stroke-dasharray={`${geo().c * geo().value} ${geo().c}`}
          stroke-linecap="round"
          transform={`rotate(-90 ${geo().size / 2} ${geo().size / 2})`}
          style={css({ color: String(geo().fillStyle.color ?? `currentColor`) })}
        />
      </svg>
      <Show when={label()}>
        <span {...part(`label`)}>{label()}</span>
      </Show>
    </span>
  )
}

export function SpinnerNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const size = () => sizeOf(ctx, `Spinner`, `root`, props.props, 20)
  const label = () => str(props.props.label) || ctx.t(`loading`)
  return (
    <span {...props.rootProps} role="status" aria-label={label()} style={mergeStyle(props.rootProps, { width: size(), height: size() })}>
      <PaintIcon icon={CHROME.spinner} aria-hidden="true" />
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
export function SkeletonNative(props: NativeProps): JSX.Element {
  const part = parts(props)
  const rows = () => Math.min(50, Math.floor(num(props.props.rows, 0)))
  const block = () => {
    const width = str(props.props.width, `100%`)
    const height = str(props.props.height, `16px`)
    return <div {...props.rootProps} aria-hidden="true" style={mergeStyle(props.rootProps, { width, height, borderRadius: bool(props.props.rounded) ? 9999 : undefined })} />
  }
  const dot = () => (props.props.dot === undefined ? true : bool(props.props.dot))
  const height = () => (props.props.rowHeight === undefined ? `var(--xui-control-row)` : `${num(props.props.rowHeight)}px`)
  const indexes = createMemo(() => Array.from({ length: Math.max(0, rows()) }, (_, r) => r))
  return (
    <Show when={rows() >= 1} fallback={block()}>
      <div {...props.rootProps} data-rows="" aria-hidden="true">
        <For each={indexes()}>
          {(r) => {
            const [lead, title] = SKELETON_BARS[r % SKELETON_BARS.length]
            return (
              <div {...part.at(`row`, r)} style={css({ height: height() })}>
                <Show when={dot()}>
                  <span {...part.at(`dot`, r)} />
                </Show>
                <span {...part.at(`bar`, `${r}.0`)} style={css({ width: lead })} />
                <span {...part.at(`bar`, `${r}.1`)} data-title="" style={css({ width: title })} />
              </div>
            )
          }}
        </For>
      </div>
    </Show>
  )
}

/** TreeGuides (round 3, a Row's `guides` part the CORE fills: `elbowAt`,
 *  `tee`, `passThrough` arrive computed): `depth` columns of
 *  `TREE_GUIDE_COLUMN` (14) px; column i's line sits at its centre. The
 *  elbow = ONE element whose left + bottom borders are the vertical and the
 *  stub, the corner rounded by `TREE_GUIDE_RADIUS`; `tee` and pass-through
 *  columns = full-height verticals from the same bridge. Stroke width +
 *  colour = the `TreeGuides/line` recipe. Physical x (never mirrored). */
export function TreeGuidesNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const depth = () => Math.max(0, Math.floor(num(props.props.depth, 0)))
  // The core fills `elbowAt`; a row it never saw (a lone root, a template item) draws no elbow.
  const elbowAt = () => (props.props.elbowAt === undefined || props.props.elbowAt === null ? -1 : num(props.props.elbowAt))
  const tee = () => bool(props.props.tee)
  const pass = createMemo(() => new Set(arr<number>(props.props.passThrough).map((v) => num(v))))
  const stroke = () => sizeOf(ctx, `TreeGuides`, `line`, props.props, 1)
  // The line's LEFT edge sits at i·14 + 7 (the reading of the three native painters).
  const x = TREE_GUIDE_COLUMN / 2
  const vertical = () => <i {...part(`line`)} data-vertical="" style={css({ left: x, width: stroke(), top: -TREE_GUIDE_BRIDGE, bottom: 0 })} />
  const cols = createMemo(() => Array.from({ length: depth() }, (_, i) => i))
  return (
    <span {...props.rootProps} aria-hidden="true" data-tee={tee() ? `true` : undefined}>
      <For each={cols()}>
        {(i) => {
          const elbow = () => i === elbowAt()
          return (
            <span class="xui-tree-col" data-col={i} style={css({ width: TREE_GUIDE_COLUMN })}>
              {pass().has(i) && !elbow() ? vertical() : null}
              {elbow() && tee() ? vertical() : null}
              {elbow() ? (
                <i
                  {...part(`line`)}
                  data-elbow=""
                  style={css({
                    left: x,
                    top: -TREE_GUIDE_BRIDGE,
                    width: TREE_GUIDE_COLUMN - x,
                    height: `calc(50% + ${TREE_GUIDE_BRIDGE + stroke() / 2}px)`,
                    borderLeftWidth: stroke(),
                    borderBottomWidth: stroke(),
                    borderBottomLeftRadius: TREE_GUIDE_RADIUS,
                  })}
                />
              ) : null}
            </span>
          )
        }}
      </For>
    </span>
  )
}

/** NEW-77: VoiceWave (React's `VoiceWaveNative`): one SVG of three closed
 *  waves, the core's `voiceWave` per frame, each ring in its part's recipe
 *  colour at the ring's alphas; the root recipe sizes the box and sets the
 *  opacity (CSS); the ONE child sits centred at its own size, drawn scaled
 *  about the centre by `voiceWaveChildBox` (never up; at `sm` kept but
 *  unpainted). While listening, thinking or speaking (and
 *  not under reduced motion) a frame clock drives the phase; the elapsed
 *  seconds count from the moment the wave started moving. */
export function VoiceWaveNative(props: NativeProps): JSX.Element {
  return (
    <Show when={voiceWaveVariant(props.props.variant) === `bars`} fallback={<VoiceWaveRingNative {...props} />}>
      <VoiceWaveBarsNative {...props} />
    </Show>
  )
}

/** VAPP-162 (round-5 contract §5; React's `VoiceWaveBarsNative`):
 *  `variant: bars`, the horizontal level meter: the core's `voiceWaveBars`
 *  per frame as rounded rects in the `bar` part colour; the root recipe
 *  sizes the box; a child stays in the tree, never painted. While moving,
 *  the frame clock moves each rect's `y` and `height` (fine-grained: only
 *  those attributes are written). */
function VoiceWaveBarsNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const activity = createMemo((): VoiceWaveActivity => {
    const a = str(props.props.activity)
    return (VOICE_WAVE_ACTIVITIES as readonly string[]).includes(a) ? (a as VoiceWaveActivity) : `idle`
  })
  const [elapsed, setElapsed] = createSignal(0)
  const animates = createMemo(() => voiceWaveAnimates(activity()) && !ctx.reducedMotion)
  createEffect(() => {
    if (!animates()) {
      setElapsed(0)
      return
    }
    onCleanup(voiceWaveClock(setElapsed))
  })
  const meter = createMemo(() =>
    voiceWaveBars({
      size: str(props.props.size, `md`) as VoiceWaveSizeName,
      activity: activity(),
      level: num(props.props.level, 0),
      bands: arr<unknown>(props.props.bands).map((b) => num(b, 0)),
      reducedMotion: ctx.reducedMotion,
      elapsed: elapsed(),
    })
  )
  const colour = () => String(resolveRecipe(ctx.theme, { component: `VoiceWave`, part: `bar`, props: nativeRecipeProps(`VoiceWave`, props.props) }, ctx.mode).color ?? `currentColor`)
  const name = () => voiceWaveName(str(props.props.label) || undefined, activity())
  return (
    <span {...props.rootProps} role={name() ? `img` : undefined} aria-label={name()} aria-hidden={name() ? undefined : `true`} data-variant="bars" style={mergeStyle(props.rootProps, { width: meter().width, height: meter().height })}>
      <svg viewBox={`0 0 ${meter().width} ${meter().height}`} width={meter().width} height={meter().height} aria-hidden="true" style={css({ color: colour() })}>
        <Index each={meter().bars}>{(bar) => <rect {...part(`bar`)} x={bar().x} y={bar().y} width={bar().width} height={bar().height} rx={bar().radius} fill="currentColor" />}</Index>
      </svg>
      <Show when={props.node.children.length > 0}>
        <span {...part(`child`)} data-hidden="true" aria-hidden="true">
          {props.children}
        </span>
      </Show>
    </span>
  )
}

function VoiceWaveRingNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const activity = createMemo((): VoiceWaveActivity => {
    const a = str(props.props.activity)
    return (VOICE_WAVE_ACTIVITIES as readonly string[]).includes(a) ? (a as VoiceWaveActivity) : `idle`
  })
  const size = () => sizeOf(ctx, `VoiceWave`, `root`, props.props, 56)
  const [elapsed, setElapsed] = createSignal(0)
  const animates = createMemo(() => voiceWaveAnimates(activity()) && !ctx.reducedMotion)
  createEffect(() => {
    if (!animates()) {
      setElapsed(0)
      return
    }
    onCleanup(voiceWaveClock(setElapsed))
  })
  const wave = createMemo(() =>
    voiceWave({
      size: size(),
      activity: activity(),
      level: num(props.props.level, 0),
      bands: arr<unknown>(props.props.bands).map((b) => num(b, 0)),
      reducedMotion: ctx.reducedMotion,
      elapsed: elapsed(),
    })
  )
  const colour = (name: string) => String(resolveRecipe(ctx.theme, { component: `VoiceWave`, part: name, props: nativeRecipeProps(`VoiceWave`, props.props) }, ctx.mode).color ?? `currentColor`)
  const name = () => voiceWaveName(str(props.props.label) || undefined, activity())
  return (
    <span {...props.rootProps} role={name() ? `img` : undefined} aria-label={name()} aria-hidden={name() ? undefined : `true`} style={mergeStyle(props.rootProps, { width: size(), height: size() })}>
      <svg viewBox={`0 0 ${size()} ${size()}`} width={size()} height={size()} aria-hidden="true">
        <Index each={wave().rings}>
          {(ring) => (
            <path
              {...part(ring().part)}
              d={voiceWavePath(ring())}
              fill="currentColor"
              fill-opacity={ring().fillAlpha}
              stroke="currentColor"
              stroke-opacity={ring().strokeAlpha}
              stroke-width={ring().strokeWidth}
              stroke-linejoin="round"
              style={css({ color: colour(ring().part) })}
            />
          )}
        </Index>
      </svg>
      <Show when={props.node.children.length > 0}>
        <span
          ref={(el) => createEffect(() => {
            const px = size()
            onCleanup(voiceWaveFitChild(el, (c) => voiceWaveChildBox(px, c).scale))
          })}
          {...part(`child`)}
          data-hidden={wave().childSize > 0 ? undefined : `true`}
          aria-hidden={wave().childSize > 0 ? undefined : `true`}
          style={css({ color: colour(`child`) })}
        >
          {props.children}
        </span>
      </Show>
    </span>
  )
}

export const NATIVES_FEEDBACK: Record<string, Component<NativeProps>> = {
  Ring: RingNative,
  Spinner: SpinnerNative,
  Skeleton: SkeletonNative,
  VoiceWave: VoiceWaveNative,
  TreeGuides: TreeGuidesNative,
}
