// VAPP-87: Ring, Spinner, Skeleton, TreeGuides (Chart: chart.tsx).

import { resolveRecipe, nativeRecipeProps, TREE_GUIDE_COLUMN } from "@exponential-at/ui"
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

/** TreeGuides (round 2 §7): `depth` columns of `treeGuideColumn` (16) px,
 *  stretched to the row; the `TreeGuides/line` recipe `width` is the
 *  STROKE of the lines drawn in them (pass-through, elbow, tee). */
export function TreeGuidesNative({ node, props, rootProps }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const depth = Math.max(0, num(props.depth, 0))
  const elbowAt = props.elbowAt === undefined ? depth - 1 : num(props.elbowAt)
  const tee = bool(props.tee)
  const pass = new Set(arr<number>(props.passThrough).map((v) => num(v)))
  const stroke = sizeOf(ctx, `TreeGuides`, `line`, props, 1)
  const line = part(`line`) as Record<string, string>
  const vertical = (key: string, style: React.CSSProperties) => <i key={key} {...line} style={{ left: `calc(50% - ${stroke / 2}px)`, width: stroke, ...style }} />
  const cols = []
  for (let i = 0; i < depth; i++) {
    const elbow = i === elbowAt
    cols.push(
      <span key={i} className="xui-tree-col" data-col={i} style={{ width: TREE_GUIDE_COLUMN }}>
        {pass.has(i) ? vertical(`p`, { top: 0, bottom: 0 }) : null}
        {elbow ? vertical(`e`, { top: 0, height: tee ? `100%` : `50%` }) : null}
        {elbow ? <i {...line} style={{ left: `50%`, top: `calc(50% - ${stroke / 2}px)`, width: `50%`, height: stroke }} /> : null}
      </span>
    )
  }
  return (
    <span {...(rootProps as Record<string, unknown>)} aria-hidden="true" data-tee={tee ? `true` : undefined}>
      {cols}
    </span>
  )
}
