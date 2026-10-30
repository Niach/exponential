// VAPP-152: a streamed `updateComponents` appends its nodes' rules in a new
// `<style>` (the earlier ones keep their element and text), re-binds only
// the nodes it changed, and a changed node still paints its new props.
import { describe, expect, it } from "vitest"
import { render } from "@solidjs/testing-library"
import type { FlatComponent } from "@exponential_app/eui"
import { ExponentialSurface } from "./surface"
import { createSurface } from "./create-surface"

const root = (cards: number): FlatComponent[] => [
  { id: `root`, component: `Box`, style: { display: `flex`, flexDirection: `column`, gap: 12 }, children: [`status`, `transcript`] },
  { id: `status`, component: `Text`, text: { path: `/status` } },
  { id: `transcript`, component: `Box`, style: { display: `flex`, flexDirection: `column`, gap: 8 }, children: Array.from({ length: cards }, (_, i) => `m${i}`) },
] as unknown as FlatComponent[]
const card = (i: number, text = `Step ${i}`): FlatComponent[] => [
  { id: `m${i}`, component: `Box`, style: { padding: 12, borderRadius: 8 }, children: [`m${i}-body`] },
  { id: `m${i}-body`, component: `Text`, text },
] as unknown as FlatComponent[]

describe(`streamed updateComponents`, () => {
  it(`appends a style batch per card, keeps the earlier ones, re-binds only the changed nodes`, () => {
    const surface = createSurface({ surfaceId: `st`, data: { status: `Starting` } })
    surface.apply({ updateComponents: { surfaceId: `st`, components: [...root(1), ...card(0)] } })
    const { container } = render(() => <ExponentialSurface id="st" surface={surface} theme="neutral" />)
    const sheets = () => [...container.querySelectorAll(`style[data-xui-style="nodes"]`)]
    const [first] = sheets()
    const firstText = first.textContent
    expect(sheets()).toHaveLength(1)
    const body0 = container.querySelector(`[data-xui-id="m0-body"]`)!
    surface.apply({ updateComponents: { surfaceId: `st`, components: [...root(2).filter((c) => c.id === `transcript`), ...card(1)] } })
    expect(sheets()[0]).toBe(first)
    expect(first.textContent).toBe(firstText)
    expect(sheets()).toHaveLength(2)
    expect(sheets()[1].textContent).toContain(`.xui-n-m1{`)
    expect(sheets()[1].textContent).not.toContain(`.xui-n-m0{`)
    expect(container.querySelector(`[data-xui-id="m0-body"]`)).toBe(body0)
    expect(container.querySelector(`[data-xui-id="m1-body"]`)!.textContent).toBe(`Step 1`)
    // A changed node paints its new text; a data write reaches its binding.
    surface.apply({ updateComponents: { surfaceId: `st`, components: card(0, `Edited`).slice(1) } })
    expect(container.querySelector(`[data-xui-id="m0-body"]`)!.textContent).toBe(`Edited`)
    surface.apply({ updateDataModel: { surfaceId: `st`, path: `/status`, value: `Working` } })
    expect(container.querySelector(`[data-xui-id="status"]`)!.textContent).toBe(`Working`)
  })
})
