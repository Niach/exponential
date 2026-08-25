/**
 * Where the screenshot store and its raw input live on disk (EXP-566).
 *
 * Two directories, both anchored on the repo root:
 *
 *   `.shots-raw/<platform>/<view-id>.png`  — gitignored capture output. Every
 *                                            lane (browser, desktop, fastlane
 *                                            importer) writes here and nowhere
 *                                            else, so the store writer has ONE
 *                                            input shape to read.
 *   `shots/<view-id>/<platform>.webp`      — the committed store, plus
 *                                            `shots/index.json`.
 *
 * Both roots honour an env override (`SHOTS_RAW_DIR` / `SHOTS_DIR`) and every
 * lookup reads the env at CALL time, never at import time — the tests point the
 * store at a tmp dir per test file, and a module-level constant would freeze the
 * real repo path before they get the chance.
 */
import { existsSync } from "node:fs"
import { dirname, isAbsolute, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { STORE_DIR, type Platform } from "@exp/view-catalog"

/** Files that only ever exist at the monorepo root. */
const ROOT_MARKERS = [`docker-compose.yaml`, `bun.lock`, `.gitignore`]

let cachedRoot: string | undefined

/**
 * The monorepo root, found by walking up from this file until a directory
 * carries the root markers AND a `packages/` sibling. Overridable with
 * `SHOTS_REPO_ROOT` so a test (or a checkout laid out differently) can pin it.
 * Falls back to the static three-levels-up path rather than throwing — a wrong
 * root surfaces as a missing file with a readable path, which beats a crash
 * inside a helper nobody suspects.
 */
export function repoRoot(): string {
  const override = process.env.SHOTS_REPO_ROOT
  if (override) return resolve(override)
  if (cachedRoot) return cachedRoot
  const here = dirname(fileURLToPath(import.meta.url))
  let dir = here
  for (let i = 0; i < 8; i++) {
    const hasMarker = ROOT_MARKERS.some((marker) => existsSync(join(dir, marker)))
    if (hasMarker && existsSync(join(dir, `packages`))) {
      cachedRoot = dir
      return dir
    }
    const parent = dirname(dir)
    if (parent === dir) break
    dir = parent
  }
  cachedRoot = resolve(here, `../../..`)
  return cachedRoot
}

/** Resolve a possibly-relative override against the repo root. */
function fromRoot(value: string): string {
  return isAbsolute(value) ? value : join(repoRoot(), value)
}

/** The committed store root (`<repo>/shots`, or `SHOTS_DIR`). */
export function storeDir(): string {
  const override = process.env.SHOTS_DIR
  return override ? fromRoot(override) : join(repoRoot(), STORE_DIR)
}

/** The gitignored raw-capture root (`<repo>/.shots-raw`, or `SHOTS_RAW_DIR`). */
export function rawDir(): string {
  const override = process.env.SHOTS_RAW_DIR
  return override ? fromRoot(override) : join(repoRoot(), `.shots-raw`)
}

/** Where a capture lane drops one platform's PNG for one view. */
export function rawShotPath(platform: Platform, viewId: string): string {
  return join(rawDir(), platform, `${viewId}.png`)
}

/** Where the store keeps one view's image for one platform. */
export function storeShotPath(viewId: string, platform: Platform): string {
  return join(storeDir(), viewId, `${platform}.webp`)
}

/** The store manifest. */
export function indexPath(): string {
  return join(storeDir(), `index.json`)
}

/**
 * The path recorded INSIDE `index.json`. Always the canonical
 * `shots/<view>/<platform>.webp` shape regardless of any `SHOTS_DIR` override,
 * so a test run and a real run produce byte-identical manifests.
 */
export function indexFileField(viewId: string, platform: Platform): string {
  return `${STORE_DIR}/${viewId}/${platform}.webp`
}
