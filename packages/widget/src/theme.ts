// Widget palettes: sRGB hex transcriptions of the zinc OKLCH themes in
// packages/design-tokens/tokens.json (dark: background 0.145, card 0.205,
// secondary/muted 0.269, foreground 0.985, mutedForeground 0.708). Hex keeps
// the widget independent of host-page CSS and safe on pre-oklch browsers.
//
// `card` is the panel surface; `background` is the INSET surface (inputs,
// mode cards) sitting visually below it.

export type ThemeMode = `dark` | `light`
export type ThemePreference = ThemeMode | `auto`

export interface WidgetPalette {
  background: string
  card: string
  secondary: string
  foreground: string
  mutedForeground: string
  border: string
  input: string
  destructive: string
  success: string
  defaultAccent: string
  defaultAccentForeground: string
  radius: string
  font: string
}

const font = `ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, sans-serif`

export const darkPalette: WidgetPalette = {
  background: `#0a0a0a`,
  card: `#171717`,
  secondary: `#262626`,
  foreground: `#fafafa`,
  mutedForeground: `#a3a3a3`,
  border: `rgba(255, 255, 255, 0.1)`,
  input: `rgba(255, 255, 255, 0.15)`,
  destructive: `#ef4444`,
  success: `#22c55e`,
  defaultAccent: `#e5e5e5`,
  defaultAccentForeground: `#171717`,
  radius: `10px`,
  font,
}

// Zinc-light counterpart (EXP-435). The default accent flips to near-black —
// the dark theme's #e5e5e5 pill would vanish on a white panel.
export const lightPalette: WidgetPalette = {
  background: `#ffffff`,
  card: `#ffffff`,
  secondary: `#f4f4f5`,
  foreground: `#171717`,
  mutedForeground: `#71717a`,
  border: `rgba(0, 0, 0, 0.12)`,
  input: `rgba(0, 0, 0, 0.16)`,
  destructive: `#dc2626`,
  success: `#16a34a`,
  defaultAccent: `#171717`,
  defaultAccentForeground: `#fafafa`,
  radius: `10px`,
  font,
}

// Back-compat alias — pre-theme call sites (and tests) read `theme.*` as the
// dark palette.
export const theme = darkPalette

// Maximal 32-bit z-index so the launcher + panel sit above any host-page
// overlay (Radix dialogs top out far below this).
export const defaultZIndex = 2147483647

const hex6Pattern = /^#([0-9a-f]{6})$/i

function parseHex(color: string): [number, number, number] | null {
  const match = hex6Pattern.exec(color.trim())
  if (!match) return null
  const value = Number.parseInt(match[1], 16)
  return [(value >> 16) & 0xff, (value >> 8) & 0xff, value & 0xff]
}

function luminanceOf(rgb: [number, number, number]): number {
  return 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

// Linear channel mix of two #rrggbb colors; `amount` 0 → base, 1 → into.
// Non-hex6 inputs return the base unchanged (palette derivation must never
// throw over a junk config value).
export function mixHex(base: string, into: string, amount: number): string {
  const from = parseHex(base)
  const to = parseHex(into)
  if (!from || !to) return base
  const channel = (index: number) =>
    Math.round(from[index] + (to[index] - from[index]) * amount)
  return `#${[0, 1, 2]
    .map((index) =>
      Math.min(255, Math.max(0, channel(index))).toString(16).padStart(2, `0`)
    )
    .join(``)}`
}

// The first valid theme value wins; junk (old configs, bad API calls) falls
// through to dark — the pre-theme behavior.
export function resolveThemePreference(
  ...candidates: unknown[]
): ThemePreference {
  for (const candidate of candidates) {
    if (
      candidate === `dark` ||
      candidate === `light` ||
      candidate === `auto`
    ) {
      return candidate
    }
  }
  return `dark`
}

export function resolveThemeMode(pref: ThemePreference): ThemeMode {
  if (pref !== `auto`) return pref
  try {
    if (
      typeof window !== `undefined` &&
      typeof window.matchMedia === `function` &&
      window.matchMedia(`(prefers-color-scheme: light)`).matches
    ) {
      return `light`
    }
  } catch {
    // matchMedia quirks fall through to dark.
  }
  return `dark`
}

// The mode's palette with the owner's custom panel/text colors applied
// (EXP-435). A custom color replaces the surface it names; every dependent
// shade (inset fields, hovers, borders, muted text) is re-derived from the
// pair so any combination stays readable.
export function paletteFor(
  mode: ThemeMode,
  overrides?: {
    backgroundColor?: string | null
    textColor?: string | null
  }
): WidgetPalette {
  const base = mode === `light` ? lightPalette : darkPalette
  const customCard =
    typeof overrides?.backgroundColor === `string` &&
    hex6Pattern.test(overrides.backgroundColor.trim())
      ? overrides.backgroundColor.trim()
      : null
  const customText =
    typeof overrides?.textColor === `string` &&
    hex6Pattern.test(overrides.textColor.trim())
      ? overrides.textColor.trim()
      : null
  if (!customCard && !customText) return base

  const card = customCard ?? base.card
  const foreground = customText ?? base.foreground
  const cardRgb = parseHex(card)
  const cardIsDark = cardRgb ? luminanceOf(cardRgb) <= 140 : true
  return {
    ...base,
    card,
    // Inset surfaces sit below the panel: darker on a dark panel, the panel
    // color itself on a light one (matching the stock light palette).
    background: customCard
      ? cardIsDark
        ? mixHex(card, `#000000`, 0.5)
        : card
      : base.background,
    foreground,
    mutedForeground: mixHex(foreground, card, 0.35),
    secondary: mixHex(card, foreground, 0.1),
    border: mixHex(card, foreground, 0.12),
    input: mixHex(card, foreground, 0.16),
  }
}

// Shared by the loader's standalone button and the bundle's Preact button so
// the hand-off is pixel-identical (the bundle removes the loader button and
// renders its own). The launcher is a tiny icon-only circle by default; on
// hover it scales up and reveals its label (`.exp-fab-label`).
//
// Themable colors go through `var(--exp-*, literal)`: inside the bundle's
// shadow root the `.exp-root` custom properties win (so setTheme live-updates
// the FAB without re-injecting this stylesheet), while the loader's
// standalone shadow root has no `.exp-root` ancestor and keeps the literal
// fallbacks.
export function buttonCss(accent: string, mode: ThemeMode = `dark`): string {
  const palette = mode === `light` ? lightPalette : darkPalette
  return `
button.exp-fab {
  all: initial;
  font-family: ${palette.font};
  font-size: 13px;
  font-weight: 600;
  line-height: 1;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 0;
  height: 44px;
  padding: 0 14px;
  border-radius: 999px;
  border: 1px solid var(--exp-border, ${palette.border});
  background: var(--exp-accent, ${accent});
  color: var(--exp-accent-foreground, ${pickForeground(accent)});
  cursor: pointer;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
  user-select: none;
  transform-origin: center;
  transition: transform 0.16s ease, gap 0.16s ease, box-shadow 0.16s ease;
}
button.exp-fab:hover {
  transform: scale(1.08);
  gap: 7px;
  box-shadow: 0 6px 22px rgba(0, 0, 0, 0.5);
}
button.exp-fab:focus-visible { outline: 2px solid var(--exp-foreground, ${palette.foreground}); outline-offset: 2px; }
button.exp-fab svg { width: 16px; height: 16px; display: block; }
button.exp-fab .exp-fab-label {
  max-width: 0;
  overflow: hidden;
  white-space: nowrap;
  opacity: 0;
  transition: max-width 0.16s ease, opacity 0.16s ease;
}
button.exp-fab:hover .exp-fab-label,
button.exp-fab:focus-visible .exp-fab-label { max-width: 180px; opacity: 1; }
`
}

// Relative-luminance check so custom accent colors keep readable text.
export function pickForeground(color: string): string {
  const rgb = parseHex(color)
  if (!rgb) return theme.defaultAccentForeground
  return luminanceOf(rgb) > 140 ? `#171717` : `#fafafa`
}

export const megaphoneIconSvg = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m3 11 18-5v12L3 14v-3z"/><path d="M11.6 16.8a3 3 0 1 1-5.8-1.6"/></svg>`
