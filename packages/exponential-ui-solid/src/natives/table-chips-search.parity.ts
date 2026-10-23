// FEED-128 (the Table / ChipInput / FileUpload / SearchList lane): what the
// three `*.parity.test.tsx` files share. `expectParity` = `expectDomParity`
// plus ONE extra id pass: `src/test/parity.ts` `GENERATED_ID` does not know
// React 19.2's `useId` form (`_r_1_`, and derived `_r_1_-0`), which the
// browser comparator (`conformance/parity-compare.ts`) does; both sides'
// generated tokens (already-renamed `$idN`, `_r_…`) are renumbered here in
// order of appearance.

import { expect } from "vitest"
import type { NestedNode } from "@exponential_app/ui"
import kitchenSink from "@exponential_app/ui/fixtures/kitchen-sink.json"
import { renderBoth, type ParityCase } from "../test/parity"

const ID_ATTRS = /\b(id|for|aria-controls|aria-labelledby|aria-describedby|aria-owns|aria-activedescendant)="([^"]*)"/g
const GENERATED = /^(\$id\d+|_r_[0-9a-z]+_.*)$/

function renumber(markup: string): string {
  const ids = new Map<string, string>()
  return markup.replace(ID_ATTRS, (_, name: string, value: string) => {
    const v = value
      .split(/\s+/)
      .map((t) => {
        if (!GENERATED.test(t)) return t
        let hit = ids.get(t)
        if (!hit) {
          hit = `#g${ids.size + 1}`
          ids.set(t, hit)
        }
        return hit
      })
      .join(` `)
    return `${name}="${v}"`
  })
}

export async function expectParity(c: ParityCase): Promise<void> {
  const { react, solid } = await renderBoth(c)
  expect(renumber(solid)).toBe(renumber(react))
}

/** A node of the kitchen sink by id. */
export function sinkNode(id: string): NestedNode {
  const walk = (n: unknown): NestedNode | null => {
    if (Array.isArray(n)) {
      for (const x of n) {
        const hit = walk(x)
        if (hit) return hit
      }
    } else if (n && typeof n === `object`) {
      if ((n as { id?: string }).id === id && `component` in (n as object)) return n as NestedNode
      for (const v of Object.values(n)) {
        const hit = walk(v)
        if (hit) return hit
      }
    }
    return null
  }
  return walk(kitchenSink)!
}
