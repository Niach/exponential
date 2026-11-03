// VAPP-218 (eApps U1): `app.navigate` on a host: a press and an action's
// call step reach the host's shell; a refusal is FUNCTION_DENIED.
import { describe, expect, test } from "bun:test"
import routerFixture from "../../fixtures/host-router.json" with { type: "json" }
import { ExponentialHost } from "./runtime"
import type { AppPackage } from "./package"
import { NAVIGATE_FUNCTION, resolveScreen, screenData } from "./screens"

const pkg = (routerFixture.packages as unknown as Record<string, AppPackage>)[`acme.screens`]!

const host = (navigate?: (t: { screen: string; params: Record<string, string> }) => boolean | void) => {
  const errors: string[] = []
  const h = new ExponentialHost({ packages: [pkg], navigate, onIssue: (i) => void errors.push(`${i.code}: ${i.message}`) })
  h.receive({ version: `v0.9`, applyTemplate: { surfaceId: `s`, packageId: pkg.id, templateId: `pipeline` } })
  return { h, errors }
}

describe(`app.navigate`, () => {
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
    const out = await h.callFunction({ surfaceId: `s`, componentId: `x`, name: `actions.won`, args: {} })
    expect(out.error).toBeUndefined()
    expect(seen).toEqual([`deals:won`])
  })
  test(`no screen or no shell = FUNCTION_DENIED`, async () => {
    const a = host(() => true)
    expect((await a.h.callFunction({ surfaceId: `s`, componentId: `go`, name: NAVIGATE_FUNCTION, args: { screen: `settings` } })).error).toBe(`navigate: settings is not a screen`)
    const b = host()
    expect((await b.h.callFunction({ surfaceId: `s`, componentId: `go`, name: NAVIGATE_FUNCTION, args: { screen: `deals` } })).decision).toBe(`deny`)
    expect(b.errors.some((e) => e.startsWith(`FUNCTION_DENIED`))).toBe(true)
  })
  test(`a screen's params land under /params`, () => {
    const h = new ExponentialHost({ packages: [pkg] })
    const screen = resolveScreen(pkg, `deals`, { stage: `won` })!
    h.receive({ version: `v0.9`, applyTemplate: { surfaceId: `d`, packageId: pkg.id, templateId: screen.screen, data: screenData(screen) } })
    expect(h.surface(`d`)!.data).toEqual({ params: { stage: `won` } })
  })
})

import hostContract from "../../catalog/host.json" with { type: "json" }
import { MAX_SCREENS, MAX_SCREEN_TITLE, SCREEN_PARAMS_KEY } from "./screens"
test(`the screens constants are catalog/host.json's`, () => {
  expect(hostContract.package.screens).toMatchObject({ max: MAX_SCREENS, titleMax: MAX_SCREEN_TITLE, navigateFunction: NAVIGATE_FUNCTION, paramsKey: SCREEN_PARAMS_KEY })
})
