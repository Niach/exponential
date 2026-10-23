// FEED-130: vanilla extension painters (`defineElementExtension`) and SVG
// markup icons (`iconsFromSvg`): the painter's element carries the root
// attributes, re-paints on a data change, holds the node's children and
// slots, and `emit` fires `xui-action`.

import { afterEach, describe, expect, it, vi } from "vitest"
import { CORE_CATALOG_ID, defineExtension, reduceNested } from "@exponential_app/ui"
import type { NestedNode } from "@exponential_app/ui"
import { applyRootProps, defineElementExtension, iconsFromSvg, registerExtension, type ElementPaintContext, type SurfaceActionDetail } from "./index"
import { cleanup, mount } from "./test/helpers"

afterEach(cleanup)

const catalog = defineExtension({
  id: `https://acme.example/catalog/v1`,
  name: `Acme`,
  extends: CORE_CATALOG_ID,
  components: {
    Gauge: { kind: `native`, group: `data`, lite: true, children: `none`, description: `A gauge.`, events: [`press`], props: { value: { type: `number`, bindable: true, description: `The value.` } } },
    Panel: { kind: `native`, group: `layout`, lite: true, children: `many`, slots: [`header`], description: `A panel.`, props: { title: { type: `string`, bindable: true, description: `The title.` } } },
  },
} as never)

const reduce = (node: NestedNode) => reduceNested(node, { catalogId: CORE_CATALOG_ID, extensions: [catalog] })

describe(`defineElementExtension`, () => {
  it(`paints a vanilla element with the root attributes, re-paints on a data change, emit fires xui-action`, () => {
    const paints: ElementPaintContext[] = []
    const ext = defineElementExtension({
      catalog,
      components: {
        Gauge: (ctx) => {
          paints.push(ctx)
          const el = document.createElement(`button`)
          el.className = `acme-gauge`
          ctx.applyRootProps(el)
          el.textContent = `gauge ${String(ctx.props.value)} ${ctx.mode}`
          el.addEventListener(`click`, () => void ctx.emit(`press`, { at: ctx.props.value }))
          return el
        },
      },
    })
    const { root } = reduce({ id: `g`, component: `Gauge` as never, props: { value: { path: `/v` } }, on: { press: { event: { name: `gauge` } } } })
    const el = mount((e) => {
      e.extensions = [ext]
      e.root = root
      e.data = { v: 3 }
      e.mode = `dark`
    })
    const painted = () => el.querySelector(`[data-xui-id="g"]`) as HTMLButtonElement
    expect(painted().textContent).toBe(`gauge 3 dark`)
    expect(painted().classList.contains(`acme-gauge`)).toBe(true)
    expect(painted().classList.contains(`xui-el`)).toBe(true)
    expect(painted().getAttribute(`data-xui-c`)).toBe(`Gauge`)
    expect(paints[0]!.theme.id).toBeTruthy()
    const first = painted()
    el.data = { v: 7 }
    expect(painted().textContent).toBe(`gauge 7 dark`)
    expect(painted()).not.toBe(first)
    expect(first.isConnected).toBe(false)
    const actions: SurfaceActionDetail[] = []
    el.addEventListener(`xui-action`, (e) => actions.push(e.detail))
    painted().click()
    expect(actions).toHaveLength(1)
    expect(actions[0]).toMatchObject({ name: `gauge`, componentId: `g`, payload: { at: 7 } })
  })

  it(`children() and slot() hand over the painted nodes (kept across re-paints)`, () => {
    const ext = defineElementExtension({
      catalog,
      components: {
        Panel: (ctx) => {
          const el = document.createElement(`section`)
          ctx.applyRootProps(el)
          const head = document.createElement(`header`)
          head.append(...ctx.slot(`header`))
          const h = document.createElement(`h3`)
          h.textContent = String(ctx.props.title)
          const body = document.createElement(`div`)
          body.className = `acme-body`
          body.append(...ctx.children())
          el.append(head, h, body)
          if (ctx.slot(`missing`).length !== 0) throw new Error(`a missing slot is empty`)
          return el
        },
      },
    })
    const { root } = reduce({
      id: `p`,
      component: `Panel` as never,
      props: { title: { path: `/title` } },
      slots: { header: { id: `hd`, component: `Text`, props: { text: `Header` } } },
      children: [
        { id: `c1`, component: `Text`, props: { text: `one` } },
        { id: `c2`, component: `Text`, props: { text: { path: `/two` } } },
      ],
    } as unknown as NestedNode)
    const el = mount((e) => {
      e.extensions = [ext]
      e.root = root
      e.data = { title: `A`, two: `two` }
    })
    const panel = () => el.querySelector(`section[data-xui-id="p"]`)!
    expect(panel().querySelector(`header [data-xui-id="hd"]`)!.textContent).toBe(`Header`)
    expect(Array.from(panel().querySelectorAll(`.acme-body > [data-xui-id]`)).map((n) => n.textContent)).toEqual([`one`, `two`])
    const c1 = panel().querySelector(`[data-xui-id="c1"]`)
    el.data = { title: `B`, two: `deux` }
    expect(panel().querySelector(`h3`)!.textContent).toBe(`B`)
    expect(panel().querySelector(`[data-xui-id="c1"]`)).toBe(c1)
    expect(panel().querySelector(`[data-xui-id="c2"]`)!.textContent).toBe(`deux`)
  })

  it(`a page-wide registerExtension; a painter override of a core native`, () => {
    const ext = defineElementExtension({
        components: {
          Switch: (ctx) => {
            const el = document.createElement(`input`)
            el.type = `checkbox`
            ctx.applyRootProps(el)
            el.checked = ctx.props.checked === true
            return el
          },
        },
      })
    // The defineElementExtension result itself (catalog null) registers.
    const off = registerExtension(ext)
    try {
      const { root } = reduce({ id: `s`, component: `Switch`, props: { label: `On`, checked: true } })
      const el = mount((e) => (e.root = root))
      const sw = el.querySelector(`[data-xui-id="s"]`) as HTMLInputElement
      expect(sw.localName).toBe(`input`)
      expect(sw.checked).toBe(true)
    } finally {
      off()
    }
  })

  it(`applyRootProps: classes merge, style entries, null skips`, () => {
    const el = document.createElement(`div`)
    el.className = `mine`
    applyRootProps(el, { class: `xui-el xui-n-a`, "data-xui-id": `a`, "aria-hidden": true, dir: undefined, style: { "--xui-d-0": `4px`, "background-color": `red` } })
    expect(el.className).toBe(`mine xui-el xui-n-a`)
    expect(el.getAttribute(`data-xui-id`)).toBe(`a`)
    expect(el.getAttribute(`aria-hidden`)).toBe(`true`)
    expect(el.hasAttribute(`dir`)).toBe(false)
    expect(el.style.getPropertyValue(`--xui-d-0`)).toBe(`4px`)
    expect(el.style.backgroundColor).toBe(`red`)
  })
})

describe(`iconsFromSvg`, () => {
  const STAR = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" class="lucide lucide-star" onload="alert(1)"><script>alert(1)</script><path d="M12 2l3 7h7l-5.5 4.5 2 7.5-6.5-4.5-6.5 4.5 2-7.5L2 9h7z"/></svg>`

  it(`paints the markup with lucide-react's root attributes (size, class, aria-hidden), sanitized`, () => {
    const resolve = vi.fn((name: string) => (name === `star` ? STAR : undefined))
    const { root } = reduceNested({ id: `root`, component: `Stack`, children: [{ id: `i`, component: `Icon`, props: { name: `star`, size: 20 } }, { id: `j`, component: `Icon`, props: { name: `ui-check` } }] } as unknown as NestedNode, { catalogId: CORE_CATALOG_ID })
    const el = mount((e) => {
      e.plugin = { icons: iconsFromSvg(resolve) }
      e.root = root
    })
    const svg = el.querySelector(`[data-xui-id="i"] svg, svg[data-xui-id="i"]`) ?? el.querySelector(`[data-icon="star"]`)
    expect(svg).not.toBeNull()
    expect(svg!.localName).toBe(`svg`)
    expect(svg!.getAttribute(`aria-hidden`)).toBe(`true`)
    expect(svg!.getAttribute(`data-icon`)).toBe(`star`)
    expect(svg!.getAttribute(`width`)).toBe(svg!.getAttribute(`height`))
    expect(svg!.getAttribute(`width`)).toMatch(/^\d+$/)
    expect(svg!.classList.contains(`lucide-star`)).toBe(true)
    expect(svg!.querySelector(`script`)).toBeNull()
    expect(svg!.hasAttribute(`onload`)).toBe(false)
    expect(svg!.querySelector(`path`)).not.toBeNull()
    // A name without markup falls through to the renderer's own glyph.
    expect(el.querySelector(`[data-icon="ui-check"]`)!.classList.contains(`lucide-check`)).toBe(true)
  })

  it(`one component per markup; unknown names are undefined`, () => {
    const icons = iconsFromSvg((n) => (n === `a` || n === `b` ? STAR : undefined)) as (name: string) => unknown
    expect(icons(`a`)).toBe(icons(`b`))
    expect(icons(`zzz`)).toBeUndefined()
  })
})
