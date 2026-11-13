// #EXP-1813: a Button is a flex item like any other (the Rust core's twin:
// apps/desktop/crates/eui-core/tests/exp1813_parity.rs): a stretch column (a
// Section's rows, a vertical Stack) stretches it across, a row, `align:
// start` or an overlay's `trigger` slot keeps its own width.

import { afterAll, beforeAll, expect, it } from "vitest"
import { openPage, startHarness, type Harness } from "./support"

let h: Harness
beforeAll(async () => {
  h = await startHarness()
})
afterAll(async () => h?.close())

const button = (id: string) => ({ id, component: `Button`, props: { label: `Add secret`, variant: `outline`, size: `sm` } })

it(`a Button stretches in a stretch column and keeps its width in a row or a trigger slot`, async () => {
  const node = {
    id: `ci`,
    component: `Section`,
    props: { title: `CI` },
    children: [
      button(`direct`),
      { id: `col`, component: `Stack`, props: { direction: `vertical` }, children: [button(`in_column`)] },
      { id: `row`, component: `Stack`, props: { direction: `horizontal` }, children: [button(`in_row`)] },
      { id: `start`, component: `Stack`, props: { direction: `vertical`, align: `start` }, children: [button(`in_start`)] },
      { id: `drawer`, component: `Drawer`, props: { title: `Add secret` }, slots: { trigger: button(`trigger`) }, children: [{ id: `body`, component: `Text`, props: { text: `x` } }] },
    ],
  }
  const page = await openPage(h, { view: `tree`, tree: JSON.stringify(node), data: `{}`, theme: `neutral`, mode: `light`, width: 390 }, { width: 390, height: 700 })
  await page.waitForSelector(`[data-xui-id="trigger"]`)
  const w = (id: string) => page.evaluate((id) => document.querySelector(`[data-xui-id="${id}"]`)!.getBoundingClientRect().width, id)
  const full = await w(`ci`)
  expect(Math.abs((await w(`direct`)) - full)).toBeLessThanOrEqual(1)
  expect(Math.abs((await w(`in_column`)) - full)).toBeLessThanOrEqual(1)
  const own = await w(`in_row`)
  expect(own).toBeLessThan(full / 2)
  expect(Math.abs((await w(`in_start`)) - own)).toBeLessThanOrEqual(1)
  expect(Math.abs((await w(`trigger`)) - own)).toBeLessThanOrEqual(1)
  await page.context().close()
})
