// EXP-1183 — the "Exponential" OpenClaw theme, generated from the styleguide:
// the light (`:root, :host`) and dark (`.dark`) token blocks of
// packages/ui/src/styles.css (the web theme every client mirrors through
// @exp/design-tokens) plus the contract's `steerWorking` verbs as OpenClaw's
// long-wait phrases. OpenClaw palettes use the same shadcn token names.
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

// OpenClaw's required semantic colours (docs/tools/theme.md).
const KEYS = [
  `background`,
  `foreground`,
  `card`,
  `card-foreground`,
  `popover`,
  `popover-foreground`,
  `primary`,
  `primary-foreground`,
  `secondary`,
  `secondary-foreground`,
  `muted`,
  `muted-foreground`,
  `accent`,
  `accent-foreground`,
  `destructive`,
  `destructive-foreground`,
  `border`,
  `input`,
  `ring`,
]

const FONT_SANS = `Inter, ui-sans-serif, system-ui, sans-serif`
const FONT_MONO = `ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace`

function block(css, selector) {
  const start = css.indexOf(`${selector} {`)
  if (start < 0) throw new Error(`styles.css has no "${selector} {" block`)
  const body = css.slice(start, css.indexOf(`\n}`, start))
  const vars = {}
  for (const [, name, value] of body.matchAll(/--([a-z-]+):\s*([^;]+);/g)) {
    vars[name] = value.trim()
  }
  return vars
}

function palette(vars) {
  const out = {}
  for (const key of KEYS) {
    // The shadcn v4 set has no destructive-foreground; text on a destructive
    // fill is the primary-foreground white, as on the web's buttons.
    const value =
      key === `destructive-foreground` ? `oklch(0.985 0 0)` : vars[key]
    if (!value) throw new Error(`styles.css is missing --${key}`)
    out[key] = value
  }
  out[`font-sans`] = FONT_SANS
  out[`font-mono`] = FONT_MONO
  return out
}

export function buildTheme() {
  const css = readFileSync(new URL(`packages/ui/src/styles.css`, ROOT), `utf8`)
  const contract = JSON.parse(
    readFileSync(new URL(`packages/domain-contract/contract.json`, ROOT), `utf8`)
  )
  return {
    name: `Exponential`,
    description: `Exponential's zinc interface: near-black surfaces, white primary actions, hairline borders and Inter, generated from the Exponential styleguide.`,
    mascot: `none`,
    workingPhrases: contract.steerWorking.verbs.slice(0, 24),
    light: palette(block(css, `:root,\n:host`)),
    dark: palette(block(css, `.dark`)),
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
