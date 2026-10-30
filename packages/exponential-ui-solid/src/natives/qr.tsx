// VAPP-162 (React's natives/qr.tsx; docs/round-5-contract.md §6, §7):
// QrCode and QrScanner.
//
// QrCode paints THE CORE'S matrix (`qrMatrix`, never a platform QR library)
// as one SVG path in module units over a `size` px plate, quiet zone 4,
// `shape-rendering: crispEdges`; of the plate and module recipe colours the
// lighter fills the plate (`qrPaint`, shared with React).
//
// QrScanner is the viewfinder CHROME; the camera is the host's: the
// `qrScanner` host option (`HostPlugin.qrScanner.start(feed, formats,
// onValue) → stop`) renders its feed into the `feed` element and reports
// decoded strings, dispatched as `scanned {value}` once per distinct value
// (`qrScanReporter`). No host (or a host that refuses) = the placeholder
// icon + `Camera unavailable`; `disabled` stops the camera.

import { For, Show, createEffect, createMemo, createSignal, onCleanup, untrack, type Component, type JSX } from "solid-js"
import { nativeRecipeProps, resolveRecipe } from "@exponential_app/eui"
import { QR_CORNERS, QR_SCANNER, qrCornerPath, qrFeedColour, qrPaint, qrScanFormats, qrScanReporter } from "@exponential_app/eui/web"
import { useSurfaceContext, type SurfaceContextValue } from "../context"
import { IconGlyph } from "../icons"
import type { NativeProps } from "../node-view"
import { bool, css, num, parts, str } from "./shared"

/** React's `focusable="false"` (not in Solid's SVG attribute types). */
const UNFOCUSABLE = { focusable: `false` } as JSX.SvgSVGAttributes<SVGSVGElement>

function recipeOf(ctx: SurfaceContextValue, component: string, part: string, props: Record<string, unknown>) {
  return resolveRecipe(ctx.theme, { component, part, props: nativeRecipeProps(component, props, ctx.extensionDefs) }, ctx.mode)
}

/** A `$string.<key>` default the reducer left in place → the built-in string. */
function stringProp(ctx: SurfaceContextValue, raw: unknown, fallbackKey: string): string {
  const s = str(raw)
  if (!s) return ctx.t(fallbackKey)
  return s.startsWith(`$string.`) ? ctx.t(s.slice(`$string.`.length)) : s
}

export function QrCodeNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const size = () => Math.max(0, num(props.props.size, 160))
  const plateColour = () => String(recipeOf(ctx, `QrCode`, `plate`, props.props).backgroundColor ?? `#ffffff`)
  const moduleColour = () => String(recipeOf(ctx, `QrCode`, `module`, props.props).color ?? `#000000`)
  const paint = createMemo(() => qrPaint(str(props.props.value), str(props.props.level, `M`), plateColour(), moduleColour()))
  const label = () => str(props.props.label)
  return (
    <div {...props.rootProps} role={label() ? `img` : undefined} aria-label={label() || undefined} aria-hidden={label() ? undefined : `true`} data-empty={paint().matrix ? undefined : `true`}>
      <span {...part(`plate`)} style={css({ width: size(), height: size(), backgroundColor: paint().plate })}>
        <Show when={paint().matrix}>
          <svg viewBox={`0 0 ${paint().modules} ${paint().modules}`} width={size()} height={size()} shape-rendering="crispEdges" aria-hidden="true" {...UNFOCUSABLE}>
            <path {...part(`module`)} d={paint().path} fill={paint().module} />
          </svg>
        </Show>
      </span>
      <Show when={label()}>
        <span {...part(`label`)} aria-hidden="true">
          {label()}
        </span>
      </Show>
    </div>
  )
}

export function QrScannerNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const disabled = () => bool(props.props.disabled)
  const formatsKey = createMemo(() => qrScanFormats(props.props.formats).join(`,`))
  let feed: HTMLSpanElement | undefined
  const [live, setLive] = createSignal(false)
  createEffect(() => {
    const host = ctx.host.qrScanner
    const off = disabled()
    const formats = formatsKey().split(`,`)
    const el = feed
    if (!host || off || !el) {
      setLive(false)
      return
    }
    // One reporter per start: a remount (or re-enable) reports a held code again.
    const report = qrScanReporter((value) => {
      ctx.announce(value)
      void props.emit(`scanned`, { value })
    })
    let stop: (() => void) | false | void
    try {
      stop = untrack(() => host.start(el, formats, report))
    } catch {
      stop = false
    }
    setLive(stop !== false)
    onCleanup(() => {
      if (typeof stop === `function`) stop()
      el.replaceChildren()
    })
  })
  const label = () => stringProp(ctx, props.props.label, `scanQrCode`)
  const radius = () => num(recipeOf(ctx, `QrScanner`, `viewfinder`, props.props).borderRadius, 0)
  const feedColour = () => String(recipeOf(ctx, `QrScanner`, `feed`, props.props).backgroundColor ?? `#000000`)
  const hint = () => str(props.props.hint)
  const camera = () => Boolean(ctx.host.qrScanner) && live()
  return (
    <div {...props.rootProps} aria-disabled={disabled() || undefined} data-live={camera() ? `true` : undefined}>
      <div {...part(`viewfinder`)} role="img" aria-label={camera() ? label() : `${label()}, ${ctx.t(`cameraUnavailable`)}`}>
        <span ref={feed} {...part(`feed`)} style={css({ backgroundColor: qrFeedColour(feedColour()) })} aria-hidden="true" />
        <For each={QR_CORNERS}>
          {(corner) => (
            <svg {...part(`corner`)} data-corner={corner} viewBox={`0 0 ${QR_SCANNER.cornerLength} ${QR_SCANNER.cornerLength}`} aria-hidden="true" {...UNFOCUSABLE}>
              <path d={qrCornerPath(corner, radius())} fill="none" stroke="currentColor" stroke-width={QR_SCANNER.cornerStroke} stroke-linecap="round" stroke-linejoin="round" />
            </svg>
          )}
        </For>
        <Show
          when={camera()}
          fallback={
            <span {...part(`placeholder`)} aria-hidden="true">
              <span {...part(`placeholderIcon`)}>
                <IconGlyph icons={ctx.host.icons} name={QR_SCANNER.placeholderIcon} width={QR_SCANNER.placeholderIconSize} height={QR_SCANNER.placeholderIconSize} />
              </span>
              <span>{ctx.t(`cameraUnavailable`)}</span>
            </span>
          }
        >
          <span {...part(`scanLine`)} aria-hidden="true" />
        </Show>
      </div>
      <Show when={hint()}>
        <span {...part(`hint`)}>{hint()}</span>
      </Show>
    </div>
  )
}

export const NATIVES_QR: Record<string, Component<NativeProps>> = {
  QrCode: QrCodeNative,
  QrScanner: QrScannerNative,
}
