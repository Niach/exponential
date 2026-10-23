// FEED-130: every renderer callback is a DOM event too: the plugin callback
// and the event both fire, `waitUntil` joins the listener's promise into
// the painter's pending state, `respond` answers a host function, the open
// is cancelable, and `xui-ready` fires once per mount.

import { afterEach, describe, expect, it, vi } from "vitest"
import type { NestedNode } from "@exponential_app/ui"
import { defineElementExtension, type ExponentialSurfaceElement, type SurfaceActionDetail, type SurfaceInputDetail } from "./index"
import { joinPending } from "./events"
import { cardTree, cleanup, mount, tick, tree } from "./test/helpers"

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

const fixture = (node: NestedNode, data: Record<string, unknown> = {}) => (el: ExponentialSurfaceElement) => {
  const { root, templates } = tree(node)
  el.root = root
  el.templates = templates
  el.data = data
}

const deferred = () => {
  let resolve!: () => void
  const promise = new Promise<void>((r) => (resolve = r))
  return { promise, resolve }
}

const fire = (el: Element | null, type: string, init: Record<string, unknown> = {}) => {
  const Ctor = type.startsWith(`key`) ? KeyboardEvent : type === `click` ? MouseEvent : type === `input` ? InputEvent : Event
  el!.dispatchEvent(new (Ctor as typeof Event)(type, { bubbles: true, cancelable: true, ...init }))
}
const typeInto = (input: HTMLInputElement, value: string) => {
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, `value`)!.set!.call(input, value)
  fire(input, `input`)
}

describe(`events`, () => {
  it(`joinPending: undefined without promises, else one promise over all`, async () => {
    expect(joinPending([undefined, 1])).toBeUndefined()
    const d = deferred()
    let done = false
    void joinPending([undefined, d.promise])!.then(() => (done = true))
    await tick()
    expect(done).toBe(false)
    d.resolve()
    await tick()
    expect(done).toBe(true)
  })

  it(`xui-action: the plugin AND the event fire; waitUntil keeps the Button pending (aria-busy) until it settles`, async () => {
    const plugin = vi.fn()
    const d = deferred()
    const seen: SurfaceActionDetail[] = []
    const el = mount((e) => {
      fixture(cardTree, { name: `Ada` })(e)
      e.plugin = { onAction: plugin }
    })
    document.body.addEventListener(`xui-action`, (e) => {
      seen.push(e.detail)
      e.detail.waitUntil(d.promise)
    })
    const button = el.querySelector(`[data-xui-id="save"]`) as HTMLButtonElement
    button.click()
    expect(plugin).toHaveBeenCalledTimes(1)
    expect(plugin.mock.calls[0][0]).toMatchObject({ surfaceId: `main`, name: `save`, componentId: `save`, context: { name: `Ada` } })
    expect(seen).toHaveLength(1)
    expect(seen[0]).toMatchObject({ name: `save`, event: `press`, context: { name: `Ada` } })
    expect(button.getAttribute(`aria-busy`)).toBe(`true`)
    expect(button.disabled).toBe(true)
    d.resolve()
    await tick()
    expect(button.hasAttribute(`aria-busy`)).toBe(false)
    expect(button.disabled).toBe(false)
  })

  it(`xui-action: the plugin's promise is joined too`, async () => {
    const d = deferred()
    const el = mount((e) => {
      fixture(cardTree)(e)
      e.plugin = { onAction: () => d.promise }
    })
    const button = el.querySelector(`[data-xui-id="save"]`) as HTMLButtonElement
    button.click()
    expect(button.getAttribute(`aria-busy`)).toBe(`true`)
    d.resolve()
    await tick()
    expect(button.hasAttribute(`aria-busy`)).toBe(false)
  })

  it(`xui-input: plugin + event; waitUntil acknowledges the revision (a host echo waits for it)`, async () => {
    const plugin = vi.fn()
    const d = deferred()
    const seen: SurfaceInputDetail[] = []
    const el = mount((e) => {
      fixture({ id: `i`, component: `Input`, props: { label: `T`, name: `t`, value: { path: `/t` } } } as unknown as NestedNode, { t: `` })(e)
      e.plugin = { onInput: plugin }
    })
    el.addEventListener(`xui-input`, (e) => {
      seen.push(e.detail)
      e.detail.waitUntil(d.promise)
    })
    const input = el.querySelector(`input`) as HTMLInputElement
    input.focus()
    typeInto(input, `abc`)
    fire(input, `keydown`, { key: `Enter` })
    input.blur()
    fire(input, `focusout`)
    await tick()
    const commit = seen.find((s) => s.kind === `commit`)
    expect(commit).toMatchObject({ componentId: `i`, name: `t`, path: `/t`, value: `abc`, kind: `commit` })
    expect(plugin).toHaveBeenCalledWith(expect.objectContaining({ kind: `commit`, value: `abc` }))
    // Not acknowledged: the host's push is held.
    el.data = { t: `server` }
    await tick()
    expect(input.value).toBe(`abc`)
    d.resolve()
    await tick()
    el.data = { t: `server2` }
    await tick()
    expect(input.value).toBe(`server2`)
  })

  it(`xui-function-call: the first respond wins and keeps the source pending; without one the plugin's result`, async () => {
    const node = { id: `b`, component: `Button`, props: { label: `Look up` }, on: { press: { functionCall: { call: `lookup`, args: { q: 1 } } } } } as unknown as NestedNode
    const plugin = vi.fn(() => `from-plugin`)
    const d = deferred()
    const el = mount((e) => {
      fixture(node)(e)
      e.plugin = { onFunctionCall: plugin }
    })
    let respond = true
    el.addEventListener(`xui-function-call`, (e) => {
      expect(e.detail.call).toMatchObject({ surfaceId: `main`, componentId: `b`, name: `lookup`, args: { q: 1 } })
      if (respond) {
        e.detail.respond(d.promise)
        e.detail.respond(`ignored`)
      }
    })
    const button = el.querySelector(`button`) as HTMLButtonElement
    button.click()
    expect(plugin).toHaveBeenCalledTimes(1)
    expect(button.getAttribute(`aria-busy`)).toBe(`true`)
    d.resolve()
    await tick()
    expect(button.hasAttribute(`aria-busy`)).toBe(false)
    respond = false
    button.click()
    expect(plugin).toHaveBeenCalledTimes(2)
    expect(button.hasAttribute(`aria-busy`)).toBe(false)
  })

  it(`xui-open-url: cancelable; the default = window.open(url, _blank, noopener,noreferrer), else plugin.openUrl`, () => {
    const open = vi.fn()
    vi.stubGlobal(`open`, open)
    const node = { id: `l`, component: `Link`, props: { label: `Docs`, href: `https://ui.exponential.at/docs` } } as unknown as NestedNode
    const el = mount(fixture(node))
    const urls: string[] = []
    let cancel = false
    el.addEventListener(`xui-open-url`, (e) => {
      urls.push(e.detail.url)
      if (cancel) e.preventDefault()
    })
    const link = el.querySelector(`a`) as HTMLAnchorElement
    link.click()
    expect(urls).toEqual([`https://ui.exponential.at/docs`])
    expect(open).toHaveBeenCalledWith(`https://ui.exponential.at/docs`, `_blank`, `noopener,noreferrer`)
    cancel = true
    link.click()
    expect(urls).toHaveLength(2)
    expect(open).toHaveBeenCalledTimes(1)
    cancel = false
    const opener = vi.fn()
    el.plugin = { openUrl: opener }
    ;(el.querySelector(`a`) as HTMLAnchorElement).click()
    expect(opener).toHaveBeenCalledWith(`https://ui.exponential.at/docs`)
    expect(open).toHaveBeenCalledTimes(1)
  })

  it(`xui-open-url never fires for a URL the policy denies`, () => {
    const open = vi.fn()
    vi.stubGlobal(`open`, open)
    const el = mount(fixture({ id: `l`, component: `Link`, props: { label: `Bad`, href: `javascript:alert(1)` } } as unknown as NestedNode))
    const urls: string[] = []
    el.addEventListener(`xui-open-url`, (e) => urls.push(e.detail.url))
    ;(el.querySelector(`a`) as HTMLAnchorElement).click()
    expect(urls).toEqual([])
    expect(open).not.toHaveBeenCalled()
  })

  it(`xui-upload: the files + target; plugin + event`, async () => {
    const plugin = vi.fn()
    const el = mount((e) => {
      fixture({ id: `up`, component: `FileUpload`, props: { label: `Files`, name: `files`, multiple: true } } as unknown as NestedNode)(e)
      e.plugin = { onUpload: plugin }
    })
    const seen: { files: File[]; target: unknown }[] = []
    el.addEventListener(`xui-upload`, (e) => seen.push(e.detail))
    const input = el.querySelector(`input[type="file"]`) as HTMLInputElement
    const file = new File([`hello`], `a.txt`, { type: `text/plain` })
    Object.defineProperty(input, `files`, { configurable: true, value: [file] })
    fire(input, `change`)
    expect(seen).toHaveLength(1)
    expect(seen[0].files).toEqual([file])
    expect(seen[0].target).toEqual({ nodeId: `up`, name: `files` })
    expect(plugin).toHaveBeenCalledWith([file], { nodeId: `up`, name: `files` })
  })

  it(`xui-theme-issues for an unknown theme id (the default theme paints)`, () => {
    const plugin = vi.fn()
    const issues: unknown[] = []
    const el = mount(
      (e) => {
        e.addEventListener(`xui-theme-issues`, (ev) => issues.push(...ev.detail.issues))
        e.plugin = { onThemeIssues: plugin }
        fixture(cardTree)(e)
      },
      { theme: `no-such-theme` }
    )
    expect(issues.length).toBeGreaterThan(0)
    expect(plugin).toHaveBeenCalledTimes(1)
    expect(el.querySelector(`.xui-surface`)!.getAttribute(`data-xui-theme`)).not.toBe(`no-such-theme`)
  })

  it(`xui-unknown for an Unknown placeholder`, () => {
    const nodes: string[] = []
    mount((e) => {
      e.addEventListener(`xui-unknown`, (ev) => nodes.push(ev.detail.node.id))
      fixture({ id: `root`, component: `Stack`, children: [{ id: `g`, component: `Gauge` as never, props: {} }] })(e)
    })
    expect(nodes).toContain(`g`)
  })

  it(`xui-paint-error when a painter throws (the rest stays mounted)`, () => {
    const errors: unknown[] = []
    const spy = vi.spyOn(console, `error`).mockImplementation(() => {})
    const el = mount((e) => {
      e.addEventListener(`xui-paint-error`, (ev) => errors.push(ev.detail.error))
      e.extensions = [
        defineElementExtension({
          components: {
            Button: () => {
              throw new Error(`boom`)
            },
          },
        }),
      ]
      fixture({ id: `root`, component: `Stack`, children: [{ id: `bad`, component: `Button`, props: { label: `x` } }, { id: `ok`, component: `Text`, props: { text: `fine` } }] })(e)
    })
    spy.mockRestore()
    expect(errors).toEqual([expect.objectContaining({ surfaceId: `main`, componentId: `bad`, message: expect.stringContaining(`boom`) })])
    expect(el.querySelector(`[data-xui-id="ok"]`)!.textContent).toBe(`fine`)
  })

  it(`xui-ready: once per mount, after the handle arrived (listeners attached after appendChild see it)`, async () => {
    const el = mount(fixture(cardTree))
    const ready = vi.fn()
    el.addEventListener(`xui-ready`, (e) => ready(e.detail.handle))
    await Promise.resolve()
    expect(ready).toHaveBeenCalledTimes(1)
    expect(ready.mock.calls[0][0]).toBe(el.handle)
    el.data = { name: `x` }
    el.theme = `orange`
    await tick()
    expect(ready).toHaveBeenCalledTimes(1)
    el.remove()
    document.body.appendChild(el)
    await tick()
    expect(ready).toHaveBeenCalledTimes(2)
  })

  it(`events bubble and are composed`, () => {
    const el = mount(fixture(cardTree))
    let event: Event | undefined
    document.addEventListener(`xui-action`, (e) => (event = e), { once: true })
    ;(el.querySelector(`[data-xui-id="save"]`) as HTMLButtonElement).click()
    expect(event!.bubbles).toBe(true)
    expect(event!.composed).toBe(true)
    expect(event!.target).toBe(el)
  })
})
