// VAPP-271 (catalog/host.json media.appFile): an `app-file:<fileId>` src
// loads through the host's `appFile` hook (a cached blob url under the image
// limits); a host without one loads nothing, so the fallback paints.

import { describe, expect, test } from "bun:test"
import { clearMediaCache, openMediaSource } from "./media"
import type { AnyHostPlugin } from "./host-types"

// A 1×1 PNG.
const PNG = Uint8Array.from(atob(`iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==`), (c) => c.charCodeAt(0))

describe(`app-file sources`, () => {
  test(`a host without appFile loads nothing (the fallback)`, () => {
    const plan = openMediaSource({} as AnyHostPlugin, `app-file:blob-1`)
    expect([plan.fetched, plan.direct, plan.denied]).toEqual([false, undefined, true])
  })

  test(`appFile's bytes become a cached blob url`, async () => {
    clearMediaCache()
    const asked: string[] = []
    const host = { appFile: async (id: string) => (asked.push(id), new Blob([PNG])) } as AnyHostPlugin
    const plan = openMediaSource(host, `app-file:blob-1`)
    expect([plan.fetched, plan.denied]).toEqual([true, false])
    const a = plan.open()
    const url = await a.promise
    expect(url.startsWith(`blob:`)).toBe(true)
    const b = openMediaSource(host, `app-file:blob-1`).open()
    expect(await b.promise).toBe(url)
    expect(asked).toEqual([`blob-1`])
    a.release()
    b.release()
  })

  test(`a missing file or a bad id fails like a 404`, async () => {
    clearMediaCache()
    const host = { appFile: async () => null } as AnyHostPlugin
    expect(openMediaSource(host, `app-file:gone`).open().promise).rejects.toThrow(`app file gone not found`)
    // Not an app-file id: the plain media policy, which denies the scheme.
    expect(openMediaSource(host, `app-file:a/b`).denied).toBe(true)
  })
})
