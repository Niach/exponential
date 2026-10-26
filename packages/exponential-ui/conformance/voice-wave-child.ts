// NEW-77: THE CHILD RULE of VoiceWave (catalog/style.json `voiceWave`)
// measured on a WEB painter in headless Chromium: every `child` case of
// fixtures/voice-wave.json is painted (the native through the harness's
// `?view=mount`, the primitive through `?view=primitives&id=plain/voice-wave`,
// whose first five waves are these cases in order) and the DRAWN child box
// (getBoundingClientRect, transforms included, relative to the wave's root)
// must sit within 0.5 px of `expected.child`. The renderers' browser tests
// (voice-wave-child.test.ts) call these.

import { readFileSync } from "node:fs"
import { join } from "node:path"
import type { Page } from "playwright"
import type { NestedNode } from "../src/index"

export interface VoiceWaveChildCase {
  name: string
  input: { size: number; activity: string; level?: number; bands?: number[]; child: { width: number; height: number } }
  expected: { x: number; y: number; width: number; height: number; scale: number }
}

export interface DrawnBox {
  x: number
  y: number
  width: number
  height: number
}

/** The px edge → the size enum (the neutral theme's `VoiceWave/root`, the
 *  primitives' VOICE_WAVE_SIZE_PX). */
const SIZE_NAME: Record<number, string> = { 32: `sm`, 56: `md`, 160: `lg`, 200: `xl` }

export const VOICE_WAVE_CHILD_TOLERANCE = 0.5

export function voiceWaveChildCases(): VoiceWaveChildCase[] {
  const fixture = JSON.parse(readFileSync(join(import.meta.dirname, `..`, `fixtures`, `voice-wave.json`), `utf8`)) as { cases: { name: string; input: VoiceWaveChildCase[`input`] & { child?: unknown }; expected: { child?: VoiceWaveChildCase[`expected`] } }[] }
  return fixture.cases.filter((c) => c.input.child && c.expected.child).map((c) => ({ name: c.name, input: c.input, expected: c.expected.child! }))
}

/** One VoiceWave per case (`vw<i>`), its one child a Box of the case's
 *  child size (`vw<i>-c`), in a wrapping row. */
export function voiceWaveChildTree(cases = voiceWaveChildCases()): NestedNode {
  return {
    id: `root`,
    component: `Box`,
    style: { display: `flex`, flexDirection: `row`, flexWrap: `wrap`, alignItems: `center`, gap: 24, padding: 24 },
    children: cases.map((c, i) => {
      const size = SIZE_NAME[c.input.size]
      if (!size) throw new Error(`${c.name}: no size enum for ${c.input.size} px`)
      return {
        id: `vw${i}`,
        component: `VoiceWave`,
        props: { activity: c.input.activity, level: c.input.level ?? 0, ...(c.input.bands ? { bands: c.input.bands } : {}), size },
        children: [{ id: `vw${i}-c`, component: `Box`, style: { width: c.input.child.width, height: c.input.child.height } }],
      }
    }),
  } as NestedNode
}

/** `true`, or what is off (x, y, width, height beyond the tolerance). */
export function checkVoiceWaveChild(expected: VoiceWaveChildCase[`expected`], drawn: DrawnBox | null): true | string {
  if (!drawn) return `child not painted`
  const off = ([`x`, `y`, `width`, `height`] as const).filter((k) => !(Math.abs(drawn[k] - expected[k]) <= VOICE_WAVE_CHILD_TOLERANCE))
  return off.length === 0 || off.map((k) => `${k} ${expected[k]}≠${drawn[k].toFixed(2)}`).join(`, `)
}

/** The drawn child of each [root selector, child selector] pair, relative
 *  to its root (null = not found), after two frames (the measure lands
 *  before the first paint; the frames let any re-measure settle). */
export function drawnChildBoxes(page: Page, pairs: [string, string][]): Promise<(DrawnBox | null)[]> {
  return page.evaluate(async (pairs) => {
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))
    return pairs.map(([rootSel, childSel]) => {
      const root = document.querySelector(rootSel)
      const child = document.querySelector(childSel)
      if (!root || !child) return null
      const r = root.getBoundingClientRect()
      const c = child.getBoundingClientRect()
      return { x: c.left - r.left, y: c.top - r.top, width: c.width, height: c.height }
    })
  }, pairs)
}

/** The native (a page at `?view=mount`): mounts the tree, measures. */
export async function measureNativeVoiceWaveChildren(page: Page, cases = voiceWaveChildCases()): Promise<{ name: string; verdict: true | string }[]> {
  await page.waitForFunction(() => typeof (window as unknown as { __xuiMount?: unknown }).__xuiMount === `function`)
  await page.evaluate((tree) => (window as unknown as { __xuiMount: (t: unknown, o?: unknown) => Promise<void> }).__xuiMount(tree, { theme: `neutral` }), voiceWaveChildTree(cases))
  const boxes = await drawnChildBoxes(
    page,
    cases.map((_, i) => [`[data-xui-id="vw${i}"]`, `[data-xui-id="vw${i}-c"]`])
  )
  return cases.map((c, i) => ({ name: c.name, verdict: checkVoiceWaveChild(c.expected, boxes[i]!) }))
}

/** The primitive (a page at `?view=primitives&id=plain/voice-wave`). */
export async function measurePrimitiveVoiceWaveChildren(page: Page, cases = voiceWaveChildCases()): Promise<{ name: string; verdict: true | string }[]> {
  await page.waitForSelector(`[data-voice-case="${cases.length - 1}"] [data-slot="voice-wave"]`, { state: `attached` })
  const boxes = await drawnChildBoxes(
    page,
    cases.map((_, i) => [`[data-voice-case="${i}"] [data-slot="voice-wave"]`, `[data-voice-case="${i}"] [data-voice-child]`])
  )
  return cases.map((c, i) => ({ name: c.name, verdict: checkVoiceWaveChild(c.expected, boxes[i]!) }))
}
