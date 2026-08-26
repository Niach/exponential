/**
 * Fastlane output → `.shots-raw/` (EXP-566).
 *
 * The native lanes cannot write the store's layout: snapshot and screengrab own
 * their output directories, name files after the UI test's shot id
 * (`01_board`, `sg_reviews`) and — on iOS — prefix every file with the simulator
 * name. This importer is the adapter. It reads the catalog's `NativeCapture`
 * (`shot` + `lane`), finds the matching PNG, and copies it to
 * `.shots-raw/<platform>/<view-id>.png` so the store writer sees exactly the
 * same shape it sees from the browser and desktop lanes.
 *
 * Two source generations are supported. The RAW dirs
 * (`screenshots-raw`, `screenshots-styleguide`, `styleguide-screenshots`) hold
 * untouched device captures and are what the current lanes write. The LEGACY
 * dirs (`fastlane/screenshots`, `metadata/android/.../phoneScreenshots`) are the
 * store-upload sets — after EXP-580 those are COMPOSITED marketing slides with a
 * headline and a device bezel, useless as parity references. They are only read
 * when the raw dir is missing, and any file carrying the compositor's
 * `exp-store-frame` PNG marker is skipped rather than imported as if it were a
 * screenshot of the app.
 *
 * A missing source is a WARNING, never an error: capture lanes are run
 * independently and half a native set is a normal intermediate state.
 */
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync } from "node:fs"
import { dirname, join } from "node:path"
import { captureFor, viewsFor, type NativeCapture, type Platform } from "@exp/view-catalog"
import { rawShotPath, repoRoot } from "./paths.ts"

/**
 * The platforms this importer handles. `ipad` rides the iOS lane: the Snapfile
 * already runs the store set on both simulators, so the tablet PNGs sit in the
 * SAME directory as the iPhone ones and differ only by their filename prefix.
 */
export const NATIVE_PLATFORMS: readonly Platform[] = [`ios`, `ipad`, `android`]

/**
 * The tEXt chunk the store compositor stamps into finished marketing slides
 * (EXP-580). Its presence means "this is a bezelled panel, not a screenshot".
 */
export const STORE_FRAME_MARKER = `exp-store-frame`

interface SourceDir {
  dir: string
  legacy: boolean
  /** Files are `<device>-<shot>.png`; the prefix must match this. */
  devicePrefix?: RegExp
}

/**
 * Candidate directories per platform+lane, most-preferred first. The device
 * prefix is matched as a PATTERN rather than a literal simulator name: the
 * Snapfile's device list moves with every iPhone generation, and the importer
 * should not need a release to follow it.
 */
function sourceDirs(platform: Platform, lane: NativeCapture[`lane`]): SourceDir[] {
  const root = repoRoot()
  const ios = (sub: string): string => join(root, `apps/ios/fastlane`, sub, `en-US`)
  const android = (sub: string): string =>
    join(root, `apps/android/fastlane`, sub, `en-US/images/phoneScreenshots`)

  switch (platform) {
    case `ios`:
      return [
        {
          dir: lane === `store` ? ios(`screenshots-raw`) : ios(`screenshots-styleguide`),
          legacy: false,
          devicePrefix: /^iPhone/,
        },
        { dir: ios(`screenshots`), legacy: true, devicePrefix: /^iPhone/ },
      ]
    case `ipad`:
      // Same dirs, same shot ids — only the simulator prefix separates the two
      // frames, which is exactly why the catalog makes `ipad` mirror `ios`.
      return [
        {
          dir: lane === `store` ? ios(`screenshots-raw`) : ios(`screenshots-styleguide`),
          legacy: false,
          devicePrefix: /^iPad/,
        },
        { dir: ios(`screenshots`), legacy: true, devicePrefix: /^iPad/ },
      ]
    case `android`:
      return [
        {
          dir: lane === `store` ? android(`screenshots-raw`) : android(`styleguide-screenshots`),
          legacy: false,
        },
        {
          dir: join(root, `apps/android/fastlane/metadata/android/en-US/images/phoneScreenshots`),
          legacy: true,
        },
      ]
    default:
      return []
  }
}

export interface ImportedShot {
  platform: Platform
  viewId: string
  shot: string
  from: string
  to: string
  legacy: boolean
}

export interface ImportNativeOptions {
  platforms?: readonly Platform[]
  /** Restrict to these view ids (the orchestrator's `--views`). */
  viewIds?: readonly string[]
  dryRun?: boolean
}

export interface ImportNativeResult {
  imported: ImportedShot[]
  warnings: string[]
}

/** Copy every native capture the catalog claims into `.shots-raw/`. */
export function importNative(opts: ImportNativeOptions = {}): ImportNativeResult {
  const platforms = (opts.platforms ?? NATIVE_PLATFORMS).filter((platform) =>
    NATIVE_PLATFORMS.includes(platform)
  )
  const wanted = opts.viewIds ? new Set(opts.viewIds) : undefined
  const imported: ImportedShot[] = []
  const warnings: string[] = []

  for (const platform of platforms) {
    for (const view of viewsFor(platform)) {
      if (wanted && !wanted.has(view.id)) continue
      const capture = captureFor(view, platform) as NativeCapture | undefined
      if (!capture) continue

      const found = findShot(platform, capture, warnings)
      if (!found) {
        warnings.push(
          `${platform}/${view.id}: no capture named \`${capture.shot}\` in the ${capture.lane} lane output — run the lane, or the view stays at its previous stored image`
        )
        continue
      }
      const to = rawShotPath(platform, view.id)
      if (!opts.dryRun) {
        mkdirSync(dirname(to), { recursive: true })
        copyFileSync(found.file, to)
      }
      imported.push({
        platform,
        viewId: view.id,
        shot: capture.shot,
        from: found.file,
        to,
        legacy: found.legacy,
      })
    }
  }

  return { imported, warnings }
}

/**
 * Find the PNG for one shot id. Accepts both the bare `<shot>.png` and
 * snapshot's `<device>-<shot>.png`, and when several devices produced the same
 * shot it takes the first in sort order and says so — silently picking one of
 * two different-sized captures is how a store gets a phone shot filed as a
 * tablet.
 */
function findShot(
  platform: Platform,
  capture: NativeCapture,
  warnings: string[]
): { file: string; legacy: boolean } | undefined {
  const target = `${capture.shot}.png`
  for (const source of sourceDirs(platform, capture.lane)) {
    if (!existsSync(source.dir)) continue
    const matches = readdirSync(source.dir)
      .filter((name) => {
        if (!name.endsWith(target)) return false
        if (name === target) return source.devicePrefix === undefined
        if (!name.endsWith(`-${target}`)) return false
        const prefix = name.slice(0, -(target.length + 1))
        return source.devicePrefix ? source.devicePrefix.test(prefix) : false
      })
      .sort()
      .map((name) => join(source.dir, name))
      .filter((file) => {
        if (!source.legacy) return true
        if (!isCompositedSlide(file)) return true
        warnings.push(
          `${platform}/${capture.shot}: skipped ${file} — it carries the \`${STORE_FRAME_MARKER}\` marker, so it is a composited store slide, not a raw capture`
        )
        return false
      })
    if (matches.length === 0) continue
    if (matches.length > 1) {
      warnings.push(
        `${platform}/${capture.shot}: ${matches.length} candidates in ${source.dir}; took ${matches[0]}`
      )
    }
    return { file: matches[0]!, legacy: source.legacy }
  }
  return undefined
}

/** Does this PNG carry the compositor's marker chunk? */
function isCompositedSlide(file: string): boolean {
  try {
    return readFileSync(file).includes(STORE_FRAME_MARKER)
  } catch {
    return false
  }
}
