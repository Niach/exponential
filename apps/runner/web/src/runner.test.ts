// VAPP-269: the web runner's controller with fakes for the browser: the
// bundle as a map, memory stores, a fake photos provider. What it paints is
// runner.json's surfaces (the shell, the failures, the sheets, the toast).

import { describe, expect, test } from "bun:test"
import { MemoryBlobStore, MemoryRecordStore, MemoryVault, runnerFailure, runnerToast, type NativeCapability } from "@exp/eengine"
import runnerFixture from "@exp/eengine/fixtures/runner.json" with { type: "json" }

import album from "../samples/album.app.json" with { type: "json" }
import { deepLinkOf, Runner, type RunnerOptions } from "./runner"
import { holdLock, LOCK } from "./lock"
import { serviceWorkerSource } from "./sw"

const CONFIG = JSON.stringify({ id: `at.acme.album`, name: `Album`, version: `1.0.0` })

const setup = (bundle: Record<string, string | null> = {}, opts: Partial<RunnerOptions> = {}) => {
  const files: Record<string, string | null> = { "runner.json": CONFIG, "package.app.json": JSON.stringify(album), ...bundle }
  const picked: unknown[] = []
  const photos: NativeCapability = {
    name: `photos`,
    call: async (tool, args) => (picked.push([tool, args]), { structuredContent: { photos: [] } }),
  }
  const vault = new MemoryVault()
  const records = new MemoryRecordStore()
  const timers: (() => void)[] = []
  const runner = new Runner({
    readBundle: async (name) => files[name] ?? null,
    openStorage: async () => ({ records, vault, blobs: new MemoryBlobStore() }),
    capabilities: [photos],
    formFactor: `wide`,
    setTimeout: (fn) => timers.push(fn),
    clearTimeout: () => {},
    ...opts,
  })
  return { runner, picked, vault, records, timers }
}

const components = (runner: Runner, surface: string) => runner.host.surface(surface)?.components.map((c) => ({ ...c })) ?? null
const ids = (runner: Runner, surface: string) => (components(runner, surface) ?? []).map((c) => c.id)
const action = (runner: Runner, surfaceId: string, name: string, context: Record<string, unknown> = {}) => runner.host.action({ surfaceId, componentId: `x`, name, context })
const tick = () => new Promise((r) => setTimeout(r, 0))

describe(`the bundle`, () => {
  test(`a runner.json with issues paints the config failure`, async () => {
    for (const config of [null, `{`, JSON.stringify({ id: `at.acme.album`, name: `Album`, version: `1.0.0`, telemetry: `on` })]) {
      const { runner } = setup({ "runner.json": config })
      await runner.start()
      expect(runner.failure).toBe(`config`)
      expect(components(runner, `app`)).toEqual(runnerFailure(`config`) as never)
    }
  })

  test(`no package file = missing, a broken one = invalid, a bad signature = signature`, async () => {
    const missing = setup({ "package.app.json": null })
    await missing.runner.start()
    expect(missing.runner.failure).toBe(`missing`)
    const invalid = setup({ "package.app.json": JSON.stringify({ ...album, formatVersion: 4 }) })
    await invalid.runner.start()
    expect(invalid.runner.failure).toBe(`invalid`)
    const forged = setup({ "package.app.json": JSON.stringify({ package: album, author: { name: `Mallory`, key: `ed25519:${`A`.repeat(43)}` }, signature: { alg: `ed25519`, value: `A`.repeat(86) } }) })
    await forged.runner.start()
    expect(forged.runner.failure).toBe(`signature`)
    expect(components(forged.runner, `app`)).toEqual(runnerFailure(`signature`) as never)
  })

  test(`the device uuid of the local identity is created once under storage.deviceKey`, async () => {
    const { runner, vault } = setup()
    await runner.start()
    expect(await vault.read(runnerFixture.storage.deviceKey)).toMatch(/^[0-9a-f-]{36}$/)
    expect(runner.config).toEqual({ id: `at.acme.album`, name: `Album`, version: `1.0.0`, network: `off` })
  })
})

describe(`the shell`, () => {
  test(`a launch opens the first screen in the wide shell; crossing md re-installs compact`, async () => {
    const { runner } = setup()
    await runner.start()
    expect(ids(runner, `app`)).toContain(`runner-nav`)
    expect(ids(runner, `app`)).toContain(`runner-permissions`)
    expect(components(runner, `app`)!.find((c) => c.id === `runner-screen-0`)!.selected).toBe(true)
    runner.setFormFactor(`compact`)
    expect(ids(runner, `app`)).toContain(`runner-tabs`)
    expect(runner.host.surface(`app`)!.data).toMatchObject({ runner: { screen: `albums` }, params: {} })
  })

  test(`runner.navigate opens the screen; an unknown one toasts`, async () => {
    const { runner } = setup()
    await runner.start()
    action(runner, `app`, `runner.navigate`, { screen: `about` })
    await tick()
    expect(runner.instance!.state.screen).toBe(`about`)
    expect(components(runner, `app`)!.find((c) => c.id === `runner-screen-1`)!.selected).toBe(true)
    action(runner, `app`, `runner.navigate`, { screen: `nope` })
    await tick()
    expect(components(runner, `toast`)).toEqual(runnerToast(`navigate: nope is not a screen`) as never)
  })

  test(`a deep link cold-launches into an exposed screen, else toasts copy.refused`, async () => {
    const { runner } = setup()
    await runner.start(deepLinkOf(new URL(`https://app.example/?screen=nope`)))
    expect(components(runner, `toast`)).toEqual(runnerToast(`Couldn't open the link`) as never)
    expect(runner.instance!.state.screen).toBe(`albums`)
  })

  test(`the toast dismisses itself`, async () => {
    const { runner, timers } = setup()
    await runner.start()
    runner.toast(`Nope`)
    expect(components(runner, `toast`)).toEqual(runnerToast(`Nope`) as never)
    timers.at(-1)!()
    expect(runner.host.surface(`toast`)).toBeUndefined()
  })
})

describe(`calls`, () => {
  test(`records writes go to the instance and the bound source re-resolves`, async () => {
    const { runner, records } = setup()
    await runner.start()
    const outcome = await runner.host.callFunction({ surfaceId: `app`, componentId: `create`, name: `records.upsert`, args: { kind: `album`, data: { title: `Vienna`, photos: 0 } } })
    expect(outcome.error).toBeUndefined()
    const rows = await records.rows(`acme.album`)
    expect(rows.map((r) => r.data.title)).toEqual([`Vienna`])
    await tick()
    expect((runner.host.surface(`app`)!.data as { albums: { count: number } }).albums.count).toBe(1)
  })

  test(`a capability asks on the consent surface, the answer runs it`, async () => {
    const { runner, picked } = setup()
    await runner.start()
    const call = runner.host.callFunction({ surfaceId: `app`, componentId: `pick`, name: `photos.pick`, args: { limit: 5 } })
    await tick()
    expect(components(runner, `consent`)![0]!.title).toBe(`Allow Album to access your photos?`)
    action(runner, `consent`, `consent.answer`, { answer: `allow` })
    const outcome = await call
    expect(outcome.error).toBeUndefined()
    expect(picked).toEqual([[`pick`, { limit: 5 }]])
    expect(runner.host.surface(`consent`)).toBeUndefined()
    expect(runner.instance!.grants()).toEqual({ "photos.read": `granted` })
  })

  test(`a denied answer refuses the call with the toast`, async () => {
    const { runner, picked } = setup()
    await runner.start()
    const call = runner.host.callFunction({ surfaceId: `app`, componentId: `pick`, name: `photos.pick`, args: {} })
    await tick()
    action(runner, `consent`, `consent.answer`, { answer: `deny` })
    expect((await call).error).toBe(`consentDenied`)
    expect(picked).toEqual([])
    expect(components(runner, `toast`)).toEqual(runnerToast(`consentDenied`) as never)
  })

  test(`Permissions paints the manage sheet; a toggle writes the grant with the gesture`, async () => {
    const { runner } = setup()
    await runner.start()
    action(runner, `app`, `runner.permissions`)
    const sheet = () => components(runner, `permissions`)!
    expect(sheet()[1]!.checked).toBe(false)
    action(runner, `permissions`, `consent.toggle`, { scope: `photos.read` })
    expect(sheet()[1]!.checked).toBe(true)
    action(runner, `permissions`, `consent.toggle`, { scope: `photos.read` })
    expect(sheet()[1]!.checked).toBe(false)
    expect(runner.instance!.grants()).toEqual({})
  })
})

test(`deep links from the URL: ?screen= or the hash`, () => {
  expect(deepLinkOf(new URL(`https://a.example/`))).toBeNull()
  expect(deepLinkOf(new URL(`https://a.example/?screen=pages&album=a1`))).toEqual({ type: `deepLink`, screen: `pages`, params: { album: `a1` } })
  expect(deepLinkOf(new URL(`https://a.example/#/pages?album=a1`))).toEqual({ type: `deepLink`, screen: `pages`, params: { album: `a1` } })
})

test(`the service worker precaches what it is given under its cache`, () => {
  const src = serviceWorkerSource(`exp-runner-abc`, [`./`, `./app/runner.json`])
  expect(src).toContain(`const CACHE = "exp-runner-abc"`)
  expect(src).toContain(`["./","./app/runner.json"]`)
  expect(src).toContain(`ignoreVary: true`)
})

describe(`one tab per origin`, () => {
  test(`a second tab paints busy and opens the app when the lock frees`, async () => {
    let free!: () => void
    let opened = 0
    const { runner } = setup({}, { hold: (waiting) => (waiting(), new Promise<void>((r) => (free = r))), openStorage: async () => (opened++, { records: new MemoryRecordStore(), vault: new MemoryVault(), blobs: new MemoryBlobStore() }) })
    const started = runner.start()
    await tick()
    expect(runner.failure).toBe(`busy`)
    expect(components(runner, `app`)).toEqual(runnerFailure(`busy`) as never)
    expect(opened).toBe(0) // never a second store while the other tab holds it
    free()
    await started
    expect(runner.failure).toBeNull()
    expect(opened).toBe(1)
    expect(ids(runner, `app`)).toContain(`runner-nav`)
  })
})

describe(`holdLock`, () => {
  /** A LockManager of one origin: exclusive locks, granted in request order. */
  const fakeLocks = () => {
    const held = new Set<string>()
    const queue: { name: string; grant: () => void }[] = []
    const release = (name: string) => {
      held.delete(name)
      const next = queue.findIndex((q) => q.name === name)
      if (next >= 0) queue.splice(next, 1)[0]!.grant()
    }
    return {
      release,
      request(name: string, a: unknown, b?: unknown): Promise<unknown> {
        const options = (typeof a === `function` ? {} : a) as { ifAvailable?: boolean }
        const cb = (typeof a === `function` ? a : b) as (lock: { name: string } | null) => unknown
        if (held.has(name) && options.ifAvailable) return Promise.resolve(cb(null))
        return new Promise((resolve) => {
          const grant = () => (held.add(name), resolve(cb({ name })))
          if (held.has(name)) queue.push({ name, grant })
          else grant()
        })
      },
    }
  }

  test(`the first tab holds it; the second waits until the first lets go`, async () => {
    const locks = fakeLocks()
    const waits: string[] = []
    await holdLock(locks as unknown as LockManager, () => waits.push(`first`))
    let second = false
    void holdLock(locks as unknown as LockManager, () => waits.push(`second`)).then(() => (second = true))
    await tick()
    expect(waits).toEqual([`second`])
    expect(second).toBe(false)
    locks.release(LOCK) // the first tab closed
    await tick()
    expect(second).toBe(true)
  })

  test(`without Web Locks the page runs`, async () => {
    await holdLock(undefined, () => {
      throw new Error(`never waits`)
    })
  })
})
