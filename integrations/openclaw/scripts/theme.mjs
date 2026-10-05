// EXP-1183 — the "Exponential" OpenClaw theme, generated from the styleguide:
// the GLASS surface system of packages/design-tokens/tokens.json (the dark,
// dark-only look every client paints — the zinc ground with white-alpha
// fills and hairline strokes) mapped onto OpenClaw's 19 semantic colours,
// plus the contract's `steerWorking` verbs as its long-wait phrases.
// OpenClaw mixes these colours with each other (color-mix), so every fill is
// emitted as the OPAQUE composite of its glass alpha over the ground.
//
//   node integrations/openclaw/scripts/theme.mjs          rewrite the theme
//   node integrations/openclaw/scripts/theme.mjs --check  fail on drift
//
// packages/mcp-apps/src/openclaw-theme.test.ts runs the check in CI.
import { readFileSync, writeFileSync } from "node:fs"
import { fileURLToPath } from "node:url"

const ROOT = new URL(`../../../`, import.meta.url)
export const THEME_PATH = fileURLToPath(
  new URL(`integrations/openclaw/themes/exponential.json`, ROOT)
)

const FONT_SANS = `Inter, ui-sans-serif, system-ui, sans-serif`
const FONT_MONO = `ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace`

/** `#rrggbb` → [r, g, b] in 0..255. */
function hexRgb(hex) {
  const value = hex.replace(`#`, ``).slice(0, 6)
  return [0, 2, 4].map((i) => Number.parseInt(value.slice(i, i + 2), 16))
}

/** An achromatic `oklch(L 0 0 …)` → sRGB 0..255 (oklab L³ is linear light). */
function oklchGray(value) {
  const l = Number.parseFloat(value.match(/oklch\(\s*([\d.]+)/)[1])
  const linear = l ** 3
  const srgb = linear <= 0.0031308 ? 12.92 * linear : 1.055 * linear ** (1 / 2.4) - 0.055
  const c = Math.round(Math.min(1, Math.max(0, srgb)) * 255)
  return [c, c, c]
}

/** The alpha of a white glass token, `oklch(1 0 0 / 6%)` → 0.06. */
function whiteAlpha(value) {
  const match = value.match(/\/\s*([\d.]+)%/)
  if (!match || !value.startsWith(`oklch(1 0 0`)) throw new Error(`not a white glass token: ${value}`)
  return Number.parseFloat(match[1]) / 100
}

/** White at `alpha` composited over `base`, as `#rrggbb`. */
function over(base, alpha) {
  return `#${base
    .map((c) => Math.round(c + (255 - c) * alpha).toString(16).padStart(2, `0`))
    .join(``)}`
}

export function buildTheme() {
  const tokens = JSON.parse(
    readFileSync(new URL(`packages/design-tokens/tokens.json`, ROOT), `utf8`)
  )
  const contract = JSON.parse(
    readFileSync(new URL(`packages/domain-contract/contract.json`, ROOT), `utf8`)
  )
  const { glass, palette } = tokens
  // The page: the gradient's lower stop, where the main panel and the chat
  // sit (the rail's upper stop is two notches darker).
  const ground = hexRgb(glass.backgroundBottom)
  const fill = (token) => over(ground, whiteAlpha(token))
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

export function renderTheme() {
  return `${JSON.stringify(buildTheme(), null, 2)}\n`
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const next = renderTheme()
  if (process.argv.includes(`--check`)) {
    if (readFileSync(THEME_PATH, `utf8`) !== next) {
      console.error(`themes/exponential.json is stale; run node integrations/openclaw/scripts/theme.mjs`)
      process.exit(1)
    }
  } else {
    writeFileSync(THEME_PATH, next)
  }
}
