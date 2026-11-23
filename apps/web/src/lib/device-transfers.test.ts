import { describe, expect, it } from "vitest"
import { formatBytes, transferEndText, transferPathError } from "@/lib/device-transfers"

describe(`device transfers (#FEED-295)`, () => {
  it(`takes absolute or ~/ paths without .. or NUL`, () => {
    for (const ok of [`/home/deck/WoW`, `~/WoW/Interface/AddOns`, `~`, `C:\\Games\\WoW`]) expect(transferPathError(ok)).toBeNull()
    for (const bad of [``, ` `, `WoW`, `~deck/x`, `/a/../b`, `~/..`, `/a/\0b`, `C:\\a\\..\\b`]) expect(transferPathError(bad)).not.toBeNull()
    expect(transferPathError(`/${`a`.repeat(5000)}`)).toMatch(/too long/)
  })

  it(`formats sizes in decimal units`, () => {
    expect(formatBytes(0)).toBe(`0 B`)
    expect(formatBytes(999)).toBe(`999 B`)
    expect(formatBytes(5_900_000_000)).toBe(`5.9 GB`)
    expect(formatBytes(164_000_000)).toBe(`164 MB`)
  })

  it(`words the wake line by outcome`, () => {
    const row = { id: `t1`, files: 2, bytes: 1500, skipped: [], error: null, srcPath: `~/a`, destPath: `/b` }
    const labels = { from: `mint`, to: `steamdeck` }
    expect(transferEndText({ ...row, status: `done` }, labels)).toBe(`Transfer t1 done: mint:~/a → steamdeck:/b, 2 files, 1.5 KB, every checksum verified.`)
    expect(transferEndText({ ...row, status: `failed`, error: `/b already exists` }, labels)).toBe(`Transfer t1 failed: mint:~/a → steamdeck:/b: /b already exists`)
    expect(transferEndText({ ...row, status: `cancelled` }, labels)).toBe(`Transfer t1 cancelled: mint:~/a → steamdeck:/b.`)
  })
})
