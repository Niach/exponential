/**
 * Reads the committed screenshot store off disk and cross-checks it against the
 * view catalog. Everything the gallery renders comes from here, so the page can
 * never claim a shot the store does not actually hold: the FILE is the truth for
 * existence, `shots/index.json` only enriches it with dimensions and bytes.
 *
 * Three states per (view, platform) pair:
 *   - `ok`      the catalog declares it and the file is on disk
 *   - `missing` the catalog declares it, nothing captured yet
 *   - `n/a`     the catalog does not declare it (the view's `notes` says why)
 */

import { existsSync, readdirSync, readFileSync, statSync } from "node:fs"
import type { Dirent } from "node:fs"
import path from "node:path"

import {
  GROUPS,
  PLATFORMS,
  PLATFORM_FRAME,
  STORE_DIR,
  VIEWS,
  captureFor,
  shotPath,
} from "@exp/view-catalog"
import type { Group, Platform, View } from "@exp/view-catalog"

export type ShotState = `ok` | `missing` | `n/a`

export interface Shot {
  platform: Platform
  /** Relative to the page, so the same URL works in `dist/` and under `serve`. */
  url: string
  state: ShotState
  w?: number
  h?: number
  bytes?: number
  /** `notes[platform]` from the catalog, when it documents this pair. */
  note?: string
}

export interface ViewEntry {
  view: View
  shots: Shot[]
}

export interface GalleryGroup {
  group: Group
  views: ViewEntry[]
}

export interface GalleryData {
  groups: GalleryGroup[]
  /** Every view in catalog order, flat — the routing/navigation order. */
  views: ViewEntry[]
  /** Paths under the store root that no view/platform pair claims. */
  undeclared: string[]
  /** Absolute path the images were read from. */
  storeDir: string
  /** Whether `index.json` was present and parseable. */
  indexPresent: boolean
  counts: { ok: number; missing: number; na: number }
}

/** One entry of the generated `shots/index.json`. */
interface IndexShot {
  file?: string
  width?: number
  height?: number
  bytes?: number
  sha256?: string
}

type ShotIndex = Record<string, Record<string, IndexShot>>

/**
 * The repo-root store. `SHOTS_DIR` overrides it — the tests and any local
 * preview point at a scratch copy rather than writing into the committed store.
 */
export function storeDir(): string {
  const override = process.env.SHOTS_DIR
  if (override !== undefined && override.length > 0) return path.resolve(override)
  return path.resolve(import.meta.dir, `../../..`, STORE_DIR)
}

function readIndex(dir: string): ShotIndex | undefined {
  const file = path.join(dir, `index.json`)
  if (!existsSync(file)) return undefined
  try {
    const parsed = JSON.parse(readFileSync(file, `utf8`)) as { views?: unknown }
    if (parsed.views === null || typeof parsed.views !== `object`) return undefined
    return parsed.views as ShotIndex
  } catch {
    return undefined
  }
}

function fileBytes(file: string): number | undefined {
  try {
    return statSync(file).size
  } catch {
    return undefined
  }
}

/** Every `<view-id>/<platform>.webp` the catalog declares. */
function declaredPaths(): Set<string> {
  const declared = new Set<string>()
  for (const view of VIEWS) {
    for (const platform of PLATFORMS) {
      if (captureFor(view, platform) === undefined) continue
      declared.add(`${view.id}/${platform}.webp`)
    }
  }
  return declared
}

/**
 * Anything in the store no catalog pair claims: stray files at the root, whole
 * directories that are not view ids, and images for platforms a view does not
 * declare. Reported by `build --check`, never silently rendered.
 */
function findUndeclared(dir: string): string[] {
  if (!existsSync(dir)) return []
  const declared = declaredPaths()
  const out: string[] = []
  let entries: Dirent[]
  try {
    entries = readdirSync(dir, { withFileTypes: true })
  } catch {
    return []
  }
  for (const entry of entries) {
    if (!entry.isDirectory()) {
      if (entry.name !== `index.json` && entry.name !== `.gitkeep`) out.push(entry.name)
      continue
    }
    let children: Dirent[]
    try {
      children = readdirSync(path.join(dir, entry.name), { withFileTypes: true })
    } catch {
      continue
    }
    if (children.length === 0) {
      out.push(`${entry.name}/`)
      continue
    }
    for (const child of children) {
      const rel = `${entry.name}/${child.name}`
      if (child.isDirectory()) {
        out.push(`${rel}/`)
        continue
      }
      if (!declared.has(rel)) out.push(rel)
    }
  }
  return out.sort((a, b) => a.localeCompare(b))
}

function buildShots(view: View, dir: string, index: ShotIndex | undefined): Shot[] {
  const indexed = index?.[view.id]
  return PLATFORMS.map((platform): Shot => {
    const url = shotPath(view.id, platform)
    const note = view.notes?.[platform]
    const frame = PLATFORM_FRAME[platform]
    if (captureFor(view, platform) === undefined) {
      return { platform, url, state: `n/a`, w: frame.w, h: frame.h, note }
    }
    const file = path.join(dir, `${view.id}`, `${platform}.webp`)
    if (!existsSync(file)) {
      return { platform, url, state: `missing`, w: frame.w, h: frame.h, note }
    }
    const meta = indexed?.[platform]
    return {
      platform,
      url,
      state: `ok`,
      w: meta?.width ?? frame.w,
      h: meta?.height ?? frame.h,
      bytes: meta?.bytes ?? fileBytes(file),
      note,
    }
  })
}

/** Read the store and pair it with the catalog. Never throws on a missing store. */
export function readGallery(dir = storeDir()): GalleryData {
  const index = readIndex(dir)
  const views: ViewEntry[] = []
  const groupOrder = new Map(GROUPS.map((group) => [group.id, group.order]))
  const ordered = [...VIEWS].sort(
    (a, b) =>
      (groupOrder.get(a.group) ?? 0) - (groupOrder.get(b.group) ?? 0) ||
      a.order - b.order ||
      a.id.localeCompare(b.id)
  )
  for (const view of ordered) {
    views.push({ view, shots: buildShots(view, dir, index) })
  }
  const groups: GalleryGroup[] = [...GROUPS]
    .sort((a, b) => a.order - b.order)
    .map((group) => ({
      group,
      views: views.filter((entry) => entry.view.group === group.id),
    }))
    .filter((section) => section.views.length > 0)

  const counts = { ok: 0, missing: 0, na: 0 }
  for (const entry of views) {
    for (const shot of entry.shots) {
      if (shot.state === `ok`) counts.ok += 1
      else if (shot.state === `missing`) counts.missing += 1
      else counts.na += 1
    }
  }

  return {
    groups,
    views,
    undeclared: findUndeclared(dir),
    storeDir: dir,
    indexPresent: index !== undefined,
    counts,
  }
}

/** Every declared-but-uncaptured pair, as `<view-id>/<platform>`. */
export function missingPairs(data: GalleryData): string[] {
  const out: string[] = []
  for (const entry of data.views) {
    for (const shot of entry.shots) {
      if (shot.state === `missing`) out.push(`${entry.view.id}/${shot.platform}`)
    }
  }
  return out
}
