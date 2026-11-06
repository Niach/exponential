// #VAPP-258 on Solid (React's markdown-parts.test.tsx): the web painter puts
// the theme scope class and the painter-supplied recipe props on every
// `Markdown/<part>` element, so a theme's part rules apply; a host chip is
// the `Markdown/chip` part over the Chip's look.

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, render } from "@solidjs/testing-library"
import { CORE_CATALOG_ID, reduceNested } from "@exponential_app/eui"
import { ExponentialSurface } from "./surface"
import type { HostPlugin } from "./host"

afterEach(cleanup)

const root = (text: string) => reduceNested({ id: `md`, component: `Markdown`, props: { text } }, { catalogId: CORE_CATALOG_ID }).root
const paint = (text: string, host: HostPlugin, theme: string) => render(() => <ExponentialSurface root={root(text)} theme={theme} id="parts" host={host} />).container

const TEXT = `# Title\n\nSee [docs](https://exponential.at/d) and \`code\` and ABC-1.\n\n- one\n\n| a |\n| --- |\n| 1 |\n\n\`\`\`ts\nconst x = 1\n\`\`\``
const chipHost: HostPlugin = { textLinks: (t) => (t.includes(`ABC-1`) ? t.split(/(ABC-1)/).filter(Boolean).map((s) => (s === `ABC-1` ? { text: s, href: `https://exponential.at/i/ABC-1`, chip: { kind: `issue`, label: `ABC-1 Fix login` } } : { text: s })) : null) }

describe(`#VAPP-258 Markdown parts carry the theme scope and recipe props`, () => {
  it(`every part has the scope class, its part name and a concrete density`, () => {
    const c = paint(TEXT, chipHost, `exponential`)
    const scope = [...c.querySelector(`[data-xui-c="Markdown"]`)!.classList].find((k) => k.startsWith(`xui-t-`))!
    for (const part of [`heading`, `paragraph`, `link`, `code`, `listItem`, `table`, `headerCell`, `cell`, `codeBlock`]) {
      const el = c.querySelector(`.xui-Markdown-${part}`)!
      expect(el, part).toBeTruthy()
      expect(el.classList.contains(scope), part).toBe(true)
      expect(el.getAttribute(`data-r-density`), part).toBe(`document`)
    }
    expect(c.querySelector(`.xui-Markdown-heading`)!.getAttribute(`data-r-level`)).toBe(`1`)
  })

  it(`the chat density reaches the parts`, () => {
    const c = paint(TEXT, { ...chipHost, markdownOptions: { density: `chat` } }, `exponential`)
    expect(c.querySelector(`.xui-Markdown-heading`)!.getAttribute(`data-r-density`)).toBe(`chat`)
  })

  it(`a chip is the Markdown chip part over the Chip look, the theme's kind rules inline`, () => {
    const exp = paint(TEXT, chipHost, `exponential`)
    const chip = exp.querySelector<HTMLElement>(`a[data-chip="issue"]`)!
    expect(chip.classList.contains(`xui-Markdown-chip`)).toBe(true)
    expect(chip.classList.contains(`xui-Chip-root`)).toBe(true)
    expect(chip.getAttribute(`data-r-chip`)).toBe(`issue`)
    expect(chip.style.borderRadius).toBe(`6px`)
    expect(exp.querySelector<HTMLElement>(`.xui-Markdown-chipLabel`)!.style.fontSize).toBe(`13px`)
    // #VAPP-258: a chip's ident paints mono before its label (Markdown/chipIdent).
    const ident = paint(`see IDT-5`, { textLinks: (t) => (t.includes(`IDT-5`) ? [{ text: `see ` }, { text: `IDT-5`, href: `https://exponential.at/i/IDT-5`, chip: { kind: `issue`, label: `Fix login`, ident: `IDT-5` } }] : null) }, `exponential`)
    const identEl = ident.querySelector<HTMLElement>(`a[data-chip="issue"] .xui-Markdown-chipIdent`)!
    expect(identEl.textContent).toBe(`IDT-5`)
    expect(identEl.nextElementSibling!.textContent).toBe(`Fix login`)
    expect(identEl.style.fontSize).toBe(`12px`)
    // The catalog theme has no Markdown chip rules: the Chip look alone.
    const neutral = paint(TEXT, chipHost, `neutral`)
    expect(neutral.querySelector<HTMLElement>(`a[data-chip="issue"]`)!.getAttribute(`style`)).toBeNull()
  })
})
