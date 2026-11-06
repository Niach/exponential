// VAPP-269: the AppInstance twin over local providers, case for case with
// the crate's `tests/instance.rs`, plus package verification against
// genUI's signature fixture.

import { describe, expect, test } from "bun:test"
import { verifyPackageFile } from "@exponential_app/eui/host"
import hostPackage from "@exponential_app/eui/fixtures/host-package.json" with { type: "json" }

import type { BrokerAuditRow } from "./capabilities"
import { AppInstance, createsOf, LoadError, screenKey, type EngineHostConfig, type SyncTransport } from "./instance"
import { MemoryBlobStore, MemoryRecordStore, MemoryVault, localDeviceId, type EngineRecordStore } from "./stores"
import type { NativeCapability } from "./tools"

test(`signatures verify like the TS reference`, async () => {
  for (const c of (hostPackage as unknown as { signatures: { name: string; signed: unknown; expected: unknown }[] }).signatures) {
    expect(await verifyPackageFile(c.signed), c.name).toEqual(c.expected as never)
  }
})

const pkg = () => ({
  formatVersion: 5,
  id: `acme.album`,
  name: `Album`,
  version: `1.0.0`,
  catalogId: `https://ui.exponential.at/catalogs/core/v1`,
  functions: [`records.*`, `app.navigate`, `photos.pick`],
  sources: [`records:*`],
  records: { note: { title: `Note`, schema: { type: `object`, properties: { text: { type: `string` } } }, policy: { update: `author`, delete: `owners` } } },
  capabilities: [{ id: `photos.read`, why: `Pick the photos for an album.` }],
  screens: [
    { template: `albums`, title: `Albums` },
    { template: `page`, title: `Page`, nav: false },
  ],
  templates: {
    albums: { components: [{ id: `root`, component: `Text`, text: `Albums` }] },
    page: { components: [{ id: `root`, component: `Text`, text: `Page` }] },
  },
})

const photos = (): NativeCapability & { calls: unknown[] } => {
  const calls: unknown[] = []
  return {
    name: `photos`,
    calls,
    // Native results per capabilities.json's `nativeOutputSchema`.
    async call(tool, args) {
      calls.push([tool, args])
      return { structuredContent: { photos: [] } }
    },
  }
}

const setup = (opts: { store?: EngineRecordStore; vault?: MemoryVault; sync?: SyncTransport } = {}) => {
  const opened: [string, Record<string, string>][] = []
  const audit: BrokerAuditRow[] = []
  const store = opts.store ?? new MemoryRecordStore()
  const vault = opts.vault ?? new MemoryVault()
  const native = photos()
  const config: EngineHostConfig = {
    host: { kind: `exported`, platform: `web`, formFactor: `desktop`, locale: `en` },
    storage: store,
    blobs: new MemoryBlobStore(),
    identity: { kind: `local`, deviceId: `d-1` },
    vault,
    window: { open: (_app, screen, params) => void opened.push([screen, params]) },
    clock: () => `2026-11-06T10:00:00.000Z`,
    network: `off`,
    sync: opts.sync ?? null,
    capabilities: [native],
    audit: { record: (row) => void audit.push(row) },
  }
  return { config, opened, audit, store, vault, native }
}

describe(`AppInstance`, () => {
  test(`a local instance runs its lifecycle and writes through`, async () => {
    const { config, opened, store, vault } = setup()
    const app = await AppInstance.load(config, pkg())
    const effects = await app.event({ type: `launch` })
    expect(effects).toContainEqual({ effect: `flush` })
    expect(opened.at(-1)![0]).toBe(`albums`)

    expect(await app.call(`records.upsert`, { kind: `note`, data: { text: `hi` } }, [`n1`])).toEqual({ value: { id: `n1` } })
    const rows = await store.rows(`acme.album`)
    expect(rows.length).toBe(1)
    expect(rows[0]!.authorId).toBe(`d-1`) // local identity = me
    expect((await app.recordsValue(`records:note`))!.count).toBe(1)
    // Local identity collapses `owners` to me.
    expect(await app.call(`records.delete`, { id: `n1` })).toEqual({ value: null })
    expect(await store.rows(`acme.album`)).toEqual([])

    // app.navigate opens a declared screen; terminate persists it; a relaunch restores it.
    expect(await app.call(`app.navigate`, { screen: `page` })).toEqual({ value: null })
    await app.event({ type: `terminate` })
    expect(await vault.read(screenKey(`acme.album`))).toContain(`page`)
    const again = await AppInstance.load(config, pkg())
    await again.event({ type: `launch` })
    expect(opened.at(-1)![0]).toBe(`page`)
  })

  test(`capability calls go through the broker`, async () => {
    const { config, vault, native, audit } = setup()
    const app = await AppInstance.load(config, pkg())
    await app.event({ type: `launch` })
    const asked = await app.call(`photos.pick`, {})
    if (!(`ask` in asked)) throw new Error(JSON.stringify(asked))
    expect(asked.sheet.components[0]!.title).toBe(`Allow Album to access your photos?`)
    const answered = await app.answerConsent(`photos.pick`, {}, asked.ask, `allow`)
    expect(answered).toEqual({ value: { photos: [] } }) // VAPP-271: the structuredContent
    expect(native.calls).toEqual([[`pick`, { limit: 1 }]]) // the bridge fills the default
    await app.broker.persisted
    expect(await vault.read(`eengine/grants/acme.album`)).toContain(`"photos.read":"granted"`)
    expect(`value` in (await app.call(`photos.pick`, {}))).toBe(true) // granted: no second sheet
    const bad = await app.call(`photos.pick`, { limit: 0 })
    expect(bad).toEqual({ refused: { code: `failed`, message: `invalidParams: /limit minimum` } }) // an isError result is a refusal
    // Only the ConsentUi after a gesture changes a grant.
    expect(app.setGrant(`agent`, true, `photos.read`, null)).toBe(`onlyConsentUi`)
    expect(app.setGrant(`consentUi`, false, `photos.read`, null)).toBe(`noGesture`)
    expect(app.setGrant(`consentUi`, true, `photos.read`, null)).toBeNull()
    expect(`ask` in (await app.call(`photos.pick`, {}))).toBe(true) // revoked: asks again
    expect(app.setGrant(`consentUi`, true, `photos.read`, `denied`)).toBeNull()
    expect(app.permissionsSheet()[1]!.checked).toBe(false)
    expect(await app.call(`camera.capture`, {})).toEqual({ refused: { code: `FORBIDDEN`, message: `notDeclared` } })
    expect(audit.map((r) => r.status)).toContain(`denied`)
  })

  test(`grants survive a reload by the same signer`, async () => {
    const { config } = setup()
    const app = await AppInstance.load(config, pkg())
    await app.event({ type: `launch` })
    expect(app.setGrant(`consentUi`, true, `photos.read`, `granted`)).toBeNull()
    await app.broker.persisted
    const again = await AppInstance.load(config, pkg())
    expect(again.grants()).toEqual({ "photos.read": `granted` })
  })

  test(`a synced instance queues and flushes`, async () => {
    const pushed: string[] = []
    const { config } = setup({ sync: { push: async (_app, call) => (pushed.push(call), { ok: true }) } })
    const app = await AppInstance.load(config, pkg())
    await app.event({ type: `launch` })
    await app.call(`records.upsert`, { kind: `note`, data: { text: `hi` } }, [`n1`])
    expect(pushed).toEqual([`upsert`])
    // Acked, not yet reflected by the store: it keeps overlaying.
    expect((await app.recordsValue(`records:note`))!.count).toBe(1)
  })

  test(`an invalid package does not load`, async () => {
    const { config } = setup()
    const error = await AppInstance.load(config, { ...pkg(), formatVersion: 4 }).catch((e) => e)
    expect(error).toBeInstanceOf(LoadError)
    expect((error as LoadError).kind).toBe(`invalid`)
    expect((error as LoadError).issues[0]!.path).toBe(`/formatVersion`)
  })

  test(`a file whose signature does not verify does not load`, async () => {
    const { config } = setup()
    const bad = (hostPackage as unknown as { signatures: { signed: unknown; expected: { status: string } }[] }).signatures.find((c) => c.expected.status === `invalid`)!
    const error = await AppInstance.load(config, bad.signed).catch((e) => e)
    expect((error as LoadError).kind).toBe(`signature`)
  })

  test(`a refused write and an unknown screen answer refusals`, async () => {
    const { config } = setup()
    const app = await AppInstance.load(config, pkg())
    await app.event({ type: `launch` })
    expect(await app.call(`records.delete`, { id: `nope` })).toEqual({ refused: { code: `NOT_FOUND`, message: `no record nope` } })
    expect(await app.call(`app.navigate`, { screen: `nope` })).toEqual({ refused: { code: `BAD_REQUEST`, message: `navigate: nope is not a screen` } })
    expect(await app.recordsValue(`weather:now`)).toBeUndefined()
  })

  test(`the store's changes reach a subscriber`, async () => {
    const { config } = setup()
    const app = await AppInstance.load(config, pkg())
    let changes = 0
    const stop = app.subscribe(() => changes++)
    await app.call(`records.upsert`, { kind: `note`, data: {} }, [`n1`])
    stop()
    await app.call(`records.upsert`, { kind: `note`, data: {} }, [`n2`])
    expect(changes).toBe(1)
  })
})

test(`createsOf counts the client ids a call takes`, () => {
  expect(createsOf(`records.upsert`, { kind: `note`, data: {} })).toBe(1)
  expect(createsOf(`records.upsert`, { id: `n1`, data: {} })).toBe(0)
  expect(createsOf(`records.batch`, { writes: [{ op: `upsert`, kind: `a` }, { op: `delete`, id: `x` }, { op: `upsert`, id: `y` }] })).toBe(1)
})

describe(`the local stores`, () => {
  test(`writeIds dedupe and history is kept`, async () => {
    const store = new MemoryRecordStore()
    const batch = { writeId: `w1`, writes: [{ op: `upsert` as const, id: `r1`, kind: `note`, data: {}, create: true }], at: `2026-11-06T10:00:00.000Z` }
    const first = await store.write(`a`, batch, `d-1`)
    const again = await store.write(`a`, batch, `d-1`)
    expect([first.replayed, again.replayed]).toEqual([false, true])
    expect(again.ids).toEqual([`r1`])
    await store.write(`a`, { writeId: `w2`, writes: [{ op: `upsert`, id: `r1`, kind: `note`, data: { text: `x` }, create: false }], at: `2026-11-06T10:01:00.000Z` }, `d-1`)
    expect((await store.history(`a`, `r1`)).map((e) => e.op)).toEqual([`create`, `update`])
    expect((await store.rows(`a`))[0]!.data.text).toBe(`x`)
    const missing = await store.write(`a`, { writeId: `w3`, writes: [{ op: `delete`, id: `nope` }], at: `2026-11-06T10:02:00.000Z` }, null).catch((e) => e)
    expect(missing.code).toBe(`NOT_FOUND`)
    // All or nothing: the failed batch left the row.
    expect((await store.rows(`a`)).length).toBe(1)
  })

  test(`the device uuid is created once`, async () => {
    const vault = new MemoryVault()
    const id = await localDeviceId(vault, `eengine/runner/device`)
    expect(id).toMatch(/^[0-9a-f-]{36}$/)
    expect(await localDeviceId(vault, `eengine/runner/device`)).toBe(id)
  })
})
