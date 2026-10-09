// VAPP-87: Ring, Spinner, Skeleton, TreeGuides (Chart: chart.tsx).

import { resolveRecipe, nativeRecipeProps, TREE_GUIDE_BRIDGE, TREE_GUIDE_COLUMN, TREE_GUIDE_RADIUS } from "@exponential-at/ui"
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

export function SkeletonNative({ props, rootProps }: NativeProps) {
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
  const elbowAt = props.elbowAt === undefined || props.elbowAt === null ? depth - 1 : num(props.elbowAt)
  const tee = bool(props.tee)
  const pass = new Set(arr<number>(props.passThrough).map((v) => num(v)))
  const stroke = sizeOf(ctx, `TreeGuides`, `line`, props, 1)
  const line = part(`line`) as Record<string, string>
  const x = TREE_GUIDE_COLUMN / 2 - stroke / 2
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
