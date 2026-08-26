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

// The mode's palette. Custom panel/text color overrides (EXP-435) were
// removed by EXP-569 — the theme presets plus one accent color are the whole
// palette surface now.
export function paletteFor(mode: ThemeMode): WidgetPalette {
  return mode === `light` ? lightPalette : darkPalette
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
/* Edge tab ("nudge", EXP-569): a 36px square flush against the viewport
   edge — its wrapper anchors at left:0/right:0, so the hover width-growth
   extends inward. No scale (a flush tab must not lift off the edge), no
   label, square + borderless on the edge side (a 1px hairline against the
   viewport edge reads as a rendering artifact). Deliberately smaller than
   the FAB (EXP-642): it now shows on desktop too, where it must nudge
   rather than compete with the host page's own chrome. */
button.exp-fab.exp-tab {
  width: 36px;
  height: 36px;
  padding: 0;
  transition: width 0.16s ease, box-shadow 0.16s ease;
}
button.exp-fab.exp-tab:hover {
  transform: none;
  gap: 0;
  width: 42px;
  box-shadow: 0 6px 22px rgba(0, 0, 0, 0.5);
}
button.exp-fab.exp-tab svg { width: 16px; height: 16px; }
button.exp-tab-right { border-radius: 10px 0 0 10px; border-right: none; }
button.exp-tab-left { border-radius: 0 10px 10px 0; border-left: none; }
`
}

// Relative-luminance check so custom accent colors keep readable text.
export function pickForeground(color: string): string {
  const rgb = parseHex(color)
  if (!rgb) return theme.defaultAccentForeground
  return luminanceOf(rgb) > 140 ? `#171717` : `#fafafa`
}

// The built-in launcher glyph (fallback when the config picks no icon).
// Byte-matches the shared icon registry's `megaphone` art (EXP-569:
// packages/icons pickable-svg output) so picking "megaphone" and picking
// nothing render identically.
export const megaphoneIconSvg = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M11 6a13 13 0 0 0 8.4-2.8A1 1 0 0 1 21 4v12a1 1 0 0 1-1.6.8A13 13 0 0 0 11 14H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2z"/><path d="M6 14a12 12 0 0 0 2.4 7.2 2 2 0 0 0 3.2-2.4A8 8 0 0 1 10 14"/><path d="M8 6v8"/></svg>`
