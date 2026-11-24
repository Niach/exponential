import { readFileSync } from "node:fs"

// EXP-1183 — the "Exponential" OpenClaw theme, generated from the styleguide:
// the GLASS surface system of packages/design-tokens/tokens.json (the dark,
// dark-only look every client paints — the zinc ground with white-alpha
// fills and hairline strokes) mapped onto OpenClaw's 19 semantic colours,
// plus the contract's `steerWorking` verbs as its long-wait phrases.
// OpenClaw mixes these colours with each other (color-mix), so every fill is
// emitted as the OPAQUE composite of its glass alpha over the ground.
// #NEW-23: moved here from integrations/openclaw/scripts/theme.mjs.

const ROOT = new URL(`../../../../`, import.meta.url)

const FONT_SANS = `Inter, ui-sans-serif, system-ui, sans-serif`
const FONT_MONO = `ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace`

type Rgb = [number, number, number]

/** `#rrggbb` → [r, g, b] in 0..255. */
function hexRgb(hex: string): Rgb {
  const value = hex.replace(`#`, ``).slice(0, 6)
  return [0, 2, 4].map((i) => Number.parseInt(value.slice(i, i + 2), 16)) as Rgb
}

/** An achromatic `oklch(L 0 0 …)` → sRGB 0..255 (oklab L³ is linear light). */
function oklchGray(value: string): Rgb {
  const l = Number.parseFloat(value.match(/oklch\(\s*([\d.]+)/)![1]!)
  const linear = l ** 3
  const srgb = linear <= 0.0031308 ? 12.92 * linear : 1.055 * linear ** (1 / 2.4) - 0.055
  const c = Math.round(Math.min(1, Math.max(0, srgb)) * 255)
  return [c, c, c]
}

/** The alpha of a white glass token, `oklch(1 0 0 / 6%)` → 0.06. */
function whiteAlpha(value: string): number {
  const match = value.match(/\/\s*([\d.]+)%/)
  if (!match || !value.startsWith(`oklch(1 0 0`)) throw new Error(`not a white glass token: ${value}`)
  return Number.parseFloat(match[1]!) / 100
}

/** White at `alpha` composited over `base`, as `#rrggbb`. */
function over(base: Rgb, alpha: number): string {
  return `#${base
    .map((c) => Math.round(c + (255 - c) * alpha).toString(16).padStart(2, `0`))
    .join(``)}`
}

export interface OpenClawTheme {
  name: string
  description: string
  mascot: string
  workingPhrases: string[]
  dark: Record<string, string>
}

export function buildOpenClawTheme(): OpenClawTheme {
  const tokens = JSON.parse(readFileSync(new URL(`packages/design-tokens/tokens.json`, ROOT), `utf8`))
  const contract = JSON.parse(readFileSync(new URL(`packages/domain-contract/contract.json`, ROOT), `utf8`))
  const { glass, palette } = tokens
  // The page: the gradient's lower stop, where the main panel and the chat
  // sit (the rail's upper stop is two notches darker).
  const ground = hexRgb(glass.backgroundBottom)
  const fill = (token: string) => over(ground, whiteAlpha(token))
  // Menus and popovers: the opaque card composite (`bg-glass-card-opaque`).
  const popover = over(oklchGray(palette.popover), whiteAlpha(glass.fillCard))
  const dark = {
    background: glass.backgroundBottom,
    foreground: palette.foreground,
    card: fill(glass.fillCard),
    "card-foreground": palette.cardForeground,
    popover,
    "popover-foreground": palette.popoverForeground,
    // EXP-594: the main scheme is white/glass — solid fills are --primary.
    primary: palette.primary,
    "primary-foreground": palette.primaryForeground,
    secondary: fill(glass.fillRow),
    "secondary-foreground": palette.secondaryForeground,
    muted: fill(glass.fillSection),
    "muted-foreground": palette.mutedForeground,
    // Selections and hovers read the active glass, never a hue.
    accent: fill(glass.fillActive),
    "accent-foreground": palette.accentForeground,
    destructive: palette.destructive,
    "destructive-foreground": palette.foreground,
    border: fill(glass.strokeCard),
    input: fill(glass.strokeStrong),
    ring: palette.ring,
    "font-sans": FONT_SANS,
    "font-mono": FONT_MONO,
  }
  return {
    name: `Exponential`,
    description: `Exponential's dark glass interface: the near-black zinc ground, white-alpha cards and hairline borders, white primary actions and Inter, generated from the Exponential styleguide.`,
    mascot: `none`,
    workingPhrases: contract.steerWorking.verbs.slice(0, 24),
    dark,
  }
}

export function renderOpenClawTheme(): string {
  return `${JSON.stringify(buildOpenClawTheme(), null, 2)}\n`
}
