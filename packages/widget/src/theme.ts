// Widget palette: sRGB hex transcription of the zinc-dark OKLCH theme in
// packages/design-tokens/tokens.json (background 0.145, card 0.205,
// secondary/muted 0.269, foreground 0.985, mutedForeground 0.708). Hex keeps
// the widget independent of host-page CSS and safe on pre-oklch browsers.
export const theme = {
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
  font: `ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, sans-serif`,
} as const

// Maximal 32-bit z-index so the launcher + panel sit above any host-page
// overlay (Radix dialogs top out far below this).
export const defaultZIndex = 2147483647

// Shared by the loader's standalone button and the bundle's Preact button so
// the hand-off is pixel-identical (the bundle removes the loader button and
// renders its own). The launcher is a tiny icon-only circle by default; on
// hover it scales up and reveals its label (`.exp-fab-label`).
export function buttonCss(accent: string): string {
  return `
button.exp-fab {
  all: initial;
  font-family: ${theme.font};
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
  border: 1px solid ${theme.border};
  background: ${accent};
  color: ${pickForeground(accent)};
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
button.exp-fab:focus-visible { outline: 2px solid ${theme.foreground}; outline-offset: 2px; }
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
  const match = /^#([0-9a-f]{6})$/i.exec(color.trim())
  if (!match) return theme.defaultAccentForeground
  const value = Number.parseInt(match[1], 16)
  const r = (value >> 16) & 0xff
  const g = (value >> 8) & 0xff
  const b = value & 0xff
  const luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b
  return luminance > 140 ? `#171717` : `#fafafa`
}

export const megaphoneIconSvg = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m3 11 18-5v12L3 14v-3z"/><path d="M11.6 16.8a3 3 0 1 1-5.8-1.6"/></svg>`
