// FEED-130: DOM parity with React. The element's `.xui-surface` subtree
// (serialised by the Solid package's helper: attributes sorted, generated
// ids renamed) equals the React renderer's for the same tree, theme, mode,
// width, direction and locale. Each renderer paints ALONE and settles
// (floating-ui placement is async), then the next one paints.

import { afterEach, describe, expect, it } from "vitest"
import type { NestedNode } from "@exponential_app/ui"
import bindTime from "@exponential_app/ui/fixtures/bind-time.json"
import { renderReact, serializeDom, type ParityCase } from "../../exponential-ui-solid/src/test/parity"
import type { ExponentialSurfaceElement } from "./element"
import { cardTree, cleanup, mount, tree } from "./test/helpers"

afterEach(cleanup)

const settle = async () => {
  for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 5))
}

// React 19.2's `useId` (`_r_1_`) and the already-renamed `$idN` tokens,
// renumbered in order of appearance on both sides (as the Solid table lane does).
const ID_ATTRS = /\b(id|for|aria-controls|aria-labelledby|aria-describedby|aria-owns|aria-activedescendant)="([^"]*)"/g
const GENERATED = /^(\$id\d+|_r_[0-9a-z]+_.*)$/
function renumber(markup: string): string {
  const ids = new Map<string, string>()
  return markup.replace(ID_ATTRS, (_, name: string, value: string) => {
    const v = value
      .split(/\s+/)
      .map((t) => {
        if (!GENERATED.test(t)) return t
        let hit = ids.get(t)
        if (!hit) {
          hit = `#g${ids.size + 1}`
          ids.set(t, hit)
        }
        return hit
      })
      .join(` `)
    return `${name}="${v}"`
  })
}

async function renderElement(c: ParityCase): Promise<{ root: Element; cleanup: () => void }> {
  const { root, templates } = tree(c.tree)
  const attrs: Record<string, string> = { "surface-id": c.id ?? `parity`, theme: c.theme ?? `neutral`, mode: c.mode ?? `light` }
  if (c.width !== undefined) attrs.width = String(c.width)
  if (c.direction) attrs.direction = c.direction
  if (c.locale) attrs.locale = c.locale
  const el = mount((e: ExponentialSurfaceElement) => {
    e.root = root
    e.templates = templates
    e.data = c.data
    if (c.states) e.states = c.states
  }, attrs)
  await Promise.resolve()
  return { root: el.querySelector(`.xui-surface`)!, cleanup: () => el.remove() }
}

async function expectElementParity(c: ParityCase): Promise<void> {
  const r = await renderReact(c)
  await settle()
  const react = serializeDom(r.root)
  r.cleanup()
  await settle()
  const e = await renderElement(c)
  await settle()
  const element = serializeDom(e.root)
  e.cleanup()
  await settle()
  expect(renumber(element)).toBe(renumber(react))
}

const form: NestedNode = {
  id: `f`,
  component: `Form`,
  props: { summary: true },
  children: [
    { id: `in`, component: `Input`, props: { label: `Title`, name: `title`, description: `Short`, value: { path: `/title` } } },
    { id: `ta`, component: `Textarea`, props: { label: `Body`, name: `body`, value: `Hello` } },
    { id: `nf`, component: `NumberField`, props: { label: `Seats`, name: `seats`, value: { path: `/seats` }, min: 1, max: 50 } },
    { id: `cb`, component: `Checkbox`, props: { label: `Agree`, name: `agree`, checked: { path: `/agree` } } },
    { id: `ra`, component: `Radio`, props: { label: `Plan`, name: `plan`, options: [{ label: `Free`, value: `free` }, { label: `Team`, value: `team` }], value: `team` } },
    { id: `sl`, component: `Slider`, props: { label: `Volume`, name: `vol`, min: 0, max: 10, value: 3 } },
    { id: `go`, component: `Button`, props: { label: `Save`, type: `submit` } },
  ],
} as unknown as NestedNode

const tabs = { id: `tabs`, component: `Tabs`, props: { tabs: [{ label: `Issue`, value: `issue` }, { label: `Run`, value: `run` }], value: `issue` }, children: [{ id: `tabs-child`, component: `Text`, props: { text: `Child` } }] } as unknown as NestedNode

const table = {
  id: `table`,
  component: `Table`,
  props: {
    columns: [{ key: `name`, label: `Name` }, { key: `role`, label: `Role` }, { key: `runs`, label: `Runs`, type: `number`, sortable: true, align: `end` }],
    rows: [{ id: `a`, name: `Alice`, role: `Owner`, runs: 12 }, { id: `b`, name: `Bob`, role: `Member`, runs: 3 }],
    selectable: `multiple`,
  },
} as unknown as NestedNode

const templatedList = {
  id: `root`,
  component: `Box`,
  children: [
    { id: `list`, component: `List`, props: { divided: true }, template: { component: `item`, path: `/items`, key: `id` }, children: [] },
    { id: `item`, component: `Text`, props: { text: { path: `t` } } },
  ],
} as unknown as NestedNode

const slotTable = (bindTime as { extra: { name: string; input: NestedNode; datasets: { data: Record<string, unknown> }[] }[] }).extra.find((x) => x.name.startsWith(`Table/`))!

const popover = { id: `root`, component: `Box`, children: [{ id: `pop`, component: `Popover`, props: { side: `top`, open: true }, slots: { trigger: { id: `pop-t`, component: `Text`, props: { text: `Pop` } } }, children: [{ id: `pop-c`, component: `Text`, props: { text: `Popover body` } }] }] } as unknown as NestedNode

describe(`<exponential-surface> DOM parity with React`, () => {
  for (const theme of [`neutral`, `orange`] as const) {
    for (const mode of [`light`, `dark`] as const) {
      it(`a card with text, button, switch (${theme}, ${mode})`, async () => expectElementParity({ tree: cardTree, data: { name: `Ada`, notify: true }, theme, mode }))
    }
  }
  it(`a card at a fixed width, rtl`, async () => expectElementParity({ tree: cardTree, data: { name: `Ada`, notify: false }, theme: `orange`, width: 360, direction: `rtl` }))
  it(`a form with inputs`, async () => expectElementParity({ tree: form, data: { title: ``, seats: 2, agree: true }, theme: `neutral` }))
  it(`a form with inputs (orange, dark, rtl)`, async () => expectElementParity({ tree: form, data: { title: `x`, seats: 4, agree: false }, theme: `orange`, mode: `dark`, direction: `rtl` }))
  it(`tabs`, async () => expectElementParity({ tree: tabs, theme: `neutral` }))
  it(`tabs (orange, dark)`, async () => expectElementParity({ tree: tabs, theme: `orange`, mode: `dark` }))
  it(`a table`, async () => expectElementParity({ tree: table, theme: `neutral` }))
  it(`a table with templated slot cells`, async () => expectElementParity({ tree: slotTable.input, data: slotTable.datasets[0]!.data, theme: `orange` }))
  it(`a templated list`, async () => expectElementParity({ tree: templatedList, data: { items: [{ id: `a`, t: `A` }, { id: `b`, t: `B` }] }, theme: `neutral`, mode: `dark` }))
  it(`a popover open`, async () => expectElementParity({ tree: popover, theme: `neutral` }))
  it(`a popover open (orange, dark, de-AT)`, async () => expectElementParity({ tree: popover, theme: `orange`, mode: `dark`, locale: `de-AT` }))
})
