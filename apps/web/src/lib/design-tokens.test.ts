import { describe, expect, it } from "vitest"
import { MOTION_DURATION_MS } from "@/lib/motion"
import { readFileSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { dirname, join } from "node:path"

// Guards that packages/design-tokens/tokens.json stays in lockstep with the web
// theme it mirrors (the `.dark` block of apps/web/src/styles.css). tokens.json
// is the single source the native palettes are generated from, so if the web
// designer changes a swatch here without updating the shared token, this fails.

const __dirname = dirname(fileURLToPath(import.meta.url))
const repoRoot = join(__dirname, `..`, `..`, `..`, `..`)

const tokens = JSON.parse(
  readFileSync(join(repoRoot, `packages/design-tokens/tokens.json`), `utf8`)
) as {
  palette: Record<string, string>
  semantic: Record<string, string>
  avatar: Record<string, string>
  glass: Record<string, string>
  motion: { duration: Record<string, number>; ease: Record<string, number[]> }
}

const stylesCss = readFileSync(
  join(repoRoot, `apps/web/src/styles.css`),
  `utf8`
)

// EXP-523: strip comments BEFORE any block matching. The `[^}]*` block
// regexes below stop at the first `}`, and a `}` inside a CSS comment — even
// one merely quoting that regex — silently truncates the capture, after which
// every assertion here passes against a partial block. Comments carry no
// declarations, so removing them is free and removes the whole failure class.
const cssNoComments = stylesCss.replace(/\/\*[\s\S]*?\*\//g, ``)

// Pull the `--var: value;` declarations out of a top-level `<selector> { … }`
// block. Neither block nests braces, so `[^}]*` stops at the right place.
function parseBlockVars(
  css: string,
  selector: RegExp,
  label: string
): Record<string, string> {
  const block = css.match(selector)
  if (!block) throw new Error(`Could not find ${label} block in styles.css`)
  // `[^}]*` in the selectors below stops at the FIRST `}`. Comments are
  // already stripped by the caller, but if either block ever grows a nested
  // at-rule the parse would still truncate and every assertion here would
  // start passing against a partial block — fail loudly instead.
  if (block[1].includes(`{`)) {
    throw new Error(
      `${label} block contains a nested block — parseBlockVars cannot read it`
    )
  }
  const vars: Record<string, string> = {}
  for (const line of block[1].split(`\n`)) {
    const m = line.match(/^\s*--([\w-]+):\s*(.+?);\s*$/)
    if (m) vars[m[1]] = m[2].trim()
  }
  return vars
}

// The glass tokens are theme-invariant (the app is dark-only) and are
// deliberately duplicated into BOTH :root and .dark. Only .dark is parity-
// checked against tokens.json, so this is the set the two blocks must agree on.
function themeInvariantVars(
  vars: Record<string, string>
): Record<string, string> {
  return Object.fromEntries(
    Object.entries(vars).filter(([k]) => k.startsWith(`glass-`))
  )
}

function kebab(camel: string): string {
  return camel.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)
}

describe(`design-tokens parity with web styles.css`, () => {
  const darkVars = parseBlockVars(
    cssNoComments,
    /\.dark\s*\{([^}]*)\}/,
    `.dark`
  )
  const rootVars = parseBlockVars(cssNoComments, /:root\s*\{([^}]*)\}/, `:root`)

  it(`every palette token matches the corresponding .dark CSS variable`, () => {
    for (const [key, value] of Object.entries(tokens.palette)) {
      if (key.startsWith(`$`)) continue
      const cssVar = kebab(key)
      expect(
        darkVars[cssVar],
        `tokens.palette.${key} should equal --${cssVar} in styles.css`
      ).toBe(value)
    }
  })

  it(`every glass token matches the corresponding --glass-* CSS variable`, () => {
    for (const [key, value] of Object.entries(tokens.glass)) {
      if (key.startsWith(`$`)) continue
      const cssVar = `glass-${kebab(key)}`
      expect(
        darkVars[cssVar],
        `tokens.glass.${key} should equal --${cssVar} in styles.css`
      ).toBe(value)
    }
  })

  // EXP-698 r3: the chat inline-code trio is the only semantic colour the web
  // carries as a CSS var (status colours ride Tailwind classes).
  it(`every semantic code* token matches the corresponding --code-* CSS variable`, () => {
    for (const [key, value] of Object.entries(tokens.semantic)) {
      if (!key.startsWith(`code`)) continue
      const cssVar = kebab(key)
      expect(darkVars[cssVar]?.toLowerCase(), `tokens.semantic.${key} should equal --${cssVar} in styles.css`).toBe(value.toLowerCase())
      expect(rootVars[cssVar]?.toLowerCase(), `tokens.semantic.${key} should equal --${cssVar} in :root`).toBe(value.toLowerCase())
    }
  })

  // EXP-698 r4: the avatar fallback palette is positional (the hash lands on
  // an index), so the web mirrors it as --avatar-<index> in tokens order.
  it(`every avatar hue matches --avatar-<index> in tokens order`, () => {
    const hues = Object.entries(tokens.avatar).filter(([k]) => !k.startsWith(`$`))
    expect(hues).toHaveLength(8)
    hues.forEach(([key, value], index) => {
      const cssVar = `avatar-${index}`
      expect(darkVars[cssVar]?.toLowerCase(), `tokens.avatar.${key} should equal --${cssVar} in styles.css`).toBe(value.toLowerCase())
      expect(rootVars[cssVar]?.toLowerCase(), `tokens.avatar.${key} should equal --${cssVar} in :root`).toBe(value.toLowerCase())
    })
    expect(darkVars[`avatar-8`]).toBeUndefined()
  })

  // EXP-594 retired the indigo --brand accent — the main scheme is white/glass.
  // Nothing may reintroduce a brand color var or token.
  it(`no brand accent exists in tokens.json or styles.css`, () => {
    expect(tokens.semantic).not.toHaveProperty(`brand`)
    expect(tokens.semantic).not.toHaveProperty(`brandStrong`)
    for (const vars of [darkVars, rootVars]) {
      expect(Object.keys(vars).filter((k) => k.startsWith(`brand`))).toEqual([])
    }
  })

  // EXP-269 duplicated the glass vars into :root as well, but only the
  // .dark copy is parity-checked above — so nothing kept the two in sync.
  it(`the :root and .dark glass blocks are identical`, () => {
    const rootInvariant = themeInvariantVars(rootVars)
    expect(
      Object.keys(rootInvariant).length,
      `expected :root to carry the glass vars`
    ).toBeGreaterThan(0)
    expect(
      rootInvariant,
      `the theme-invariant glass vars must be byte-identical in :root and .dark`
    ).toEqual(themeInvariantVars(darkVars))
  })

  // EXP-523: motion tokens are theme-invariant, so styles.css carries them in
  // :root ONLY (unlike the glass vars, which are duplicated into .dark).
  // The generator emits `motion` for the three native clients; web is hand-
  // authored, so this is the only thing keeping all four in step. Durations
  // render as `<n>ms`; easings render with JSON's own number formatting, so
  // the CSS must read `cubic-bezier(0.2, 0, 0, 1)` — `0`, not `0.0`.
  it(`every motion token matches the corresponding :root CSS variable`, () => {
    for (const [key, ms] of Object.entries(tokens.motion.duration)) {
      if (key.startsWith(`$`)) continue
      const cssVar = `motion-duration-${kebab(key)}`
      expect(
        rootVars[cssVar],
        `tokens.motion.duration.${key} should equal --${cssVar} in styles.css`
      ).toBe(`${ms}ms`)
    }
    for (const [key, curve] of Object.entries(tokens.motion.ease)) {
      if (key.startsWith(`$`)) continue
      const cssVar = `motion-ease-${kebab(key)}`
      expect(
        rootVars[cssVar],
        `tokens.motion.ease.${key} should equal --${cssVar} in styles.css`
      ).toBe(`cubic-bezier(${curve.join(`, `)})`)
    }
  })

  // lib/motion.ts is the JS-side copy, for exit-animation unmount timers that
  // outlive the state change. Same hand-authored + parity-tested arrangement
  // as the CSS vars: a drift here would leave a panel unmounting before (or
  // long after) its transition finishes.
  it(`lib/motion.ts durations match the shared tokens`, () => {
    const expected = Object.fromEntries(
      Object.entries(tokens.motion.duration).filter(([k]) => !k.startsWith(`$`))
    )
    expect({ ...MOTION_DURATION_MS }).toEqual(expected)
  })

  // The `--ease-*` @theme aliases are what make `ease-standard` a Tailwind
  // utility; they must point at the raw --motion-ease-* vars, not restate them.
  it(`the @theme ease-* aliases reference the motion vars`, () => {
    const themeBlock = cssNoComments.match(/@theme inline\s*\{([^}]*)\}/)
    if (!themeBlock) throw new Error(`Could not find @theme inline block`)
    for (const key of Object.keys(tokens.motion.ease)) {
      if (key.startsWith(`$`)) continue
      const name = kebab(key)
      expect(
        themeBlock[1],
        `@theme inline should alias --ease-${name} to --motion-ease-${name}`
      ).toContain(`--ease-${name}: var(--motion-ease-${name});`)
    }
  })
})
