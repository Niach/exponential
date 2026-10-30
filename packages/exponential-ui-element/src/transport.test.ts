// FEED-130: the host API on the element: a `host` property (here over the
// in-memory transport) paints what arrives and sends actions back; `src` /
// `post` / `transport` build the right transport, connect on attach, close
// on detach and rebuild when they change.

import { afterEach, describe, expect, it, vi } from "vitest"
import { CORE_CATALOG_ID, ExponentialHost, JsonlStreamTransport, MemoryTransport, SseTransport, WebSocketTransport } from "@exponential_app/eui"
import type { ClientMessage, FlatComponent } from "@exponential_app/eui"
import { createHost, createTransport, decideTransport, transportKind, transportSchemes } from "./transport"
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

describe(`VAPP-107: src and post pass the page's URL policy before a transport exists`, () => {
  it(`a private, loopback or link-local src is denied; the page's own private origin (jsdom: localhost) is not`, () => {
    for (const src of [`http://10.0.0.5/ui`, `http://192.168.1.10/ui`, `http://169.254.169.254/latest/ui`, `http://127.0.0.1:3000/ui`, `ws://10.0.0.5/ui`, `wss://printer.local/ui`, `http://[::1]/ui`, `http://2130706433/ui`]) {
      const d = decideTransport({ src })
      expect([src, d.allowed, d.reason]).toEqual([src, false, `src denied by the URL policy (private)`])
    }
    expect(new URL(document.baseURI).hostname).toBe(`localhost`)
    expect(decideTransport({ src: `/ui` }).allowed).toBe(true)
    expect(decideTransport({ src: `/ui` }).src.url).toBe(`${new URL(document.baseURI).origin}/ui`)
    expect(decideTransport({ src: `https://ui.test/stream`, post: `https://ui.test/act` }).allowed).toBe(true)
  })
  it(`allowPrivate opts a host in; a hosts allowlist alone does not; hosts hold for ws(s) too`, () => {
    expect(decideTransport({ src: `http://10.0.0.5/ui` }, { allowPrivate: [`10.0.0.5`] }).allowed).toBe(true)
    expect(decideTransport({ src: `wss://10.0.0.5/ui` }, { allowPrivate: [`10.0.0.5`] }).allowed).toBe(true)
    expect(decideTransport({ src: `http://10.0.0.6/ui` }, { allowPrivate: [`10.0.0.5`] }).reason).toBe(`src denied by the URL policy (private)`)
    expect(decideTransport({ src: `http://10.0.0.5/ui` }, { hosts: [`10.0.0.5`] }).reason).toBe(`src denied by the URL policy (private)`)
    expect(decideTransport({ src: `wss://evil.example/ui` }, { hosts: [`ui.test`] }).reason).toBe(`src denied by the URL policy (host)`)
    expect(decideTransport({ src: `wss://ui.test/ws` }, { hosts: [`ui.test`] }).allowed).toBe(true)
  })
  it(`schemes: ["https"] is https + wss only; javascript:, file: and data: never carry a transport`, () => {
    expect(transportSchemes(undefined)).toEqual([`https`, `http`, `wss`, `ws`])
    expect(transportSchemes({ schemes: [`https`] })).toEqual([`https`, `wss`])
    expect(transportSchemes({ schemes: [`https`, `mailto`, `tel`] })).toEqual([`https`, `wss`])
    const https = { schemes: [`https`] }
    expect(decideTransport({ src: `http://ui.test/stream` }, https).reason).toBe(`src denied by the URL policy (scheme)`)
    expect(decideTransport({ src: `ws://ui.test/stream` }, https).reason).toBe(`src denied by the URL policy (scheme)`)
    expect(decideTransport({ src: `wss://ui.test/stream` }, https).allowed).toBe(true)
    expect(decideTransport({ src: `https://ui.test/stream`, post: `http://ui.test/act` }, https).reason).toBe(`post denied by the URL policy (scheme)`)
    for (const src of [`javascript:alert(1)`, `file:///etc/passwd`, `data:text/plain,x`, `mailto:a@b.c`]) expect(decideTransport({ src }).reason).toBe(`src denied by the URL policy (scheme)`)
    expect(decideTransport({ src: `http://[bad` }).reason).toBe(`src denied by the URL policy (invalid)`)
  })
  it(`a denied post denies the whole transport; createHost throws before any transport exists`, () => {
    const d = decideTransport({ src: `https://ui.test/stream`, post: `http://10.0.0.5/act` })
    expect(d.allowed).toBe(false)
    expect(d.src.allowed).toBe(true)
    expect(d.post).toEqual({ allowed: false, url: `http://10.0.0.5/act`, reason: `private` })
    expect(d.reason).toBe(`post denied by the URL policy (private)`)
    expect(() => createHost({ src: `http://10.0.0.5/ui` })).toThrow(/src denied by the URL policy \(private\)/)
    expect(createHost({ src: `http://10.0.0.5/ui` }, [], { allowPrivate: [`*`] }).hasTransport).toBe(true)
  })
  it(`the element: a denied src connects nothing, reports closed with the reason, and recovers when the policy opts it in`, async () => {
    const { fetch, calls } = streamingFetch(jsonl)
    vi.stubGlobal(`fetch`, fetch)
    const statuses: StatusDetail[] = []
    const el = mount((e) => e.addEventListener(`xui-status`, (ev) => statuses.push(ev.detail)), { src: `http://10.0.0.5/stream` })
    await tick()
    expect(calls).toEqual([])
    expect(el.status).toBe(`closed`)
    expect(el.statusDetail).toBe(`src denied by the URL policy (private)`)
    expect(statuses).toEqual([{ status: `closed`, detail: `src denied by the URL policy (private)` }])
    expect(surfaceOf(el)).toBeNull()
    // The page opts the host in through its plugin's URL policy: the host is built and connects.
    el.plugin = { urls: { allowPrivate: [`10.0.0.5`] } }
    await tick()
    expect(calls.map((c) => c.url)).toEqual([`http://10.0.0.5/stream`])
    expect(el.status).toBe(`open`)
    expect(el.querySelector(`[data-xui-id="t"]`)!.textContent).toBe(`Hello`)
    // A post change to a denied url closes the built host: nothing is POSTed.
    const signal = calls[0]!.init!.signal as AbortSignal
    el.post = `http://192.168.1.1/act`
    await tick()
    expect(signal.aborted).toBe(true)
    expect(el.status).toBe(`closed`)
    expect(el.statusDetail).toBe(`post denied by the URL policy (private)`)
    expect(surfaceOf(el)).toBeNull()
    el.remove()
    expect(el.status).toBeUndefined()
  })
  it(`the element: schemes: ["https"] in the plugin refuses an http src; a javascript: src never runs`, async () => {
    const { fetch, calls } = streamingFetch(jsonl)
    vi.stubGlobal(`fetch`, fetch)
    const el = mount((e) => (e.plugin = { urls: { schemes: [`https`] } }), { src: `http://ui.test/stream` })
    await tick()
    expect(calls).toEqual([])
    expect(el.statusDetail).toBe(`src denied by the URL policy (scheme)`)
    el.src = `javascript:alert(1)`
    await tick()
    expect(calls).toEqual([])
    expect(el.statusDetail).toBe(`src denied by the URL policy (scheme)`)
    el.src = `https://ui.test/stream`
    await tick()
    expect(calls.map((c) => c.url)).toEqual([`https://ui.test/stream`])
    expect(el.status).toBe(`open`)
  })
})

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
