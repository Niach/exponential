// EXP-569 — shared launcher appearance logic. The launcher renders twice (the
// loader's standalone shadow host and the bundle's Preact button) and both
// must resolve mode/position/icon identically or the hand-off visibly jumps,
// so every decision lives here and nowhere else. Loader-size sensitive: this
// module rides into loader.js.
import type {
  ExponentialWidgetInitOptions,
  WidgetLauncherMode,
  WidgetLauncherPlacement,
  WidgetLauncherPosition,
  WidgetRemoteConfig,
} from "./types"

// The desktop/mobile split for per-device launcher settings. Matches the web
// app's phone breakpoint; the panel's 480px bottom-sheet query is
// deliberately narrower.
export const mobileMediaQuery = `(max-width: 767px)`

export function isMobileViewport(): boolean {
  try {
    return (
      typeof window !== `undefined` &&
      typeof window.matchMedia === `function` &&
      window.matchMedia(mobileMediaQuery).matches
    )
  } catch {
    // matchMedia quirks resolve as desktop.
    return false
  }
}

// Re-render hook for viewport-size changes crossing the breakpoint. Returns
// an unsubscribe; environments without matchMedia get a no-op (the launcher
// then just keeps its initial device resolution).
export function watchMobileViewport(onChange: () => void): () => void {
  try {
    if (typeof window === `undefined` || typeof window.matchMedia !== `function`) {
      return () => {}
    }
    const query = window.matchMedia(mobileMediaQuery)
    if (typeof query.addEventListener === `function`) {
      query.addEventListener(`change`, onChange)
      return () => query.removeEventListener(`change`, onChange)
    }
    // Legacy Safari (pre-14) MediaQueryList.
    if (typeof query.addListener === `function`) {
      query.addListener(onChange)
      return () => query.removeListener(onChange)
    }
  } catch {
    // Fall through to the no-op.
  }
  return () => {}
}

export const defaultLauncher: Record<
  `desktop` | `mobile`,
  WidgetLauncherPlacement
> = {
  desktop: { mode: `fab`, position: `bottom-right` },
  mobile: { mode: `tab`, position: `middle-right` },
}

function isValidMode(value: unknown): value is WidgetLauncherMode {
  return value === `fab` || value === `tab`
}

function isValidPosition(value: unknown): value is WidgetLauncherPosition {
  return (
    value === `top-left` ||
    value === `top-right` ||
    value === `middle-left` ||
    value === `middle-right` ||
    value === `bottom-left` ||
    value === `bottom-right`
  )
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === `object`
}

// The legacy two-value `init({position})` snippet argument (pre-EXP-569, a
// public API): a bottom-corner FAB on both devices. The served config's own
// legacy `position` is gone (EXP-672) — every stored row carries `launcher`.
function legacyPosition(value: unknown): WidgetLauncherPosition | null {
  return value === `bottom-left` || value === `bottom-right` ? value : null
}

export interface ResolvedLauncher extends WidgetLauncherPlacement {
  iconSvg: string | null
}

// The valid fields of one device's placement entry, junk dropped per-field.
function readPlacement(entry: unknown): Partial<WidgetLauncherPlacement> {
  if (!isRecord(entry)) return {}
  const out: Partial<WidgetLauncherPlacement> = {}
  if (isValidMode(entry.mode)) out.mode = entry.mode
  if (isValidPosition(entry.position)) out.position = entry.position
  return out
}

// Per-device resolution: init `launcher` field > legacy init `position`
// (ignored entirely once `launcher` is passed, so hosts can pin one device
// and leave the other to the config) > served `launcher` > defaults. Every
// remote value is re-validated — the config JSON is an unvalidated fetch.
export function resolveLauncher(
  options: ExponentialWidgetInitOptions | undefined,
  config: WidgetRemoteConfig | null | undefined,
  isMobile: boolean
): ResolvedLauncher {
  const device = isMobile ? `mobile` : `desktop`
  const fallback = defaultLauncher[device]

  const initLauncher = options?.launcher
  const init = readPlacement(isRecord(initLauncher) ? initLauncher[device] : null)
  const legacyInit =
    initLauncher == null ? legacyPosition(options?.position) : null

  const remoteLauncher: unknown = config?.form?.launcher
  const remote = readPlacement(
    isRecord(remoteLauncher) ? remoteLauncher[device] : null
  )

  const mode: WidgetLauncherMode =
    init.mode ?? (legacyInit ? `fab` : (remote.mode ?? fallback.mode))
  const position: WidgetLauncherPosition =
    init.position ?? legacyInit ?? remote.position ?? fallback.position

  return {
    mode,
    position,
    iconSvg: sanitizeIconSvg(
      isRecord(remoteLauncher) ? remoteLauncher.iconSvg : null
    ),
  }
}

// The wrapper inline-style fragment: fixed offsets + middle centering. The
// SAME string styles the loader's shadow host and the bundle's wrapper div,
// so the two renders can never drift. Centering (translateY) lives on the
// wrapper, hover transforms on the button — never combine the two on one
// element. Tabs sit flush against the screen edge; FABs keep 20px margins
// (bottom ones clearing notched-phone home indicators, top ones sitting
// 40px down so a host page's sticky header stays reachable).
export function launcherPlacementCss(p: WidgetLauncherPlacement): string {
  const left = p.position.endsWith(`left`)
  // Double declaration on both edges: browsers without env() drop only the
  // calc and keep the plain px value. Top launchers keep 40px so they clear
  // the sticky headers host pages put at the top of the viewport (EXP-642).
  const vertical = p.position.startsWith(`top`)
    ? `top:40px;top:calc(40px + env(safe-area-inset-top, 0px));`
    : p.position.startsWith(`middle`)
      ? `top:50%;transform:translateY(-50%);`
      : `bottom:20px;bottom:calc(20px + env(safe-area-inset-bottom, 0px));`
  const horizontal =
    p.mode === `tab`
      ? left
        ? `left:0;`
        : `right:0;`
      : left
        ? `left:20px;`
        : `right:20px;`
  return vertical + horizontal
}

// FAB hover scale grows AWAY from the anchored edges so it never clips at
// the viewport. Set inline on the button (overriding buttonCss's static
// `transform-origin: center`); tabs don't scale, so they don't need it.
export function launcherOrigin(position: WidgetLauncherPosition): string {
  const vertical = position.startsWith(`top`)
    ? `top`
    : position.startsWith(`middle`)
      ? `center`
      : `bottom`
  return `${vertical} ${position.endsWith(`left`) ? `left` : `right`}`
}

export function launcherButtonClass(p: WidgetLauncherPlacement): string {
  return p.mode === `tab`
    ? `exp-fab exp-tab ${p.position.endsWith(`left`) ? `exp-tab-left` : `exp-tab-right`}`
    : `exp-fab`
}

// Middle launchers sit BESIDE the panel, so its horizontal offset must clear
// them (launcher width + 12px gap; tabs are flush 36px wide, FABs 20px in).
// Top/bottom launchers share their corner with the panel — the stock 20px
// stands.
export function panelSideOffset(p: WidgetLauncherPlacement): string {
  if (!p.position.startsWith(`middle`)) return `20px`
  return p.mode === `tab` ? `48px` : `76px`
}

// The served icon markup gets innerHTML'd into the host page, and `host` in
// the init options lets an embedder point the widget at an arbitrary origin —
// so never trust the string, however trusted the first-party server is.
// Server-emitted icons are pure lucide geometry (svg + path/shape children
// with numeric attributes) and pass untouched; anything script-shaped is
// dropped in favor of the built-in megaphone.
export function sanitizeIconSvg(value: unknown): string | null {
  if (typeof value !== `string` || value.length > 10_000) return null
  const svg = value.trim()
  if (!/^<svg[\s>][\s\S]*<\/svg>$/i.test(svg)) return null
  if (
    /<(script|foreignobject|iframe|animate|set|use|image)\b|on[a-z]+\s*=|javascript:|href\s*=/i.test(
      svg
    )
  ) {
    return null
  }
  return svg
}
