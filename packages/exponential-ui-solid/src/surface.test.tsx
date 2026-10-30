// FEED-128: the Solid surface's behaviour — the kitchen sink mounts with the
// two style tags, every node carries its id, a mode switch never recompiles
// the theme sheet, a press routes through the plugin with its context,
// `createSurface` applies A2UI messages, a re-reduce re-binds IN PLACE.

import { describe, expect, it } from "vitest"
import { createSignal } from "solid-js"
import { fireEvent, render } from "@solidjs/testing-library"
import { CORE_CATALOG_ID, builtinTheme, reduceNested } from "@exponential_app/eui"
import type { NestedNode, UiNode } from "@exponential_app/eui"
import { BASE_CSS, compiledTheme, walkNodes } from "@exponential_app/eui/web"
import kitchenSink from "@exponential_app/eui/fixtures/kitchen-sink.json"
import { ExponentialSurface, type SurfaceHandle } from "./surface"
import { createSurface } from "./create-surface"
import type { HostPlugin, SurfaceActionEvent } from "./host"

const tree = (node: NestedNode) => reduceNested(node, { catalogId: CORE_CATALOG_ID })

const nested: NestedNode = {
  id: `root`,
  component: `Stack`,
  props: { gap: `sm` },
  children: [
    { id: `h`, component: `Heading`, props: { text: `Title` } },
    { id: `row`, component: `Stack`, props: { direction: `horizontal` }, children: [{ id: `a`, component: `Text`, props: { text: { path: `/a` } } }, { id: `b`, component: `Icon`, props: { name: `star` } }] },
    { id: `box`, component: `Box`, style: { padding: 8 }, children: [{ id: `c`, component: `Text`, props: { text: `deep` } }] },
  ],
}

describe(`ExponentialSurface`, () => {
  it(`mounts the kitchen sink: a .xui-surface root, the theme sheet = BASE_CSS + the compiled css, the node sheet`, () => {
    const { root, templates } = tree(kitchenSink as unknown as NestedNode)
    const { container } = render(() => <ExponentialSurface id="ks" root={root} templates={templates} theme="orange" mode="dark" />)
    const surface = container.querySelector(`.xui-surface`) as HTMLElement
    expect(surface).not.toBeNull()
    expect(surface.classList.contains(compiledTheme(builtinTheme(`orange`)).scope)).toBe(true)
    expect(surface.dataset.xuiTheme).toBe(`orange`)
    expect(surface.dataset.xuiMode).toBe(`dark`)
    expect(container.querySelector(`style[data-xui-style="theme"]`)!.textContent).toBe(`${BASE_CSS}\n${compiledTheme(builtinTheme(`orange`)).css}`)
    expect(container.querySelector(`style[data-xui-style="nodes"]`)!.textContent).toContain(`.xui-s-ks`)
    expect(surface.querySelector(`[data-xui-layer="overlay"]`)).not.toBeNull()
    expect(surface.querySelector(`[data-xui-layer="toast"]`)).not.toBeNull()
    expect(surface.querySelectorAll(`[data-xui-live]`).length).toBe(2)
    expect(surface.querySelector(`[data-xui-id="${root.id}"]`)).not.toBeNull()
  })

  it(`gives every node of a nested tree its data-xui-id`, () => {
    const { root } = tree(nested)
    const { container } = render(() => <ExponentialSurface id="n" root={root} data={{ a: `bound` }} theme="neutral" />)
    for (const n of walkNodes(root)) expect(container.querySelector(`[data-xui-id="${n.id}"]`), n.id).not.toBeNull()
    expect(container.querySelector(`[data-xui-id="a"]`)!.textContent).toBe(`bound`)
  })

  it(`a mode switch flips data-xui-mode without recompiling the theme sheet`, () => {
    const { root } = tree(nested)
    const [mode, setMode] = createSignal<`light` | `dark`>(`light`)
    const { container } = render(() => <ExponentialSurface id="m" root={root} theme="exponential" mode={mode()} />)
    const surface = container.querySelector(`.xui-surface`) as HTMLElement
    const sheet = container.querySelector(`style[data-xui-style="theme"]`)!
    const before = sheet.textContent
    expect(surface.dataset.xuiMode).toBe(`light`)
    setMode(`dark`)
    expect(surface.dataset.xuiMode).toBe(`dark`)
    expect(container.querySelector(`style[data-xui-style="theme"]`)).toBe(sheet)
    expect(sheet.textContent).toBe(before)
  })

  it(`a press on a Text inside a pressable Box routes onAction through the plugin with the resolved context`, async () => {
    const actions: SurfaceActionEvent[] = []
    const host: HostPlugin = { onAction: (e) => void actions.push(e) }
    const { root } = tree({ id: `root`, component: `Box`, props: { pressable: true }, on: { press: { event: { name: `open`, context: { source: { path: `/src` } } } } }, children: [{ id: `label`, component: `Text`, props: { text: `Open` } }] })
    const { container } = render(() => <ExponentialSurface id="act" root={root} data={{ src: `reddit` }} host={host} theme="neutral" />)
    fireEvent.click(container.querySelector(`[data-xui-id="label"]`)!)
    expect(actions).toHaveLength(1)
    expect(actions[0]).toMatchObject({ surfaceId: `act`, event: `press`, name: `open`, componentId: `root`, context: { source: `reddit` } })
  })

  it(`a set function writes the data model before the event and re-binds the Text`, () => {
    const actions: SurfaceActionEvent[] = []
    const { root } = tree({ id: `root`, component: `Box`, props: { pressable: true }, on: { press: { functionCall: { call: `set`, args: { path: `/n`, value: 2 } }, event: { name: `bumped`, context: { n: { path: `/n` } } } } }, children: [{ id: `n`, component: `Text`, props: { text: { path: `/n` } } }] })
    const { container } = render(() => <ExponentialSurface id="set" root={root} data={{ n: 1 }} host={{ onAction: (e) => void actions.push(e) }} theme="neutral" />)
    expect(container.querySelector(`[data-xui-id="n"]`)!.textContent).toBe(`1`)
    fireEvent.click(container.querySelector(`[data-xui-id="root"]`)!)
    expect(container.querySelector(`[data-xui-id="n"]`)!.textContent).toBe(`2`)
    expect(actions[0].context).toEqual({ n: 1 })
  })

  it(`an unknown component paints the placeholder and tells the host`, () => {
    const seen: string[] = []
    const { root } = tree({ id: `root`, component: `Gauge` as never, props: {} })
    const { container } = render(() => <ExponentialSurface root={root} theme="neutral" id="u" host={{ onUnknown: (n: UiNode) => void seen.push(String(n.props.component)) }} />)
    expect(container.querySelector(`[data-xui-c="Unknown"]`)!.textContent).toContain(`Unknown component Gauge`)
    expect(seen).toEqual([`Gauge`])
  })

  it(`hands the host its handle (focus, scrollIntoView)`, () => {
    let handle: SurfaceHandle | undefined
    const { root } = tree({ id: `root`, component: `Box`, props: { pressable: true }, children: [] })
    const { container } = render(() => <ExponentialSurface root={root} theme="neutral" id="h" handleRef={(h) => (handle = h)} />)
    expect(handle!.focus(`root`)).toBe(true)
    expect(document.activeElement).toBe(container.querySelector(`[data-xui-id="root"]`))
    expect(handle!.focus(`nope`)).toBe(false)
    expect(handle!.scrollToIndex(`nope`, 0)).toBe(false)
  })
})

describe(`createSurface`, () => {
  it(`applies A2UI messages, resolves bindings and renders a template per item; a re-reduce keeps the painted nodes`, () => {
    const s = createSurface({ surfaceId: `s` })
    s.apply({ version: `v0.9`, createSurface: { surfaceId: `s`, catalogId: CORE_CATALOG_ID } })
    s.apply({
      version: `v0.9`,
      updateComponents: {
        surfaceId: `s`,
        components: [
          { id: `root`, component: `List`, children: { componentId: `row`, path: `/rows` } },
          { id: `row`, component: `Text`, text: { path: `title` } },
        ],
      },
    })
    s.apply({ version: `v0.9`, updateDataModel: { surfaceId: `s`, path: `/rows`, value: [{ title: `one` }, { title: `two` }] } })
    expect(s.root?.component).toBe(`List`)
    const { container } = render(() => <ExponentialSurface surface={s} theme="neutral" />)
    const texts = () => Array.from(container.querySelectorAll(`[data-xui-c="Text"]`)).map((e) => e.textContent)
    expect(texts()).toEqual([`one`, `two`])
    const first = container.querySelector(`[data-xui-id="row.0"]`)
    expect(first).not.toBeNull()
    // A data write re-binds in place.
    s.setData(`/rows/0/title`, `uno`)
    expect(texts()).toEqual([`uno`, `two`])
    expect(container.querySelector(`[data-xui-id="row.0"]`)).toBe(first)
    // A re-reduce (new node objects, same ids) keeps the painted elements.
    s.apply({ version: `v0.9`, updateComponents: { surfaceId: `s`, components: [{ id: `row`, component: `Text`, text: { path: `title` }, variant: `caption` }] } })
    expect(container.querySelector(`[data-xui-id="row.0"]`)).toBe(first)
    expect(first!.getAttribute(`data-r-variant`)).toBe(`caption`)
    s.apply({ version: `v0.9`, deleteSurface: { surfaceId: `s` } })
    expect(s.root).toBeNull()
    expect(s.deleted).toBe(true)
  })
  it(`ignores messages for another surface`, () => {
    const s = createSurface({ surfaceId: `s` })
    s.apply({ updateDataModel: { surfaceId: `other`, path: `/x`, value: 1 } })
    expect(s.data).toEqual({})
  })
})
