// #EDE-65 lane M: fixtures/embed.json on React. `render`: mount the host
// root with its data under the neutral theme; every `texts` entry is
// painted in order (the nested surface in place of its Embed), no `absent`
// one is. `events`: press the inner node `press.node` of the
// `press.embed`-th painted Embed; the host hears exactly `actions`, its
// host-function handler `functions` (an inner host function is denied),
// then `texts` / `absent` hold (an inner `set` writes the nested model).

import { afterEach, describe, expect, it } from "vitest"
import { act, cleanup, fireEvent, render } from "@testing-library/react"
import { CORE_CATALOG_ID, embedSurfaceId, reduceNested, type NestedNode } from "@exponential_app/eui"
import fixture from "@exponential_app/eui/fixtures/embed.json"
import { ExponentialSurface } from "./surface"
import type { SurfaceActionEvent } from "./host"

afterEach(() => cleanup())

interface RenderCase {
  name: string
  root: NestedNode
  data: Record<string, unknown>
  texts: string[]
  absent: string[]
}
interface EventCase extends RenderCase {
  press: { embed: number; node: string }
  actions: { name: string; context: Record<string, unknown> }[]
  functions: { name: string; args: Record<string, unknown> }[]
}
const fx = fixture as unknown as { render: RenderCase[]; events: EventCase[] }

/** The painted text, `<style>` sheets left out. */
function paintedText(el: Element): string {
  let out = ``
  const walk = (n: Node) => {
    if (n.nodeType === 3) out += `${n.nodeValue ?? ``}\n`
    else if (n.nodeType === 1 && (n as Element).localName !== `style`) for (const c of Array.from(n.childNodes)) walk(c)
  }
  walk(el)
  return out
}

function expectTexts(el: Element, texts: string[], absent: string[]) {
  const text = paintedText(el)
  let at = 0
  for (const t of texts) {
    const i = text.indexOf(t, at)
    expect(i, `"${t}" painted after offset ${at}`).toBeGreaterThanOrEqual(0)
    at = i + t.length
  }
  for (const t of absent) expect(text, `"${t}" absent`).not.toContain(t)
}

function mount(c: RenderCase) {
  const actions: SurfaceActionEvent[] = []
  const functions: { name: string; args: Record<string, unknown> }[] = []
  const { root, templates } = reduceNested(c.root, { catalogId: CORE_CATALOG_ID })
  const r = render(
    <ExponentialSurface
      id="host"
      root={root}
      templates={templates}
      data={c.data}
      theme="neutral"
      mode="light"
      host={{ onAction: (e) => void actions.push(e), onFunctionCall: (call) => void functions.push({ name: call.name, args: call.args }) }}
    />
  )
  return { ...r, actions, functions }
}

describe(`fixtures/embed.json render`, () => {
  for (const c of fx.render) {
    it(c.name, () => {
      const { container } = mount(c)
      expectTexts(container, c.texts, c.absent)
    })
  }
})

describe(`fixtures/embed.json events`, () => {
  for (const c of fx.events) {
    it(c.name, async () => {
      const { container, actions, functions } = mount(c)
      const embeds = Array.from(container.querySelectorAll<HTMLElement>(`[data-xui-c="Embed"]`))
      const embed = embeds[c.press.embed]
      expect(embed, `Embed #${c.press.embed}`).toBeDefined()
      const target = Array.from(embed.querySelectorAll<HTMLElement>(`[data-xui-id="${c.press.node}"]`)).find((el) => el.closest(`[data-xui-c="Embed"]`) === embed)
      expect(target, `inner node ${c.press.node}`).toBeDefined()
      await act(async () => {
        fireEvent.click(target!)
        await new Promise((r) => setTimeout(r, 0))
      })
      expect(actions.map((a) => ({ name: a.name, context: a.context }))).toEqual(c.actions)
      expect(functions).toEqual(c.functions)
      expectTexts(container, c.texts, c.absent)
    })
  }
})

describe(`Embed sandbox`, () => {
  it(`the nested surface's id is embedSurfaceId(host, painted id); host and inner ids never meet`, () => {
    const c = fx.render.find((r) => r.name === `same-ids-as-host`)!
    const { container } = mount(c)
    const inner = container.querySelector(`[data-xui-c="Embed"] .xui-surface`)!
    expect(inner.getAttribute(`data-xui-surface`)).toBe(embedSurfaceId(`host`, `card`))
    expect(inner.getAttribute(`data-xui-theme`)).toBe(`neutral`)
    expect(inner.getAttribute(`data-xui-mode`)).toBe(`light`)
  })
  it(`template rows get one nested surface each, by instance id`, () => {
    const c = fx.render.find((r) => r.name === `template-rows`)!
    const { container } = mount(c)
    const ids = Array.from(container.querySelectorAll(`[data-xui-c="Embed"] > .xui-surface`)).map((s) => s.getAttribute(`data-xui-surface`))
    expect(ids).toEqual([embedSurfaceId(`host`, `row.a`), embedSurfaceId(`host`, `row.b`)])
    // The nested surface starts at its root: no host instance suffix on its ids.
    expect(container.querySelectorAll(`[data-xui-c="Embed"] [data-xui-id="t"]`).length).toBe(2)
  })
  it(`a deep change of the bound surface remounts the card; an equal one keeps its state`, async () => {
    const tree: NestedNode = { id: `host`, component: `Embed`, props: { surface: { "@path": `/card` } } } as NestedNode
    const card = (label: string) => ({ root: { id: `s`, component: `Stack`, children: [{ id: `v`, component: `Text`, props: { text: { "@path": `/v` } } }, { id: `b`, component: `Button`, props: { label }, on: { press: { functionCall: { "@call": `set`, args: { path: `/v`, value: `bumped` } } } } }] }, data: { v: `fresh` } })
    const { root, templates } = reduceNested(tree, { catalogId: CORE_CATALOG_ID })
    const r = render(<ExponentialSurface id="h" root={root} templates={templates} data={{ card: card(`Go`) }} theme="neutral" />)
    await act(async () => void fireEvent.click(r.container.querySelector(`[data-xui-id="b"]`)!))
    expect(paintedText(r.container)).toContain(`bumped`)
    r.rerender(<ExponentialSurface id="h" root={root} templates={templates} data={{ card: card(`Go`) }} theme="neutral" />)
    expect(paintedText(r.container)).toContain(`bumped`)
    r.rerender(<ExponentialSurface id="h" root={root} templates={templates} data={{ card: card(`Again`) }} theme="neutral" />)
    expect(paintedText(r.container)).toContain(`fresh`)
    expect(paintedText(r.container)).toContain(`Again`)
  })
})
