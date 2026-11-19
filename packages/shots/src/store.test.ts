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
import {
  EDE_FAINT_CHECK,
  EDE_TOLERANCE,
  NEAR_MISS_SHARE,
  WRITE_ANY_CHANGE_FLOOR,
  formatDiffReport,
  indexStore,
  toleranceFor,
  writeShot,
} from "./store.ts"
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

  test(`writeAnyChange writes a sub-tolerance change but still keeps an identical one`, async () => {
    // EXP-670: a compact real change (a queue reorder, a section that
    // appeared) sits inside the tolerance just as comfortably as timestamp
    // churn does, so a hand-narrowed `--views` run must not drop it.
    await writeShot(`board`, `web`, await png(400, 300))
    const before = sha(storeShotPath(`board`, `web`))

    const tiny = await writeShot(`board`, `web`, await png(400, 300, 0.002), {
      writeAnyChange: true,
    })
    expect(tiny.state).toBe(`updated`)
    expect(tiny.changedRatio ?? 0).toBeLessThanOrEqual(0.005)
    expect(sha(storeShotPath(`board`, `web`))).not.toBe(before)

    // …and it is not `force`: an unchanged re-render still leaves the file be,
    // so the flag can never manufacture a diff.
    const after = sha(storeShotPath(`board`, `web`))
    const same = await writeShot(`board`, `web`, await png(400, 300, 0.002), {
      writeAnyChange: true,
    })
    expect(same.state).toBe(`kept`)
    expect(sha(storeShotPath(`board`, `web`))).toBe(after)
  })

  test(`writeAnyChange floors at encoder noise, so it cannot churn every run`, async () => {
    // A 1-in-10,000 patch is below WRITE_ANY_CHANGE_FLOOR: at that scale the
    // lossy encoder is the only thing talking.
    await writeShot(`board`, `web`, await png(400, 300))
    const before = sha(storeShotPath(`board`, `web`))
    const result = await writeShot(`board`, `web`, await png(400, 300, 0.0001), {
      writeAnyChange: true,
    })
    expect(result.state).toBe(`kept`)
    expect(result.changedRatio ?? 0).toBeLessThan(WRITE_ANY_CHANGE_FLOOR)
    expect(sha(storeShotPath(`board`, `web`))).toBe(before)
  })

  test(`without writeAnyChange the same sub-tolerance change is kept`, async () => {
    await writeShot(`board`, `web`, await png(400, 300))
    const before = sha(storeShotPath(`board`, `web`))
    const result = await writeShot(`board`, `web`, await png(400, 300, 0.002))
    expect(result.state).toBe(`kept`)
    expect(sha(storeShotPath(`board`, `web`))).toBe(before)
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

describe(`formatDiffReport`, () => {
  test(`lists moved and near-miss shots, closest to the tolerance first, and nothing identical`, () => {
    // EXP-658: the issue-comments chip change sat under the default tolerance
    // and was kept without a word in the log. The report makes that visible.
    const lines = formatDiffReport([
      { viewId: `board`, platform: `web`, state: `kept`, changedRatio: 0, tolerance: 0.005 },
      { viewId: `inbox`, platform: `web`, state: `kept`, changedRatio: 0.0005, tolerance: 0.005 },
      { viewId: `issue-comments`, platform: `ios`, state: `kept`, changedRatio: 0.0041, tolerance: 0.005 },
      { viewId: `issue-detail`, platform: `web`, state: `updated`, changedRatio: 0.0312, tolerance: 0.005 },
      { viewId: `search`, platform: `web`, state: `new`, tolerance: 0.0005 },
      { viewId: `terminal`, platform: `desktop`, state: `updated`, tolerance: 0.005 }, // forced: no compare
    ])
    expect(lines).toHaveLength(3)
    expect(lines[0]).toContain(`updated  issue-detail/web`)
    expect(lines[0]).toContain(`0.0312`)
    expect(lines[1]).toContain(`kept     issue-comments/ios`)
    expect(lines[1]).toContain(`0.0041 of 0.0050 (82%)`)
    expect(lines[1]).toContain(`near miss`)
    expect(lines[2]).toContain(`inbox/web`)
    expect(lines[2]).toContain(`(10%)`)
    expect(lines[2]).not.toContain(`near miss`)
  })

  test(`an updated shot under its tolerance is flagged for the reviewer's eye`, () => {
    // EXP-670: only a --views run produces these, and they are the ones most
    // likely to be churn — so they say so instead of hiding among real moves.
    const lines = formatDiffReport([
      { viewId: `reviews`, platform: `web`, state: `updated`, changedRatio: 0.0029, tolerance: 0.005 },
      { viewId: `board`, platform: `web`, state: `updated`, changedRatio: 0.031, tolerance: 0.005 },
    ])
    expect(lines[0]).toContain(`board/web`)
    expect(lines[0]).not.toContain(`under tolerance`)
    expect(lines[1]).toContain(`reviews/web`)
    expect(lines[1]).toContain(`under tolerance, eyeball it`)
    expect(NEAR_MISS_SHARE).toBeGreaterThan(0)
    expect(NEAR_MISS_SHARE).toBeLessThan(1)
  })
})

// #EDE-106: an eDE view captured before the shell drew it is the empty
// desktop (ede-panel's picture); it never replaces the view's golden.
describe(`writeShot: the empty eDE desktop`, () => {
  /** The empty desktop: a dark frame with a small patch (the taskbar) top-left. */
  const desktop = () => png(400, 300, 0.02)
  /** The desktop with a light block (the view) at `left`. */
  async function drawn(left: number): Promise<Uint8Array> {
    const block = await sharp({ create: { width: 120, height: 120, channels: 3, background: { r: 120, g: 160, b: 220 } } })
      .png()
      .toBuffer()
    return new Uint8Array(await sharp(Buffer.from(await desktop())).composite([{ input: block, left, top: 150 }]).png().toBuffer())
  }

  test(`refuses the empty desktop over a view's golden, even forced, and keeps the golden`, async () => {
    await writeShot(`ede-panel`, `ede`, await desktop())
    await writeShot(`ede-chat`, `ede`, await drawn(40))
    const before = sha(storeShotPath(`ede-chat`, `ede`))
    for (const force of [false, true]) {
      await expect(writeShot(`ede-chat`, `ede`, await desktop(), { force })).rejects.toThrow(/empty desktop/)
    }
    expect(sha(storeShotPath(`ede-chat`, `ede`))).toBe(before)
  })

  test(`a view that moved, ede-panel itself and a first golden still store`, async () => {
    await writeShot(`ede-panel`, `ede`, await desktop())
    await writeShot(`ede-chat`, `ede`, await drawn(40))
    expect((await writeShot(`ede-chat`, `ede`, await drawn(260))).state).toBe(`updated`)
    expect((await writeShot(`ede-panel`, `ede`, await desktop(), { force: true })).state).toBe(`updated`)
    expect((await writeShot(`ede-osd`, `ede`, await desktop())).state).toBe(`new`)
  })
})

// #EDE-140: an eDE golden has a fixed clock, date, demo data and fonts, so it
// is held to what the two runners' noise leaves: a small clear change and a
// faint broad one are both a change. The default tolerance (0.5 % of the
// pixels at a loose colour distance) kept nine stale goldens after a surface
// token moved by 14 levels.
describe(`writeShot: eDE goldens`, () => {
  const FIXTURES = join(import.meta.dir, `fixtures`)
  /** An eDE frame: the ground, a card over 35 % of it, the taskbar's icons. */
  async function desk(opts: { card?: number; bell?: boolean } = {}): Promise<Uint8Array> {
    const flat = (width: number, height: number, level: number) =>
      sharp({ create: { width, height, channels: 3, background: { r: level, g: level, b: level } } }).png().toBuffer()
    const layers = [{ input: await flat(700, 460, opts.card ?? 23), left: 290, top: 120 }]
    for (const [index, left] of [1180, 1212, 1244].entries()) {
      if (index === 0 && opts.bell === false) continue
      layers.push({ input: await flat(16, 16, 230), left, top: 692 })
    }
    return new Uint8Array(await sharp(await flat(1280, 720, 10)).composite(layers).png().toBuffer())
  }

  test(`a 16×16 icon that went is a change on eDE, and inside the tolerance elsewhere`, async () => {
    await writeShot(`ede-chat`, `ede`, await desk())
    const ede = await writeShot(`ede-chat`, `ede`, await desk({ bell: false }))
    expect(ede.state).toBe(`updated`)
    expect(ede.changedRatio ?? 0).toBeGreaterThan(EDE_TOLERANCE)
    expect(ede.changedRatio ?? 1).toBeLessThan(0.005)

    await writeShot(`board`, `web`, await desk())
    expect((await writeShot(`board`, `web`, await desk({ bell: false }))).state).toBe(`kept`)
  })

  test(`a card whose colour moved by 8 levels is a change on eDE, and nothing elsewhere`, async () => {
    // #171717 → #1f1f1f: no pixel moves far enough for the loose pass.
    await writeShot(`ede-history`, `ede`, await desk())
    const ede = await writeShot(`ede-history`, `ede`, await desk({ card: 31 }))
    expect(ede.state).toBe(`updated`)
    expect(ede.changedRatio).toBe(0)
    expect(ede.faintRatio ?? 0).toBeGreaterThan(0.3)

    await writeShot(`board`, `web`, await desk())
    const web = await writeShot(`board`, `web`, await desk({ card: 31 }))
    expect(web.state).toBe(`kept`)
    expect(web.faintRatio).toBeUndefined()
  })

  test(`the same frame again is kept, both ways`, async () => {
    await writeShot(`ede-chat`, `ede`, await desk())
    const again = await writeShot(`ede-chat`, `ede`, await desk())
    expect(again.state).toBe(`kept`)
    expect([again.changedRatio, again.faintRatio]).toEqual([0, 0])
  })

  test(`the two runners' renders of one commit are the same golden`, async () => {
    // ede-chat (the densest text) of one commit as the Mint's runner and the
    // GitHub-hosted fallback drew it (lossless webp): 47,000 pixels differ at
    // all, by a level or two at glyph edges. Whichever recorded the golden,
    // the other keeps it.
    const [mint, hosted] = [`ede-chat.mint.webp`, `ede-chat.hosted.webp`].map((name) => new Uint8Array(readFileSync(join(FIXTURES, name))))
    for (const [golden, capture] of [[mint, hosted], [hosted, mint]] as const) {
      rmSync(join(dir, `ede-chat`), { recursive: true, force: true })
      await writeShot(`ede-chat`, `ede`, golden!)
      const result = await writeShot(`ede-chat`, `ede`, capture!)
      expect(result.state).toBe(`kept`)
      // At least half of each tolerance is left.
      expect(result.changedRatio ?? 1).toBeLessThan(EDE_TOLERANCE / 2)
      expect(result.faintRatio ?? 1).toBeLessThan(EDE_FAINT_CHECK.tolerance / 2)
    }
  })

  test(`the tolerance is eDE's own; a view's and the caller's still win`, () => {
    expect(toleranceFor(`ede-chat`, undefined, `ede`)).toBe(EDE_TOLERANCE)
    expect(toleranceFor(`ede-chat`, 0.25, `ede`)).toBe(0.25)
    expect(toleranceFor(`board`, undefined, `web`)).toBe(0.005)
    expect(toleranceFor(`board`)).toBe(0.005)
    expect(EDE_TOLERANCE).toBeLessThanOrEqual(WRITE_ANY_CHANGE_FLOOR)
  })

  test(`the report names the faint pass when it is the one that moved`, () => {
    const lines = formatDiffReport([
      { viewId: `ede-history`, platform: `ede`, state: `updated`, changedRatio: 0, tolerance: 0.0001, faintRatio: 0.35, faintTolerance: 0.02 },
      { viewId: `ede-chat`, platform: `ede`, state: `kept`, changedRatio: 0, tolerance: 0.0001, faintRatio: 0.0043, faintTolerance: 0.02 },
      { viewId: `ede-panel`, platform: `ede`, state: `kept`, changedRatio: 0, tolerance: 0.0001, faintRatio: 0, faintTolerance: 0.02 },
    ])
    expect(lines).toHaveLength(2)
    expect(lines[0]).toContain(`updated  ede-history/ede`)
    expect(lines[0]).toContain(`faint 0.3500 (tolerance 0.0200)`)
    expect(lines[0]).not.toContain(`under tolerance`)
    expect(lines[1]).toContain(`kept     ede-chat/ede`)
    expect(lines[1]).toContain(`faint 0.0043 of 0.0200 (22%)`)
  })
})
