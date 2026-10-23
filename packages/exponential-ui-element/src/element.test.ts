// FEED-130: the element's inputs and lifecycle: every observed attribute
// reaches the painted surface, string properties reflect, `apply()` feeds a
// store it makes, detach disposes and re-attach paints again.

import { afterEach, describe, expect, it } from "vitest"
import { builtinTheme } from "@exponential_app/ui"
import type { NestedNode } from "@exponential_app/ui"
import { ExponentialSurfaceElement, TAG, define } from "./index"
import { cardTree, cleanup, mount, surfaceOf, tick, tree } from "./test/helpers"

afterEach(cleanup)

const fixture = (el: ExponentialSurfaceElement, node: NestedNode = cardTree, data: Record<string, unknown> = { name: `Ada`, notify: true }) => {
  const { root, templates } = tree(node)
  el.root = root
  el.templates = templates
  el.data = data
}

const textOf = (el: Element, id: string) => el.querySelector(`[data-xui-id="${id}"]`)?.textContent

describe(`<exponential-surface>`, () => {
  it(`registers once; define() is idempotent; another class on the tag throws; another tag gets a subclass`, () => {
    expect(customElements.get(TAG)).toBe(ExponentialSurfaceElement)
    expect(() => define()).not.toThrow()
    customElements.define(`taken-surface`, class extends HTMLElement {})
    expect(() => define(`taken-surface`)).toThrow(/already defined by another class/)
    define(`my-surface`)
    expect(document.createElement(`my-surface`)).toBeInstanceOf(ExponentialSurfaceElement)
    expect(document.createElement(TAG)).toBeInstanceOf(ExponentialSurfaceElement)
  })

  it(`is display: contents through ONE shared zero-specificity rule (the .xui-surface root is the box); the host gets no inline style`, () => {
    const el = mount(fixture)
    expect(el.getAttribute(`style`)).toBeNull()
    const rules = document.head.querySelectorAll(`style[data-xui-element="${TAG}"]`)
    expect(rules).toHaveLength(1)
    expect(rules[0].textContent).toBe(`:where(${TAG}){display:contents}`)
    define()
    expect(document.head.querySelectorAll(`style[data-xui-element="${TAG}"]`)).toHaveLength(1)
    expect(surfaceOf(el)!.parentElement).toBe(el)
  })

  it(`mounts the first element of a task at once and the rest together in a microtask`, async () => {
    const first = mount(fixture)
    const second = mount(fixture)
    const third = mount(fixture)
    expect(surfaceOf(first)).not.toBeNull()
    expect(surfaceOf(second)).toBeNull()
    expect(surfaceOf(third)).toBeNull()
    third.remove()
    await Promise.resolve()
    expect(surfaceOf(second)).not.toBeNull()
    expect(surfaceOf(third)).toBeNull()
    document.body.appendChild(third)
    expect(surfaceOf(third)).not.toBeNull()
  })

  it(`paints a fixture (root + templates + data) and binds data`, () => {
    const el = mount(fixture)
    const surface = surfaceOf(el)!
    expect(surface.classList.contains(`xui-s-main`)).toBe(true)
    expect(textOf(el, `text`)).toBe(`Ada`)
    el.data = { name: `Grace`, notify: false }
    expect(textOf(el, `text`)).toBe(`Grace`)
  })

  it(`theme: attribute → paint, property → attribute; an object theme paints and is not reflected`, () => {
    const el = mount(fixture, { theme: `orange` })
    expect(surfaceOf(el)!.dataset.xuiTheme).toBe(`orange`)
    expect(el.theme).toBe(`orange`)
    el.theme = `exponential`
    expect(el.getAttribute(`theme`)).toBe(`exponential`)
    expect(surfaceOf(el)!.dataset.xuiTheme).toBe(`exponential`)
    el.setAttribute(`theme`, `neutral`)
    expect(el.theme).toBe(`neutral`)
    expect(surfaceOf(el)!.dataset.xuiTheme).toBe(`neutral`)
    el.theme = builtinTheme(`playful`)
    expect(el.getAttribute(`theme`)).toBe(`neutral`)
    expect(surfaceOf(el)!.dataset.xuiTheme).toBe(`playful`)
    el.theme = null
    expect(el.hasAttribute(`theme`)).toBe(false)
  })

  it(`mode, density, contrast`, () => {
    const el = mount(fixture, { mode: `dark`, theme: `neutral` })
    const surface = () => surfaceOf(el)!
    expect(surface().dataset.xuiMode).toBe(`dark`)
    el.mode = `light`
    expect(el.getAttribute(`mode`)).toBe(`light`)
    expect(surface().dataset.xuiMode).toBe(`light`)
    expect(surface().dataset.xuiDensity).toBeUndefined()
    el.setAttribute(`density`, `compact`)
    expect(el.density).toBe(`compact`)
    expect(surface().dataset.xuiDensity).toBe(`compact`)
    const sheet = () => surface().querySelector(`style[data-xui-style="theme"]`)!.textContent
    el.contrast = `normal`
    const normal = sheet()
    el.setAttribute(`contrast`, `high`)
    expect(sheet()).not.toBe(normal)
    el.mode = `bogus` as never
    expect(surface().dataset.xuiMode === `light` || surface().dataset.xuiMode === `dark`).toBe(true)
  })

  it(`locale, time-zone, direction`, () => {
    const node: NestedNode = {
      id: `root`,
      component: `Stack`,
      children: [
        { id: `n`, component: `Text`, props: { text: { call: `formatNumber`, args: { value: 1234.5 } } } },
        { id: `d`, component: `Text`, props: { text: { call: `formatDate`, args: { value: `2026-10-14T23:30:00Z`, format: `MMM d, yyyy` } } } },
      ],
    } as unknown as NestedNode
    const el = mount((e) => fixture(e, node, {}), { locale: `en-US`, "time-zone": `UTC` })
    expect(textOf(el, `n`)).toBe(`1,234.5`)
    expect(textOf(el, `d`)).toBe(`Oct 14, 2026`)
    el.locale = `de-DE`
    expect(el.getAttribute(`locale`)).toBe(`de-DE`)
    expect(textOf(el, `n`)).toBe(`1.234,5`)
    el.timeZone = `Asia/Tokyo`
    expect(el.getAttribute(`time-zone`)).toBe(`Asia/Tokyo`)
    expect(textOf(el, `d`)).toContain(`15`)
    expect(surfaceOf(el)!.getAttribute(`dir`)).toBe(`ltr`)
    el.locale = `ar`
    expect(surfaceOf(el)!.getAttribute(`dir`)).toBe(`rtl`)
    el.setAttribute(`direction`, `ltr`)
    expect(el.direction).toBe(`ltr`)
    expect(surfaceOf(el)!.getAttribute(`dir`)).toBe(`ltr`)
    el.direction = `rtl`
    expect(el.getAttribute(`direction`)).toBe(`rtl`)
    expect(surfaceOf(el)!.getAttribute(`dir`)).toBe(`rtl`)
  })

  it(`width and height: a numeric string = px, any other string a CSS length; a number property is not reflected`, () => {
    const el = mount(fixture, { width: `320`, height: `200` })
    const surface = () => surfaceOf(el)!
    expect(surface().style.width).toBe(`320px`)
    expect(surface().style.height).toBe(`200px`)
    el.width = `50%`
    expect(el.getAttribute(`width`)).toBe(`50%`)
    expect(surface().style.width).toBe(`50%`)
    el.width = 480
    expect(el.getAttribute(`width`)).toBe(`50%`)
    expect(surface().style.width).toBe(`480px`)
    el.height = `20rem`
    expect(surface().style.height).toBe(`20rem`)
    el.removeAttribute(`height`)
    expect(surface().style.height).toBe(``)
  })

  it(`surface-id names the surface (class, data-xui-surface) and the store apply() makes`, () => {
    const el = mount(fixture, { "surface-id": `settings` })
    expect(el.surfaceId).toBe(`settings`)
    expect(surfaceOf(el)!.classList.contains(`xui-s-settings`)).toBe(true)
    expect(surfaceOf(el)!.dataset.xuiSurface).toBe(`settings`)
    el.surfaceId = `other`
    expect(el.getAttribute(`surface-id`)).toBe(`other`)
    expect(surfaceOf(el)!.classList.contains(`xui-s-other`)).toBe(true)
  })

  it(`properties set before attach (and before upgrade) paint on attach`, async () => {
    const el = document.createElement(TAG)
    el.theme = `orange`
    el.mode = `dark`
    fixture(el)
    expect(el.getAttribute(`theme`)).toBe(`orange`)
    document.body.appendChild(el)
    expect(surfaceOf(el)!.dataset.xuiTheme).toBe(`orange`)
    expect(surfaceOf(el)!.dataset.xuiMode).toBe(`dark`)

    const raw = document.createElement(`late-surface`) as ExponentialSurfaceElement
    ;(raw as unknown as Record<string, unknown>).theme = `playful`
    document.body.appendChild(raw)
    define(`late-surface`)
    ;(raw as unknown as { root: unknown }).root = tree(cardTree).root
    // The second element attached in this task mounts in the microtask batch.
    await Promise.resolve()
    expect(surfaceOf(raw)!.dataset.xuiTheme).toBe(`playful`)
    expect(raw.getAttribute(`theme`)).toBe(`playful`)
  })

  it(`apply(): a store on first use; updateDataModel re-binds; deleteSurface empties; surfaceState + issues`, () => {
    const el = mount()
    expect(el.surfaceState).toBeNull()
    expect(el.issues).toEqual([])
    el.apply({ createSurface: { surfaceId: `main`, catalogId: `https://a2ui.org/specification/v0_9/basic_catalog.json` } })
    el.apply({ updateComponents: { surfaceId: `main`, components: [{ id: `root`, component: `Column`, children: [`t`] }, { id: `t`, component: `Text`, text: { path: `/greeting` } }] as never } })
    el.apply({ updateDataModel: { surfaceId: `main`, path: `/greeting`, value: `Hello` } })
    expect(el.surfaceState).not.toBeNull()
    expect(el.surfaceState!.root).not.toBeNull()
    expect(textOf(el, `t`)).toBe(`Hello`)
    el.apply({ updateDataModel: { surfaceId: `main`, path: `/greeting`, value: `Bye` } })
    expect(textOf(el, `t`)).toBe(`Bye`)
    el.setData(`/greeting`, `Set`)
    expect(textOf(el, `t`)).toBe(`Set`)
    expect(el.issues).toBe(el.surfaceState!.issues)
    el.apply({ updateDataModel: { surfaceId: `elsewhere`, path: `/greeting`, value: `Nope` } })
    expect(textOf(el, `t`)).toBe(`Set`)
    el.apply({ deleteSurface: { surfaceId: `main` } })
    expect(el.querySelector(`[data-xui-id="t"]`)).toBeNull()
    expect(el.surfaceState!.deleted).toBe(true)
  })

  it(`apply() forwards to a surface property`, async () => {
    const { createSurface } = await import(`./index`)
    const store = createSurface({ surfaceId: `main` })
    const el = mount((e) => (e.surface = store))
    el.apply({ updateComponents: { surfaceId: `main`, components: [{ id: `root`, component: `Text`, text: `From the store` }] as never } })
    expect(store.root).not.toBeNull()
    expect(el.surfaceState).toBe(store)
    expect(textOf(el, `root`)).toBe(`From the store`)
  })

  it(`setData in fixture mode writes a new data model`, () => {
    const el = mount(fixture)
    el.setData(`/name`, `Linus`)
    expect(el.data).toMatchObject({ name: `Linus` })
    expect(textOf(el, `text`)).toBe(`Linus`)
  })

  it(`detach disposes the painted DOM; re-attach paints again with its props`, async () => {
    const el = mount(fixture, { theme: `orange` })
    await tick()
    expect(el.handle).not.toBeNull()
    el.remove()
    expect(el.childNodes.length).toBe(0)
    expect(el.handle).toBeNull()
    document.body.appendChild(el)
    expect(surfaceOf(el)!.dataset.xuiTheme).toBe(`orange`)
    expect(textOf(el, `text`)).toBe(`Ada`)
  })

  it(`clears author children on attach`, () => {
    const el = document.createElement(TAG)
    el.innerHTML = `<p>fallback</p>`
    fixture(el)
    document.body.appendChild(el)
    expect(el.querySelector(`p`)).toBeNull()
    expect(surfaceOf(el)).not.toBeNull()
  })

  it(`the handle methods: false / no-op before the first paint, through the handle after`, () => {
    const el = document.createElement(TAG)
    expect(el.focus(`save`)).toBe(false)
    expect(el.scrollIntoView(`save`)).toBe(false)
    expect(el.scrollToIndex(`list`, 3)).toBe(false)
    expect(() => el.announce(`hi`)).not.toThrow()
    fixture(el)
    document.body.appendChild(el)
    expect(el.focus(`save`)).toBe(true)
    expect(document.activeElement).toBe(el.querySelector(`[data-xui-id="save"]`))
    expect(el.focus(`missing`)).toBe(false)
    expect(el.scrollIntoView(`save`)).toBe(true)
    expect(el.scrollToIndex(`nope`, 1)).toBe(false)
    el.run({ focus: { id: `notify` } })
    expect(el.querySelector(`[data-xui-id="notify"]`)!.contains(document.activeElement)).toBe(true)
  })
})
