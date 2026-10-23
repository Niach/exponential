// FEED-128: the theme as an app's unscoped shadcn globals.

import { describe, expect, it } from "bun:test"
import { builtinTheme } from "@exponential_app/ui"
import { globalsCss } from "./theme-css"

describe(`globalsCss`, () => {
  const theme = builtinTheme(`exponential`)
  const css = globalsCss(theme)
  it(`emits :root (light + radius + fonts) and .dark with literal values`, () => {
    expect(css.startsWith(`:root {\n`)).toBe(true)
    expect(css).toContain(`  --primary: ${theme.modes.light.color.primary};`)
    expect(css).toContain(`  --radius: ${theme.tokens.radius.lg}px;`)
    expect(css).toContain(`  --font-sans: "${theme.tokens.type.family.sans}"`)
    const dark = css.slice(css.indexOf(`.dark {`))
    expect(dark).toContain(`  --primary: ${theme.modes.dark.color.primary};`)
    expect(dark).toContain(`  --sidebar-ring: ${theme.modes.dark.color.ring};`)
    expect(dark).not.toContain(`--radius`)
  })
  it(`never references the surface variables`, () => {
    expect(css).not.toContain(`--xui-`)
    expect(css).not.toContain(`var(`)
  })
  it(`takes other selectors`, () => {
    const scoped = globalsCss(theme, { light: `.app`, dark: `.app.dark` })
    expect(scoped.startsWith(`.app {\n`)).toBe(true)
    expect(scoped).toContain(`\n.app.dark {\n`)
  })
})
