/**
 * `bun run shots:check` (#EXP-1779: rehomed from the retired styleguide app's
 * `build --check`): the drift check between the committed store (`shots/`)
 * and the view catalog. Per declared (view, platform) pair the FILE is the
 * truth:
 *
 *   - missing     declared, nothing captured: the pipeline owes it (exit 1)
 *   - manual      declared, a `manual` desktop drive: waits on a person
 *                 (printed, never fails: a gate red for work only a person
 *                 can do is a gate everyone learns to ignore)
 *   - undeclared  a file in the store no pair claims: a renamed view or a
 *                 stale shot (exit 1)
 *
 * The iPad column holds the store set only (#NEW-12): a view without `ipad`
 * owes no iPad shot.
 */
import { existsSync, readdirSync } from "node:fs"
import type { Dirent } from "node:fs"
import { join } from "node:path"
import { PLATFORMS, VIEWS, captureFor } from "@exp/view-catalog"
import type { View } from "@exp/view-catalog"
import { storeDir } from "./paths.ts"

export interface StoreCheck {
  /** Declared pairs with a file, as `<view-id>/<platform>`. */
  captured: string[]
  missing: string[]
  manual: string[]
  /** Store paths no pair claims (`dir/` for a stray or empty directory). */
  undeclared: string[]
}

/** The non-image files that legitimately live in the store root. */
const STORE_FILES = new Set([`index.json`, `.gitkeep`, `README.md`])

const isManual = (view: View): boolean => {
  const capture = captureFor(view, `desktop`) as unknown
  return typeof capture === `object` && capture !== null && `drive` in capture && (capture as { drive: { kind: string } }).drive.kind === `manual`
}

function undeclared(dir: string, declared: Set<string>): string[] {
  if (!existsSync(dir)) return []
  const out: string[] = []
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (!entry.isDirectory()) {
      if (!STORE_FILES.has(entry.name)) out.push(entry.name)
      continue
    }
    let children: Dirent[]
    try {
      children = readdirSync(join(dir, entry.name), { withFileTypes: true })
    } catch {
      continue
    }
    if (children.length === 0) out.push(`${entry.name}/`)
    for (const child of children) {
      const rel = `${entry.name}/${child.name}`
      if (child.isDirectory()) out.push(`${rel}/`)
      else if (!declared.has(rel)) out.push(rel)
    }
  }
  return out.sort((a, b) => a.localeCompare(b))
}

/** Pair the store at `dir` with the catalog. Never throws on a missing store. */
export function checkStore(dir = storeDir()): StoreCheck {
  const result: StoreCheck = { captured: [], missing: [], manual: [], undeclared: [] }
  const declared = new Set<string>()
  for (const view of VIEWS) {
    for (const platform of PLATFORMS) {
      if (captureFor(view, platform) === undefined) continue
      declared.add(`${view.id}/${platform}.webp`)
      if (platform === `ipad` && view.ipad === undefined) continue
      const pair = `${view.id}/${platform}`
      if (existsSync(join(dir, view.id, `${platform}.webp`))) result.captured.push(pair)
      else if (platform === `desktop` && isManual(view)) result.manual.push(pair)
      else result.missing.push(pair)
    }
  }
  result.undeclared = undeclared(dir, declared)
  return result
}

if (import.meta.main) {
  const dir = storeDir()
  const r = checkStore(dir)
  for (const pair of r.missing) console.log(`  missing  ${pair}`)
  for (const pair of r.manual) console.log(`  awaiting manual capture  ${pair}`)
  for (const stray of r.undeclared) console.log(`  undeclared  ${stray}`)
  console.log(`shots:check ${dir}: ${r.captured.length} captured · ${r.missing.length} missing · ${r.manual.length} awaiting manual capture · ${r.undeclared.length} undeclared`)
  if (r.missing.length > 0 || r.undeclared.length > 0) {
    console.error(`check failed: ${r.missing.length} missing, ${r.undeclared.length} undeclared`)
    process.exit(1)
  }
  console.log(`  check ok`)
}
