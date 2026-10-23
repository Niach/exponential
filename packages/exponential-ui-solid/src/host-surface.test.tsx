// VAPP-107 on Solid (React's host-surface.test.tsx, the policy half):
// <HostSurface> on an ExponentialHost follows the SAME allowlists as the
// React one, because both route through `hostPlugin(host)` and the host
// core decides: host functions through the policy gate (deny wins, a
// package's `functions` list narrows), hrefs and openUrl through the URL
// policy (https only, private hosts denied), and a package surface's
// bindings only to the sources its package lists (`sources`; outside it the
// host answers SOURCE_DENIED and nothing is bound).

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, render } from "@solidjs/testing-library"
import { CORE_CATALOG_ID, ExponentialHost, MemoryTransport } from "@exponential-at/ui"
import type { ClientMessage, FlatComponent, VappPackage } from "@exponential-at/ui"
import { HostSurface, hostPlugin } from "./host-surface"

afterEach(() => cleanup())

const V = `v0.9`
const create = (surfaceId: string, catalogId = CORE_CATALOG_ID) => ({ version: V, createSurface: { surfaceId, catalogId } })
const components = (surfaceId: string, list: FlatComponent[]) => ({ version: V, updateComponents: { surfaceId, components: list } })
const tick = () => new Promise((r) => setTimeout(r, 0))

/** `host-router.json`'s acme.scoped: one template bound to exp:devices,
 *  one function, two listed sources (exp:devices and the acme.* prefix). */
const scoped: VappPackage = {
  id: `acme.scoped`,
  name: `Scoped`,
  version: `1.0.0`,
  catalogId: CORE_CATALOG_ID,
  templates: {
    list: {
      components: [
        { id: `root`, component: `Stack`, children: [`count`, `extra`, `leak`] },
        { id: `count`, component: `Text`, text: { path: `/devices/count` } },
        { id: `extra`, component: `Text`, text: { path: `/extra` } },
        { id: `leak`, component: `Text`, text: { path: `/issues` } },
      ],
      bindings: [{ path: `/devices`, source: `exp:devices` }],
    },
  },
  functions: [`harness.toast`],
  sources: [`exp:devices`, `acme.*`],
} as unknown as VappPackage

describe(`HostSurface follows the host's allowlists`, () => {
  it(`runs host functions through the gate and urls through the policy (https only)`, async () => {
    const calls: unknown[] = []
    const opened: string[] = []
    const sent: ClientMessage[] = []
    const host = new ExponentialHost({
      onSend: (m) => sent.push(m),
      functions: { "harness.toast": (args) => void calls.push(args), "harness.mcp": () => void calls.push(`mcp`) },
      policy: { functions: { deny: [`harness.mcp`] }, openUrl: (u) => opened.push(u), urls: { schemes: [`https`] } },
    })
    host.receive(create(`s`))
    host.receive(
      components(`s`, [
        { id: `root`, component: `Box`, children: [`a`, `b`, `c`, `d`, `e`, `l1`, `l2`] },
        { id: `a`, component: `Button`, label: `Toast`, on: { press: { functionCall: { call: `harness.toast`, args: { message: `hi` } } } } },
        { id: `b`, component: `Button`, label: `Mcp`, on: { press: { functionCall: { call: `harness.mcp`, args: {} } } } },
        { id: `c`, component: `Button`, label: `Docs`, on: { press: { functionCall: { call: `openUrl`, args: { url: `https://ui.exponential.at` } } } } },
        { id: `d`, component: `Button`, label: `Bad`, on: { press: { functionCall: { call: `openUrl`, args: { url: `http://insecure.example` } } } } },
        { id: `e`, component: `Button`, label: `Lan`, on: { press: { functionCall: { call: `openUrl`, args: { url: `https://192.168.1.1/admin` } } } } },
        { id: `l1`, component: `Link`, label: `Secure`, href: `https://exponential.at/x`, external: true },
        { id: `l2`, component: `Link`, label: `Plain`, href: `http://exponential.at/x`, external: true },
      ])
    )
    const view = render(() => <HostSurface host={host} surfaceId="s" theme="neutral" />)
    for (const name of [`Toast`, `Mcp`, `Docs`, `Bad`, `Lan`]) {
      fireEvent.click(view.getByRole(`button`, { name }))
      await tick()
    }
    expect(calls).toEqual([{ message: `hi` }])
    expect(opened).toEqual([`https://ui.exponential.at/`])
    expect(sent.map((m) => (`error` in m ? m.error.code : `action`))).toEqual([`FUNCTION_DENIED`])
    const links = [...view.container.querySelectorAll(`a`)].map((a) => [a.textContent, a.getAttribute(`href`), a.hasAttribute(`data-denied`)])
    expect(links).toEqual([
      [`Secure`, `https://exponential.at/x`, false],
      [`Plain`, null, true],
    ])
  })

  it(`a package surface follows only its listed sources: outside them SOURCE_DENIED, nothing bound`, async () => {
    const sent: ClientMessage[] = []
    const resolved: string[] = []
    const emitters: Record<string, (v: unknown) => void> = {}
    const host = new ExponentialHost({
      onSend: (m) => sent.push(m),
      packages: [scoped],
      functions: { "harness.toast": () => 1, "harness.mcp": () => 42 },
      sources: {
        exp: (source, emit) => {
          resolved.push(`exp:${source.name}`)
          emitters[source.name] = emit
          emit({ count: 2 })
        },
        "acme.data": (source, emit) => {
          resolved.push(`acme.data:${source.name}`)
          emit(`extra value`)
        },
      },
    })
    host.receive({ version: V, applyTemplate: { surfaceId: `p`, templateId: `list`, packageId: `acme.scoped` } })
    const view = render(() => <HostSurface host={host} surfaceId="p" theme="neutral" />)
    expect(view.getByText(`2`)).toBeTruthy()
    // exp:issues is a registered scheme but not in the package's sources.
    host.receive({ version: V, bindDataModel: { surfaceId: `p`, path: `/issues`, source: `exp:issues?board=b1` } })
    // acme.* is listed as a prefix: acme.data binds.
    host.receive({ version: V, bindDataModel: { surfaceId: `p`, path: `/extra`, source: `acme.data:list/items` } })
    await tick()
    expect(resolved).toEqual([`exp:devices`, `acme.data:list/items`])
    expect(sent).toEqual([{ version: V, error: { code: `SOURCE_DENIED`, surfaceId: `p`, message: `source exp:issues?board=b1 is not in package acme.scoped's sources`, path: `/issues` } }])
    expect(view.getByText(`extra value`)).toBeTruthy()
    expect(host.surface(`p`)!.data.issues).toBeUndefined()
    expect(view.container.querySelector(`[data-xui-id="leak"]`)!.textContent).toBe(``)
    // The package also narrows the function gate: harness.mcp is registered but not listed, harness.toast is.
    expect((await host.callFunction({ surfaceId: `p`, componentId: `b`, name: `harness.mcp`, args: {} })).decision).toBe(`deny`)
    expect((await host.callFunction({ surfaceId: `p`, componentId: `b`, name: `harness.toast`, args: {} })).decision).toBe(`allow`)
    expect(sent.map((m) => (`error` in m ? m.error.code : `action`))).toEqual([`SOURCE_DENIED`, `FUNCTION_DENIED`])
  })

  it(`hostPlugin hands the renderer the host's URL and media policy, private hosts included`, () => {
    const transport = new MemoryTransport()
    const host = new ExponentialHost({ transport, policy: { urls: { schemes: [`https`], allowPrivate: [`10.0.0.5`] }, media: { schemes: [`https`], allowPrivate: [`10.0.0.5`] } } })
    const plugin = hostPlugin(host)
    expect(plugin.urls).toEqual({ baseUrl: undefined, schemes: [`https`], allowPrivate: [`10.0.0.5`] })
    expect(plugin.media).toEqual({ schemes: [`https`], allowPrivate: [`10.0.0.5`] })
    host.receive(create(`m`))
    host.receive(
      components(`m`, [
        { id: `root`, component: `Box`, children: [`ok`, `lan`, `http`] },
        { id: `ok`, component: `Image`, src: `https://10.0.0.5/a.png`, alt: `ok` },
        { id: `lan`, component: `Image`, src: `https://10.0.0.6/a.png`, alt: `lan` },
        { id: `http`, component: `Image`, src: `http://10.0.0.5/a.png`, alt: `http` },
      ])
    )
    const view = render(() => <HostSurface host={host} surfaceId="m" theme="neutral" />)
    const srcs = [`ok`, `lan`, `http`].map((id) => view.container.querySelector(`[data-xui-id="${id}"] img`)?.getAttribute(`src`) ?? null)
    expect(srcs).toEqual([`https://10.0.0.5/a.png`, null, null])
  })
})
