// VAPP-91: <HostSurface> on an ExponentialHost — messages in through the
// in-memory transport, actions out as A2UI client messages, host functions
// through the policy gate, the URL policy, sources feeding the data model.

import { describe, expect, it } from "vitest"
import { act, fireEvent, render } from "@testing-library/react"
import { CORE_CATALOG_ID, ExponentialHost, MemoryTransport, defineExtension } from "@exponential-at/ui"
import type { ClientMessage, FlatComponent } from "@exponential-at/ui"
import { HostSurface, useHostStatus, useHostSurfaceIds } from "./host-surface"
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
