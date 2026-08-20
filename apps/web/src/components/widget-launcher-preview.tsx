import { useEffect, useRef, useState } from "react"
import { createPortal } from "react-dom"
import {
  buttonCss,
  megaphoneIconSvg,
  paletteFor,
  pickForeground,
} from "@exp/widget/theme"
import type { ThemeMode } from "@exp/widget/theme"
import { launcherButtonClass, launcherOrigin } from "@exp/widget/launcher"
import type {
  WidgetLauncherMode,
  WidgetLauncherPosition,
} from "@exp/widget/types"
import { cn } from "@/lib/utils"

// The launcher preview shared by the widget settings dialog and the
// getting-started widget card. Since EXP-569 it is the REAL launcher: the
// shipped `buttonCss` stylesheet plus the widget's own markup inside a shadow
// root, so the icon-only rest state, the hover label reveal, and the edge-tab
// nudge behave exactly as embedders see them — nothing here can drift.

export const DEFAULT_ACCENT = paletteFor(`dark`).defaultAccent

export const previewForeground = pickForeground

export function WidgetLauncherPreview({
  accentColor,
  label,
  theme = `dark`,
  mode = `fab`,
  position = `bottom-right`,
  iconSvg,
  className,
}: {
  accentColor?: string
  label?: string
  theme?: ThemeMode
  mode?: WidgetLauncherMode
  position?: WidgetLauncherPosition
  iconSvg?: string | null
  className?: string
}) {
  const hostRef = useRef<HTMLSpanElement>(null)
  const [container, setContainer] = useState<HTMLElement | null>(null)
  const styleRef = useRef<HTMLStyleElement | null>(null)

  const accent = accentColor || paletteFor(theme).defaultAccent

  useEffect(() => {
    const host = hostRef.current
    if (!host || host.shadowRoot) return
    const root = host.attachShadow({ mode: `open` })
    const style = document.createElement(`style`)
    root.appendChild(style)
    styleRef.current = style
    const mount = document.createElement(`span`)
    mount.style.display = `inline-flex`
    root.appendChild(mount)
    setContainer(mount)
  }, [])

  // buttonCss bakes accent + palette literals, so it re-renders per change
  // (mirroring the loader's destructive restyle).
  useEffect(() => {
    if (styleRef.current) styleRef.current.textContent = buttonCss(accent, theme)
  }, [accent, theme])

  const button = (
    <button
      type="button"
      tabIndex={-1}
      className={launcherButtonClass({ mode, position })}
      style={
        mode === `fab`
          ? { transformOrigin: launcherOrigin(position) }
          : undefined
      }
      aria-label="Send feedback"
    >
      <span
        style={{ display: `flex` }}
        dangerouslySetInnerHTML={{ __html: iconSvg ?? megaphoneIconSvg }}
      />
      {mode === `fab` ? (
        <span className="exp-fab-label">{label || `Feedback`}</span>
      ) : null}
    </button>
  )

  return (
    <span ref={hostRef} className={cn(`inline-flex`, className)}>
      {container ? createPortal(button, container) : null}
    </span>
  )
}

// The dialog's mock viewport: a bordered frame with the live launcher pinned
// at its configured position (tabs sit flush against the frame edge, fabs
// keep a margin) — the position picker's visual echo.
export function WidgetLauncherPreviewViewport({
  mode,
  position,
  ...launcherProps
}: {
  accentColor?: string
  label?: string
  theme?: ThemeMode
  mode: WidgetLauncherMode
  position: WidgetLauncherPosition
  iconSvg?: string | null
}) {
  const vertical = position.startsWith(`top`)
    ? `top-3`
    : position.startsWith(`middle`)
      ? `top-1/2 -translate-y-1/2`
      : `bottom-3`
  const horizontal = position.endsWith(`left`)
    ? mode === `tab`
      ? `left-0`
      : `left-3`
    : mode === `tab`
      ? `right-0`
      : `right-3`
  return (
    <div className="relative h-40 overflow-hidden rounded-md border bg-muted/30">
      <WidgetLauncherPreview
        mode={mode}
        position={position}
        {...launcherProps}
        className={cn(`absolute`, vertical, horizontal)}
      />
    </div>
  )
}
