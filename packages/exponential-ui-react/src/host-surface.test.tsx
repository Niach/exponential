// VAPP-91: <HostSurface> on an ExponentialHost — messages in through the
// in-memory transport, actions out as A2UI client messages, host functions
// through the policy gate, the URL policy, sources feeding the data model.

import { afterEach, describe, expect, it, vi } from "vitest"
import { act, fireEvent, render } from "@testing-library/react"
import { CORE_CATALOG_ID, ExponentialHost, MemoryTransport, defineExtension, reduceNested } from "@exponential-at/ui"
import type { ClientMessage, FlatComponent } from "@exponential-at/ui"
import { HostSurface, useHostStatus, useHostSurface, useHostSurfaceIds } from "./host-surface"
import { clearMediaCache, mediaCacheStats, MEDIA_CACHE_IDLE_MAX, useMediaSrc } from "./media"
import { ExponentialSurface, sameData } from "./surface"
import { renderHook } from "@testing-library/react"
import { defineReactExtension } from "./extensions"

const V = `v0.9`
const create = (surfaceId: string, catalogId = CORE_CATALOG_ID) => ({ version: V, createSurface: { surfaceId, catalogId } })
const components = (surfaceId: string, list: FlatComponent[]) => ({ version: V, updateComponents: { surfaceId, components: list } })

function Status({ host }: { host: ExponentialHost }) {
  const { status, unsupportedCatalog } = useHostStatus(host)
  const ids = useHostSurfaceIds(host)
  return (
    <p data-testid="status">
      {status} {ids.join(`,`)} {unsupportedCatalog ?? ``}
    </p>
  )
}

describe(`HostSurface`, () => {
  it(`paints what the transport sends and sends actions back`, async () => {
    const transport = new MemoryTransport()
    const host = new ExponentialHost({ transport })
    const view = render(
      <>
        <Status host={host} />
        <HostSurface host={host} surfaceId="s1" theme="neutral" mode="light" fallback={<p>waiting</p>} />
      </>
    )
    expect(view.getByText(`waiting`)).toBeTruthy()
    act(() => host.connect())
    act(() =>
      transport.feed(
        create(`s1`),
        components(`s1`, [
          { id: `root`, component: `Box`, children: [`t`, `b`] },
          { id: `t`, component: `Text`, text: { path: `/greeting` } },
          { id: `b`, component: `Button`, label: `Save`, on: { press: { event: { name: `save`, context: { who: { path: `/greeting` } } } } } },
        ]),
        { version: V, updateDataModel: { surfaceId: `s1`, path: `/greeting`, value: `Hello` } }
      )
    )
    expect(view.getByTestId(`status`).textContent).toContain(`open s1`)
    expect(view.getByText(`Hello`)).toBeTruthy()
    await act(async () => {
      fireEvent.click(view.getByRole(`button`, { name: `Save` }))
    })
    const sent = transport.sent as Extract<ClientMessage, { action: unknown }>[]
    expect(sent).toHaveLength(1)
    expect(sent[0]!.action).toMatchObject({ name: `save`, surfaceId: `s1`, sourceComponentId: `b`, context: { who: `Hello` } })
    act(() => transport.feed({ version: V, deleteSurface: { surfaceId: `s1` } }))
    expect(view.getByText(`waiting`)).toBeTruthy()
  })

  it(`runs host functions through the gate and urls through the policy`, async () => {
    const calls: unknown[] = []
    const opened: string[] = []
    const host = new ExponentialHost({
      functions: { "harness.toast": (args) => void calls.push(args) },
      policy: { functions: { deny: [`harness.mcp`] }, openUrl: (u) => opened.push(u), urls: { schemes: [`https`] } },
    })
    act(() => {
      host.receive(create(`s`))
      host.receive(
        components(`s`, [
          { id: `root`, component: `Box`, children: [`a`, `b`, `c`, `d`] },
          { id: `a`, component: `Button`, label: `Toast`, on: { press: { functionCall: { call: `harness.toast`, args: { message: `hi` } } } } },
          { id: `b`, component: `Button`, label: `Mcp`, on: { press: { functionCall: { call: `harness.mcp`, args: {} } } } },
          { id: `c`, component: `Button`, label: `Docs`, on: { press: { functionCall: { call: `openUrl`, args: { url: `https://ui.exponential.at` } } } } },
          { id: `d`, component: `Button`, label: `Bad`, on: { press: { functionCall: { call: `openUrl`, args: { url: `http://insecure.example` } } } } },
        ])
      )
    })
    const view = render(<HostSurface host={host} surfaceId="s" theme="neutral" />)
    for (const name of [`Toast`, `Mcp`, `Docs`, `Bad`])
      await act(async () => {
        fireEvent.click(view.getByRole(`button`, { name }))
      })
    expect(calls).toEqual([{ message: `hi` }])
    expect(opened).toEqual([`https://ui.exponential.at/`])
  })

  it(`a bound source and an extension painter`, () => {
    let emit: (v: unknown) => void = () => {}
    const catalog = defineExtension({
      id: `https://acme.example/catalog/v1`,
      name: `Acme`,
      extends: CORE_CATALOG_ID,
      components: { Gauge: { kind: `native`, group: `data`, lite: true, children: `none`, description: `A gauge.`, props: { value: { type: `number`, bindable: true, description: `The value.` } } } },
    } as never)
    const host = new ExponentialHost({ extensions: [catalog], sources: { acme: (_s, e) => void (emit = e) } })
    act(() => {
      host.receive(create(`g`, catalog.id))
      host.receive(components(`g`, [{ id: `root`, component: `Gauge`, value: { path: `/v` } }]))
      host.receive({ version: V, bindDataModel: { surfaceId: `g`, path: `/v`, source: `acme:gauge` } })
    })
    const ext = defineReactExtension({ catalog, components: { Gauge: ({ props, rootProps }) => <div {...rootProps}>gauge {String(props.value)}</div> } })
    const view = render(<HostSurface host={host} surfaceId="g" extensions={[ext]} />)
    act(() => emit(7))
    expect(view.getByText(`gauge 7`)).toBeTruthy()
    expect(host.supportedCatalogIds).toContain(catalog.id)
  })

  it(`an unsupported catalog raises the update banner state`, () => {
    const transport = new MemoryTransport()
    const host = new ExponentialHost({ transport })
    const view = render(<Status host={host} />)
    act(() => host.connect())
    act(() => transport.feed(create(`x`, `https://ui.exponential.at/catalogs/core/v2`)))
    expect(view.getByTestId(`status`).textContent).toContain(`https://ui.exponential.at/catalogs/core/v2`)
  })

  it(`template items wear their component's style rules`, () => {
    const host = new ExponentialHost()
    act(() => {
      host.receive(create(`t`))
      host.receive(
        components(`t`, [
          { id: `root`, component: `List`, children: { componentId: `row`, path: `/rows` } },
          { id: `row`, component: `Box`, style: { flexDirection: `column`, gap: 12 }, children: [`label`] },
          { id: `label`, component: `Text`, text: { path: `name` } },
        ])
      )
      host.receive({ version: V, updateDataModel: { surfaceId: `t`, path: `/rows`, value: [{ name: `a` }, { name: `b` }] } })
    })
    const view = render(<HostSurface host={host} surfaceId="t" />)
    const item = view.container.querySelector(`[data-xui-id="row.1"]`)!
    expect(item.className).toContain(`xui-n-row`)
    const sheet = view.container.querySelector(`style[data-xui-style="nodes"]`)!.textContent!
    expect(sheet).toContain(`.xui-n-row{`)
    expect(view.getByText(`b`)).toBeTruthy()
  })
})

describe(`review fixes`, () => {
  it(`the server's createSurface.theme wins over the theme prop`, () => {
    const host = new ExponentialHost()
    act(() => {
      host.receive({ version: V, createSurface: { surfaceId: `th`, catalogId: CORE_CATALOG_ID, theme: `playful` } })
      host.receive(components(`th`, [{ id: `root`, component: `Text`, text: `hi` }]))
      host.receive({ version: V, createSurface: { surfaceId: `plain`, catalogId: CORE_CATALOG_ID } })
      host.receive(components(`plain`, [{ id: `root`, component: `Text`, text: `ho` }]))
    })
    const view = render(
      <>
        <HostSurface host={host} surfaceId="th" theme="neutral" />
        <HostSurface host={host} surfaceId="plain" theme="neutral" />
      </>
    )
    const themes = [...view.container.querySelectorAll(`[data-xui-theme]`)].map((el) => el.getAttribute(`data-xui-theme`))
    expect(themes).toEqual([`playful`, `neutral`])
  })

  it(`a host surface's templates are the store's (no second reduce)`, () => {
    const host = new ExponentialHost()
    act(() => {
      host.receive(create(`t`))
      host.receive(
        components(`t`, [
          { id: `root`, component: `List`, children: { componentId: `row`, path: `/rows` } },
          { id: `row`, component: `Text`, text: { path: `name` } },
        ])
      )
    })
    const { result } = renderHook(() => useHostSurface(host, `t`))
    expect(result.current!.templates).toBe(host.surface(`t`)!.templates)
    expect(Object.keys(result.current!.templates!)).toEqual([`row`])
  })
})

describe(`media cache`, () => {
  afterEach(() => clearMediaCache())

  it(`is ref-counted and bounded: idle entries past the cap are evicted and revoked`, async () => {
    let n = 0
    const created: string[] = []
    const revoked: string[] = []
    const fetchSpy = vi.spyOn(globalThis, `fetch`).mockImplementation(async () => new Response(`x`, { status: 200 }))
    const createSpy = vi.spyOn(URL, `createObjectURL`).mockImplementation(() => {
      const u = `blob:${++n}`
      created.push(u)
      return u
    })
    const revokeSpy = vi.spyOn(URL, `revokeObjectURL`).mockImplementation((u) => void revoked.push(u))
    try {
      const host = { mediaRequest: (src: string) => ({ url: `https://x.test/${src}`, headers: { authorization: `Bearer t` } }) }
      const Img = ({ src }: { src: string }) => <i>{useMediaSrc(host, src) ?? `loading`}</i>
      // two consumers of one image share one fetch
      const a = render(
        <>
          <Img src="0" />
          <Img src="0" />
        </>
      )
      await act(async () => {
        await new Promise((r) => setTimeout(r, 0))
      })
      expect(a.container.textContent).toBe(`blob:1blob:1`)
      expect(fetchSpy).toHaveBeenCalledTimes(1)
      expect(mediaCacheStats()).toEqual({ entries: 1, inUse: 1 })
      a.unmount()
      expect(mediaCacheStats()).toEqual({ entries: 1, inUse: 0 })
      expect(revoked).toEqual([])
      // MEDIA_CACHE_IDLE_MAX + 5 more distinct images, each mounted then unmounted
      for (let i = 1; i <= MEDIA_CACHE_IDLE_MAX + 5; i++) {
        const v = render(<Img src={String(i)} />)
        await act(async () => {
          await new Promise((r) => setTimeout(r, 0))
        })
        v.unmount()
      }
      expect(mediaCacheStats()).toEqual({ entries: MEDIA_CACHE_IDLE_MAX, inUse: 0 })
      // the least recently released ones went, their urls revoked
      expect(revoked).toEqual(created.slice(0, 6))
      // an image still on screen is never evicted
      const kept = render(<Img src="kept" />)
      await act(async () => {
        await new Promise((r) => setTimeout(r, 0))
      })
      for (let i = 0; i < MEDIA_CACHE_IDLE_MAX + 2; i++) {
        const v = render(<Img src={`more-${i}`} />)
        await act(async () => {
          await new Promise((r) => setTimeout(r, 0))
        })
        v.unmount()
      }
      expect(revoked).not.toContain(kept.container.textContent)
      expect(mediaCacheStats().inUse).toBe(1)
      kept.unmount()
    } finally {
      fetchSpy.mockRestore()
      createSpy.mockRestore()
      revokeSpy.mockRestore()
    }
  })
})

describe(`the data prop`, () => {
  it(`sameData: identity first, then structure, no serialisation`, () => {
    const items = [{ id: 1 }]
    expect(sameData({ items, q: `a` }, { items, q: `a` })).toBe(true)
    expect(sameData({ items, q: `a` }, { items: [{ id: 1 }], q: `a` })).toBe(true)
    expect(sameData({ items, q: `a` }, { items, q: `b` })).toBe(false)
    expect(sameData({ a: 1 }, { a: 1, b: undefined })).toBe(false)
    expect(sameData([1, 2], [1, 2])).toBe(true)
    expect(sameData([1, 2], { 0: 1, 1: 2 })).toBe(false)
    expect(sameData(undefined, {})).toBe(false)
  })

  it(`an inline data object re-created per render keeps edits; new content replaces the model; no JSON.stringify`, () => {
    const root = reduceNested(
      { id: `root`, component: `Box`, children: [{ id: `in`, component: `Input`, props: { label: `Name`, name: `name`, value: { path: `/name` } } }, { id: `out`, component: `Text`, props: { text: { path: `/name` } } }] },
      { catalogId: CORE_CATALOG_ID }
    ).root!
    const stringify = vi.spyOn(JSON, `stringify`)
    try {
      const view = render(<ExponentialSurface root={root} data={{ name: `Ada` }} theme="neutral" id="dp" />)
      const input = () => view.container.querySelector(`input`)!
      const out = () => view.container.querySelector(`[data-xui-id="out"]`)!.textContent
      expect(out()).toBe(`Ada`)
      fireEvent.change(input(), { target: { value: `Ada L` } })
      expect(out()).toBe(`Ada L`)
      stringify.mockClear()
      view.rerender(<ExponentialSurface root={root} data={{ name: `Ada` }} theme="neutral" id="dp" />)
      expect(out()).toBe(`Ada L`)
      expect(stringify.mock.calls.filter(([v]) => (v as { name?: string } | null)?.name === `Ada`)).toEqual([])
      view.rerender(<ExponentialSurface root={root} data={{ name: `Grace` }} theme="neutral" id="dp" />)
      expect(out()).toBe(`Grace`)
    } finally {
      stringify.mockRestore()
    }
  })
})
