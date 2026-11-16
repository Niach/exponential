// #EXP-1882: an inline chip keeps the punctuation it touches on its line
// ("(#ABC-1)", "#ABC-1.", "#ABC-1,"): at every width, the glyph next to it
// sits on the chip's line (Chromium breaks before and after an atomic inline
// unless the painter glues them, `.xui-md-glue`), and a chip capped at the
// line leaves it room there.

import { afterAll, beforeAll, expect, it } from "vitest"
import { openPage, startHarness, type Harness } from "./support"

let h: Harness
beforeAll(async () => {
  h = await startHarness()
})
afterAll(async () => h?.close())

const CASES: [string, string, string][] = [`#ABC-1`, `#LONG-1`].flatMap((chip): [string, string, string][] => [
  [`${chip} + ")"`, `merged it (${chip}) ok`, `)`],
  [`${chip} + "."`, `linked it to ${chip}.`, `.`],
  [`${chip} + ","`, `see ${chip}, merged it.`, `,`],
  [`"(" + ${chip}`, `open (${chip} now`, `(`],
])

it(`a chip's punctuation never wraps away from it`, async () => {
  const tree = { id: `root`, component: `Stack`, props: { direction: `vertical` }, children: CASES.map(([, text], i) => ({ id: `md${i}`, component: `Markdown`, props: { text } })) }
  const page = await openPage(h, { view: `tree`, tree: JSON.stringify(tree), data: `{}`, theme: `exponential`, mode: `dark`, chips: `1` }, { width: 600, height: 700 })
  await page.waitForSelector(`.xui-md-chip`)
  const stray = await page.evaluate((cases) => {
    const out: string[] = []
    cases.forEach(([name, , glyph], i) => {
      const md = document.querySelector<HTMLElement>(`[data-xui-id="md${i}"]`)!
      const chip = md.querySelector(`.xui-md-chip`)!
      for (let w = Math.min(Math.ceil(chip.getBoundingClientRect().width) + 1, 160); w <= 360; w++) {
        md.style.width = `${w}px`
        const c = chip.getBoundingClientRect()
        const walker = document.createTreeWalker(md, NodeFilter.SHOW_TEXT)
        const r = document.createRange()
        for (let n = walker.nextNode() as Text | null; n; n = walker.nextNode() as Text | null) {
          if (n.parentElement!.closest(`.xui-md-chip`)) continue
          const k = n.data.indexOf(glyph)
          if (k < 0) continue
          r.setStart(n, k)
          r.setEnd(n, k + 1)
          const g = r.getBoundingClientRect()
          if (Math.abs((g.top + g.bottom) / 2 - (c.top + c.bottom) / 2) > c.height / 2) out.push(`${name} @${w}px`)
          if (g.right > md.getBoundingClientRect().right + 0.5) out.push(`${name} @${w}px overflows`)
        }
      }
    })
    return out
  }, CASES)
  expect(stray).toEqual([])
  await page.context().close()
})
