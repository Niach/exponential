#!/usr/bin/env bun
// VAPP-127: what the NATIVE renders measured. The shots pipeline's `eval`
// lane (`bun run shots -- --platform <p> --views eval`) paints every
// recorded surface in the Compose example, the Swift example and the IDE
// (eval/eval-specimens.json) and stores eval/out/<id>/<platform>.webp plus the
// frames each host dumped (<platform>.frames.json: every visible node's box,
// dp / pt / px in surface coordinates). This turns the frames into the SAME
// measures render.ts takes on the web (render.ts measureFrames: the gaps
// between the root's children, the first text's inset, the height, the
// nodes, a sideways overflow) and compares each with the React render at the
// corpus width closest to the native's: the gaps and the inset should agree
// (the same spacing tokens), the height may not (a different width).
//
//   bun eval/natives.ts
//
// → eval/report.json `natives` (committed; the pictures stay gitignored).

import { existsSync, readFileSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import { reduceSurface } from "../src/reducer"
import corpus from "./corpus.json" with { type: "json" }
import { parseReply, surfaceOf, type Corpus } from "./lib"
import { measureFrames, type Frame, type RendererMeasure, type RenderMeasure } from "./render"
import { REPORT_PATH, buildReport, readFixture, readReport } from "./score"

export const NATIVE_PLATFORMS = [`android`, `ios`, `desktop`] as const
export type NativePlatform = (typeof NATIVE_PLATFORMS)[number]
const OUT = join(import.meta.dir, `out`)

export interface NativeMeasure extends RendererMeasure {
  /** The surface width the host laid out at (dp / pt / px). */
  width: number
  /** The React render this one is compared with (its corpus width). */
  webWidth: number
  /** gaps / textInset that differ from that React render (empty = the same spacing). */
  differences: string[]
}

export interface NativesReport {
  $comment: string
  surfaces: Record<string, Partial<Record<NativePlatform, NativeMeasure>>>
}

interface FramesFile {
  surface: string
  width: number
  frames: Frame[]
}

/** The native measure against React's: the spacing must agree, the height may not. */
export function compareWithWeb(native: Omit<RendererMeasure, `picture`>, web: Omit<RendererMeasure, `picture`>): string[] {
  const out: string[] = []
  const near = (a: number, b: number) => Math.abs(a - b) <= 1
  if (native.gaps.length !== web.gaps.length || native.gaps.some((g, i) => !near(g, web.gaps[i])))
    out.push(`gaps: web ${JSON.stringify(web.gaps)}, native ${JSON.stringify(native.gaps)}`)
  if ((native.textInset === null) !== (web.textInset === null) || (native.textInset !== null && !near(native.textInset, web.textInset!)))
    out.push(`textInset: web ${web.textInset}, native ${native.textInset}`)
  if (native.overflowsX) out.push(`overflowsX`)
  return out
}

export function measureNatives(): NativesReport {
  const render = (readReport()?.render ?? {}) as Record<string, RenderMeasure[]>
  const surfaces: NativesReport[`surfaces`] = {}
  for (const entry of (corpus as Corpus).prompts) {
    const fixture = readFixture(entry.id)
    if (!fixture) continue
    const surface = surfaceOf(parseReply(fixture.text).messages)
    const { root } = reduceSurface(surface.components, { catalogId: surface.catalogId })
    for (const platform of NATIVE_PLATFORMS) {
      const file = join(OUT, entry.id, `${platform}.frames.json`)
      if (!existsSync(file)) continue
      const dump = JSON.parse(readFileSync(file, `utf8`)) as FramesFile
      const width = Math.round(dump.width)
      const measure = measureFrames(dump.frames, root, width)
      const webs = render[entry.id] ?? []
      const web = [...webs].sort((a, b) => Math.abs(a.width - width) - Math.abs(b.width - width))[0]
      ;(surfaces[entry.id] ??= {})[platform] = {
        ...measure,
        picture: `eval/out/${entry.id}/${platform}.webp`,
        width,
        webWidth: web?.width ?? 0,
        differences: web ? compareWithWeb(measure, web.react) : [`no web render on record`],
      }
    }
  }
  return {
    $comment: `What the native renders measured (VAPP-127): the frames the Compose example, the Swift example and the IDE dumped for each eval specimen, measured like render.ts measures the web, and compared with the React render at the closest corpus width (gaps and the first text's inset within 1 px; the height is the width's). The shots pipeline's eval lane writes the frames; natives.ts writes this.`,
    surfaces,
  }
}

if (import.meta.main) {
  const natives = measureNatives()
  for (const [id, byPlatform] of Object.entries(natives.surfaces)) {
    for (const [platform, m] of Object.entries(byPlatform)) {
      console.log(`${id} ${platform} @${m!.width}: ${m!.nodes} nodes, ${m!.height} tall, gaps ${m!.distinctGaps.join(`/`) || `–`}, inset ${m!.textInset ?? `–`}; vs web @${m!.webWidth}: ${m!.differences.length ? m!.differences.join(`; `) : `same spacing`}`)
    }
  }
  const report = buildReport({ ...readReport(), natives })
  writeFileSync(REPORT_PATH, `${JSON.stringify(report, null, 2)}\n`)
  console.log(`→ ${REPORT_PATH} (natives: ${Object.keys(natives.surfaces).length} surfaces)`)
}
