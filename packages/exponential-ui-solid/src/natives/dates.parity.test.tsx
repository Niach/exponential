// FEED-128: DOM parity of the date natives (natives/dates.tsx) with React:
// every catalog-components.json example of DatePicker / DateRangePicker /
// TimePicker, closed and OPENED (the trigger clicked: the portalled
// popover, the calendar or the time list), forced states, and dark + rtl +
// de-AT (a Monday week start).

import { describe, expect, it } from "vitest"
import { act } from "react"
import fixture from "@exponential_app/ui/fixtures/catalog-components.json"
import type { NestedNode } from "@exponential_app/ui"
import { renderReact, renderSolidSurface, serializeDom, type ParityCase } from "../test/parity"

const COMPONENTS = new Set([`DatePicker`, `DateRangePicker`, `TimePicker`])
const cases = (fixture as unknown as { cases: { name: string; node: NestedNode }[] }).cases.filter((c) => COMPONENTS.has(c.node.component))

const tick = () => new Promise((r) => setTimeout(r, 0))

/** React 19's `useId` (`_r_1_`) is a generated id too: prefixed so the
 *  parity serialiser renames it in document order like Radix's/Solid's. */
function markReactIds(root: Element): void {
  for (const el of [root, ...Array.from(root.querySelectorAll(`*`))])
    for (const a of Array.from(el.attributes)) if (/(^|\s)_r_[0-9a-z]+_/.test(a.value) && [`id`, `for`, `aria-controls`, `aria-labelledby`, `aria-describedby`, `aria-activedescendant`].includes(a.name)) el.setAttribute(a.name, a.value.replace(/(^|\s)(_r_[0-9a-z]+_)/g, `$1radix-$2`))
}

/** Both DOMs, optionally after opening the picker (a click on its trigger). */
async function both(c: ParityCase, open: boolean): Promise<{ react: string; solid: string }> {
  // One at a time: an open popover dismisses on focus moving outside it,
  // and the other renderer's calendar IS outside.
  const r = await renderReact(c)
  let react: string
  try {
    if (open) {
      const rt = r.root.querySelector<HTMLButtonElement>(`[data-xui-part$="/trigger"]`)!
      await act(async () => {
        rt.click()
        await tick()
      })
      await act(async () => {
        await tick()
      })
    }
    markReactIds(r.root)
    react = serializeDom(r.root)
  } finally {
    r.cleanup()
  }
  const s = await renderSolidSurface(c)
  try {
    if (open) {
      s.root.querySelector<HTMLButtonElement>(`[data-xui-part$="/trigger"]`)!.click()
      await tick()
      await tick()
    }
    return { react, solid: serializeDom(s.root) }
  } finally {
    s.cleanup()
  }
}

async function expectParity(c: ParityCase, open = false): Promise<void> {
  const { react, solid } = await both(c, open)
  if (open) expect(react).toContain(`data-radix-popper-content-wrapper`)
  expect(solid).toBe(react)
}

describe(`DOM parity: dates`, { timeout: 60_000 }, () => {
  for (const c of cases) {
    it(`${c.name}`, async () => expectParity({ tree: c.node }))
    it(`${c.name} (open)`, async () => {
      if (c.node.props?.disabled === true) return
      await expectParity({ tree: c.node }, true)
    })
  }
  for (const component of COMPONENTS) {
    const c = cases.find((x) => x.node.component === component)!
    it(`${component}: forced states`, async () => expectParity({ tree: c.node, states: [`hover`, `focus`, `pressed`, `checked`, `open`, `disabled`, `invalid`] }))
    it(`${component}: dark, rtl, de-AT, opened`, async () => {
      const { react, solid } = await both({ tree: c.node, theme: `exponential`, mode: `dark`, direction: `rtl`, locale: `de-AT` }, true)
      // de-AT: the week starts on Monday.
      if (component !== `TimePicker`) expect(react).toMatch(/xui-calendar-weekday" role="columnheader" title="Mo/)
      expect(solid).toBe(react)
    })
  }
  it(`DatePicker: min/max disable days, firstDayOfWeek, placeholder, a validation error`, async () => {
    const tree = {
      id: `f`,
      component: `Form`,
      props: {},
      children: [{ id: `dp`, component: `DatePicker`, props: { label: `When`, name: `when`, value: `2026-10-14`, min: `2026-10-05`, max: `2026-10-20`, firstDayOfWeek: 3, checks: [{ call: `required`, message: `Pick a day` }], validateOn: `change` } }],
    } as unknown as NestedNode
    await expectParity({ tree }, true)
    await expectParity({ tree: { id: `dp`, component: `DatePicker`, props: { label: `When`, placeholder: `Pick a day` } } })
  })
  it(`TimePicker: step, min/max, the locale clock`, async () => {
    await expectParity({ tree: { id: `tp`, component: `TimePicker`, props: { label: `At`, value: `13:20`, step: 20, min: `08:00`, max: `18:00` } }, locale: `de-AT` }, true)
  })
})
