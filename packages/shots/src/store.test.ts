/**
 * The store writer's one job is "write ONLY when the image actually changed" —
 * these tests pin that contract with synthesized PNGs so a re-encode of the
 * same screen never churns git and a real change always lands.
 */
import { afterEach, beforeEach, describe, expect, test } from "bun:test"
import { createHash } from "node:crypto"
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import sharp from "sharp"
import { indexStore, writeShot } from "./store.ts"
import { storeShotPath } from "./paths.ts"

let dir: string

beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), `exp-shots-test-`))
  process.env.SHOTS_DIR = dir
})

afterEach(() => {
  delete process.env.SHOTS_DIR
  rmSync(dir, { recursive: true, force: true })
})

/** A flat dark PNG with an optional bright patch covering `patch` of the area. */
async function png(width: number, height: number, patch = 0): Promise<Uint8Array> {
  const base = sharp({
    create: { width, height, channels: 3, background: { r: 24, g: 24, b: 27 } },
  })
  if (patch <= 0) return new Uint8Array(await base.png().toBuffer())
  const pw = Math.round(width * Math.sqrt(patch))
  const ph = Math.round(height * Math.sqrt(patch))
  const overlay = await sharp({
    create: { width: pw, height: ph, channels: 3, background: { r: 250, g: 250, b: 250 } },
  })
    .png()
    .toBuffer()
  return new Uint8Array(
    await base
      .composite([{ input: overlay, left: 0, top: 0 }])
      .png()
      .toBuffer()
  )
}

function sha(file: string): string {
  return createHash(`sha256`).update(readFileSync(file)).digest(`hex`)
}

describe(`writeShot`, () => {
  test(`cold store → new`, async () => {
    const result = await writeShot(`board`, `web`, await png(400, 300))
    expect(result.state).toBe(`new`)
    expect(result.width).toBe(400)
    expect(result.height).toBe(300)
    expect(sha(storeShotPath(`board`, `web`))).toBeTruthy()
  })

  test(`re-encode of the same image → kept, file untouched`, async () => {
    await writeShot(`board`, `web`, await png(400, 300))
    const before = sha(storeShotPath(`board`, `web`))
    const result = await writeShot(`board`, `web`, await png(400, 300))
    expect(result.state).toBe(`kept`)
    expect(result.changedRatio).toBeLessThanOrEqual(0.005)
    expect(sha(storeShotPath(`board`, `web`))).toBe(before)
  })

  test(`a 5%-area patch → updated`, async () => {
    await writeShot(`board`, `web`, await png(400, 300))
    const result = await writeShot(`board`, `web`, await png(400, 300, 0.05))
    expect(result.state).toBe(`updated`)
    expect(result.changedRatio ?? 0).toBeGreaterThan(0.005)
  })

  test(`dimension change → updated without a pixel diff`, async () => {
    await writeShot(`board`, `web`, await png(400, 300))
    const result = await writeShot(`board`, `web`, await png(402, 300))
    expect(result.state).toBe(`updated`)
    expect(result.changedRatio).toBeUndefined()
  })

  test(`force rewrites an unchanged image`, async () => {
    await writeShot(`board`, `web`, await png(400, 300))
    const result = await writeShot(`board`, `web`, await png(400, 300), { force: true })
    expect(result.state).toBe(`updated`)
  })

  test(`dryRun decides without touching disk`, async () => {
    const result = await writeShot(`board`, `web`, await png(400, 300), { dryRun: true })
    expect(result.state).toBe(`new`)
    expect(() => readFileSync(storeShotPath(`board`, `web`))).toThrow()
  })
})

describe(`indexStore`, () => {
  test(`byte-stable across two runs, orphans reported, prune deletes`, async () => {
    await writeShot(`board`, `web`, await png(400, 300))
    await writeShot(`inbox`, `web`, await png(400, 300))
    // An orphan: a view id the catalog does not know.
    mkdirSync(join(dir, `bogus-view`), { recursive: true })
    writeFileSync(join(dir, `bogus-view`, `web.webp`), readFileSync(storeShotPath(`board`, `web`)))

    const first = await indexStore()
    expect(first.entries).toBe(2)
    expect(first.orphans).toHaveLength(1)
    expect(first.orphans[0]?.viewId).toBe(`bogus-view`)
    expect(first.changed).toBe(true)
    expect(first.index.views[`board`]?.[`web`]?.file).toBe(`shots/board/web.webp`)

    const second = await indexStore()
    expect(second.json).toBe(first.json)
    expect(second.changed).toBe(false)

    const pruned = await indexStore({ prune: true })
    expect(pruned.pruned).toHaveLength(1)
    expect(() => readFileSync(join(dir, `bogus-view`, `web.webp`))).toThrow()
  })
})
