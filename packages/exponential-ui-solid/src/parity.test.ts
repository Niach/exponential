// FEED-128: DOM parity with the React renderer, case by case (the pattern
// every native port extends: add the native's cases here; see
// src/test/parity.ts).

import { describe, it } from "vitest"
import type { NestedNode } from "@exponential_app/eui"
import { expectDomParity } from "./test/parity"

const layout: NestedNode = {
  id: `root`,
  component: `Stack`,
  props: { gap: `md` },
  style: { padding: 16, backgroundColor: `$color.surface` },
  children: [
    { id: `title`, component: `Heading`, props: { text: `Release notes`, level: 2 } },
    { id: `lede`, component: `Text`, props: { text: { path: `/lede` }, lines: 2 }, style: { color: `$color.mutedForeground` } },
    {
      id: `row`,
      component: `Stack`,
      props: { direction: `horizontal`, gap: `sm`, align: `center` },
      children: [
        { id: `star`, component: `Icon`, props: { name: `star`, label: `Starred` } },
        { id: `count`, component: `Text`, props: { text: { path: `/count` }, live: `polite` } },
        { id: `box`, component: `Box`, style: { width: { path: `/w` }, height: 12, borderRadius: 4 } },
      ],
    },
    { id: `card`, component: `Card`, props: { title: `Card`, description: { path: `/lede` } }, children: [{ id: `badge`, component: `Badge`, props: { text: `New`, icon: `star` } }] },
    { id: `press`, component: `Box`, props: { pressable: true }, accessibility: { label: `Open` }, children: [{ id: `press-t`, component: `Text`, props: { text: `Open`, align: `center` } }] },
  ],
}

describe(`DOM parity with React`, () => {
  it(`text + layout: Heading, Text (bound, clamped, live), Stack/Card/Badge macros, Box with styles, a pressable Box, Icon`, async () => {
    await expectDomParity({ tree: layout, data: { lede: `Two lines at most of a long lede.`, count: 42, w: 64 } })
  })
  it(`the same tree in the exponential theme, dark, rtl, with forced states`, async () => {
    await expectDomParity({ tree: layout, data: { lede: `x`, count: 1, w: 8 }, theme: `exponential`, mode: `dark`, direction: `rtl`, states: [`hover`] })
  })
  it(`a templated List with dividers and a section header`, async () => {
    await expectDomParity({
      tree: {
        id: `root`,
        component: `Box`,
        children: [
          {
            id: `list`,
            component: `List`,
            props: { divided: true, gap: `sm`, sectionBy: `day` },
            template: { component: `item`, path: `/items`, key: `id` },
            children: [],
            slots: { section: { id: `head`, component: `Text`, props: { text: { path: `value` } } } },
          },
          { id: `item`, component: `Text`, props: { text: { path: `t` } } },
          { id: `flat`, component: `List`, props: { gap: `md` }, children: [{ id: `f1`, component: `Text`, props: { text: `one` } }, { id: `f2`, component: `Text`, props: { text: `two` } }] },
        ],
      } as unknown as NestedNode,
      data: { items: [{ id: `a`, day: `Today`, t: `A` }, { id: `b`, day: `Today`, t: `B` }, { id: `c`, day: `Earlier`, t: `C` }] },
    })
  })
  it(`a List past the window threshold (WindowedList)`, async () => {
    await expectDomParity({
      tree: { id: `root`, component: `List`, props: { divided: true }, children: Array.from({ length: 70 }, (_, i) => ({ id: `r${i}`, component: `Text`, props: { text: `row ${i}` } })) },
    })
  })
  it(`Markdown (headings, emphasis, code, a task list, a table, a link) and the Unknown placeholder`, async () => {
    await expectDomParity({
      tree: {
        id: `root`,
        component: `Stack`,
        children: [
          { id: `md`, component: `Markdown`, props: { text: `# Title\n\nSome **bold**, *em*, \`code\` and [a link](https://example.com).\n\n- [x] done\n- [ ] todo\n\n| a | b |\n| :--- | ---: |\n| 1 | 2 |\n\n---` } },
          { id: `gauge`, component: `Gauge` as never, props: {} },
        ],
      },
    })
  })
  // EXP-1605 (M4 §4): with on.press a Motion is a focusable button named by
  // its label; with only other events (hover, state) it stays an image.
  it(`Motion: a pressable state-machine button, an evented image, a decorative one`, async () => {
    const lottie = { v: `5.12.0`, fr: 30, ip: 0, op: 30, w: 10, h: 10, layers: [] }
    const machine = { initial: `off`, states: [{ name: `off`, type: `PlaybackState`, segment: [0, 10] }] }
    await expectDomParity({
      tree: {
        id: `root`,
        component: `Stack`,
        children: [
          { id: `like`, component: `Motion`, props: { lottie, stateMachine: machine, label: `Like` }, on: { press: { event: { name: `liked` } }, state: { event: { name: `state` } } } },
          { id: `bar`, component: `Motion`, props: { lottie, stateMachine: machine, inputs: { path: `/sm` }, label: `Level` }, on: { hover: { event: { name: `h` } } } },
          { id: `deco`, component: `Motion`, props: { lottie, style: { width: 24 } } },
        ],
      } as unknown as NestedNode,
      data: { sm: { level: 30 } },
    })
  })
})
