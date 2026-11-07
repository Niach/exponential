// VAPP-218 (eApps U1): `app_navigate` on a host: a press and an action's
// call step reach the host's shell; a refusal is INVALID_FUNCTION_CALL.
import { describe, expect, test } from "bun:test"
import routerFixture from "../../fixtures/host-router.json" with { type: "json" }
import { ExponentialHost } from "./runtime"
import { NAVIGATE_FUNCTION, crossAppTarget, resolveScreen, screenData } from "./screens"
import { PACKAGE_FORMAT_VERSION, type AppPackage } from "./package"
import { CORE_CATALOG_ID } from "../catalog"

const pkg = (routerFixture.packages as unknown as Record<string, AppPackage>)[`acme.screens`]!

const host = (navigate?: (t: { screen: string; params: Record<string, string> }) => boolean | void) => {
  const errors: string[] = []
  const h = new ExponentialHost({ packages: [pkg], navigate, onIssue: (i) => void errors.push(`${i.code}: ${i.message}`) })
  h.applyTemplate({ surfaceId: `s`, packageId: pkg.id, templateId: `pipeline` })
  return { h, errors }
}

describe(`app_navigate`, () => {
  test(`a press reaches the shell, without the package listing it`, async () => {
    const seen: unknown[] = []
    const { h } = host((t) => void seen.push(t))
    const out = await h.callFunction({ surfaceId: `s`, componentId: `go`, name: NAVIGATE_FUNCTION, args: { screen: `deals`, params: { stage: `won` } } })
    expect(out.decision).toBe(`allow`)
    expect(seen).toEqual([{ surfaceId: `s`, packageId: `acme.screens`, screen: `deals`, params: { stage: `won` } }])
  })
  test(`an action's call step navigates after its plan`, async () => {
    const seen: string[] = []
    const { h } = host((t) => void seen.push(`${t.screen}:${t.params.stage}`))
    const out = await h.callFunction({ surfaceId: `s`, componentId: `x`, name: `actions_won`, args: {} })
    expect(out.error).toBeUndefined()
    expect(seen).toEqual([`deals:won`])
  })
  test(`no screen or no shell = INVALID_FUNCTION_CALL`, async () => {
    const a = host(() => true)
    expect((await a.h.callFunction({ surfaceId: `s`, componentId: `go`, name: NAVIGATE_FUNCTION, args: { screen: `settings` } })).error).toBe(`navigate: settings is not a screen`)
    const b = host()
    expect((await b.h.callFunction({ surfaceId: `s`, componentId: `go`, name: NAVIGATE_FUNCTION, args: { screen: `deals` } })).decision).toBe(`deny`)
    expect(b.errors.some((e) => e.startsWith(`INVALID_FUNCTION_CALL`))).toBe(true)
  })
  // #EXP-1674: params are data: opening a screen runs nothing, whatever a link
  // carries; only a person's press does.
  test(`opening a screen never runs an action or function`, async () => {
    const called: string[] = []
    const opened = {
      formatVersion: PACKAGE_FORMAT_VERSION,
      id: `acme.open`,
      name: `Open`,
      version: `1.0.0`,
      catalogId: CORE_CATALOG_ID,
      functions: [`app.record`],
      screens: [{ template: `deal`, title: `Deal`, exposed: { description: `A deal`, params: { run: {} } } }],
      templates: {
        deal: {
          components: [
            { id: `root`, component: `Stack`, children: [`go`] },
            { id: `go`, component: `Button`, label: { "@path": `/params/run` }, on: { press: { functionCall: { "@call": `app.record`, args: { what: `wipe` } } } } },
          ],
        },
      },
    } as unknown as AppPackage
    const h = new ExponentialHost({ packages: [opened], functions: { "app.record": ({ what }) => void called.push(String(what)) } })
    const target = crossAppTarget(opened, `deal`, { run: `app.record` })
    if (`error` in target) throw new Error(target.error)
    const screen = resolveScreen(opened, target.screen, target.params)!
    h.applyTemplate({ surfaceId: `o`, packageId: opened.id, templateId: screen.screen, data: screenData(screen) })
    await Promise.resolve()
    expect(h.surface(`o`)!.data).toEqual({ params: { run: `app.record` } })
    expect(called).toEqual([])
  })
  test(`a screen's params land under /params`, () => {
    const h = new ExponentialHost({ packages: [pkg] })
    const screen = resolveScreen(pkg, `deals`, { stage: `won` })!
    h.applyTemplate({ surfaceId: `d`, packageId: pkg.id, templateId: screen.screen, data: screenData(screen) })
    expect(h.surface(`d`)!.data).toEqual({ params: { stage: `won` } })
  })
})

import hostContract from "../../catalog/host.json" with { type: "json" }
import { MAX_SCREENS, MAX_SCREEN_TITLE, SCREEN_PARAMS_KEY } from "./screens"
test(`the screens constants are catalog/host.json's`, () => {
  expect(hostContract.package.screens).toMatchObject({ max: MAX_SCREENS, titleMax: MAX_SCREEN_TITLE, navigateFunction: NAVIGATE_FUNCTION, paramsKey: SCREEN_PARAMS_KEY })
})

// #EXP-1674: a link per list row: `href` is a literal, so a press computes it.
test(`a row's press opens ~/ with its own data through the URL policy`, async () => {
  const opened: string[] = []
  const pkg = {
    formatVersion: PACKAGE_FORMAT_VERSION,
    id: `acme.links`,
    name: `Links`,
    version: `1.0.0`,
    catalogId: CORE_CATALOG_ID,
    templates: {
      t: {
        data: { row: { identifier: `EXP-1` } },
        components: [
          { id: `root`, component: `Stack`, children: [`go`] },
          { id: `go`, component: `Button`, label: `Open`, on: { press: { functionCall: { "@call": `openUrl`, args: { url: { "@call": `formatString`, args: { value: `~/issues/\${/row/identifier}` } } } } } } },
        ],
      },
    },
  } as unknown as AppPackage
  const h = new ExponentialHost({ packages: [pkg], policy: { urls: { baseUrl: `https://app.example`, linkBase: `/t/acme` }, openUrl: (url) => void opened.push(url) } })
  h.applyTemplate({ surfaceId: `l`, packageId: pkg.id, templateId: `t` })
  expect(h.openUrl(`~/issues/EXP-1`)).toBe(true)
  expect(opened).toEqual([`https://app.example/t/acme/issues/EXP-1`])
})
