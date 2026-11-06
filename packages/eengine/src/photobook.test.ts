// VAPP-271: the Photobook (samples/apps/photobook.app.json) end to end on
// the Engine, the way a runner drives it: genUI's host runs the package's
// actions, every host function goes to the AppInstance (records and the
// capabilities through the broker), the natives are fakes and nothing
// reaches the network. An album from picked photos, its PDF (the package's
// own action builds the blocks from the records), then Save and Share.

import { afterAll, beforeAll, describe, expect, test } from "bun:test"
import { ExponentialHost, type AppPackage } from "@exponential_app/eui/host"

import file from "../../../samples/apps/photobook.app.json" with { type: "json" }
import capabilities from "../fixtures/capabilities.json" with { type: "json" }
import { AppInstance, createsOf, packageOf, type CallResult, type EngineHostConfig } from "./instance"
import { MemoryBlobStore, MemoryRecordStore, MemoryVault } from "./stores"
import { base64Encode, type NativeCapability } from "./tools"

const pdfCase = (capabilities as unknown as { pdfCases: { blobs: Record<string, string> }[] }).pdfCases.find((c) => c.blobs?.jpg1 && c.blobs?.png1)!
const JPG = pdfCase.blobs.jpg1!
const PNG = pdfCase.blobs.png1!

/** A fake native: records every call, answers per tool. */
const native = (name: string, answer: (tool: string, args: Record<string, unknown>) => unknown) => {
  const calls: [string, Record<string, unknown>][] = []
  const cap: NativeCapability = {
    name,
    async call(tool, args) {
      calls.push([tool, args as Record<string, unknown>])
      return answer(tool, args as Record<string, unknown>) as never
    },
  }
  return { cap, calls }
}

const bytesOf = (b64: string) => Uint8Array.from(atob(b64), (c) => c.charCodeAt(0))
const text = (bytes: Uint8Array) => new TextDecoder(`latin1`).decode(bytes)

let realFetch: typeof fetch
const fetched: string[] = []
beforeAll(() => {
  realFetch = globalThis.fetch
  globalThis.fetch = (async (url: unknown) => {
    fetched.push(String(url))
    throw new Error(`no network`)
  }) as unknown as typeof fetch
})
afterAll(() => {
  globalThis.fetch = realFetch
})

async function photobook() {
  const blobs = new MemoryBlobStore()
  const store = new MemoryRecordStore()
  const picks = [
    [
      { id: `lib-1`, name: `lake.jpg`, mimeType: `image/jpeg`, takenAt: null, data: JPG },
      { id: `lib-2`, name: `sky.png`, mimeType: `image/png`, takenAt: null, data: PNG },
    ],
    [{ id: `lib-3`, name: `dock.png`, mimeType: `image/png`, takenAt: null, data: PNG }],
  ]
  const photos = native(`photos`, () => ({ structuredContent: { photos: picks.shift() ?? [] } }))
  const saved: { name: string; mimeType: string; data: string }[] = []
  const files = native(`files`, (tool, args) => (tool === `save` ? (saved.push(args as never), { structuredContent: { saved: true } }) : { content: [{ type: `text`, text: `unavailable` }], isError: true }))
  const shared: Record<string, unknown>[] = []
  const share = native(`share`, (_tool, args) => (shared.push(args), { structuredContent: { shared: true } }))
  const print = native(`print`, () => ({ structuredContent: { printed: true } }))
  // Declared nowhere: a call would mean the package reached the network.
  const http = native(`http`, () => ({ content: [{ type: `text`, text: `unavailable` }], isError: true }))
  const opened: [string, Record<string, string>][] = []
  let clock = 0
  const config: EngineHostConfig = {
    host: { kind: `exported`, platform: `web`, formFactor: `desktop`, locale: `en` },
    storage: store,
    blobs,
    identity: { kind: `local`, deviceId: `d-1` },
    vault: new MemoryVault(),
    window: { open: (_app, screen, params) => void opened.push([screen, params]) },
    // Each write a later instant, so pages sort by createdAt.
    clock: () => new Date(Date.UTC(2026, 10, 6, 10, 0, clock++)).toISOString(),
    network: `off`,
    sync: null,
    capabilities: [photos.cap, files.cap, share.cap, print.cap, http.cap],
  }
  const app = await AppInstance.load(config, file)
  await app.event({ type: `launch` })
  let ids = 0
  // The runner's settle: a value, a refusal (the call rejects), or the consent sheet answered `allow`.
  const settle = async (name: string, args: unknown, r: CallResult): Promise<unknown> => {
    if (`value` in r) return r.value
    if (`refused` in r) throw new Error(r.refused.message)
    return settle(name, args, await app.answerConsent(name, args, r.ask, `allow`))
  }
  const call = (name: string) => async (args: Record<string, unknown>) =>
    settle(name, args, await app.call(name, args, Array.from({ length: createsOf(name, args) }, () => `r${++ids}`)))
  const names = [`records.upsert`, `records.delete`, `records.batch`, `photos.pick`, `print.pdf`, `files.save`, `share.send`]
  const failed: string[] = []
  const pkg = packageOf(file) as unknown as AppPackage
  const host = new ExponentialHost({
    packages: [pkg],
    functions: Object.fromEntries(names.map((n) => [n, call(n)])),
    sources: {
      records: (source, emit) => {
        const resolve = () => void app.recordsValue(source.uri).then((v) => emit(v ?? null))
        resolve()
        return app.subscribe(resolve)
      },
    },
    navigate: ({ screen, params }) => void app.call(`app.navigate`, { screen, params }),
    onActionFailed: ({ message }) => void failed.push(message),
  })
  const open = (templateId: string, params: Record<string, string> = {}) => host.receive({ version: `v0.9`, applyTemplate: { surfaceId: `app`, packageId: pkg.id, templateId, data: { params } } })
  const settled = () => new Promise((r) => setTimeout(r, 0))
  const press = async (componentId: string, name: string, args: Record<string, unknown>) => {
    const out = await host.callFunction({ surfaceId: `app`, componentId, name, args })
    await settled()
    return out
  }
  const data = () => host.surface(`app`)!.data as Record<string, any>
  return { app, host, store, blobs, open, press, data, settled, photos, files, share, print, http, saved, shared, failed, opened }
}

describe(`the Photobook on the Engine (VAPP-271)`, () => {
  test(`the package loads: core catalog, local, four capabilities, no network`, async () => {
    const pkg = packageOf(file) as Record<string, unknown>
    expect(pkg.catalogId).toBe(`https://ui.exponential.at/catalogs/core/v1`)
    expect((pkg.capabilities as { id: string }[]).map((c) => c.id).sort()).toEqual([`files.write`, `photos.read`, `print.pdf`, `share.send`])
    for (const key of [`tools`, `upstreams`, `exposes`, `agent`, `automations`]) expect(pkg[key]).toBeUndefined()
    const { app } = await photobook()
    expect(app.appId).toBe(`at.exponential.photobook`)
  })

  test(`an album from picked photos, its PDF, then Save and Share`, async () => {
    const t = await photobook()
    // Albums: New album.
    t.open(`albums`)
    await t.settled()
    expect((await t.press(`create`, `actions.newAlbum`, { title: `Summer 2026` })).error).toBeUndefined()
    const [album] = await t.store.rows(t.app.appId)
    expect(album!.data).toEqual({ title: `Summer 2026` })

    // The album: Add photos = a page per pick, then photos onto that page.
    t.open(`album`, { album: album!.id })
    await t.settled()
    expect((await t.press(`pick`, `actions.addPage`, { album: album!.id, caption: `At the lake`, cover: `` })).error).toBeUndefined()
    let pages = t.data().pages.rows as { id: string; photos: string[]; caption?: string }[]
    expect(pages.length).toBe(1)
    expect(pages[0]!.photos.length).toBe(2)
    expect(pages[0]!.caption).toBe(`At the lake`)
    // Each picked photo is an app file; the first is the album's cover.
    for (const id of pages[0]!.photos) expect(await t.blobs.get(t.app.appId, id)).not.toBeNull()
    expect(t.data().albums.rows[0].cover).toBe(pages[0]!.photos[0])
    expect((await t.press(`more`, `actions.addPhotos`, { page: pages[0]!.id, photos: pages[0]!.photos })).error).toBeUndefined()
    pages = t.data().pages.rows
    expect(pages[0]!.photos.length).toBe(3)
    // A cancelled pick adds nothing (the failed-action toast).
    expect((await t.press(`pick`, `actions.addPage`, { album: album!.id, caption: ``, cover: pages[0]!.photos[0] })).error).toBe(`No photos picked`)
    expect(t.failed).toEqual([`No photos picked`])

    // Export PDF: print.pdf with heading, text and image blocks from the records.
    expect((await t.press(`export`, `actions.exportPdf`, { title: `Summer 2026`, cover: pages[0]!.photos[0] })).error).toBeUndefined()
    const pdf = t.data().pdf as { name: string; mimeType: string; size: number; fileId: string }
    expect(pdf.name).toBe(`Summer 2026.pdf`)
    expect(pdf.mimeType).toBe(`application/pdf`)
    const bytes = (await t.blobs.get(t.app.appId, pdf.fileId))!
    expect(bytes.length).toBe(pdf.size)
    const body = text(bytes)
    expect(body.startsWith(`%PDF-1.4\n`)).toBe(true)
    expect(body.trimEnd().endsWith(`%%EOF`)).toBe(true)
    expect(body).toContain(`(Summer 2026) Tj`) // the heading
    expect(body).toContain(`(At the lake) Tj`) // the caption
    // Three picked files (a JPEG, two PNGs), each one image object; drawn four times (the cover is the first photo).
    expect(body.match(/\/Subtype \/Image/g)?.length).toBe(3)
    expect(body.match(/\/Im\d+ Do/g)?.length).toBe(4)
    expect(body).toContain(`/DCTDecode`)
    // The cover alone on the first sheet, then one sheet per page: each page opens with its break, so none is blank.
    expect(body.match(/\/Type \/Page(?!s)/g)?.length).toBe(2)
    expect(body).toContain(`/FlateDecode`)
    // Every startxref offset points at the xref table.
    const at = Number(body.match(/startxref\n(\d+)\n%%EOF/)![1])
    expect(body.slice(at, at + 4)).toBe(`xref`)

    // Save and Share the produced file.
    expect((await t.press(`save`, `files.save`, { fileId: pdf.fileId, name: pdf.name })).error).toBeUndefined()
    expect(t.saved).toEqual([{ name: `Summer 2026.pdf`, mimeType: `application/pdf`, data: base64Encode(bytes) }])
    expect((await t.press(`share`, `share.send`, { fileId: pdf.fileId, name: pdf.name, mimeType: `application/pdf` })).error).toBeUndefined()
    expect(t.shared).toEqual([{ file: { name: `Summer 2026.pdf`, mimeType: `application/pdf`, data: base64Encode(bytes) } }])

    // No network: no http capability call, no fetch.
    expect(t.http.calls).toEqual([])
    expect(fetched).toEqual([])
    expect(t.photos.calls.map(([tool]) => tool)).toEqual([`pick`, `pick`, `pick`])
    expect(t.print.calls).toEqual([]) // print.pdf runs in the Engine
  })
})
