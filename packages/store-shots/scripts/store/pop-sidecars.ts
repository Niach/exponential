/* Collect the pop-out rects the UI tests measured, into the sidecars the store
   compositor reads (EXP-627).

   `store-crops.ts` resolves a pop-out rect sidecar → HAND_RECTS → nothing. The
   hand rects were measured by eye against one generation of captures and go
   stale the moment a seed or a layout moves; the sidecars are the accurate
   path, because the UI test knows exactly where the element it just
   photographed was. `PopRects.swift` / `PopRects.kt` write one file per shot as
   they run — but they write it where their harness can write, in the shape one
   DEVICE saw, and the compositor wants one form-keyed file per shot next to the
   raw capture. This script is that adapter.

     bun run screenshots:pop-sidecars -- --platform ios
     bun run screenshots:pop-sidecars -- --platform android
     bun run screenshots:pop-sidecars -- --platform ios --from <dir> --dry-run

   Sources, matching where each harness can actually write:

     ios      ~/Library/Caches/tools.fastlane/exp-pop/<simulator>-pop-<shot>.json
              — a sibling of the host dir SnapshotHelper writes PNGs to. The
              simulator prefix picks the form, exactly as `formForFile` does for
              the images: anything matching /ipad/i is the tablet.
     android  apps/android/fastlane/exp-pop/pop-<shot>.json — where the
              `screenshots` lane `adb pull`s /sdcard/…/exp-pop after the run.
              One emulator, one form.

   Destination: `pop-<shot>.json` in the platform's RAW dir (`LAYOUT[*].raw`),
   the directory the compositor reads its captures from — form-keyed, so the
   iPhone and iPad passes accumulate into one file instead of overwriting each
   other. A form this run did not measure is left exactly as it was, which is
   what makes the two iOS simulators safe to run separately.

   Consumed inputs are deleted, so a later run cannot re-apply a stale rect
   measured against a capture that has since been replaced. Nothing here is
   committed — store screenshots and their sidecars stay out of git (EXP-348). */

import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs"
import { homedir } from "node:os"
import { basename, join } from "node:path"
import { LAYOUT, REPO, type Form, type Platform } from "./raw-store"
import { isRect, type Rect } from "./store-crops"

/** Every form the compositor knows — needed to widen a legacy bare-rect sidecar. */
const FORMS: readonly Form[] = [`ios-phone`, `ios-tablet`, `android-phone`]

/** Where each harness leaves its measurements. */
function sourceDir(platform: Platform): string {
  return platform === `ios`
    // A sibling of fastlane's `screenshots` cache dir: the collector `rm_rf`s
    // that one after moving the PNGs out, so sidecars written there vanish
    // before this script runs (learned the hard way, EXP-627).
    ? join(homedir(), `Library/Caches/tools.fastlane/exp-pop`)
    : join(REPO, `apps/android/fastlane/exp-pop`)
}

interface Measured {
  shot: string
  form: Form
  rect: Rect
  file: string
}

/**
 * `iPhone 17 Pro Max-pop-01_board.json` / `pop-1_board.json` → shot + form.
 * The harnesses name the file after the STORE shot (`01_board`), the
 * compositor keys everything by the bare name (`board`, see `shotKey` in
 * raw-store.ts) — strip the numeric prefix here, once.
 */
function parseName(platform: Platform, file: string): { shot: string; form: Form } | null {
  const match = /^(.*?)pop-(?:\d+_)?([a-z0-9-]+)\.json$/.exec(basename(file))
  if (!match) return null
  const prefix = match[1]!.replace(/-$/, ``)
  const form: Form =
    platform === `android` ? `android-phone` : /ipad/i.test(prefix) ? `ios-tablet` : `ios-phone`
  return { shot: match[2]!, form }
}

/** The harnesses write a bare rect; accept a `{rect}` wrapper too. */
function readRect(path: string): Rect | null {
  let parsed: unknown
  try {
    parsed = JSON.parse(readFileSync(path, `utf8`))
  } catch {
    console.warn(`  ${basename(path)}: not valid JSON — ignored`)
    return null
  }
  const wrapped = (parsed as { rect?: unknown } | null)?.rect
  const candidate = isRect(wrapped) ? wrapped : parsed
  if (!isRect(candidate)) {
    console.warn(`  ${basename(path)}: no usable 0..1 rect — ignored`)
    return null
  }
  return candidate
}

function collect(platform: Platform, from: string): Measured[] {
  if (!existsSync(from)) return []
  const out: Measured[] = []
  for (const name of readdirSync(from).sort()) {
    if (!name.endsWith(`.json`) || !name.includes(`pop-`)) continue
    const parsed = parseName(platform, name)
    if (!parsed) continue
    const path = join(from, name)
    const rect = readRect(path)
    if (rect) out.push({ ...parsed, rect, file: path })
  }
  return out
}

/**
 * Existing sidecar → the forms it already carries. A LEGACY bare rect applied
 * to every form (that is how `popRect` resolves one), so it is widened rather
 * than dropped: merging a form key into a bare rect would otherwise silently
 * un-tune every other form.
 */
function existingForms(path: string): Partial<Record<Form, Rect>> {
  if (!existsSync(path)) return {}
  let parsed: unknown
  try {
    parsed = JSON.parse(readFileSync(path, `utf8`))
  } catch {
    return {}
  }
  if (isRect(parsed)) {
    return Object.fromEntries(FORMS.map((form) => [form, parsed])) as Record<Form, Rect>
  }
  const out: Partial<Record<Form, Rect>> = {}
  for (const form of FORMS) {
    const value = (parsed as Record<string, unknown> | null)?.[form]
    if (isRect(value)) out[form] = value
  }
  return out
}

function main(): void {
  const argv = process.argv.slice(2)
  const flag = (name: string): string | undefined => {
    const index = argv.indexOf(`--${name}`)
    return index === -1 ? undefined : argv[index + 1]
  }
  const platform = flag(`platform`)
  if (platform !== `ios` && platform !== `android`) {
    console.error(`usage: bun run screenshots:pop-sidecars -- --platform ios|android [--from <dir>] [--dry-run]`)
    process.exit(1)
  }
  const dryRun = argv.includes(`--dry-run`)
  const from = flag(`from`) ?? sourceDir(platform)
  const to = LAYOUT[platform].raw

  console.log(`pop-sidecars: ${platform}`)
  console.log(`  from ${from}`)
  console.log(`  to   ${to}`)

  const measured = collect(platform, from)
  if (measured.length === 0) {
    // Not an error: the pop-rect writers are best-effort, and a lane that ran
    // without them still composes perfectly well off HAND_RECTS.
    console.log(`  nothing to merge (no pop-*.json) — the compositor falls back to HAND_RECTS`)
    return
  }

  const byShot = new Map<string, Measured[]>()
  for (const entry of measured) {
    byShot.set(entry.shot, [...(byShot.get(entry.shot) ?? []), entry])
  }

  if (!dryRun) mkdirSync(to, { recursive: true })
  for (const [shot, entries] of [...byShot].sort(([a], [b]) => a.localeCompare(b))) {
    const target = join(to, `pop-${shot}.json`)
    const merged: Partial<Record<Form, Rect>> = existingForms(target)
    for (const entry of entries) merged[entry.form] = entry.rect
    const forms = entries.map((entry) => entry.form).join(`, `)
    console.log(
      `  pop-${shot}.json  ${forms}${Object.keys(merged).length > entries.length ? ` (+ kept ${Object.keys(merged).length - entries.length} other form(s))` : ``}`
    )
    if (dryRun) continue
    writeFileSync(target, `${JSON.stringify(merged, null, 2)}\n`)
    for (const entry of entries) rmSync(entry.file, { force: true })
  }

  console.log(
    `  ${byShot.size} sidecar(s) ${dryRun ? `would be written` : `written`} from ${measured.length} measurement(s)`
  )
}

main()
