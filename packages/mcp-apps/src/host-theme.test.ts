import { readFileSync } from "node:fs"
import { join } from "node:path"
import { describe, expect, it } from "vitest"
import theme from "../../exponential-ui/themes/exponential.theme.json"
import { applyColorMode, applyHostContext, hostColorMode } from "./host-theme"

const fresh = () => document.implementation.createHTMLDocument(`view`)

describe(`host theme (#NEW-24)`, () => {
  it(`takes the host's theme; a context that names none changes nothing`, () => {
    const doc = fresh()
    applyColorMode(`dark`, doc)
    expect(applyHostContext({}, doc)).toBeNull()
    expect(doc.documentElement.classList.contains(`dark`)).toBe(true)
    expect(applyHostContext({ theme: `light` }, doc)).toBe(`light`)
    expect(doc.documentElement.classList.contains(`dark`)).toBe(false)
    // A later change that carries only the display mode keeps the theme.
    expect(applyHostContext({ displayMode: `fullscreen` }, doc)).toBeNull()
    expect(doc.documentElement.classList.contains(`dark`)).toBe(false)
    expect(applyHostContext({ theme: `dark` }, doc)).toBe(`dark`)
    expect(doc.documentElement.classList.contains(`dark`)).toBe(true)
    expect(hostColorMode({ theme: `sepia` as never })).toBeNull()
  })

  it(`sets the host's style variables on the root, and only those`, () => {
    const doc = fresh()
    applyHostContext({ styles: { variables: {
      "--color-background-primary": `light-dark(#fff, #111)`,
      "--font-sans": `Host Sans, sans-serif`,
      "--border-radius-md": `6px`,
      "--background": `red`,
      "color": `red`,
      "--color-text-primary": undefined,
    } } }, doc)
    const style = doc.documentElement.style
    expect(style.getPropertyValue(`--color-background-primary`)).toBe(`light-dark(#fff, #111)`)
    expect(style.getPropertyValue(`--font-sans`)).toBe(`Host Sans, sans-serif`)
    expect(style.getPropertyValue(`--border-radius-md`)).toBe(`6px`)
    expect(style.getPropertyValue(`--background`)).toBe(``)
    expect(style.getPropertyValue(`color`)).toBe(``)
  })

  it(`adds the host's font faces once`, () => {
    const doc = fresh()
    applyHostContext({ styles: { css: { fonts: `@font-face{font-family:"Host Sans";src:url(a.woff2)}` } } }, doc)
    applyHostContext({ styles: { css: { fonts: `@font-face{font-family:"Other"}` } } }, doc)
    expect(doc.querySelectorAll(`style#exp-host-fonts`)).toHaveLength(1)
    expect(doc.getElementById(`exp-host-fonts`)?.textContent).toContain(`Host Sans`)
  })

  it(`keeps the safe-area insets as px variables`, () => {
    const doc = fresh()
    applyHostContext({ safeAreaInsets: { top: 44, right: 0, bottom: 34, left: -3 } }, doc)
    const style = doc.documentElement.style
    expect(style.getPropertyValue(`--exp-safe-top`)).toBe(`44px`)
    expect(style.getPropertyValue(`--exp-safe-bottom`)).toBe(`34px`)
    expect(style.getPropertyValue(`--exp-safe-left`)).toBe(`0px`)
  })

  it(`the light glass values are the exponential theme's own light mode`, () => {
    const css = readFileSync(join(__dirname, `styles.css`), `utf8`)
    const light = css.slice(css.indexOf(`html:not(.dark) {`), css.indexOf(`/* end light */`))
    const color = theme.modes.light.color
    const pairs: Array<[string, string]> = [
      [`--glass-background-top`, color.background],
      [`--glass-background-bottom`, color.background],
      [`--glass-fill-card`, color.card],
      [`--glass-fill-section`, color.muted],
      [`--glass-fill-panel`, color.muted],
      [`--glass-fill-row`, color.secondary],
      [`--glass-fill-active`, color.accent],
      [`--glass-stroke-row`, color.border],
      [`--glass-stroke-section`, color.border],
      [`--glass-stroke-card`, color.border],
      [`--glass-stroke-strong`, color.borderStrong],
      [`--glass-stroke-active`, color.strokeActive],
      [`--glass-fill-own`, color.bubbleOwn],
      [`--code-text`, color.link],
    ]
    for (const [name, value] of pairs) {
      const line = light.split(`\n`).find((row) => row.trim().startsWith(`${name}:`)) ?? ``
      expect(line, name).toContain(value)
    }
  })
})
