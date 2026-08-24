/* Raw-capture store for the store-screenshot compositor (EXP-566).

   The PNG plumbing here — `pngSize()`, the `exp-store-frame` tEXt marker and
   the stash-the-unmarked-original-on-first-sight rule — is ported VERBATIM
   from the satori-based `scripts/frame-store-screenshots.tsx` (EXP-580), which
   this directory replaces. The rationale for the marker is unchanged and worth
   repeating: composed outputs carry it so a re-run can tell "already composed,
   use the stashed raw" from "fresh capture, stash it first" by the file itself.
   An older EXP-580 run trusted whatever sat in `*-raw/` and a stale stash from
   a dry run silently replaced three real captures.

   What is NEW here is `syncRaws()`. Since EXP-566 the fastlane captures write
   STRAIGHT into a `screenshots-raw/` tree and the compositor OWNS the upload
   dir (it clears and rewrites it), so raws and uploads no longer share a
   directory. Both layouts are handled: unmarked PNGs found in an upload dir
   are migrated into the raw tree, and the two legacy `*-raw/` stash locations
   are read as fallback raw sources. Nothing here is committed — store
   screenshots stay out of git (EXP-348). */

import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
} from "node:fs"
import { basename, dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const HERE = dirname(fileURLToPath(import.meta.url))
export const REPO = resolve(HERE, `../../../..`)
export const MARKETING = resolve(HERE, `../..`)

export type Platform = `ios` | `android`
export type Form = `ios-phone` | `ios-tablet` | `android-phone`

/* ── PNG bits (ported from EXP-580) ──────────────────────────────────────── */

/** Width/height from the PNG IHDR chunk — no image dep needed. */
export function pngSize(buf: Buffer): { width: number; height: number } {
  if (buf.readUInt32BE(12) !== 0x49484452) throw new Error(`not a PNG (no IHDR)`)
  return { width: buf.readUInt32BE(16), height: buf.readUInt32BE(20) }
}

const MARKER_KEY = `exp-store-frame`

export function hasMarker(buf: Buffer): boolean {
  return buf.includes(Buffer.from(`${MARKER_KEY}\0`))
}

/** Insert a tEXt chunk right after IHDR. */
export function withMarker(png: Buffer): Buffer {
  const data = Buffer.from(`${MARKER_KEY}\0EXP-566`, `latin1`)
  const type = Buffer.from(`tEXt`, `latin1`)
  const len = Buffer.alloc(4)
  len.writeUInt32BE(data.length)
  const crc = Buffer.alloc(4)
  crc.writeUInt32BE(Bun.hash.crc32(Buffer.concat([type, data])) >>> 0)
  const ihdrEnd = 8 + 4 + 4 + 13 + 4
  return Buffer.concat([png.subarray(0, ihdrEnd), len, type, data, crc, png.subarray(ihdrEnd)])
}

/** `iPhone 17 Pro Max-04_steering.png` / `4_steering.png` → `steering`. */
export function shotKey(file: string): string | null {
  const m = /(?:^|-)\d+_([a-z-]+)\.png$/.exec(basename(file))
  return m ? m[1] : null
}

/* ── Capture-tree layout ─────────────────────────────────────────────────── */

type Layout = {
  /** Where the compositor WRITES; fastlane uploads from here. */
  upload: string
  /** Where the untouched captures live (the new fastlane output_directory). */
  raw: string
  /** Older stash/raw locations read as a fallback source. */
  legacyRaw: string[]
}

export const LAYOUT: Record<Platform, Layout> = {
  ios: {
    upload: resolve(REPO, `apps/ios/fastlane/screenshots/en-US`),
    raw: resolve(REPO, `apps/ios/fastlane/screenshots-raw/en-US`),
    legacyRaw: [resolve(REPO, `apps/ios/fastlane/screenshots-raw/en-US`)],
  },
  android: {
    upload: resolve(REPO, `apps/android/fastlane/metadata/android/en-US/images/phoneScreenshots`),
    raw: resolve(REPO, `apps/android/fastlane/screenshots-raw/en-US/images/phoneScreenshots`),
    legacyRaw: [
      resolve(REPO, `apps/android/fastlane/metadata/android/en-US/images/phoneScreenshots-raw`),
      resolve(REPO, `apps/android/fastlane/screenshots-raw/en-US/images/phoneScreenshots`),
    ],
  },
}

/** iOS filenames carry the simulator name; the prefix picks the form. */
export function formForFile(platform: Platform, file: string): Form {
  if (platform === `android`) return `android-phone`
  return /ipad/i.test(basename(file)) ? `ios-tablet` : `ios-phone`
}

export type Raw = {
  platform: Platform
  form: Form
  shot: string
  file: string
  path: string
  width: number
  height: number
}

function pngsIn(dir: string): string[] {
  if (!existsSync(dir) || !statSync(dir).isDirectory()) return []
  return readdirSync(dir)
    .filter((f) => f.endsWith(`.png`))
    .sort()
}

/**
 * Reconcile the raw tree with whatever the last capture left behind, then index
 * it. Handles both capture layouts:
 *
 *   - NEW: fastlane wrote unmarked PNGs straight into `screenshots-raw/`.
 *   - LEGACY: fastlane wrote into the upload dir. Unmarked files there are the
 *     truth — MOVE them into the raw tree (the compositor owns the upload dir
 *     and clears it, so leaving them would destroy them).
 *   - LEGACY stash: raws sitting in an old `*-raw/` location are copied across.
 *
 * `ownOutputs` is the set of file names THIS compositor writes into the upload
 * dir. Marked files matching it are our own disposable output (the dir gets
 * cleared and rewritten every run). Any OTHER marked file is a leftover from the
 * legacy in-place layout, where a composed image sat exactly where its own raw
 * used to; with no raw counterpart anywhere that is a hard error, because there
 * is nothing left to compose from.
 */
export function syncRaws(platform: Platform, ownOutputs: ReadonlySet<string> = new Set()): Raw[] {
  const { upload, raw, legacyRaw } = LAYOUT[platform]
  mkdirSync(raw, { recursive: true })

  // Legacy stash locations first, so a fresh capture in the upload dir still wins.
  for (const dir of legacyRaw) {
    if (resolve(dir) === resolve(raw)) continue
    for (const file of pngsIn(dir)) {
      const dest = join(raw, file)
      if (!existsSync(dest)) copyFileSync(join(dir, file), dest)
    }
  }

  for (const file of pngsIn(upload)) {
    const from = join(upload, file)
    const dest = join(raw, file)
    if (!hasMarker(readFileSync(from))) {
      // A fresh capture is the truth: stash it, overwriting any older raw, and
      // take it out of the upload dir the compositor is about to clear.
      copyFileSync(from, dest)
      rmSync(from)
    } else if (!ownOutputs.has(file) && !existsSync(dest)) {
      throw new Error(`${file} is already composed and its raw copy is missing — recapture it`)
    }
  }

  const raws: Raw[] = []
  for (const file of pngsIn(raw)) {
    const shot = shotKey(file)
    if (!shot) continue
    const path = join(raw, file)
    const { width, height } = pngSize(readFileSync(path))
    raws.push({ platform, form: formForFile(platform, file), shot, file, path, width, height })
  }
  return raws
}

/** Base64 data URI for a PNG on disk. */
export function pngDataUri(path: string): string {
  return `data:image/png;base64,${readFileSync(path).toString(`base64`)}`
}
