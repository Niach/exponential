// VAPP-162 (docs/round-5-contract.md §6, §7): QrCode and QrScanner.
//
// QrCode paints THE CORE'S matrix (`qrMatrix`, never a platform QR library)
// as one SVG path in module units over a `size` px plate, quiet zone 4,
// `shape-rendering: crispEdges` (module edges snap to device pixels, the
// outer size stays exact); of the plate and module recipe colours the
// lighter fills the plate (a dark theme still scans).
//
// QrScanner is the viewfinder CHROME; the camera is the host's: the
// `qrScanner` host option (`HostPlugin.qrScanner.start(feed, formats,
// onValue) → stop`) renders its feed into the `feed` element and reports
// decoded strings, dispatched as `scanned {value}` once per distinct value.
// No host (or a host that refuses) = the placeholder icon + `Camera
// unavailable`; `disabled` stops the camera and dims the chrome.

import { useEffect, useMemo, useRef, useState } from "react"
import { nativeRecipeProps, resolveRecipe } from "@exponential_app/ui"
import { QR_CORNERS, QR_SCANNER, qrCornerPath, qrFeedColour, qrPaint, qrScanFormats, qrScanReporter } from "@exponential_app/ui/web"
import { useSurfaceContext } from "../context"
import { IconGlyph } from "../icons"
import type { NativeProps } from "../node-view"
import { bool, num, str, useParts } from "./shared"

type Ctx = ReturnType<typeof useSurfaceContext>

function recipeOf(ctx: Ctx, component: string, part: string, props: Record<string, unknown>) {
  return resolveRecipe(ctx.theme, { component, part, props: nativeRecipeProps(component, props, ctx.extensionDefs) }, ctx.mode)
}

/** A `$string.<key>` default the reducer left in place → the built-in string. */
function stringProp(ctx: Ctx, raw: unknown, fallbackKey: string): string {
  const s = str(raw)
  if (!s) return ctx.t(fallbackKey)
  return s.startsWith(`$string.`) ? ctx.t(s.slice(`$string.`.length)) : s
}

export function QrCodeNative({ node, props, rootProps }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const size = Math.max(0, num(props.size, 160))
  const value = str(props.value)
  const level = str(props.level, `M`)
  const plateStyle = recipeOf(ctx, `QrCode`, `plate`, props)
  const moduleStyle = recipeOf(ctx, `QrCode`, `module`, props)
  const paint = useMemo(
    () => qrPaint(value, level, String(plateStyle.backgroundColor ?? `#ffffff`), String(moduleStyle.color ?? `#000000`)),
    [value, level, plateStyle.backgroundColor, moduleStyle.color]
  )
  const label = str(props.label)
  return (
    <div {...(rootProps as Record<string, unknown>)} role={label ? `img` : undefined} aria-label={label || undefined} aria-hidden={label ? undefined : `true`} data-empty={paint.matrix ? undefined : `true`}>
      <span {...(part(`plate`) as Record<string, string>)} style={{ width: size, height: size, backgroundColor: paint.plate }}>
        {paint.matrix ? (
          <svg viewBox={`0 0 ${paint.modules} ${paint.modules}`} width={size} height={size} shapeRendering="crispEdges" aria-hidden="true" focusable="false">
            <path {...(part(`module`) as Record<string, string>)} d={paint.path} fill={paint.module} />
          </svg>
        ) : null}
      </span>
      {label ? (
        <span {...(part(`label`) as Record<string, string>)} aria-hidden="true">
          {label}
        </span>
      ) : null}
    </div>
  )
}

export function QrScannerNative({ node, props, rootProps, emit }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const disabled = bool(props.disabled)
  const host = ctx.host.qrScanner
  const formats = qrScanFormats(props.formats)
  const formatsKey = formats.join(`,`)
  const feedRef = useRef<HTMLSpanElement | null>(null)
  const [live, setLive] = useState(false)
  const emitRef = useRef(emit)
  emitRef.current = emit
  const announce = useRef(ctx.announce)
  announce.current = ctx.announce
  useEffect(() => {
    const feed = feedRef.current
    if (!host || disabled || !feed) {
      setLive(false)
      return
    }
    // One reporter per start: a remount (or re-enable) reports a held code again.
    const report = qrScanReporter((value) => {
      announce.current(value)
      void emitRef.current(`scanned`, { value })
    })
    let stop: (() => void) | false | void
    try {
      stop = host.start(feed, formatsKey.split(`,`), report)
    } catch {
      stop = false
    }
    setLive(stop !== false)
    return () => {
      if (typeof stop === `function`) stop()
      feed.replaceChildren()
    }
  }, [host, disabled, formatsKey])
  const label = stringProp(ctx, props.label, `scanQrCode`)
  const radius = num(recipeOf(ctx, `QrScanner`, `viewfinder`, props).borderRadius, 0)
  const feedColour = String(recipeOf(ctx, `QrScanner`, `feed`, props).backgroundColor ?? `#000000`)
  const hint = str(props.hint)
  const camera = Boolean(host) && live
  return (
    <div {...(rootProps as Record<string, unknown>)} aria-disabled={disabled || undefined} data-live={camera ? `true` : undefined}>
      <div {...(part(`viewfinder`) as Record<string, string>)} role="img" aria-label={camera ? label : `${label}, ${ctx.t(`cameraUnavailable`)}`}>
        <span ref={feedRef} {...(part(`feed`) as Record<string, string>)} style={{ backgroundColor: qrFeedColour(feedColour) }} aria-hidden="true" />
        {QR_CORNERS.map((corner) => (
          <svg key={corner} {...(part(`corner`) as Record<string, string>)} data-corner={corner} viewBox={`0 0 ${QR_SCANNER.cornerLength} ${QR_SCANNER.cornerLength}`} aria-hidden="true" focusable="false">
            <path d={qrCornerPath(corner, radius)} fill="none" stroke="currentColor" strokeWidth={QR_SCANNER.cornerStroke} strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        ))}
        {camera ? (
          <span {...(part(`scanLine`) as Record<string, string>)} aria-hidden="true" />
        ) : (
          <span {...(part(`placeholder`) as Record<string, string>)} aria-hidden="true">
            <span {...(part(`placeholderIcon`) as Record<string, string>)}>
              <IconGlyph icons={ctx.host.icons} name={QR_SCANNER.placeholderIcon} width={QR_SCANNER.placeholderIconSize} height={QR_SCANNER.placeholderIconSize} />
            </span>
            <span>{ctx.t(`cameraUnavailable`)}</span>
          </span>
        )}
      </div>
      {hint ? <span {...(part(`hint`) as Record<string, string>)}>{hint}</span> : null}
    </div>
  )
}

