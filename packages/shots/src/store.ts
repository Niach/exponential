/**
 * The screenshot store writer (EXP-566).
 *
 * The store is COMMITTED, and a capture run photographs ~40 views on up to six
 * platforms. Lossy webp is not bit-reproducible across a re-render of the same
 * screen — antialiasing shifts a pixel, an avatar loads a frame later — so a
 * naive "always write" turns every run into a 200-file binary diff nobody can
 * review. That is the problem this file exists to solve:
 *
 *   write a shot ONLY when it actually changed.
 *
 * "Changed" is decided on the ENCODED images (what git will store), not the raw
 * captures: encode the candidate, decode both webps to RGBA, count differing
 * pixels with pixelmatch, and keep the existing file when the changed fraction
 * is inside the view's tolerance. A run over an unchanged product produces an
 * EMPTY `git status shots/`, which is the whole point — the diff that shows up
 * in review is exactly the set of views the change actually moved.
 *
 * `index.json` is derived, never accumulated: `indexStore` re-walks the store
 * and rebuilds the manifest from what is on disk, with sorted keys and no
 * timestamps, so two runs over the same store produce byte-identical JSON.
 */
import { createHash } from "node:crypto"
import { mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"
import pixelmatch from "pixelmatch"
import {
  PLATFORMS,
  STORE_DEFAULT_TOLERANCE,
  captureFor,
  viewById,
  type Platform,
} from "@exp/view-catalog"
import { encodeShot, imageSize, toRawRgba } from "./encode.ts"
import { indexFileField, indexPath, storeDir, storeShotPath } from "./paths.ts"

export { repoRoot, storeDir, rawDir, rawShotPath, storeShotPath, indexPath } from "./paths.ts"

/**
 * `new` — nothing was stored for this view/platform yet.
 * `updated` — the image moved beyond tolerance (or `force`) and was rewritten.
 * `kept` — inside tolerance; the EXISTING file is left byte-for-byte untouched.
 */
export type ShotState = `new` | `updated` | `kept`

export interface WriteShotOptions {
  /** Rewrite even when the diff is inside tolerance. */
  force?: boolean
  /**
   * Write whenever ANY pixel differs, ignoring the tolerance but not skipping
   * the comparison (EXP-670). For a run the operator narrowed by hand with
   * `--views`: they are going to open every changed file anyway, so a shot the
   * tolerance would have swallowed costs them one revert, while a swallowed
   * REAL change costs a stale screenshot nobody notices. Unlike `force` this
   * still leaves a byte-identical shot alone, so it never manufactures a diff.
   *
   * "Any" is floored at `WRITE_ANY_CHANGE_FLOOR` rather than at zero.
   */
  writeAnyChange?: boolean
  /** Overrides the view's `diffTolerance` and `STORE_DEFAULT_TOLERANCE`. */
  tolerance?: number
  /** Decide the state, touch nothing on disk. */
  dryRun?: boolean
}

export interface WriteShotResult {
  state: ShotState
  /** Absolute path of the stored file. */
  file: string
  width: number
  height: number
  /** Size of the file that is now on disk (the kept one, when kept). */
  bytes: number
  /** Fraction of pixels that differed, when a comparison happened. */
  changedRatio?: number
}

/**
 * Pixelmatch's per-pixel colour-distance threshold. Deliberately loose (0.1) and
 * antialiasing-blind: the question is "did this view CHANGE", not "is this
 * render pixel-perfect". Subpixel text rendering and one-frame-late avatars must
 * not read as a product change.
 */
const PIXELMATCH_OPTIONS = { threshold: 0.1, includeAA: false } as const

/** The tolerance that applies to one view: explicit → per-view → catalog default. */
export function toleranceFor(viewId: string, override?: number): number {
  if (override !== undefined) return override
  return viewById(viewId)?.diffTolerance ?? STORE_DEFAULT_TOLERANCE
}

/** One written shot, as the run log reports it. */
export interface ShotDiffReport {
  viewId: string
  platform: Platform
  state: ShotState
  changedRatio?: number
  tolerance: number
}

/** The share of its tolerance above which a KEPT shot is flagged as a near miss. */
export const NEAR_MISS_SHARE = 0.5

/**
 * The floor `writeAnyChange` uses instead of zero — 2 changed pixels in 10,000.
 *
 * Below this is not a change, it is the lossy encoder: re-encoding the SAME
 * screen and diffing it against the stored webp measured under 0.00005, and a
 * re-render of an unchanged view landed at 0.0002. Above it sits every real
 * change EXP-670 was written for — the smallest was a whole "Pending invites"
 * section appearing, at 0.0025, an order of magnitude clear.
 *
 * Without a floor a narrowed run rewrites every view it names on every run,
 * which is the churn the store writer exists to prevent, only wearing a
 * different hat.
 */
export const WRITE_ANY_CHANGE_FLOOR = 0.0002

/**
 * The per-view lines of the diff-skip report (EXP-658).
 *
 * The tolerance exists to absorb the seed's drifting relative timestamps, but
 * a redesigned chip or badge is about the same pixel area as a re-rendered
 * "22 hr. ago" — so a real change CAN land under it and keep a stale shot with
 * nothing in the log to say so. Every kept shot that differed at all is listed
 * with its fraction against the tolerance, closest first, and one at or above
 * `NEAR_MISS_SHARE` is marked, so a reviewer sees "0.0041 of 0.0050" instead of
 * a silent `kept`. Updated shots list their fraction too, so the log shows how
 * far each one moved. Identical keeps — the common case — print nothing.
 *
 * The share is a WEAK signal and must not be read as one (EXP-670). It measures
 * area, and area does not separate a real change from churn: a queue reorder
 * read 95% of its tolerance while a whole new "Pending invites" section read
 * 50%, and a page of drifting relative timestamps read 74%. Spatial refinements
 * were measured and rejected — bounding box and pixel density overlap just as
 * badly. So under `writeAnyChange` the tolerance stops gating the WRITE and
 * becomes advice: a written shot below it is flagged for the reviewer's eye
 * rather than silently dropped.
 */
export function formatDiffReport(results: ShotDiffReport[]): string[] {
  const share = (entry: ShotDiffReport): number => (entry.changedRatio ?? 0) / entry.tolerance
  const updated = results
    .filter((entry) => entry.state === `updated` && entry.changedRatio !== undefined)
    .sort((a, b) => (b.changedRatio ?? 0) - (a.changedRatio ?? 0))
  const kept = results
    .filter((entry) => entry.state === `kept` && (entry.changedRatio ?? 0) > 0)
    .sort((a, b) => share(b) - share(a))
  const lines: string[] = []
  for (const entry of updated) {
    const under = entry.changedRatio! <= entry.tolerance
    lines.push(
      `  updated  ${`${entry.viewId}/${entry.platform}`.padEnd(32)}${entry.changedRatio!.toFixed(4)} (tolerance ${entry.tolerance.toFixed(4)})` +
        (under ? `  ← under tolerance, eyeball it` : ``)
    )
  }
  for (const entry of kept) {
    const percent = Math.round(share(entry) * 100)
    lines.push(
      `  kept     ${`${entry.viewId}/${entry.platform}`.padEnd(32)}${entry.changedRatio!.toFixed(4)} of ${entry.tolerance.toFixed(4)} (${percent}%)` +
        (share(entry) >= NEAR_MISS_SHARE ? `  ← near miss` : ``)
    )
  }
  return lines
}

/**
 * Encode one raw capture and store it if it changed.
 *
 * Order of the cheap checks matters: an absent file and a dimension change are
 * both decisive on their own, and neither needs a pixel decode. Only a
 * same-size pair pays for the RGBA round trip.
 */
export async function writeShot(
  viewId: string,
  platform: Platform,
  rawPng: Uint8Array,
  opts: WriteShotOptions = {}
): Promise<WriteShotResult> {
  const candidate = await encodeShot(rawPng)
  const file = storeShotPath(viewId, platform)
  const existing = readIfExists(file)

  if (!existing) {
    if (!opts.dryRun) writeFile(file, candidate.buf)
    return {
      state: `new`,
      file,
      width: candidate.width,
      height: candidate.height,
      bytes: candidate.buf.byteLength,
    }
  }

  const updated = (changedRatio?: number): WriteShotResult => {
    if (!opts.dryRun) writeFile(file, candidate.buf)
    return {
      state: `updated`,
      file,
      width: candidate.width,
      height: candidate.height,
      bytes: candidate.buf.byteLength,
      changedRatio,
    }
  }

  if (opts.force) return updated()

  const existingSize = await imageSize(existing)
  if (existingSize.width !== candidate.width || existingSize.height !== candidate.height) {
    return updated()
  }

  const [before, after] = await Promise.all([toRawRgba(existing), toRawRgba(candidate.buf)])
  const changed = pixelmatch(
    before.data,
    after.data,
    undefined,
    before.width,
    before.height,
    PIXELMATCH_OPTIONS
  )
  const changedRatio = changed / (before.width * before.height)
  const tolerance = toleranceFor(viewId, opts.tolerance)
  const limit = opts.writeAnyChange ? Math.min(WRITE_ANY_CHANGE_FLOOR, tolerance) : tolerance
  if (changedRatio > limit) return updated(changedRatio)

  return {
    state: `kept`,
    file,
    width: existingSize.width,
    height: existingSize.height,
    bytes: existing.byteLength,
    changedRatio,
  }
}

export interface IndexEntry {
  file: string
  width: number
  height: number
  bytes: number
  sha256: string
}

export interface StoreIndex {
  schema: 1
  views: Record<string, Record<string, IndexEntry>>
}

/** A stored file the catalog does not claim — a renamed view, a dropped platform. */
export interface Orphan {
  viewId: string
  platform: string
  file: string
  reason: string
}

export interface IndexStoreOptions {
  /** Delete orphans instead of only reporting them. */
  prune?: boolean
  /** Report what the manifest would say; do not write it or delete anything. */
  dryRun?: boolean
}

export interface IndexStoreResult {
  index: StoreIndex
  /** The exact bytes written (or that would be written) to `shots/index.json`. */
  json: string
  entries: number
  orphans: Orphan[]
  pruned: string[]
  /** False when `index.json` was already byte-identical. */
  changed: boolean
  indexFile: string
}

const PLATFORM_SET = new Set<string>(PLATFORMS)

/**
 * Re-derive `shots/index.json` from what is on disk.
 *
 * Derived, not accumulated: a view deleted from the catalog and pruned off disk
 * disappears from the manifest on the next run without anyone maintaining a
 * removal list. Keys are sorted and nothing carries a timestamp, so the file is
 * a stable diff — a changed hash in the manifest means a changed image, always.
 */
export async function indexStore(opts: IndexStoreOptions = {}): Promise<IndexStoreResult> {
  const root = storeDir()
  const orphans: Orphan[] = []
  const pruned: string[] = []
  const collected: { viewId: string; platform: Platform; entry: IndexEntry }[] = []

  for (const viewId of listDirs(root).sort()) {
    const view = viewById(viewId)
    for (const name of listFiles(join(root, viewId)).sort()) {
      if (!name.endsWith(`.webp`)) continue
      const platform = name.slice(0, -`.webp`.length)
      const file = join(root, viewId, name)
      const reason = !view
        ? `no such view in the catalog`
        : !PLATFORM_SET.has(platform)
          ? `unknown platform`
          : captureFor(view, platform as Platform) === undefined
            ? `the catalog declares no ${platform} capture for this view`
            : undefined
      if (reason) {
        orphans.push({ viewId, platform, file, reason })
        if (opts.prune && !opts.dryRun) {
          rmSync(file, { force: true })
          pruned.push(file)
        }
        continue
      }
      const bytes = readFileSync(file)
      const size = await imageSize(bytes)
      collected.push({
        viewId,
        platform: platform as Platform,
        entry: {
          file: indexFileField(viewId, platform as Platform),
          width: size.width,
          height: size.height,
          bytes: bytes.byteLength,
          sha256: createHash(`sha256`).update(bytes).digest(`hex`),
        },
      })
    }
    if (opts.prune && !opts.dryRun) removeIfEmpty(join(root, viewId))
  }

  // Sorted insertion IS the sort: JSON.stringify preserves insertion order for
  // string keys, so building the object in order is enough — no serializer hook.
  const views: StoreIndex[`views`] = {}
  for (const { viewId, platform, entry } of collected.sort(
    (a, b) => a.viewId.localeCompare(b.viewId) || a.platform.localeCompare(b.platform)
  )) {
    const bucket = views[viewId] ?? (views[viewId] = {})
    bucket[platform] = entry
  }

  const index: StoreIndex = { schema: 1, views }
  const json = `${JSON.stringify(index, null, 2)}\n`
  const indexFile = indexPath()
  const previous = readIfExists(indexFile)
  const changed = !previous || previous.toString(`utf8`) !== json
  if (changed && !opts.dryRun) writeFile(indexFile, Buffer.from(json, `utf8`))

  return { index, json, entries: collected.length, orphans, pruned, changed, indexFile }
}

/** Read `shots/index.json` if it exists and parses; otherwise an empty store. */
export function readIndex(): StoreIndex {
  const raw = readIfExists(indexPath())
  if (!raw) return { schema: 1, views: {} }
  try {
    return JSON.parse(raw.toString(`utf8`)) as StoreIndex
  } catch {
    return { schema: 1, views: {} }
  }
}

function readIfExists(file: string): Buffer | undefined {
  try {
    return readFileSync(file)
  } catch {
    return undefined
  }
}

function writeFile(file: string, buf: Buffer): void {
  mkdirSync(dirname(file), { recursive: true })
  writeFileSync(file, buf)
}

function listDirs(dir: string): string[] {
  try {
    return readdirSync(dir, { withFileTypes: true })
      .filter((entry) => entry.isDirectory())
      .map((entry) => entry.name)
  } catch {
    return []
  }
}

function listFiles(dir: string): string[] {
  try {
    return readdirSync(dir, { withFileTypes: true })
      .filter((entry) => entry.isFile())
      .map((entry) => entry.name)
  } catch {
    return []
  }
}

function removeIfEmpty(dir: string): void {
  try {
    if (statSync(dir).isDirectory() && readdirSync(dir).length === 0) rmSync(dir, { recursive: true })
  } catch {
    /* a directory that vanished under us needs no cleanup */
  }
}
