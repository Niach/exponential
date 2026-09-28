// The fixed fake measure (`FixedMeasure::intrinsic` in
// apps/desktop/crates/vapp-spike/src/measure.rs), reproduced byte for byte so
// geometry mode compares CSS layout against taffy with identical leaf sizes.
import type { VappNode } from "./fixture"

const CHAR = 8
const LINE = 20

const chars = (s: string) => [...s].length

export function fixedIntrinsic(node: VappNode): { w: number; h: number } {
  const p = node.props ?? {}
  const str = (k: string) => (typeof p[k] === `string` ? (p[k] as string) : ``)
  switch (node.kind) {
    case `text`:
      return { w: CHAR * chars(str(`text`)), h: LINE }
    case `markdown`: {
      // Rust `str::lines()`: split on \n, a trailing newline adds no line.
      const text = str(`text`)
      const lines = text.replace(/\n$/, ``).split(`\n`)
      const n = Math.max(lines.length, 1)
      const widest = lines.reduce((m, l) => Math.max(m, chars(l)), 0)
      return { w: CHAR * widest, h: LINE * n }
    }
    case `button`:
      return { w: CHAR * chars(str(`label`)) + 24, h: 36 }
    case `pill`:
      return { w: CHAR * chars(str(`label`)) + 20, h: 28 }
    case `listrow`:
      return { w: CHAR * (chars(str(`title`)) + chars(str(`meta`))) + 16, h: 32 }
    case `textfield`:
    case `select`:
      return { w: 160, h: 36 }
    case `textarea`:
      return { w: 160, h: 72 }
    case `toggle`:
      return { w: 44, h: 24 }
    case `badge`:
      return { w: 20, h: 20 }
    case `avatar`: {
      const s = typeof p.size === `number` ? p.size : 32
      return { w: s, h: s }
    }
    case `image`:
      return { w: 320, h: 180 }
    case `divider`:
      return { w: 0, h: 1 }
    case `progress`:
      return { w: 160, h: 8 }
    case `box`:
    case `card`:
      return { w: 0, h: 0 }
  }
}
