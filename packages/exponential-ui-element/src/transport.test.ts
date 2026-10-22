// FEED-130: the host API on the element: a `host` property (here over the
// in-memory transport) paints what arrives and sends actions back; `src` /
// `post` / `transport` build the right transport, connect on attach, close
// on detach and rebuild when they change.

import { afterEach, describe, expect, it, vi } from "vitest"
import { CORE_CATALOG_ID, ExponentialHost, JsonlStreamTransport, MemoryTransport, SseTransport, WebSocketTransport } from "@exponential-at/ui"
import type { ClientMessage, FlatComponent } from "@exponential-at/ui"
import { createHost, createTransport, transportKind } from "./transport"
import type { StatusDetail, SurfaceActionDetail } from "./index"
import { cleanup, mount, surfaceOf, tick } from "./test/helpers"

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

const V = `v0.9`
const create = (surfaceId = `main`) => ({ version: V, createSurface: { surfaceId, catalogId: CORE_CATALOG_ID } })
const components = (list: FlatComponent[], surfaceId = `main`) => ({ version: V, updateComponents: { surfaceId, components: list } })
const SCREEN = [
  create(),
  components([
    { id: `root`, component: `Box`, children: [`t`, `b`] },
    { id: `t`, component: `Text`, text: { path: `/greeting` } },
    { id: `b`, component: `Button`, label: `Save`, on: { press: { event: { name: `save`, context: { who: { path: `/greeting` } } } } } },
  ] as FlatComponent[]),
  { version: V, updateDataModel: { surfaceId: `main`, path: `/greeting`, value: `Hello` } },
]
const jsonl = SCREEN.map((m) => JSON.stringify(m)).join(`\n`) + `\n`

describe(`transport attributes`, () => {
  it(`a host built from src carries the element's extension catalogs`, () => {
    const catalog = { id: `x.metrics`, components: { TrendLine: { kind: `native`, props: {} } } }
    const defs = createHost({ src: `/ui` }, [catalog as never]).extensionDefs
    expect(defs.map((d) => d.id)).toEqual([`x.metrics`])
    expect(createHost({ src: `/ui` }).extensionDefs).toEqual([])
  })
  it(`pick the transport class`, () => {
    expect(transportKind({ src: `https://x.test/ui` })).toBe(`jsonl`)
    expect(transportKind({ src: `wss://x.test/ui` })).toBe(`ws`)
    expect(transportKind({ src: `/ui`, transport: `ws` })).toBe(`ws`)
    expect(transportKind({ src: `/ui`, transport: `sse` })).toBe(`sse`)
    expect(createTransport({ src: `/ui` })).toBeInstanceOf(JsonlStreamTransport)
    expect(createTransport({ src: `/ui`, transport: `sse`, post: `/act` })).toBeInstanceOf(SseTransport)
    expect(createTransport({ src: `ws://x.test/ui` })).toBeInstanceOf(WebSocketTransport)
  })
})

describe(`host property`, () => {
  it(`connects a host that never connected, paints its surface (nothing before), status + xui-status, actions go back through the host`, async () => {
    const transport = new MemoryTransport()
    const sentByHost: ClientMessage[] = []
    const host = new ExponentialHost({ transport, onSend: (m) => sentByHost.push(m) })
    const statuses: StatusDetail[] = []
    const actions: SurfaceActionDetail[] = []
    const el = mount((e) => {
      e.addEventListener(`xui-status`, (ev) => statuses.push(ev.detail))
      e.addEventListener(`xui-action`, (ev) => actions.push(ev.detail))
      e.host = host
      e.theme = `neutral`
    })
    expect(el.status).toBe(`open`)
    expect(statuses.map((s) => s.status)).toContain(`open`)
    expect(surfaceOf(el)).toBeNull()
    transport.feedJsonl(jsonl)
    expect(surfaceOf(el)).not.toBeNull()
    expect(el.querySelector(`[data-xui-id="t"]`)!.textContent).toBe(`Hello`)
    expect(el.surfaceState?.surfaceId).toBe(`main`)
    ;(el.querySelector(`[data-xui-id="b"]`) as HTMLButtonElement).click()
    await tick()
    const sent = transport.sent as Extract<ClientMessage, { action: unknown }>[]
    expect(sent).toHaveLength(1)
    expect(sent[0]!.action).toMatchObject({ name: `save`, surfaceId: `main`, sourceComponentId: `b`, context: { who: `Hello` } })
    expect(sentByHost).toHaveLength(1)
    expect(actions).toHaveLength(1)
    // apply() goes to the host too.
    el.apply({ updateDataModel: { surfaceId: `main`, path: `/greeting`, value: `Applied` } })
    expect(el.querySelector(`[data-xui-id="t"]`)!.textContent).toBe(`Applied`)
    transport.feed({ version: V, deleteSurface: { surfaceId: `main` } })
    expect(surfaceOf(el)).toBeNull()
  })

  it(`surface-id picks the host surface`, () => {
    const host = new ExponentialHost()
    host.receive(create(`a`))
    host.receive(components([{ id: `root`, component: `Text`, text: `A` }] as FlatComponent[], `a`))
    host.receive(create(`b`))
    host.receive(components([{ id: `root`, component: `Text`, text: `B` }] as FlatComponent[], `b`))
    const el = mount((e) => (e.host = host), { "surface-id": `b` })
    expect(el.querySelector(`[data-xui-id="root"]`)!.textContent).toBe(`B`)
    el.surfaceId = `a`
    expect(el.querySelector(`[data-xui-id="root"]`)!.textContent).toBe(`A`)
    expect(surfaceOf(el)!.classList.contains(`xui-s-a`)).toBe(true)
  })

  it(`a host property is the page's: detach never closes it`, async () => {
    const host = new ExponentialHost()
    host.receive(create())
    host.receive(components([{ id: `root`, component: `Text`, text: `kept` }] as FlatComponent[]))
    const close = vi.spyOn(host, `close`)
    const el = mount((e) => (e.host = host))
    el.remove()
    expect(close).not.toHaveBeenCalled()
    document.body.appendChild(el)
    await Promise.resolve()
    expect(el.querySelector(`[data-xui-id="root"]`)!.textContent).toBe(`kept`)
  })
})

/** A streamed HTTP body that stays open until aborted. */
function streamingFetch(text: string) {
  const calls: { url: string; init?: RequestInit }[] = []
  const fetch = vi.fn(async (url: string, init?: RequestInit) => {
    calls.push({ url, init })
    if (init?.method === `POST`) return { ok: true, status: 200 }
    const bytes = new TextEncoder().encode(text)
    let sent = false
    return {
      ok: true,
      status: 200,
      body: {
        getReader: () => ({
          read: () =>
            sent
              ? new Promise((resolve) => init?.signal?.addEventListener(`abort`, () => resolve({ done: true, value: undefined })))
              : ((sent = true), Promise.resolve({ done: false, value: bytes })),
        }),
      },
    }
  })
  return { fetch, calls }
}

describe(`src / post / transport`, () => {
  it(`jsonl: streams src, POSTs actions to post, closes on detach`, async () => {
    const { fetch, calls } = streamingFetch(jsonl)
    vi.stubGlobal(`fetch`, fetch)
    const statuses: string[] = []
    const el = mount((e) => e.addEventListener(`xui-status`, (ev) => statuses.push(ev.detail.status)), { src: `https://ui.test/stream`, post: `https://ui.test/act` })
    await tick()
    expect(calls[0]!.url).toBe(`https://ui.test/stream`)
    expect((calls[0]!.init!.headers as Record<string, string>).accept).toContain(`application/jsonl`)
    expect(statuses).toEqual([`connecting`, `open`])
    expect(el.status).toBe(`open`)
    expect(el.querySelector(`[data-xui-id="t"]`)!.textContent).toBe(`Hello`)
    ;(el.querySelector(`[data-xui-id="b"]`) as HTMLButtonElement).click()
    await tick()
    const post = calls.find((c) => c.init?.method === `POST`)!
    expect(post.url).toBe(`https://ui.test/act`)
    expect(JSON.parse(post.init!.body as string).action).toMatchObject({ name: `save` })
    const signal = calls[0]!.init!.signal as AbortSignal
    el.remove()
    expect(signal.aborted).toBe(true)
    expect(el.status).toBeUndefined()
  })

  it(`sse: the event-stream accept header; a src change closes the old host and builds anew`, async () => {
    const sse = SCREEN.map((m) => `data: ${JSON.stringify(m)}\n\n`).join(``)
    const { fetch, calls } = streamingFetch(sse)
    vi.stubGlobal(`fetch`, fetch)
    const el = mount(undefined, { src: `https://ui.test/a`, transport: `sse` })
    await tick()
    expect((calls[0]!.init!.headers as Record<string, string>).accept).toBe(`text/event-stream`)
    expect(el.querySelector(`[data-xui-id="t"]`)!.textContent).toBe(`Hello`)
    const first = calls[0]!.init!.signal as AbortSignal
    el.src = `https://ui.test/b`
    await tick()
    expect(first.aborted).toBe(true)
    expect(calls.filter((c) => c.init?.method !== `POST`).map((c) => c.url)).toEqual([`https://ui.test/a`, `https://ui.test/b`])
    expect(el.querySelector(`[data-xui-id="t"]`)!.textContent).toBe(`Hello`)
  })

  it(`ws: a WebSocket on the url, one message per frame, closed on detach`, () => {
    const sockets: FakeSocket[] = []
    class FakeSocket {
      onopen?: () => void
      onmessage?: (ev: { data: string }) => void
      onerror?: () => void
      onclose?: () => void
      readyState = 0
      sentFrames: string[] = []
      closed = false
      constructor(readonly url: string) {
        sockets.push(this)
      }
      send(data: string) {
        this.sentFrames.push(data)
      }
      close() {
        this.closed = true
      }
    }
    vi.stubGlobal(`WebSocket`, FakeSocket)
    const el = mount(undefined, { src: `wss://ui.test/ws` })
    expect(sockets).toHaveLength(1)
    expect(sockets[0]!.url).toBe(`wss://ui.test/ws`)
    expect(el.status).toBe(`connecting`)
    sockets[0]!.readyState = 1
    sockets[0]!.onopen?.()
    expect(el.status).toBe(`open`)
    for (const m of SCREEN) sockets[0]!.onmessage?.({ data: JSON.stringify(m) })
    expect(el.querySelector(`[data-xui-id="t"]`)!.textContent).toBe(`Hello`)
    ;(el.querySelector(`[data-xui-id="b"]`) as HTMLButtonElement).click()
    expect(JSON.parse(sockets[0]!.sentFrames[0]!).action).toMatchObject({ name: `save` })
    el.remove()
    expect(sockets[0]!.closed).toBe(true)
  })
})
